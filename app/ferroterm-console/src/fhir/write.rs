// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writing a resource: what a write sends, what it answers, and how a refusal
//! reads.
//!
//! The server carries create, update, delete, and `_history` on `CodeSystem`,
//! `ValueSet`, and `ConceptMap` (<https://hl7.org/fhir/R4B/http.html>). An
//! update states the version it is replacing in `If-Match`, so two readers
//! editing the same resource cannot silently overwrite each other
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>).

use serde_json::Value;

use crate::fhir::release::{Reads, document};
use crate::locale::{Msg, tr_with};

/// What the console reads of one stored resource, from the one its release
/// decoded: its type and logical id, its `meta`, and the names a screen
/// draws it by.
///
/// The console writes `CodeSystem`, `ValueSet` and `ConceptMap`, so those
/// are the resources a stamp is read off; any other resource stamps as none.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Stamp {
    /// The resource type.
    pub(crate) resource_type: Option<String>,
    /// The logical id.
    pub(crate) id: Option<String>,
    /// `meta.versionId`, the version an update states in `If-Match`
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) version_id: Option<String>,
    /// `meta.lastUpdated`.
    pub(crate) last_updated: Option<String>,
    /// `meta.source`.
    pub(crate) source: Option<String>,
    /// The canonical URL.
    pub(crate) url: Option<String>,
    /// The business version.
    pub(crate) version: Option<String>,
    /// The computer-friendly name.
    pub(crate) name: Option<String>,
    /// The name written for a person.
    pub(crate) title: Option<String>,
    /// The publication status.
    pub(crate) status: Option<String>,
}

/// One stored resource as a release decoded it: its stamp, and the document
/// it is, which the history screen compares and a restore sends back.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Stored {
    /// What the console reads of it.
    pub(crate) stamp: Stamp,
    /// The resource, as the JSON document it is.
    pub(crate) document: Value,
}

/// The `history` `Bundle` the history interaction answers, read from the one
/// its release decoded (<https://hl7.org/fhir/R4B/http.html#history>).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct History {
    /// Each entry, in the server's order.
    pub(crate) entry: Vec<HistoryEntry>,
}

/// One entry of a history `Bundle`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HistoryEntry {
    /// The resource as it stood at that version, absent for a delete.
    pub(crate) resource: Option<Stored>,
    /// `request.method`, the interaction that made the version.
    pub(crate) method: Option<String>,
    /// `request.url`, what the interaction addressed.
    pub(crate) url: Option<String>,
    /// `response.etag`, the version this entry is.
    pub(crate) etag: Option<String>,
    /// `response.lastModified`, when the server wrote it.
    pub(crate) last_modified: Option<String>,
}

impl Stored {
    /// `resource`, a resource its release decoded, with its stamp.
    pub(crate) fn of<T: Reads<Stamp> + serde::Serialize>(resource: &T) -> Self {
        Self {
            stamp: resource.read(),
            document: document(resource),
        }
    }
}

/// The stamp of `$resource`, a resource of the type `$resource_type` of the
/// `fhir-types` module `$module`, from its id, its `meta` and the elements it
/// names itself by.
macro_rules! stamp {
    ($module:ident, $resource:expr, $resource_type:literal) => {{
        let text = |value: Option<&fhir_types::$module::primitives::String>| {
            value.and_then(|value| value.value.clone())
        };
        let meta: Option<&fhir_types::$module::meta::Meta> = $resource.meta.as_ref();
        Stamp {
            resource_type: Some($resource_type.to_owned()),
            id: $resource.id.clone(),
            version_id: meta
                .and_then(|meta| meta.version_id.as_ref())
                .and_then(|id| id.value.clone()),
            last_updated: meta
                .and_then(|meta| meta.last_updated.as_ref())
                .and_then(|at| at.value.clone()),
            source: meta
                .and_then(|meta| meta.source.as_ref())
                .and_then(|source| source.value.clone()),
            url: $resource.url.as_ref().and_then(|url| url.value.clone()),
            version: text($resource.version.as_ref()),
            name: text($resource.name.as_ref()),
            title: text($resource.title.as_ref()),
            status: $resource.status.value.clone(),
        }
    }};
}

