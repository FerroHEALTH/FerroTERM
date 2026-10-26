// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What FerroTERM puts into the usage report of `ferrohealth-usage-report`.
//!
//! The crate carries the envelope v2, the reporter, the instance id and the
//! window. FerroTERM adds its `status` (the loaded code system families,
//! [`Status`]), its request groups by matched route template ([`grouping`]),
//! and one counter per served FHIR version ([`count_version`]). A report
//! names no code, no system or value set URL, no request path and no host.
//! No FHIR or SNOMED CT specification governs any of this: our own design.

use std::sync::Arc;
use std::time::Instant;

use ferrohealth_telemetry::request_log::RouteNamer;
use ferrohealth_usage_report::envelope::{Key, LicenceGrant, Product, report_git_sha};
use ferrohealth_usage_report::reporter::{Identity, Notice, ProductSection};
use ferrohealth_usage_report::section::UsageReportSection;
use ferrohealth_usage_report::window::Window;
use http::Method;
use serde::Serialize;
use tokio::task::JoinHandle;

use crate::reload::Serving;
use crate::state::Origin;

/// What the boot line says about the report.
pub const NOTICE: Notice = Notice {
    contents: "instance id, version, licence type, deployment, uptime, host size, loaded code \
               system families and coarse operation latency",
    switch: "FERROTERM__USAGE_REPORT__ENABLED=false",
    state_dir_variable: "FERROTERM__USAGE_REPORT__STATE_DIR",
    url: "https://ferroterm.eu/docs/operate/usage-report.html",
};

/// The most families a status lists.
pub const MAX_FAMILIES: usize = 16;

/// The commit this binary was built from, as the build recorded it.
const GIT_SHA: &str = env!("FERROTERM_GIT_SHA");

/// The identity of this build.
///
/// The server installs no commercial licence, so it runs under what the
/// Business Source License grants everyone, the non-commercial grant.
#[must_use]
pub fn identity() -> Identity {
    Identity {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        git_sha: report_git_sha(GIT_SHA),
        licence: LicenceGrant::NonCommercial,
    }
}

/// The window of one process, grouping requests with [`grouping`].
#[must_use]
pub fn window() -> Window {
    Window::new(grouping)
}

/// Logs the boot line and, when the report runs, starts its detached task.
///
/// `ci` is the CI variable `ferrohealth_usage_report::section::ci_of` found
/// in the process environment, if any.
pub async fn start(
    section: &UsageReportSection,
    serving: &Serving,
    started: Instant,
    ci: Option<&'static str>,
) -> Option<JoinHandle<()>> {
    ferrohealth_usage_report::reporter::start(
        section,
        &NOTICE,
        identity(),
        Arc::new(serving.clone()),
        serving.usage_window(),
        started,
        ci,
    )
    .await
}

impl ProductSection for Serving {
    type Status = Status;

    const PRODUCT: Product = Product::Term;

    fn status(&self) -> impl Future<Output = Status> + Send {
        std::future::ready(status_of(self))
    }
}

/// The status of the served set as of now.
fn status_of(serving: &Serving) -> Status {
    let state = serving.current();
    let families: Vec<Family> = state
        .origins()
        .map(|(url, origin)| Family::of(url, origin))
        .collect();
    Status::new(!families.is_empty(), families, state.persists())
}

/// A served FHIR version, by its path prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FhirVersion {
    /// `/r4`.
    R4,
    /// `/r4b`.
    R4b,
    /// `/r5`.
    R5,
    /// `/r6`.
    R6,
}

impl FhirVersion {
    /// The version a path prefix segment names.
    #[must_use]
    pub fn of_segment(segment: &str) -> Option<Self> {
        match segment {
            "r4" => Some(Self::R4),
            "r4b" => Some(Self::R4b),
            "r5" => Some(Self::R5),
            "r6" => Some(Self::R6),
            _ => None,
        }
    }

    /// The version whose base `path` lies under.
    #[must_use]
    pub fn of_path(path: &str) -> Option<Self> {
        Self::of_segment(path.split('/').find(|segment| !segment.is_empty())?)
    }

