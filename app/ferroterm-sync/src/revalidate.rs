// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Revalidating the deployment's own value sets and maps after a release is
//! activated.
//!
//! A new release of a code system can retire a code a locally authored
//! `ValueSet` enumerates, remove it, or leave it outside the value set it was
//! included through. No FHIR or SNOMED CT specification defines that check:
//! our own design. It reports and never edits; a finding is something a
//! terminologist decides about.
//!
//! The check runs entirely over the server's public FHIR API, with
//! `ValueSet/$validate-code` and `ValueSet/$expand`, both of which take
//! `activeOnly` from the terminology ecosystem's requirements
//! (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so it sees
//! exactly what a client sees and needs no access to the engine. Both
//! operations go through `ferrobridge-term` as `GET` requests, which writes
//! them and reads their answers as the base's FHIR release defines them.
//!
//! The service reads two fields out of a searched resource it never writes
//! back, so it models no FHIR resource of its own.

use std::collections::BTreeSet;

use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, Form, WireVersion};
use ferrobridge_term::outcome::{ExpandOutcome, ExpansionContains};
use ferrobridge_term::request::{Coded, Expansion, Validation};

use crate::record::{Activated, Finding, FindingKind, Revalidation};

/// How many codes of one resource are checked.
///
/// A local value set that enumerates more than this is reported as partly
/// checked rather than turning one run into thousands of requests.
pub const MAX_CODES: usize = 1000;

/// How many members of an expansion are compared.
pub const MAX_EXPANSION: u32 = 1000;

/// A check that could not be performed.
#[derive(Debug, thiserror::Error)]
pub enum RevalidateError {
    /// The configured FHIR base is no URL, or names no FHIR release the
    /// server mounts as its last segment.
    #[error("the FHIR base {base_url} names no served FHIR release (r4, r4b, r5 or r6)")]
    Base {
        /// The configured base.
        base_url: String,
    },
    /// The terminology client could not be built.
    #[error("the FHIR client for {base_url} could not be built")]
    Client {
        /// The configured base.
        base_url: String,
        /// Why the client was refused.
        #[source]
        source: Box<ferrobridge_term::error::Error>,
    },
    /// The request never reached the server.
    #[error("the request to {url} failed")]
    Request {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The server answered with a status that is not a success.
    #[error("{url} answered {status}")]
    Status {
        /// The address the request was sent to.
        url: String,
        /// The status the server answered with.
        status: u16,
    },
    /// The answer is not the JSON the FHIR API serves.
    #[error("the answer from {url} is not JSON")]
    Body {
        /// The address the request was sent to.
        url: String,
        /// Why the answer did not read.
        #[source]
        source: reqwest::Error,
    },
    /// A terminology operation was refused, or its answer did not read.
    #[error("{operation} on the value set {value_set} did not answer")]
    Operation {
        /// The operation, as `Resource/$code`.
        operation: &'static str,
        /// The canonical of the value set it ran on.
        value_set: String,
        /// The refusal, with the upstream status and body.
        #[source]
        source: Box<ferrobridge_term::error::Error>,
    },
    /// A terminology operation answered in a form this service does not read.
    #[error(
        "{operation} on the value set {value_set} answered in a form this service does not read"
    )]
    Unread {
        /// The operation, as `Resource/$code`.
        operation: &'static str,
        /// The canonical of the value set it ran on.
        value_set: String,
    },
    /// `$expand` answered that the server does not know the value set.
    #[error("the server does not know the value set {value_set}")]
    UnknownValueSet {
        /// The canonical of the value set.
        value_set: String,
    },
}

/// The server's FHIR API, as this service reads it.
#[derive(Debug, Clone)]
pub struct FhirClient {
    http: reqwest::Client,
    base_url: String,
    terminology: Client,
}

impl FhirClient {
    /// The client for the FHIR base at `base_url`, such as
    /// `http://ferroterm:8080/r4b`, whose last segment names the FHIR
    /// release the operations are written in.
    ///
    /// # Errors
    ///
    /// Returns [`RevalidateError::Base`] when `base_url` is no URL or its last
    /// segment names no release the server mounts, and
    /// [`RevalidateError::Client`] when the terminology client is refused.
    pub fn new(http: reqwest::Client, base_url: &str) -> Result<Self, RevalidateError> {
        let base_url = base_url.trim_end_matches('/').to_owned();
        let refused = || RevalidateError::Base {
            base_url: base_url.clone(),
        };
        let parsed = url::Url::parse(&base_url).map_err(|_unparsed| refused())?;
        let release = parsed
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .and_then(release)
            .ok_or_else(refused)?;
        let config = Config::new(parsed, release).with_form(Form::Get);
        let terminology =
            Client::with_http(config, http.clone()).map_err(|source| RevalidateError::Client {
                base_url: base_url.clone(),
                source: Box::new(source),
            })?;
        Ok(Self {
            http,
            base_url,
            terminology,
        })
    }