/// Reads the stamp of the three resources the console writes, and the
/// history `Bundle`, of the `fhir-types` module `$module`.
macro_rules! reads_stored {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::bundle::Bundle;
            use fhir_types::$module::resource::Resource;

            use super::{History, HistoryEntry, Reads, Stamp, Stored};

            impl Reads<Stamp> for Resource {
                fn read(&self) -> Stamp {
                    match self {
                        Resource::CodeSystem(resource) => stamp!($module, resource, "CodeSystem"),
                        Resource::ValueSet(resource) => stamp!($module, resource, "ValueSet"),
                        Resource::ConceptMap(resource) => stamp!($module, resource, "ConceptMap"),
                        _ => Stamp::default(),
                    }
                }
            }

            impl Reads<History> for Bundle {
                fn read(&self) -> History {
                    History {
                        entry: self
                            .entry
                            .iter()
                            .map(|entry| HistoryEntry {
                                resource: entry.resource.as_ref().map(Stored::of),
                                method: entry
                                    .request
                                    .as_ref()
                                    .and_then(|request| request.method.value.clone()),
                                url: entry
                                    .request
                                    .as_ref()
                                    .and_then(|request| request.url.value.clone()),
                                etag: entry
                                    .response
                                    .as_ref()
                                    .and_then(|response| response.etag.as_ref())
                                    .and_then(|etag| etag.value.clone()),
                                last_modified: entry
                                    .response
                                    .as_ref()
                                    .and_then(|response| response.last_modified.as_ref())
                                    .and_then(|at| at.value.clone()),
                            })
                            .collect(),
                    }
                }
            }
        }
    };
}

reads_stored!(r4);
reads_stored!(r4b);
reads_stored!(r5);
reads_stored!(r6);

/// One version of a resource, as a version list draws it.
///
/// It is the same shape whichever interaction produced it, so a root that
/// answers the history `Bundle` and one that answers only the version read
/// draw the same table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Version {
    /// `meta.versionId`, which is also the row's key.
    pub(crate) id: String,
    /// `meta.lastUpdated`, or the entry's `response.lastModified`.
    pub(crate) last_updated: Option<String>,
    /// The interaction that made the version, where the answer states one.
    pub(crate) method: Option<String>,
    /// `meta.source`, the source system the content came from, where the
    /// server states one (<https://hl7.org/fhir/R4B/resource.html#meta>).
    ///
    /// It identifies a system rather than a person: FHIR puts who changed a
    /// resource in `Provenance` and `AuditEvent`, and says a server "SHOULD
    /// generally leave this unchanged" on a write. A screen that drew it as
    /// an author would be claiming more than the element carries.
    pub(crate) source: Option<String>,
    /// The resource as it stood at that version, as the server sent it.
    pub(crate) resource: Option<Value>,
}

impl Version {
    /// The version `resource` is, read out of the resource the server sent.
    ///
    /// `position` names the row when the server stated no `meta.versionId`, so
    /// every row has an identifier for the pickers and the restore control to
    /// carry.
    pub(crate) fn of(resource: Option<Stored>, position: usize) -> Self {
        let stamp = resource
            .as_ref()
            .map(|stored| stored.stamp.clone())
            .unwrap_or_default();
        Self {
            id: stamp
                .version_id
                .unwrap_or_else(|| format!("entry-{position}")),
            last_updated: stamp.last_updated,
            method: None,
            source: stamp.source,
            resource: resource.map(|stored| stored.document),
        }
    }
}