    /// The counter of the version's requests.
    #[must_use]
    pub const fn counter(self) -> &'static str {
        match self {
            Self::R4 => "fhir_r4",
            Self::R4b => "fhir_r4b",
            Self::R5 => "fhir_r5",
            Self::R6 => "fhir_r6",
        }
    }

    /// The route the request log names a request under this base no route
    /// matched.
    #[must_use]
    pub const fn unmatched_route(self) -> &'static str {
        match self {
            Self::R4 => "/r4/{*unmatched}",
            Self::R4b => "/r4b/{*unmatched}",
            Self::R5 => "/r5/{*unmatched}",
            Self::R6 => "/r6/{*unmatched}",
        }
    }
}

/// Counts one request to `path` under its FHIR version's counter; a request
/// outside every FHIR base is not counted.
pub fn count_version(window: &Window, path: &str) {
    if let Some(key) = FhirVersion::of_path(path).and_then(|version| Key::new(version.counter())) {
        window.count(key);
    }
}

/// Names a request under a FHIR base that no route matched, so the window
/// counts it as `other`; any other unmatched request stays unnamed.
#[derive(Debug, Clone, Copy, Default)]
pub struct FhirBaseNamer;

impl RouteNamer for FhirBaseNamer {
    fn route(&self, _method: &Method, path: &str) -> Option<String> {
        FhirVersion::of_path(path).map(|version| version.unmatched_route().to_owned())
    }
}

/// The request group of a matched route template under a FHIR base, or
/// `None` for a request outside every base (`/health`, `/metrics`) or one no
/// route matched.
#[must_use]
pub fn grouping(method: &Method, route: Option<&str>) -> Option<Key> {
    let segments: Vec<&str> = route?.split('/').filter(|s| !s.is_empty()).collect();
    let (prefix, rest) = segments.split_first()?;
    FhirVersion::of_segment(prefix)?;
    Key::new(group_of(method, rest))
}

/// The group of a request from its method and the route template segments
/// below the version prefix. Parameter segments (`{id}`) are recognised by
/// shape and never read.
fn group_of(method: &Method, segments: &[&str]) -> &'static str {
    if let Some(operation) = segments.last().and_then(|s| s.strip_prefix('$')) {
        return match operation {
            "lookup" => "lookup",
            "validate-code" => "validate_code",
            "expand" => "expand",
            "subsumes" => "subsumes",
            "translate" => "translate",
            "closure" => "closure",
            _ => "other",
        };
    }
    let parameter = |segment: &&str| segment.starts_with('{');
    let resource = |segment: &&str| segment.chars().next().is_some_and(char::is_uppercase);
    match segments {
        ["metadata"] => "metadata",
        [kind] if resource(kind) => match *method {
            Method::GET => "search",
            Method::POST => "write",
            _ => "other",
        },
        [kind, id] if resource(kind) && parameter(id) => match *method {
            Method::GET => "read",
            Method::PUT | Method::DELETE => "write",
            _ => "other",
        },
        [kind, rest @ ..]
            if resource(kind) && rest.contains(&"_history") && *method == Method::GET =>
        {
            "read"
        }
        _ => "other",
    }
}

/// A code system family, the only way a report names what is loaded.
///
/// The order is the report's: the list is sorted ascending by the serialized
/// name, which is the declaration order here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// The Dutch DHD thesauri.
    Dhd,
    /// FHIR `CodeSystem` resources loaded from `terminology.code_systems` that
    /// no other family names.
    Fhir,
    /// The Dutch G-Standaard product ladder.
    Gstandaard,
    /// WHO ICD-10 and its national editions.
    Icd10,
    /// ICD-10-CM.
    Icd10cm,
    /// ICD-11.
    Icd11,
    /// The Dutch Labcodeset.
    Labcodeset,
    /// LOINC.
    Loinc,
    /// Any loaded system the closed set does not name.
    Other,
    /// `RxNorm`.
    Rxnorm,
    /// SNOMED CT.
    Snomed,
}

