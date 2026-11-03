// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What was written lately: the type-level history of the three definitional
//! resources this server publishes, merged into one list, newest first.
//!
//! A type-level `_history` answers a `history` `Bundle` of every version of
//! every resource of the type (<https://hl7.org/fhir/R4B/http.html#history>).
//! The dashboard reads the newest few of each type and merges them here, so
//! the order is the server's `lastModified` and nothing else.

use serde::{Deserialize, Serialize};

use crate::fhir::write::{History, HistoryEntry, Stamp};

/// One write the dashboard lists.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Change {
    /// The resource type, `CodeSystem`, `ValueSet` or `ConceptMap`.
    pub(crate) resource_type: String,
    /// The resource's id.
    pub(crate) id: String,
    /// The title, else the name, else the canonical the resource carries;
    /// empty for a delete, which carries no body.
    pub(crate) named: String,
    /// The version id the write made, when the answer states it.
    pub(crate) version_id: Option<String>,
    /// `POST`, `PUT` or `DELETE`, when the answer states it.
    pub(crate) method: Option<String>,
    /// When the server wrote it, as the answer states it.
    pub(crate) when: Option<String>,
}

impl Change {
    /// The write `entry` records, of the type `resource_type`.
    fn of(resource_type: &str, entry: &HistoryEntry) -> Option<Self> {
        let stamp = entry.resource.as_ref().map(|stored| &stored.stamp);
        let text = |of: fn(&Stamp) -> &Option<String>| {
            stamp
                .and_then(|stamp| of(stamp).clone())
                .filter(|text| !text.is_empty())
        };
        let id = text(|stamp| &stamp.id).or_else(|| {
            entry
                .url
                .as_deref()
                .and_then(|url| url.split('?').next())
                .and_then(|path| path.trim_end_matches('/').rsplit('/').next())
                .map(str::to_owned)
        })?;
        Some(Self {
            resource_type: resource_type.to_owned(),
            id,
            named: text(|stamp| &stamp.title)
                .or_else(|| text(|stamp| &stamp.name))
                .or_else(|| text(|stamp| &stamp.url))
                .unwrap_or_default(),
            version_id: stamp.and_then(|stamp| stamp.version_id.clone()),
            method: entry.method.clone(),
            when: stamp
                .and_then(|stamp| stamp.last_updated.clone())
                .or_else(|| entry.last_modified.clone()),
        })
    }

    /// The day of the write, `YYYY-MM-DD`, when the answer states when.
    pub(crate) fn day(&self) -> Option<&str> {
        self.when.as_deref().and_then(|when| when.get(..10))
    }
}

/// The writes of `histories`, each a type-level history of the type it is
/// paired with, merged newest first and cut to `limit`.
///
/// The timestamps are ISO 8601 instants in the server's own zone, which sort
/// as text; one without a time sorts last.
pub(crate) fn recent(histories: &[(&str, &History)], limit: usize) -> Vec<Change> {
    let mut changes: Vec<Change> = histories
        .iter()
        .flat_map(|(resource_type, history)| {
            history
                .entry
                .iter()
                .filter_map(|entry| Change::of(resource_type, entry))
        })
        .collect();
    changes.sort_by(|left, right| right.when.cmp(&left.when));
    changes.truncate(limit);
    changes
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::recent;
    use crate::fhir::write::History;

    /// `value`, a history `Bundle` the server answered, decoded at R4B.
    fn history(value: serde_json::Value) -> History {
        crate::fhir::version::FhirVersion::R4B
            .decode(&value.to_string())
            .expect("the server's own history Bundle decodes at R4B")
    }

    #[test]
    fn the_writes_of_three_types_merge_newest_first_and_name_each_resource() {
        let systems = history(json!({"resourceType": "Bundle", "type": "history", "entry": [
            {"resource": {"resourceType": "CodeSystem", "id": "a", "title": "Colours",
              "status": "active", "content": "complete",
              "meta": {"versionId": "2", "lastUpdated": "2026-10-02T10:00:00Z"}},
             "request": {"method": "PUT", "url": "CodeSystem/a"}}
        ]}));
        let sets = history(json!({"resourceType": "Bundle", "type": "history", "entry": [
            {"resource": {"resourceType": "ValueSet", "id": "b", "name": "Warm", "status": "active",
              "meta": {"versionId": "1", "lastUpdated": "2026-10-03T09:00:00Z"}},
             "request": {"method": "POST", "url": "ValueSet"}},
            {"request": {"method": "DELETE", "url": "ValueSet/c"},
             "response": {"status": "204", "lastModified": "2026-10-01T08:00:00Z"}}
        ]}));
        let merged = recent(&[("CodeSystem", &systems), ("ValueSet", &sets)], 10);
        let ids: Vec<&str> = merged.iter().map(|change| change.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
        assert_eq!(merged[0].named, "Warm");
        assert_eq!(merged[1].named, "Colours");
        assert_eq!(merged[2].named, "", "a delete carries no body to name");
        assert_eq!(merged[2].method.as_deref(), Some("DELETE"));
        assert_eq!(
            recent(&[("ValueSet", &sets)], 1).len(),
            1,
            "cut to the limit"
        );
    }
}
