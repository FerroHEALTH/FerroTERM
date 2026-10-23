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
use serde_json::Value;

use crate::fhir::write::{History, HistoryEntry};

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
        let resource = entry.resource.as_ref();
        let text = |key: &str| {
            resource
                .and_then(|resource| resource.get(key))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        let meta = |key: &str| {
            resource
                .and_then(|resource| resource.get("meta"))
                .and_then(|meta| meta.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let request = entry.request.as_ref();
        // NOTE: <https://hl7.org/fhir/R4B/http.html#history>: a delete carries
        // no resource, so its id is the last segment its request names.
        let id = text("id").or_else(|| {
            request
                .and_then(|request| request.url.as_deref())
                .and_then(|url| url.split('?').next())
                .and_then(|path| path.trim_end_matches('/').rsplit('/').next())
                .map(str::to_owned)
        })?;
        let response = entry.response.as_ref();
        Some(Self {
            resource_type: resource_type.to_owned(),
            id,
            named: text("title")
                .or_else(|| text("name"))
                .or_else(|| text("url"))
                .unwrap_or_default(),
            version_id: meta("versionId"),
            method: request.and_then(|request| request.method.clone()),
            when: meta("lastUpdated")
                .or_else(|| response.and_then(|response| response.last_modified.clone())),
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

/// The writes per day an activity chart draws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Days {
    /// `(day index, count)` for each day a write fell on, oldest first.
    pub(crate) points: Vec<(u32, u32)>,
    /// The first day, `YYYY-MM-DD`.
    pub(crate) first: String,
    /// The last day, `YYYY-MM-DD`.
    pub(crate) last: String,
}

/// How many of `changes` fall on each of the days they span, oldest first.
/// `None` when no change states its day.
pub(crate) fn per_day(changes: &[Change]) -> Option<Days> {
    let mut days: Vec<&str> = changes.iter().filter_map(Change::day).collect();
    days.sort_unstable();
    let first = (*days.first()?).to_owned();
    let last = (*days.last()?).to_owned();
    let mut points: Vec<(u32, u32)> = Vec::new();
    let mut distinct: Vec<&str> = Vec::new();
    for day in &days {
        if distinct.last() != Some(day) {
            distinct.push(day);
        }
    }
    for (index, day) in distinct.iter().enumerate() {
        let count = days.iter().filter(|other| *other == day).count();
        points.push((
            u32::try_from(index).unwrap_or(u32::MAX),
            u32::try_from(count).unwrap_or(u32::MAX),
        ));
    }
    Some(Days {
        points,
        first,
        last,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{per_day, recent};
    use crate::fhir::write::History;

    fn history(value: serde_json::Value) -> History {
        serde_json::from_value(value).expect("a history bundle")
    }

    #[test]
    fn the_writes_of_three_types_merge_newest_first_and_name_each_resource() {
        let systems = history(json!({"entry": [
            {"resource": {"resourceType": "CodeSystem", "id": "a", "title": "Colours",
              "meta": {"versionId": "2", "lastUpdated": "2026-10-02T10:00:00Z"}},
             "request": {"method": "PUT"}}
        ]}));
        let sets = history(json!({"entry": [
            {"resource": {"resourceType": "ValueSet", "id": "b", "name": "Warm",
              "meta": {"versionId": "1", "lastUpdated": "2026-10-03T09:00:00Z"}},
             "request": {"method": "POST"}},
            {"request": {"method": "DELETE", "url": "ValueSet/c"},
             "response": {"lastModified": "2026-10-01T08:00:00Z"}}
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

    #[test]
    fn the_writes_count_per_day_oldest_first() {
        let sets = history(json!({"entry": [
            {"resource": {"id": "a", "meta": {"lastUpdated": "2026-10-03T09:00:00Z"}}},
            {"resource": {"id": "b", "meta": {"lastUpdated": "2026-10-03T10:00:00Z"}}},
            {"resource": {"id": "c", "meta": {"lastUpdated": "2026-10-01T10:00:00Z"}}}
        ]}));
        let changes = recent(&[("ValueSet", &sets)], 10);
        let days = per_day(&changes).expect("days");
        assert_eq!(days.points, [(0, 1), (1, 2)]);
        assert_eq!(
            (days.first.as_str(), days.last.as_str()),
            ("2026-10-01", "2026-10-03")
        );
        assert!(per_day(&[]).is_none());
    }
}