impl Family {
    /// The family of a loaded system: by its URL when the closed set names
    /// it, else `fhir` for a `CodeSystem` resource and `other` for an
    /// artifact.
    ///
    /// The URL is read here and never leaves the process.
    #[must_use]
    pub fn of(url: &str, origin: Origin) -> Self {
        // NOTE: the URLs are the canonicals each source publishes under
        // (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
        match url {
            "http://snomed.info/sct" => Self::Snomed,
            "http://loinc.org" => Self::Loinc,
            "http://hl7.org/fhir/sid/icd-10" | "http://hl7.org/fhir/sid/icd-10-nl" => Self::Icd10,
            "http://hl7.org/fhir/sid/icd-10-cm" => Self::Icd10cm,
            "http://www.nlm.nih.gov/research/umls/rxnorm" => Self::Rxnorm,
            "urn:oid:2.16.840.1.113883.2.4.3.120.5.1" => Self::Dhd,
            "urn:oid:2.16.840.1.113883.2.4.4.1"
            | "urn:oid:2.16.840.1.113883.2.4.4.7"
            | "urn:oid:2.16.840.1.113883.2.4.4.8"
            | "urn:oid:2.16.840.1.113883.2.4.4.10" => Self::Gstandaard,
            "https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc" => Self::Labcodeset,
            _ if url.starts_with("http://id.who.int/icd/") => Self::Icd11,
            _ => match origin {
                Origin::CodeSystems => Self::Fhir,
                Origin::Index => Self::Other,
            },
        }
    }
}

/// FerroTERM's `status` object, carried by both events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Whether at least one code system the deployment loaded is serving.
    pub index_ok: bool,
    /// The loaded code system families, unique and sorted ascending.
    pub code_systems: Vec<Family>,
    /// Whether a resources database is configured.
    pub persistence: bool,
}

