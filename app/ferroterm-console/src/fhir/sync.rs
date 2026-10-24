// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synchronisation service's runs, as its admin listener answers them,
//! and the two admin listeners' triggers.
//!
//! No FHIR specification governs these: they are FerroTERM's own. The sync
//! service's listener answers `GET /runs` with the run summaries newest
//! first, `GET /runs/{id}` with one whole record, and `POST /run` and
//! `POST /activate` with the summary of the run they performed, once it has
//! finished. The server's admin listener answers `POST /reload` with what
//! the reload did. The console's server calls both with the signed-in
//! reader's access token, at `[ferroterm] sync_url` and `admin_url`; each
//! listener admits a token carrying its product scope.
//!
//! Only the elements a screen draws are read. Every element of the record is
//! optional here, so a record a newer service writes still draws, and the
//! whole record is kept as the service sent it for the download.

use http::StatusCode;
use serde::Deserialize;

use crate::fhir::FhirClient;
use crate::fhir::error::FhirError;
use crate::fhir::exchange::{self, Exchange, Method};
use crate::locale::{Msg, tr_with};
use crate::url::RequestUrl;

/// The media type every admin listener answers in.
const JSON: &str = "application/json";

/// How one run ended, as the service records it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunOutcome {
    /// Everything the run attempted worked.
    Ok,
    /// Something failed; what the server serves is unchanged.
    Failed,
    /// An outcome this console predates.
    #[default]
    #[serde(other)]
    Other,
}

/// One run, as the run list and the triggers carry it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct RunSummary {
    /// The run's identifier, which addresses its record.
    pub(crate) id: String,
    /// What started the run: `schedule`, `manual`, `run-once` or `activate`.
    pub(crate) trigger: String,
    /// How the run ended.
    pub(crate) outcome: RunOutcome,
    /// When the run started, an RFC 3339 timestamp.
    pub(crate) started: String,
    /// When the run ended, an RFC 3339 timestamp.
    pub(crate) finished: String,
    /// How long the run took.
    pub(crate) duration_ms: u64,
    /// How many entries the run took.
    pub(crate) entries_taken: u64,
    /// How many entries it left behind for a reason other than the
    /// subscription.
    pub(crate) entries_skipped: u64,
    /// How many entries it left behind because the subscription does not name
    /// them.
    pub(crate) entries_not_subscribed: u64,
    /// How many staged items reached the server.
    pub(crate) activated: u64,
    /// How many local codes the activated release changed.
    pub(crate) revalidation_findings: u64,
    /// What revalidating the deployment's own resources found, in a sentence.
    pub(crate) revalidation: String,
    /// Everything that went wrong.
    pub(crate) errors: Vec<String>,
}

/// One entry a source offered and the run took.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct Taken {
    /// The entry's title in the feed.
    pub(crate) title: String,
    /// The version the entry carries.
    pub(crate) version: String,
    /// How many bytes it downloaded.
    pub(crate) bytes: u64,
    /// How taking it ended.
    pub(crate) outcome: RunOutcome,
    /// Why taking it failed, when it did.
    pub(crate) error: Option<String>,
}

/// What one source contributed to a run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct SourceRun {
    /// The source's name in the service's configuration.
    pub(crate) source: String,
    /// The feed the source read.
    pub(crate) feed_url: String,
    /// How many entries the feed offered.
    pub(crate) entries_seen: u64,
    /// The FHIR API the source listed beside its feed, when it has one.
    pub(crate) fhir_api_url: Option<String>,
    /// How many resources that API listed.
    pub(crate) api_entries_seen: u64,
    /// The entries the run took.
    pub(crate) taken: Vec<Taken>,
    /// The entries it left behind for a reason other than the subscription,
    /// one element each.
    pub(crate) skipped: Vec<serde_json::Value>,
    /// The entries it left behind because the subscription does not name
    /// them, counted per origin.
    pub(crate) not_subscribed: Vec<NotSubscribed>,
    /// What went wrong with this source.
    pub(crate) errors: Vec<String>,
}

