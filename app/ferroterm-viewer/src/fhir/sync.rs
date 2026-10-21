//! What a synchronisation run recorded about the deployment's own resources.
//!
//! No FHIR specification governs these records: they are the sync service's
//! own, and its admin listener answers `GET /runs` with the runs newest first
//! and `GET /runs/{id}` with one whole record. The listener authenticates
//! nobody and is never published, so the viewer reads it same-origin, which
//! means a deployment that wants these findings on screen puts the listener
//! behind the same address the server is served from. Nothing answers there
//! by default, and the screen then shows no findings at all.
//!
//! The record is read out of the JSON document rather than through a derived
//! decoder. The document is already decoded to a `serde_json::Value` by the
//! code that reads the resources beside it, and a derive over four more types
//! compiled another 6,898 bytes of decoder into the bundle for the five fields
//! a screen draws. Only those five are read: the record carries its counts as
//! pointer-sized integers, and WebAssembly is 32-bit.

use crate::locale::{Msg, tr};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One local code a new release changed, as the screen draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Finding {
    /// The locally authored resource that names the code, as the run named it.
    pub(crate) resource: String,
    /// The code system the code comes from.
    pub(crate) system: String,
    /// The code itself.
    pub(crate) code: String,
    /// What the release did to it.
    pub(crate) kind: String,
    /// The release the run activated for that system, when it activated one.
    pub(crate) release: Option<String>,
}

impl Finding {
    /// What the release did to the code, as a reader reads it.
    ///
    /// The kind is rendered whether or not it is one the viewer knows, so a
    /// run that records a kind this bundle predates still says something.
    pub(crate) fn what_happened(&self) -> String {
        match self.kind.as_str() {
            "inactive" => tr(Msg::ReleaseMadeCodeInactive),
            "absent" => tr(Msg::ReleaseNoLongerCarriesCode),
            "outside-value-set" => tr(Msg::CodeNoLongerFallsInsideValue),
            "" => tr(Msg::RunRecordedNoKindFinding),
            other => other.to_owned(),
        }
    }
}