impl Status {
    /// The status with `families` deduplicated, sorted, and capped at
    /// [`MAX_FAMILIES`].
    #[must_use]
    pub fn new(
        index_ok: bool,
        families: impl IntoIterator<Item = Family>,
        persistence: bool,
    ) -> Self {
        let mut code_systems: Vec<Family> = families.into_iter().collect();
        code_systems.sort_unstable();
        code_systems.dedup();
        code_systems.truncate(MAX_FAMILIES);
        Self {
            index_ok,
            code_systems,
            persistence,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use ferrohealth_telemetry::request_log::RouteNamer;
    use ferrohealth_usage_report::envelope::{Key, report_git_sha};
    use ferrohealth_usage_report::reporter::{Off, prepare};
    use ferrohealth_usage_report::section::{
        Deployment, ENDPOINT, Endpoint, UsageReportSection, ci_of,
    };
    use http::{Method, StatusCode};

    use super::{Family, FhirBaseNamer, FhirVersion, MAX_FAMILIES, Status, count_version, window};
    use crate::config::{Config, ConfigError};
    use crate::state::Origin;

    fn group(method: &Method, route: &str) -> Option<String> {
        super::grouping(method, Some(route)).map(|key| key.as_str().to_owned())
    }

    fn key(name: &str) -> Key {
        Key::new(name).expect("a valid key")
    }

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn read(pairs: &[(&str, &str)]) -> Result<UsageReportSection, ConfigError> {
        let environment = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_sources(None, &environment).map(|config| config.usage_report)
    }

    #[test]
    fn templates_map_to_their_group() {
        let get = Method::GET;
        let post = Method::POST;
        let cases = [
            (&get, "/r4b/CodeSystem/$lookup", "lookup"),
            (&post, "/r4b/CodeSystem/$lookup", "lookup"),
            (&get, "/r5/CodeSystem/{id}/$lookup", "lookup"),
            (&get, "/r4b/CodeSystem/$validate-code", "validate_code"),
            (&post, "/r4b/ValueSet/{id}/$validate-code", "validate_code"),
            (&get, "/r4/ValueSet/$expand", "expand"),
            (&get, "/r4/ValueSet/{id}/$expand", "expand"),
            (&get, "/r4b/CodeSystem/$subsumes", "subsumes"),
            (&get, "/r4b/ConceptMap/$translate", "translate"),
            (&post, "/r4b/$closure", "closure"),
            (&get, "/r4b/metadata", "metadata"),
            (&get, "/r4b/CodeSystem/{id}", "read"),
            (&get, "/r4b/CodeSystem/{id}/_history", "read"),
            (&get, "/r4b/CodeSystem/{id}/_history/{version}", "read"),
            (&get, "/r4b/ValueSet/_history", "read"),
            (&get, "/r4b/ConceptMap", "search"),
            (&post, "/r4b/ConceptMap", "write"),
            (&Method::PUT, "/r4b/ValueSet/{id}", "write"),
            (&Method::DELETE, "/r4b/ValueSet/{id}", "write"),
            (&post, "/r4b/ValueSet/$batch-validate-code", "other"),
            (&get, "/r4b/$versions", "other"),
            (&post, "/r4b/$cache-control", "other"),
            (&get, "/r4b/.well-known/smart-configuration", "other"),
            (&post, "/r4b", "other"),
            (&post, "/r4b/", "other"),
        ];
        for (method, template, expected) in cases {
            assert_eq!(
                group(method, template).as_deref(),
                Some(expected),
                "{method} {template}"
            );
        }
    }

    #[test]
    fn the_version_prefix_decides_whether_a_request_counts() {
        assert_eq!(
            group(&Method::GET, "/r6/metadata").as_deref(),
            Some("metadata")
        );
        for outside in ["/health", "/metrics", "/ui/", "/", "/reload"] {
            assert_eq!(group(&Method::GET, outside), None, "{outside}");
            assert_eq!(
                FhirBaseNamer.route(&Method::GET, outside),
                None,
                "{outside}"
            );
        }
        let named = FhirBaseNamer
            .route(&Method::GET, "/r4b/Nope/123")
            .expect("an unmatched request under a base is named");
        assert_eq!(named, "/r4b/{*unmatched}");
        assert_eq!(
            group(&Method::GET, &named).as_deref(),
            Some("other"),
            "an unmatched request under a base is other, and its path is not read further"
        );
        assert_eq!(FhirBaseNamer.route(&Method::GET, "/r7/metadata"), None);
        assert_eq!(super::grouping(&Method::GET, None), None);
    }

    #[test]
    fn a_parameter_segment_never_decides_the_group() {
        assert_eq!(
            group(&Method::GET, "/r4b/{metadata}").as_deref(),
            Some("other")
        );
    }

    #[test]
    fn a_take_empties_the_window_and_a_restore_refills_it() {
        let window = window();
        for (route, status) in [
            ("/r4b/CodeSystem/$lookup", StatusCode::OK),
            ("/r4b/CodeSystem/$lookup", StatusCode::INTERNAL_SERVER_ERROR),
            ("/health", StatusCode::OK),
        ] {
            window.record(&Method::GET, Some(route), status, Duration::from_millis(3));
            count_version(&window, route);
        }
        let taken = window.take();
        let lookup = taken.groups.get(&key("lookup")).expect("lookup");
        assert_eq!(lookup.total(), 2);
        assert_eq!(lookup.errors_5xx, 1);
        assert_eq!(taken.groups.len(), 1, "the probe is not counted");
        assert_eq!(
            taken.counters,
            BTreeMap::from([(key("fhir_r4b"), 2)]),
            "only the versions that saw a request are counted"
        );
        assert!(window.take().counters.is_empty(), "a take leaves nothing");
        window.restore(&taken);
        let again = window.take();
        assert_eq!(again, taken);
        let metrics = again.to_metrics();
        let rendered = metrics
            .window_24h
            .groups
            .get(&key("lookup"))
            .expect("lookup");
        assert_eq!(rendered.errors_5xx, 1);
        assert!(!metrics.window_24h.groups.contains_key(&key("expand")));
    }

    #[test]
    fn every_version_has_a_valid_counter_and_route() {
        for version in [
            FhirVersion::R4,
            FhirVersion::R4b,
            FhirVersion::R5,
            FhirVersion::R6,
        ] {
            assert!(Key::new(version.counter()).is_some(), "{version:?}");
            assert!(
                group(&Method::GET, version.unmatched_route()).is_some(),
                "{version:?}"
            );
        }
    }

    #[test]
    fn a_git_sha_outside_the_contract_reports_zeros() {
        assert_eq!(report_git_sha("1a2b3c4d5e6f"), "1a2b3c4d5e6f");
        assert_eq!(report_git_sha(""), "0000000");
        assert_eq!(report_git_sha("1A2B3C4"), "0000000");
        assert_eq!(report_git_sha("abc"), "0000000");
    }

    #[test]
    fn a_loaded_system_maps_to_its_family_and_the_rest_to_fhir_or_other() {
        let index = |url| Family::of(url, Origin::Index);
        assert_eq!(index("http://snomed.info/sct"), Family::Snomed);
        assert_eq!(index("http://loinc.org"), Family::Loinc);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10"), Family::Icd10);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10-nl"), Family::Icd10);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10-cm"), Family::Icd10cm);
        assert_eq!(index("http://id.who.int/icd/release/11/mms"), Family::Icd11);
        assert_eq!(
            index("http://www.nlm.nih.gov/research/umls/rxnorm"),
            Family::Rxnorm
        );
        assert_eq!(
            index("urn:oid:2.16.840.1.113883.2.4.3.120.5.1"),
            Family::Dhd
        );
        assert_eq!(
            index("urn:oid:2.16.840.1.113883.2.4.4.10"),
            Family::Gstandaard
        );
        assert_eq!(index("http://www.whocc.no/atc"), Family::Other);
        assert_eq!(
            Family::of(
                "https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc",
                Origin::CodeSystems
            ),
            Family::Labcodeset
        );
        assert_eq!(
            Family::of("http://example.org/cs", Origin::CodeSystems),
            Family::Fhir
        );
        assert_eq!(
            Family::of("http://snomed.info/sct", Origin::CodeSystems),
            Family::Snomed,
            "a named family wins over the source"
        );
    }