impl History {
    /// The versions this `Bundle` carries, in the order the server sent them.
    ///
    /// The server sends them most recent first
    /// (<https://hl7.org/fhir/R4B/http.html#history>), and each entry's
    /// `request.method` names the interaction that made the version. An entry
    /// with no resource, a delete, is keyed by the version its
    /// `response.etag` names, where it names one.
    pub(crate) fn versions(&self) -> Vec<Version> {
        self.entry
            .iter()
            .enumerate()
            .map(|(position, entry)| {
                let mut version = Version::of(entry.resource.clone(), position);
                let stated = entry
                    .resource
                    .as_ref()
                    .and_then(|stored| stored.stamp.version_id.as_ref());
                if stated.is_none()
                    && let Some(etag) = entry
                        .etag
                        .as_deref()
                        .map(strong_tag_of)
                        .filter(|tag| !tag.is_empty())
                {
                    etag.clone_into(&mut version.id);
                }
                version.method.clone_from(&entry.method);
                if version.last_updated.is_none() {
                    version.last_updated.clone_from(&entry.last_modified);
                }
                version
            })
            .collect()
    }
}

/// What a write answered, beyond the resource itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Written {
    /// What the console reads of the resource the server echoed, when it
    /// echoed one.
    pub(crate) resource: Option<Stamp>,
    /// The `ETag` the server answered, which is the version to state next
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) etag: Option<String>,
    /// The `Location` a create answers, naming where the resource now lives.
    pub(crate) location: Option<String>,
}

impl Written {
    /// The version the next update of this resource states in `If-Match`.
    ///
    /// The `ETag` is preferred over `meta.versionId`, because it is what the
    /// server just committed to and what `If-Match` is compared against.
    pub(crate) fn version_id(&self) -> Option<&str> {
        self.etag
            .as_deref()
            .map(strong_tag_of)
            .or_else(|| {
                self.resource
                    .as_ref()
                    .and_then(|stamp| stamp.version_id.as_deref())
            })
    }
}

/// The version inside a weak or strong entity tag.
///
/// FHIR states the version as a weak `ETag` (`W/"2"`), so the quotes and the
/// weakness marker are stripped back to the version
/// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
fn strong_tag_of(etag: &str) -> &str {
    etag.trim().trim_start_matches("W/").trim_matches('"')
}