/// How many entries of one origin the subscription does not name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct NotSubscribed {
    /// How many entries.
    pub(crate) entries: u64,
}

/// One answer the server gave a reload the activation asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct ReloadReply {
    /// The status the server answered.
    pub(crate) status: u16,
}

/// How the activation of a run ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct Activation {
    /// Whether the service activates by itself or waits for an activate.
    pub(crate) mode: String,
    /// What reached the server, one element each.
    pub(crate) activated: Vec<serde_json::Value>,
    /// How many items stay staged.
    pub(crate) staged: u64,
    /// The server's answers to the reloads it was asked for.
    pub(crate) reloads: Vec<ReloadReply>,
    /// Whether the activation was rolled back.
    pub(crate) rolled_back: bool,
    /// Why the activation failed, when it did.
    pub(crate) error: Option<String>,
}

/// One local code a release changed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct Finding {
    /// The locally written resource that names the code: its type and its
    /// canonical, or its type and logical id (`ValueSet/x`).
    pub(crate) resource: String,
    /// The code system the code comes from.
    pub(crate) system: String,
    /// The code.
    pub(crate) code: String,
    /// What the release did to it: `inactive`, `absent` or
    /// `outside-value-set`.
    pub(crate) kind: String,
    /// The release the run activated for that system, when it activated one.
    pub(crate) release: Option<String>,
}

impl Finding {
    /// The resource type and the canonical or logical id the finding names,
    /// split.
    #[must_use]
    pub(crate) fn subject(&self) -> (&str, Subject<'_>) {
        if let Some((resource_type, canonical)) = self.resource.split_once(' ') {
            (resource_type, Subject::Canonical(canonical))
        } else if let Some((resource_type, id)) = self.resource.split_once('/') {
            (resource_type, Subject::Id(id))
        } else {
            (self.resource.as_str(), Subject::Id(""))
        }
    }
}

/// How a finding names its resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Subject<'a> {
    /// By its canonical URL.
    Canonical(&'a str),
    /// By its logical id.
    Id(&'a str),
}

/// What revalidating the deployment's own resources found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct Revalidation {
    /// How many locally written resources it read.
    pub(crate) resources: u64,
    /// How many codes it checked.
    pub(crate) checked: u64,
    /// What it found, in the order it wrote them.
    pub(crate) findings: Vec<Finding>,
    /// What it found, in a sentence.
    pub(crate) statement: String,
    /// Why it did not run, when it did not.
    pub(crate) skipped: Option<String>,
}

/// One whole run record, in the elements a screen draws.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct RunRecord {
    /// The run's identifier.
    pub(crate) id: String,
    /// What started the run.
    pub(crate) trigger: String,
    /// When the run started.
    pub(crate) started: String,
    /// When the run ended.
    pub(crate) finished: String,
    /// How long the run took.
    pub(crate) duration_ms: u64,
    /// How the run ended.
    pub(crate) outcome: RunOutcome,
    /// What each source contributed.
    pub(crate) sources: Vec<SourceRun>,
    /// How the activation ended.
    pub(crate) activation: Activation,
    /// What retention removed, one element each.
    pub(crate) retention: Vec<serde_json::Value>,
    /// What revalidation found.
    pub(crate) revalidation: Revalidation,
    /// How many bytes the run staged.
    pub(crate) bytes_staged: u64,
    /// How many entries the run took.
    pub(crate) entries_taken: u64,
    /// Everything that went wrong.
    pub(crate) errors: Vec<String>,
    /// The record as the service sent it, for the download.
    #[serde(skip)]
    pub(crate) text: String,
}

/// What the server's admin listener answered a reload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reloaded {
    /// The status it answered.
    pub(crate) status: StatusCode,
    /// What it said, as it said it.
    pub(crate) body: String,
}

