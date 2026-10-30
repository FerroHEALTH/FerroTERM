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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::locale::{Msg, tr_with};

/// The resource elements the viewer reads back off a write or a history read.
///
/// The viewer never mirrors a whole FHIR resource; these are the elements a
/// screen shows and the one an update needs.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredResource {
    /// `resourceType`, mandatory in FHIR JSON
    /// (<https://hl7.org/fhir/R4B/json.html>).
    #[serde(default)]
    #[serde(rename = "resourceType")]
    pub(crate) resource_type: Option<String>,
    /// `id`, the logical id the server assigned.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// `meta`, which carries the version an update has to state.
    #[serde(default)]
    pub(crate) meta: Option<Meta>,
    /// `url`, the canonical the resource was published under.
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// `name`, the computer-friendly name.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// `title`, the name written for a person.
    #[serde(default)]
    pub(crate) title: Option<String>,
}

/// The `Resource.meta` elements the viewer reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Meta {
    /// `meta.versionId`, the version an `If-Match` states
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    #[serde(default)]
    #[serde(rename = "versionId")]
    pub(crate) version_id: Option<String>,
    /// `meta.lastUpdated`, when the server last changed the resource.
    #[serde(default)]
    #[serde(rename = "lastUpdated")]
    pub(crate) last_updated: Option<String>,
    /// `meta.source`, where a server records where the content came from
    /// (<https://hl7.org/fhir/R4B/resource.html#meta>). Most do not.
    #[serde(default)]
    pub(crate) source: Option<String>,
}

impl StoredResource {
    /// The version an update of this resource states in `If-Match`.
    pub(crate) fn version_id(&self) -> Option<&str> {
        self.meta.as_ref()?.version_id.as_deref()
    }

    /// The elements read here, taken out of a resource the server sent.
    ///
    /// Every field is optional, so any JSON object reads; a document that is
    /// not an object reads as the empty resource.
    pub(crate) fn of(resource: &Value) -> Self {
        serde_json::from_value(resource.clone()).unwrap_or_default()
    }
}

/// The `history` `Bundle` a `_history` read answers
/// (<https://hl7.org/fhir/R4B/http.html#history>).
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
pub(crate) struct History {
    /// `Bundle.total`, the number of versions the server counted.
    pub(crate) total: Option<u32>,
    /// `Bundle.entry`, one per version, most recent first.
    pub(crate) entry: Vec<HistoryEntry>,
}

/// One version in a history `Bundle`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
pub(crate) struct HistoryEntry {
    /// The resource as it stood at that version; absent for a delete
    /// (<https://hl7.org/fhir/R4B/http.html#history>).
    ///
    /// It is kept as the server sent it, because a restore sends that whole
    /// resource back and an element this viewer has never heard of cannot
    /// survive a typed read.
    pub(crate) resource: Option<Value>,
    /// `entry.request`, which names the interaction that made the version.
    pub(crate) request: Option<HistoryRequest>,
    /// `entry.response`, which carries the version's `ETag` and when it was
    /// written (<https://hl7.org/fhir/R4B/http.html#history>).
    pub(crate) response: Option<HistoryResponse>,
}

/// The `entry.request` elements a history entry carries.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
pub(crate) struct HistoryRequest {
    /// `POST`, `PUT`, or `DELETE`.
    pub(crate) method: Option<String>,
    /// `entry.request.url`, the resource the interaction was made on, which
    /// names a deleted resource that carries no body.
    pub(crate) url: Option<String>,
}

/// The `entry.response` elements a history entry carries.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
pub(crate) struct HistoryResponse {
    /// `response.etag`, the version this entry is.
    pub(crate) etag: Option<String>,
    /// `response.lastModified`, when the server wrote it.
    #[serde(rename = "lastModified")]
    pub(crate) last_modified: Option<String>,
}