    #[test]
    fn families_serialize_sorted_by_their_name_and_unique() {
        let status = Status::new(
            true,
            [Family::Snomed, Family::Loinc, Family::Snomed, Family::Fhir],
            false,
        );
        let json = serde_json::to_value(&status).expect("serializes");
        assert_eq!(
            json["code_systems"],
            serde_json::json!(["fhir", "loinc", "snomed"])
        );
        let all = [
            Family::Dhd,
            Family::Fhir,
            Family::Gstandaard,
            Family::Icd10,
            Family::Icd10cm,
            Family::Icd11,
            Family::Labcodeset,
            Family::Loinc,
            Family::Other,
            Family::Rxnorm,
            Family::Snomed,
        ];
        let names: Vec<String> = all
            .iter()
            .map(|f| {
                serde_json::to_value(f)
                    .expect("serializes")
                    .as_str()
                    .expect("a string")
                    .to_owned()
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "the declaration order is the name order");
        assert!(all.len() <= MAX_FAMILIES);
    }

    #[test]
    fn the_default_is_on_without_a_state_directory() {
        let section = read(&[]).expect("reads");
        assert_eq!(section, UsageReportSection::default());
        assert!(section.enabled);
        assert_eq!(section.endpoint.url().as_str(), ENDPOINT);
        assert_eq!(section.state_dir, None);
        assert_eq!(section.deployment, Deployment::Unknown);
    }

    #[test]
    fn every_key_is_read() {
        let section = read(&[
            ("FERROTERM__USAGE_REPORT__ENABLED", "false"),
            (
                "FERROTERM__USAGE_REPORT__ENDPOINT",
                "http://127.0.0.1:9/v1/report",
            ),
            ("FERROTERM__USAGE_REPORT__STATE_DIR", "/data/state"),
            ("FERROTERM__USAGE_REPORT__DEPLOYMENT", "compose"),
        ])
        .expect("reads");
        assert!(!section.enabled);
        assert_eq!(
            section.endpoint.url().as_str(),
            "http://127.0.0.1:9/v1/report"
        );
        assert_eq!(
            section.state_dir.as_deref(),
            Some(std::path::Path::new("/data/state"))
        );
        assert_eq!(section.deployment, Deployment::Compose);
    }

    #[test]
    fn a_bad_switch_endpoint_or_deployment_refuses_to_start() {
        assert!(matches!(
            read(&[("FERROTERM__USAGE_REPORT__ENABLED", "maybe")]),
            Err(ConfigError::Load(_))
        ));
        for endpoint in [
            "http://report.example/v1",
            "https://user:pw@report.example/v1",
            "ftp://report.example/",
            "not a url",
        ] {
            assert!(Endpoint::new(endpoint).is_err(), "{endpoint}");
            assert!(
                matches!(
                    read(&[("FERROTERM__USAGE_REPORT__ENDPOINT", endpoint)]),
                    Err(ConfigError::Load(_))
                ),
                "{endpoint}"
            );
        }
        assert!(matches!(
            read(&[("FERROTERM__USAGE_REPORT__DEPLOYMENT", "k8s")]),
            Err(ConfigError::Load(_))
        ));
    }

    #[test]
    fn ci_is_detected_from_either_variable_and_true_or_one_only() {
        assert_eq!(ci_of(lookup(&[])), None);
        assert_eq!(
            ci_of(lookup(&[("GITHUB_ACTIONS", "true")])),
            Some("GITHUB_ACTIONS")
        );
        assert_eq!(
            ci_of(lookup(&[("GITHUB_ACTIONS", "TRUE")])),
            Some("GITHUB_ACTIONS")
        );
        assert_eq!(ci_of(lookup(&[("CI", "1")])), Some("CI"));
        assert_eq!(ci_of(lookup(&[("CI", "True")])), Some("CI"));
        assert_eq!(
            ci_of(lookup(&[("CI", "true"), ("GITHUB_ACTIONS", "true")])),
            Some("GITHUB_ACTIONS"),
            "GITHUB_ACTIONS is read first"
        );
        for no in ["false", "0", "", "yes"] {
            assert_eq!(
                ci_of(lookup(&[("CI", no), ("GITHUB_ACTIONS", no)])),
                None,
                "{no}"
            );
        }
    }

    #[test]
    fn an_explicit_on_still_yields_to_ci() {
        let section = read(&[("FERROTERM__USAGE_REPORT__ENABLED", "true")]).expect("reads");
        assert!(section.enabled);
        let ci = ci_of(lookup(&[("GITHUB_ACTIONS", "true")]));
        assert_eq!(ci, Some("GITHUB_ACTIONS"));
        assert!(matches!(
            prepare(&section, ci),
            Err(Off::Ci("GITHUB_ACTIONS"))
        ));
    }

    #[test]
    fn the_switch_then_ci_then_the_state_directory_decide() {
        let dir = tempfile::tempdir().expect("tempdir");
        let with_dir = UsageReportSection {
            state_dir: Some(dir.path().join("state")),
            ..UsageReportSection::default()
        };
        let off = UsageReportSection {
            enabled: false,
            ..with_dir.clone()
        };
        assert!(matches!(prepare(&off, Some("CI")), Err(Off::Switch)));
        assert!(matches!(
            prepare(&with_dir, Some("GITHUB_ACTIONS")),
            Err(Off::Ci("GITHUB_ACTIONS"))
        ));
        assert!(
            !dir.path().join("state").exists(),
            "nothing is written while the report is off"
        );
        assert!(matches!(
            prepare(&UsageReportSection::default(), None),
            Err(Off::NoStateDir)
        ));
        let on = prepare(&with_dir, None).expect("runs");
        assert!(on.instance.created);
        assert!(!prepare(&with_dir, None).expect("runs").instance.created);
    }

    #[test]
    fn a_state_directory_that_is_a_file_turns_the_report_off() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("file");
        std::fs::write(&file, b"").expect("writes");
        let section = UsageReportSection {
            state_dir: Some(file),
            ..UsageReportSection::default()
        };
        assert!(matches!(prepare(&section, None), Err(Off::StateDir { .. })));
    }
}