impl FhirClient {
    /// The address one admin route is disclosed at: the path below the
    /// console's forwarding, which decides the listener it reaches.
    fn admin_url(&self, segments: &[&str]) -> String {
        segments
            .iter()
            .fold(RequestUrl::new(), |url, segment| url.segment(segment))
            .render(self.root.as_str())
    }

    /// Sends one admin request of `method` to `url` and decodes a JSON answer.
    async fn admin_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
    ) -> Result<(T, String), FhirError> {
        let answer = self.admin(method, url).await?;
        let status = answered(answer.0, url)?;
        if !status.is_success() {
            return Err(FhirError::Status {
                url: url.to_owned(),
                status,
                body: answer.1,
            });
        }
        let value = serde_json::from_str(&answer.1).map_err(|error| FhirError::Decode {
            url: url.to_owned(),
            message: error.to_string(),
        })?;
        Ok((value, answer.1))
    }

    /// Sends one admin request with no body and answers its status and body.
    async fn admin(&self, method: Method, url: &str) -> Result<(u16, String), FhirError> {
        let path = url
            .strip_prefix(self.root.as_str())
            .filter(|path| path.starts_with('/'))
            .unwrap_or(url)
            .to_owned();
        let answer = exchange::send(Exchange::new(method, path, JSON))
            .await
            .map_err(|error| FhirError::Transport {
                url: url.to_owned(),
                message: error.to_string(),
            })?;
        Ok((answer.status, answer.body))
    }

    /// The runs the sync service performed, newest first.
    ///
    /// # Errors
    ///
    /// Returns the [`FhirError`] of a listener that could not be reached,
    /// refused, or answered something that is no run list.
    pub(crate) async fn runs(&self) -> Result<Vec<RunSummary>, FhirError> {
        let url = self.admin_url(&[crate::upstream::RUNS]);
        Ok(self.admin_json(Method::Get, &url).await?.0)
    }

    /// The whole record of the run `id`.
    ///
    /// # Errors
    ///
    /// Returns the [`FhirError`] of a listener that could not be reached,
    /// refused (`404` for a run it holds no record of), or answered something
    /// that is no record.
    pub(crate) async fn run_record(&self, id: &str) -> Result<RunRecord, FhirError> {
        let url = self.admin_url(&[crate::upstream::RUNS, id]);
        let (mut record, text): (RunRecord, String) = self.admin_json(Method::Get, &url).await?;
        record.text = text;
        Ok(record)
    }

    /// Runs the synchronisation now, answered once the run has finished.
    ///
    /// # Errors
    ///
    /// Returns the [`FhirError`] of a listener that could not be reached or
    /// refused.
    pub(crate) async fn run_now(&self) -> Result<RunSummary, FhirError> {
        let url = self.admin_url(&[crate::upstream::RUN]);
        Ok(self.admin_json(Method::Post, &url).await?.0)
    }

    /// Activates what the sync service holds staged.
    ///
    /// # Errors
    ///
    /// Returns the [`FhirError`] of a listener that could not be reached or
    /// refused.
    pub(crate) async fn activate(&self) -> Result<RunSummary, FhirError> {
        let url = self.admin_url(&[crate::upstream::ACTIVATE]);
        Ok(self.admin_json(Method::Post, &url).await?.0)
    }

    /// Asks the server's admin listener to reload its served set, and
    /// answers what it said, whatever the status.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Transport`] when no answer came.
    pub(crate) async fn reload(&self) -> Result<Reloaded, FhirError> {
        let url = self.admin_url(&[crate::upstream::RELOAD]);
        let (status, body) = self.admin(Method::Post, &url).await?;
        let status = answered(status, &url)?;
        Ok(Reloaded { status, body })
    }
}