/// The `If-Match` header value for a resource at `version_id`.
pub(crate) fn if_match(version_id: &str) -> String {
    // NOTE: RFC 9110 §8.8.3: the header is wire syntax, so it never follows the page's language.
    tr_with(
        &ferrohealth_ui::i18n::Locale::default(),
        Msg::W,
        &[("version_id", (version_id).to_string().into())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::version::FhirVersion;

    /// `json`, decoded at R4B as what the console reads of it.
    fn decoded<A: crate::fhir::release::Decodes>(json: &str) -> A {
        FhirVersion::R4B
            .decode(json)
            .expect("the server's own answer decodes at R4B")
    }

    #[test]
    fn the_if_match_header_is_the_weak_tag_fhir_states() {
        assert_eq!(if_match("3"), "W/\"3\"");
    }

    #[test]
    fn the_etag_the_server_answered_is_the_version_the_next_update_states() {
        let written = Written {
            resource: Some(Stamp {
                version_id: Some(String::from("1")),
                ..Stamp::default()
            }),
            etag: Some(String::from("W/\"2\"")),
            location: None,
        };
        assert_eq!(
            written.version_id(),
            Some("2"),
            "the ETag is what If-Match is compared against"
        );
    }

    #[test]
    fn a_write_that_answered_no_etag_falls_back_to_the_resource() {
        let written = Written {
            resource: Some(Stamp {
                version_id: Some(String::from("7")),
                ..Stamp::default()
            }),
            etag: None,
            location: None,
        };
        assert_eq!(written.version_id(), Some("7"));
        assert_eq!(Written::default().version_id(), None);
    }

    #[test]
    fn a_strong_tag_reads_as_its_version_too() {
        assert_eq!(strong_tag_of("W/\"12\""), "12");
        assert_eq!(strong_tag_of("\"12\""), "12");
        assert_eq!(strong_tag_of(" W/\"12\" "), "12");
    }

    #[test]
    fn a_history_row_is_keyed_by_its_version_and_never_by_its_position() {
        let history: History = decoded(
            r#"{"resourceType":"Bundle","type":"history","total":2,"entry":[
                 {"resource":{"resourceType":"CodeSystem","id":"colours","status":"active","content":"complete",
                   "meta":{"versionId":"2","lastUpdated":"2026-09-24T10:00:00Z","source":"https://sync.example/run/9"}},
                  "request":{"method":"PUT","url":"CodeSystem/colours"},
                  "response":{"status":"200","etag":"W/\"2\""}},
                 {"request":{"method":"DELETE","url":"CodeSystem/colours"},
                  "response":{"status":"204","lastModified":"2026-09-23T10:00:00Z"}}]}"#,
        );
        let versions = history.versions();
        let first = versions.first().expect("the bundle carries two entries");
        assert_eq!(first.id, "2");
        assert_eq!(first.method.as_deref(), Some("PUT"));
        assert_eq!(first.source.as_deref(), Some("https://sync.example/run/9"));
        assert_eq!(first.last_updated.as_deref(), Some("2026-09-24T10:00:00Z"));
        let second = versions.get(1).expect("the bundle carries two entries");
        assert_eq!(
            second.id, "entry-1",
            "a deleted version carries no resource, so the position is the only key left"
        );
        assert_eq!(second.method.as_deref(), Some("DELETE"));
        assert_eq!(
            second.last_updated.as_deref(),
            Some("2026-09-23T10:00:00Z"),
            "an entry with no resource still says when the server wrote it"
        );
        assert!(second.resource.is_none());
    }

    #[test]
    fn a_delete_in_a_history_is_keyed_by_the_version_its_etag_names() {
        let history: History = decoded(
            r#"{"resourceType":"Bundle","type":"history","total":2,"entry":[
                 {"fullUrl":"https://tx.example.org/r4b/CodeSystem/colours",
                  "request":{"method":"DELETE","url":"CodeSystem/colours"},
                  "response":{"status":"204 No Content","etag":"W/\"3\"","lastModified":"2026-09-24T11:00:00Z"}},
                 {"fullUrl":"https://tx.example.org/r4b/CodeSystem/colours",
                  "resource":{"resourceType":"CodeSystem","id":"colours","status":"active","content":"complete",
                   "meta":{"versionId":"2","lastUpdated":"2026-09-24T10:00:00Z"}},
                  "request":{"method":"PUT","url":"CodeSystem/colours"},
                  "response":{"status":"200 OK","etag":"W/\"2\"","lastModified":"2026-09-24T10:00:00Z"}}]}"#,
        );
        let ids: Vec<String> = history
            .versions()
            .into_iter()
            .map(|version| version.id)
            .collect();
        assert_eq!(ids, ["3", "2"]);
    }

    #[test]
    fn a_version_read_answers_the_same_row_a_history_entry_does() {
        let resource: Stored = decoded(
            r#"{"resourceType":"CodeSystem","id":"colours","status":"active","content":"complete",
                "meta":{"versionId":"3","lastUpdated":"2026-09-24T11:00:00Z"}}"#,
        );
        let version = Version::of(Some(resource), 0);
        assert_eq!(version.id, "3");
        assert_eq!(
            version.last_updated.as_deref(),
            Some("2026-09-24T11:00:00Z")
        );
        assert_eq!(
            version.method, None,
            "a version read states no interaction, and the screen says so"
        );
        assert_eq!(Version::of(None, 4).id, "entry-4");
    }

    #[test]
    fn a_resource_the_server_echoed_states_the_version_to_send_back() {
        let resource: Stamp = decoded(
            r#"{"resourceType":"CodeSystem","id":"colours","status":"active","content":"complete","meta":{"versionId":"4","lastUpdated":"2026-09-23T10:00:00Z"},"url":"https://terminology.example/colours"}"#,
        );
        assert_eq!(resource.version_id.as_deref(), Some("4"));
        assert_eq!(resource.resource_type.as_deref(), Some("CodeSystem"));
        assert_eq!(resource.id.as_deref(), Some("colours"));
    }
}