    /// The resources of `resource_type` the server holds in its write store.
    ///
    /// # Errors
    ///
    /// Returns [`RevalidateError`] when the search does not answer or does not
    /// read.
    pub async fn local(
        &self,
        resource_type: &str,
    ) -> Result<Vec<serde_json::Value>, RevalidateError> {
        let url = format!("{}/{resource_type}", self.base_url);
        let bundle = self.get(&url).await?;
        Ok(bundle
            .get("entry")
            .and_then(serde_json::Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("resource").cloned())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Whether `code` from `system` is a member of the value set `url`.
    ///
    /// # Errors
    ///
    /// Returns [`RevalidateError::Operation`] when the operation is refused
    /// or its answer does not read.
    pub async fn validates(
        &self,
        url: &str,
        system: &str,
        code: &str,
        active_only: bool,
    ) -> Result<bool, RevalidateError> {
        let validation = Validation {
            active_only: active_only.then_some(true),
            ..Validation::new(
                url,
                Coded::Code {
                    system: system.to_owned(),
                    code: code.to_owned(),
                    version: None,
                    display: None,
                },
            )
        };
        self.terminology
            .validate(&validation)
            .await
            .map(|outcome| outcome.is_valid())
            .map_err(|source| RevalidateError::Operation {
                operation: ferrobridge_term::VALIDATE_CODE,
                value_set: url.to_owned(),
                source: Box::new(source),
            })
    }

    /// The members of the expansion of the value set `url`, the entries at
    /// its top level.
    ///
    /// # Errors
    ///
    /// Returns [`RevalidateError::Operation`] when the operation is refused
    /// or its answer does not read, and [`RevalidateError::UnknownValueSet`]
    /// when the server does not know the value set.
    pub async fn expansion(
        &self,
        url: &str,
        active_only: bool,
    ) -> Result<BTreeSet<(String, String)>, RevalidateError> {
        let expansion = Expansion {
            count: Some(MAX_EXPANSION),
            active_only: active_only.then_some(true),
            ..Expansion::new(url)
        };
        let outcome = self
            .terminology
            .expand(&expansion)
            .await
            .map_err(|source| RevalidateError::Operation {
                operation: ferrobridge_term::EXPAND,
                value_set: url.to_owned(),
                source: Box::new(source),
            })?;
        match outcome {
            ExpandOutcome::Expanded(expanded) => {
                Ok(expanded.contains.into_iter().filter_map(member).collect())
            }
            ExpandOutcome::NotFound { .. } => Err(RevalidateError::UnknownValueSet {
                value_set: url.to_owned(),
            }),
            _ => Err(RevalidateError::Unread {
                operation: ferrobridge_term::EXPAND,
                value_set: url.to_owned(),
            }),
        }
    }

    /// One `GET` against the FHIR API, read as JSON.
    async fn get(&self, url: &str) -> Result<serde_json::Value, RevalidateError> {
        let response = self
            .http
            .get(url)
            .header("accept", "application/fhir+json")
            .send()
            .await
            .map_err(|source| RevalidateError::Request {
                url: url.to_owned(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(RevalidateError::Status {
                url: url.to_owned(),
                status: status.as_u16(),
            });
        }
        response
            .json()
            .await
            .map_err(|source| RevalidateError::Body {
                url: url.to_owned(),
                source,
            })
    }
}

/// The FHIR release a base's last segment names, as the server mounts it.
// NOTE: no specification governs this: our own design; the server mounts each
// release at `/r4`, `/r4b`, `/r5` and `/r6`.
fn release(segment: &str) -> Option<WireVersion> {
    match segment.to_ascii_lowercase().as_str() {
        "r4" => Some(WireVersion::R4),
        "r4b" => Some(WireVersion::R4B),
        "r5" => Some(WireVersion::R5),
        "r6" => Some(WireVersion::R6),
        _ => None,
    }
}

/// The system and code of one expansion entry.
// NOTE: <https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>:
// an entry with no code only groups its children, so it names no member.
fn member(entry: ExpansionContains) -> Option<(String, String)> {
    Some((entry.system?, entry.code?))
}

/// One string field of a JSON object.
fn field(value: &serde_json::Value, name: &str) -> Option<String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Checks every locally authored value set and map against the served set.
///
/// `activated` is what this run put in front of the server, so a finding can
/// name the release that changed the code.
///
/// # Errors
///
/// Returns [`RevalidateError`] when the server does not answer; the release is
/// already served by then, so a caller reports the check as not performed
/// rather than failing the run.
pub async fn revalidate(
    client: &FhirClient,
    activated: &[Activated],
) -> Result<Revalidation, RevalidateError> {
    let mut report = Revalidation::default();
    for value_set in client.local("ValueSet").await? {
        report.resources = report.resources.saturating_add(1);
        check_value_set(client, &value_set, activated, &mut report).await?;
    }
    for map in client.local("ConceptMap").await? {
        report.resources = report.resources.saturating_add(1);
        check_concept_map(client, &map, activated, &mut report).await?;
    }
    report.statement = statement(&report);
    Ok(report)
}

/// The sentence a run record and a webhook read.
fn statement(report: &Revalidation) -> String {
    if report.resources == 0 {
        return String::from("no locally authored value set or map is served");
    }
    if report.findings.is_empty() {
        return format!(
            "{} locally authored resources were revalidated and no local code changed",
            report.resources
        );
    }
    format!(
        "{} locally authored resources were revalidated and {} local codes changed",
        report.resources,
        report.findings.len()
    )
}

/// Checks one locally authored value set.
async fn check_value_set(
    client: &FhirClient,
    value_set: &serde_json::Value,
    activated: &[Activated],
    report: &mut Revalidation,
) -> Result<(), RevalidateError> {
    let Some(url) = field(value_set, "url") else {
        return Ok(());
    };
    let name = resource_name("ValueSet", value_set, &url);
    report.checked_resources.push(name.clone());
    let enumerated = enumerated_codes(value_set);
    if enumerated.is_empty() {
        let all = client.expansion(&url, false).await?;
        let active = client.expansion(&url, true).await?;
        report.checked = report.checked.saturating_add(all.len());
        for (system, code) in all.difference(&active) {
            report.findings.push(finding(
                &name,
                system,
                code,
                FindingKind::Inactive,
                activated,
            ));
        }
        return Ok(());
    }
    for (system, code) in enumerated.iter().take(MAX_CODES) {
        report.checked = report.checked.saturating_add(1);
        if client.validates(&url, system, code, false).await? {
            if !client.validates(&url, system, code, true).await? {
                report.findings.push(finding(
                    &name,
                    system,
                    code,
                    FindingKind::Inactive,
                    activated,
                ));
            }
            continue;
        }
        let kind = if client
            .validates(&implicit_value_set(system), system, code, false)
            .await?
        {
            FindingKind::OutsideValueSet
        } else {
            FindingKind::Absent
        };
        report
            .findings
            .push(finding(&name, system, code, kind, activated));
    }
    if enumerated.len() > MAX_CODES {
        tracing::warn!(
            resource = name,
            codes = enumerated.len(),
            checked = MAX_CODES,
            "only the first codes of the value set were revalidated"
        );
    }
    Ok(())
}

/// Checks one locally authored concept map.
///
/// A map names its codes in its groups, and each group names the system its
/// codes come from, so every code is checked against that system's implicit
/// all-codes value set.
async fn check_concept_map(
    client: &FhirClient,
    map: &serde_json::Value,
    activated: &[Activated],
    report: &mut Revalidation,
) -> Result<(), RevalidateError> {
    let url = field(map, "url").unwrap_or_default();
    let name = resource_name("ConceptMap", map, &url);
    report.checked_resources.push(name.clone());
    for (system, code) in mapped_codes(map).iter().take(MAX_CODES) {
        report.checked = report.checked.saturating_add(1);
        let value_set = implicit_value_set(system);
        if !client.validates(&value_set, system, code, false).await? {
            report
                .findings
                .push(finding(&name, system, code, FindingKind::Absent, activated));
            continue;
        }
        if !client.validates(&value_set, system, code, true).await? {
            report.findings.push(finding(
                &name,
                system,
                code,
                FindingKind::Inactive,
                activated,
            ));
        }
    }
    Ok(())
}

/// The name a finding gives a resource: its type and its identity.
fn resource_name(resource_type: &str, resource: &serde_json::Value, url: &str) -> String {
    if url.is_empty() {
        let id = field(resource, "id").unwrap_or_else(|| String::from("(no id)"));
        return format!("{resource_type}/{id}");
    }
    format!("{resource_type} {url}")
}

/// The implicit value set of every code in `system`.
///
/// The FHIR specification defines `[system]?fhir_vs` as the value set of all
/// codes in a code system
/// (<https://hl7.org/fhir/R4B/valueset.html#intensional>).
fn implicit_value_set(system: &str) -> String {
    format!("{system}?fhir_vs")
}

/// One finding, with the release that changed the code when this run brought it.
fn finding(
    resource: &str,
    system: &str,
    code: &str,
    kind: FindingKind,
    activated: &[Activated],
) -> Finding {
    Finding {
        resource: resource.to_owned(),
        system: system.to_owned(),
        code: code.to_owned(),
        kind,
        release: release_of(system, activated),
    }
}

/// The version identifier this run activated for `system`, when it activated one.
fn release_of(system: &str, activated: &[Activated]) -> Option<String> {
    activated
        .iter()
        .find(|one| one.canonical == system || one.canonical.starts_with(system))
        .map(|one| one.version.clone())
}

/// The codes a value set enumerates, in `compose.include[].concept[]`.
fn enumerated_codes(value_set: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(includes) = value_set
        .get("compose")
        .and_then(|compose| compose.get("include"))
        .and_then(serde_json::Value::as_array)
    else {
        return out;
    };
    for include in includes {
        let Some(system) = field(include, "system") else {
            continue;
        };
        let Some(concepts) = include.get("concept").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for concept in concepts {
            if let Some(code) = field(concept, "code") {
                out.push((system.clone(), code));
            }
        }
    }
    out
}

/// The codes a concept map names, on both sides of every group.
fn mapped_codes(map: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(groups) = map.get("group").and_then(serde_json::Value::as_array) else {
        return out;
    };
    for group in groups {
        let source = field(group, "source").or_else(|| field(group, "sourceCanonical"));
        let target = field(group, "target").or_else(|| field(group, "targetCanonical"));
        let Some(elements) = group.get("element").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for element in elements {
            if let (Some(system), Some(code)) = (source.clone(), field(element, "code")) {
                out.push((system, code));
            }
            let Some(targets) = element.get("target").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for mapped in targets {
                if let (Some(system), Some(code)) = (target.clone(), field(mapped, "code")) {
                    out.push((system, code));
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::{enumerated_codes, implicit_value_set, mapped_codes, release_of};
    use crate::record::Activated;

    #[test]
    fn the_implicit_value_set_is_the_system_with_the_fhir_vs_marker() {
        assert_eq!(
            implicit_value_set("http://snomed.info/sct"),
            "http://snomed.info/sct?fhir_vs",
            "the FHIR specification spells the all-codes value set this way"
        );
    }

    #[test]
    fn a_value_set_enumerates_its_codes_per_include() -> Result<(), Box<dyn core::error::Error>> {
        let value_set: serde_json::Value = serde_json::from_str(
            r#"{"resourceType":"ValueSet","url":"https://example.invalid/vs","compose":{"include":[
                {"system":"http://snomed.info/sct","concept":[{"code":"1"},{"code":"2"}]},
                {"system":"http://loinc.org","concept":[{"code":"3"}]},
                {"system":"http://example.invalid/cs"}]}}"#,
        )?;
        assert_eq!(
            enumerated_codes(&value_set),
            vec![
                (String::from("http://snomed.info/sct"), String::from("1")),
                (String::from("http://snomed.info/sct"), String::from("2")),
                (String::from("http://loinc.org"), String::from("3")),
            ],
            "an include that enumerates nothing contributes nothing"
        );
        Ok(())
    }

    #[test]
    fn a_concept_map_names_the_codes_on_both_sides() -> Result<(), Box<dyn core::error::Error>> {
        let map: serde_json::Value = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","url":"https://example.invalid/cm","group":[
                {"source":"http://snomed.info/sct","target":"http://loinc.org",
                 "element":[{"code":"1","target":[{"code":"9"}]}]}]}"#,
        )?;
        assert_eq!(
            mapped_codes(&map),
            vec![
                (String::from("http://snomed.info/sct"), String::from("1")),
                (String::from("http://loinc.org"), String::from("9")),
            ],
            "both the source code and the target code are checked"
        );
        Ok(())
    }

    #[test]
    fn a_finding_names_the_release_this_run_activated() {
        let activated = vec![Activated {
            lane: String::from("index"),
            target: std::path::PathBuf::from("/data/index/snomed-1-20260930"),
            canonical: String::from("http://snomed.info/sct/11000146104"),
            version: String::from("http://snomed.info/sct/11000146104/version/20260930"),
        }];
        assert_eq!(
            release_of("http://snomed.info/sct", &activated).as_deref(),
            Some("http://snomed.info/sct/11000146104/version/20260930"),
            "the edition this run brought is the release that changed the code"
        );
        assert_eq!(
            release_of("http://loinc.org", &activated),
            None,
            "a system this run did not touch names no release"
        );
    }
}