/// The status `status` as a [`StatusCode`]: a number outside 100 to 999 is
/// no HTTP status (<https://www.rfc-editor.org/rfc/rfc9110#section-15>).
fn answered(status: u16, url: &str) -> Result<StatusCode, FhirError> {
    StatusCode::from_u16(status).map_err(|_invalid| FhirError::Transport {
        url: url.to_owned(),
        message: tr_with(
            Msg::ServerAnsweredStatus,
            &[("status", status.to_string().into())],
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record in the shape the sync service writes.
    const RECORD: &str = r#"{"id":"20261021T030000Z","trigger":"schedule",
        "started":"2026-10-21T03:00:00Z","finished":"2026-10-21T03:04:12Z","duration_ms":252000,
        "outcome":"ok","sources":[{"source":"nts","feed_url":"https://feed.example/atom",
        "entries_seen":412,"fhir_api_url":null,"api_entries_seen":0,"not_visible":[],
        "taken":[{"entry_id":"e1","title":"Synthetic findings 2026-10","origin":"feed",
        "canonical":"https://terminology.example/findings","version":"2026-10","category":"x",
        "lane":"rf2","bytes":3100000,"download_ms":10,"build_ms":20,"staged":null,"fixups":[],
        "outcome":"ok","error":null}],"skipped":[],"not_subscribed":[],"errors":[]}],
        "activation":{"mode":"auto","activated":[{"lane":"rf2","target":"/x","canonical":"c",
        "version":"v"}],"staged":0,"reloads":[{"status":200,"body":"{}"}],"rolled_back":false,
        "error":null},"retention":[],"revalidation":{"resources":12,"checked_resources":[],
        "checked":214,"findings":[{"resource":"ValueSet https://terminology.example/referral",
        "system":"https://terminology.example/findings","code":"NF-204","kind":"inactive",
        "release":"2026-10"},{"resource":"CodeSystem/colours","system":"s","code":"red",
        "kind":"absent","release":null}],"statement":"12 resources read, 1 finding",
        "skipped":null},"bytes_used":1,"bytes_staged":3100000,"entries_taken":1,"errors":[],
        "a_newer_element":true}"#;

    #[test]
    fn a_record_reads_what_a_screen_draws_and_keeps_going_past_a_newer_element() {
        let record: RunRecord = serde_json::from_str(RECORD).expect("the record parses");
        assert_eq!(record.outcome, RunOutcome::Ok);
        assert_eq!(record.duration_ms, 252_000);
        assert_eq!(record.revalidation.findings.len(), 2);
        assert_eq!(
            record.activation.reloads.first().map(|reply| reply.status),
            Some(200)
        );
        let source = record.sources.first().expect("one source");
        assert_eq!(source.entries_seen, 412);
        assert_eq!(
            source.taken.first().map(|taken| taken.bytes),
            Some(3_100_000)
        );
    }

    #[test]
    fn a_finding_names_its_resource_by_canonical_or_by_id() {
        let record: RunRecord = serde_json::from_str(RECORD).expect("the record parses");
        let mut findings = record.revalidation.findings.iter();
        assert_eq!(
            findings.next().map(Finding::subject),
            Some((
                "ValueSet",
                Subject::Canonical("https://terminology.example/referral")
            ))
        );
        assert_eq!(
            findings.next().map(Finding::subject),
            Some(("CodeSystem", Subject::Id("colours")))
        );
    }

    #[test]
    fn an_outcome_this_console_predates_is_kept_as_other() {
        let summary: RunSummary =
            serde_json::from_str(r#"{"id":"a","outcome":"partial"}"#).expect("parses");
        assert_eq!(summary.outcome, RunOutcome::Other);
        let summaries: Vec<RunSummary> = serde_json::from_str(
            r#"[{"id":"b","trigger":"manual","outcome":"failed","errors":["the feed refused"]}]"#,
        )
        .expect("a run list parses");
        assert_eq!(
            summaries.first().map(|summary| summary.outcome),
            Some(RunOutcome::Failed)
        );
    }
}