/// One version of a resource, as a version list draws it.
///
/// It is the same shape whichever interaction produced it, so a root that
/// answers the history `Bundle` and one that answers only the version read
/// draw the same table.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
    pub(crate) fn of(resource: Option<Value>, position: usize) -> Self {
        let read = resource
            .as_ref()
            .map(StoredResource::of)
            .unwrap_or_default();
        let meta = read.meta.unwrap_or_default();
        Self {
            id: meta
                .version_id
                .unwrap_or_else(|| format!("entry-{position}")),
            last_updated: meta.last_updated,
            method: None,
            source: meta.source,
            resource,
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
                    .map(StoredResource::of)
                    .and_then(|read| read.meta)
                    .and_then(|meta| meta.version_id);
                if stated.is_none()
                    && let Some(etag) = entry
                        .response
                        .as_ref()
                        .and_then(|response| response.etag.as_deref())
                        .map(strong_tag_of)
                        .filter(|tag| !tag.is_empty())
                {
                    etag.clone_into(&mut version.id);
                }
                version.method = entry
                    .request
                    .as_ref()
                    .and_then(|request| request.method.clone());
                if version.last_updated.is_none() {
                    version.last_updated = entry
                        .response
                        .as_ref()
                        .and_then(|response| response.last_modified.clone());
                }
                version
            })
            .collect()
    }
}

/// What a write answered, beyond the resource itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Written {
    /// The resource the server echoed, when it echoed one.
    pub(crate) resource: Option<StoredResource>,
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
            .or_else(|| self.resource.as_ref().and_then(StoredResource::version_id))
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
        &ferrohealth_ui::i18n::Locale::english(),
        Msg::W,
        &[("version_id", (version_id).to_string().into())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_if_match_header_is_the_weak_tag_fhir_states() {
        assert_eq!(if_match("3"), "W/\"3\"");
    }

    #[test]
    fn the_etag_the_server_answered_is_the_version_the_next_update_states() {
        let written = Written {
            resource: Some(StoredResource {
                meta: Some(Meta {
                    version_id: Some(String::from("1")),
                    last_updated: None,
                    source: None,
                }),
                ..StoredResource::default()
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
            resource: Some(StoredResource {
                meta: Some(Meta {
                    version_id: Some(String::from("7")),
                    last_updated: None,
                    source: None,
                }),
                ..StoredResource::default()
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
        let history: History = serde_json::from_str(
            r#"{"resourceType":"Bundle","type":"history","total":2,"entry":[
                 {"resource":{"resourceType":"CodeSystem","id":"colours",
                   "meta":{"versionId":"2","lastUpdated":"2026-09-24T10:00:00Z","source":"https://sync.example/run/9"}},
                  "request":{"method":"PUT","url":"CodeSystem/colours"},
                  "response":{"status":"200","etag":"W/\"2\""}},
                 {"request":{"method":"DELETE","url":"CodeSystem/colours"},
                  "response":{"status":"204","lastModified":"2026-09-23T10:00:00Z"}}]}"#,
        )
        .expect("the server's own history Bundle parses");
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
        let history: History = serde_json::from_str(
            r#"{"resourceType":"Bundle","type":"history","total":2,"entry":[
                 {"fullUrl":"https://tx.example.org/r4b/CodeSystem/colours",
                  "request":{"method":"DELETE","url":"CodeSystem/colours"},
                  "response":{"status":"204 No Content","etag":"W/\"3\"","lastModified":"2026-09-24T11:00:00Z"}},
                 {"fullUrl":"https://tx.example.org/r4b/CodeSystem/colours",
                  "resource":{"resourceType":"CodeSystem","id":"colours",
                   "meta":{"versionId":"2","lastUpdated":"2026-09-24T10:00:00Z"}},
                  "request":{"method":"PUT","url":"CodeSystem/colours"},
                  "response":{"status":"200 OK","etag":"W/\"2\"","lastModified":"2026-09-24T10:00:00Z"}}]}"#,
        )
        .expect("the server's own history Bundle parses");
        let ids: Vec<String> = history
            .versions()
            .into_iter()
            .map(|version| version.id)
            .collect();
        assert_eq!(ids, ["3", "2"]);
    }

    #[test]
    fn a_version_read_answers_the_same_row_a_history_entry_does() {
        let resource: Value = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","id":"colours",
                "meta":{"versionId":"3","lastUpdated":"2026-09-24T11:00:00Z"}}"#,
        )
        .expect("the server's own answer parses");
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
        let resource: StoredResource = serde_json::from_str(
            r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"4","lastUpdated":"2026-09-23T10:00:00Z"},"url":"https://terminology.example/colours"}"#,
        )
        .expect("the server's own answer parses");
        assert_eq!(resource.version_id(), Some("4"));
        assert_eq!(resource.resource_type.as_deref(), Some("CodeSystem"));
        assert_eq!(resource.id.as_deref(), Some("colours"));
    }
}