/// The text `name` holds in `held`, empty where it holds none.
fn text(held: &Value, name: &str) -> String {
    held.get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The identifier of the newest run `listed` carries.
///
/// The listener answers the runs newest first, so the first entry is the run
/// whose findings a screen wants.
pub(crate) fn newest_run(listed: &Value) -> Option<String> {
    let id = text(listed.as_array()?.first()?, "id");
    (!id.is_empty()).then_some(id)
}

/// The findings one whole run record carries, in the order it wrote them.
pub(crate) fn findings_of(record: &Value) -> Vec<Finding> {
    let Some(listed) = record
        .get("revalidation")
        .and_then(|held| held.get("findings"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    listed
        .iter()
        .map(|held| Finding {
            resource: text(held, "resource"),
            system: text(held, "system"),
            code: text(held, "code"),
            kind: text(held, "kind"),
            release: held
                .get("release")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect()
}

/// The names one run gives the resource a screen has open.
///
/// A run names a resource by its type and its canonical, and by its type and
/// its logical id where it has no canonical, so the screen matches on both.
pub(crate) fn names_of(resource_type: &str, id: &str, canonical: &str) -> Vec<String> {
    let mut names = Vec::new();
    if !canonical.trim().is_empty() {
        names.push(format!("{resource_type} {canonical}"));
    }
    if !id.trim().is_empty() {
        names.push(format!("{resource_type}/{id}"));
    }
    names
}

/// The findings that concern the resource `names` names.
///
/// A run checks every locally authored resource, so all but a few of its
/// findings are about something else on screen somewhere else.
pub(crate) fn concerning(findings: &[Finding], names: &[String]) -> Vec<Finding> {
    findings
        .iter()
        .filter(|finding| names.contains(&finding.resource))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record shape the sync service writes, in the elements read here.
    fn record() -> Value {
        serde_json::from_str(
            r#"{"id":"20260924T101500Z","trigger":"schedule","outcome":"ok",
                "revalidation":{"resources":2,"checked":40,
                  "statement":"2 locally authored resources were revalidated and 2 local codes changed",
                  "findings":[
                    {"resource":"CodeSystem https://terminology.example/colours","system":"https://terminology.example/base",
                     "code":"red","kind":"inactive","release":"2026-09-01"},
                    {"resource":"ValueSet https://terminology.example/palette","system":"https://terminology.example/base",
                     "code":"blue","kind":"absent","release":null},
                    {"resource":"CodeSystem/colours","system":"https://terminology.example/base",
                     "code":"green","kind":"outside-value-set","release":null}]}}"#,
        )
        .expect("the run record the service writes parses")
    }

    #[test]
    fn a_record_reads_every_finding_it_carries() {
        let found = findings_of(&record());
        assert_eq!(found.len(), 3);
        let first = found.first().expect("the record carries three findings");
        assert_eq!(first.code, "red");
        assert_eq!(first.system, "https://terminology.example/base");
        assert_eq!(first.release.as_deref(), Some("2026-09-01"));
        assert_eq!(
            found.get(1).and_then(|finding| finding.release.as_deref()),
            None,
            "a run that activated no release for the system states none"
        );
    }

    #[test]
    fn a_record_with_no_revalidation_carries_no_findings() {
        let empty: Value = serde_json::from_str(r#"{"id":"x"}"#).expect("the fixture parses");
        assert_eq!(findings_of(&empty), Vec::<Finding>::new());
        assert_eq!(findings_of(&Value::Null), Vec::<Finding>::new());
    }

    #[test]
    fn only_the_findings_naming_the_open_resource_are_listed() {
        let found = findings_of(&record());
        let names = names_of(
            "CodeSystem",
            "colours",
            "https://terminology.example/colours",
        );
        assert_eq!(
            concerning(&found, &names)
                .iter()
                .map(|finding| finding.code.clone())
                .collect::<Vec<String>>(),
            ["red", "green"],
            "a run names a resource by its canonical or by its logical id, and both are this one"
        );
    }

    #[test]
    fn a_finding_about_another_resource_is_left_out() {
        let found = findings_of(&record());
        let names = names_of("ValueSet", "palette", "https://terminology.example/other");
        assert!(
            concerning(&found, &names).is_empty(),
            "the canonical does not match and the id does not either"
        );
    }

    #[test]
    fn a_resource_with_no_canonical_is_named_by_its_id_alone() {
        assert_eq!(
            names_of("ConceptMap", "m1", "   "),
            vec![String::from("ConceptMap/m1")]
        );
        assert_eq!(names_of("ConceptMap", "", ""), Vec::<String>::new());
    }

    #[test]
    fn every_kind_the_service_records_reads_as_a_sentence() {
        for (kind, expected) in [
            ("inactive", "the release made this code inactive"),
            ("absent", "the release no longer carries this code"),
            (
                "outside-value-set",
                "the code no longer falls inside the value set it was included through",
            ),
            ("", "the run recorded no kind for this finding"),
            ("something-new", "something-new"),
        ] {
            let finding = Finding {
                kind: kind.to_owned(),
                ..Finding::default()
            };
            assert_eq!(finding.what_happened(), expected, "{kind}");
        }
    }

    #[test]
    fn the_newest_run_is_the_one_the_screen_reads() {
        let listed: Value = serde_json::from_str(
            r#"[{"id":"20260924T101500Z","trigger":"schedule","outcome":"ok","entries_taken":1},
                {"id":"20260923T101500Z","trigger":"manual","outcome":"failed","entries_taken":0}]"#,
        )
        .expect("the run list the service writes parses");
        assert_eq!(newest_run(&listed).as_deref(), Some("20260924T101500Z"));
        assert_eq!(
            newest_run(&serde_json::json!([])),
            None,
            "a service that has recorded no run has nothing to read"
        );
        assert_eq!(newest_run(&Value::Null), None);
        assert_eq!(
            newest_run(&serde_json::json!([{ "trigger": "schedule" }])),
            None,
            "a run with no identifier addresses no record"
        );
    }
}
