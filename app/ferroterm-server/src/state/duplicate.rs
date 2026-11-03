// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Canonicals that more than one persisted or loaded resource holds.

use std::collections::BTreeMap;

use crate::persistence::{Record, ResourceType};
use crate::state::ids::canonical;

/// Whether two `url` and `version` pairs name one canonical resource, an empty
/// version counting as none
/// (<https://hl7.org/fhir/R4B/references.html#canonical>).
pub(super) fn same_canonical(left: (&str, Option<&str>), right: (&str, Option<&str>)) -> bool {
    left.0 == right.0 && present_version(left.1) == present_version(right.1)
}

/// `version`, when it is present and not empty.
pub(super) fn present_version(version: Option<&str>) -> Option<&str> {
    version.filter(|version| !version.is_empty())
}

/// The persisted records in the order they were written, oldest first, so
/// that of two records carrying one canonical the later write is layered last
/// and answers.
///
/// No FHIR/SNOMED spec governs this: our own design. A store written before
/// the server refused a duplicate canonical may hold one, and the most recent
/// write is the one its writer last meant.
pub(super) fn in_write_order(records: &BTreeMap<(ResourceType, String), Record>) -> Vec<&Record> {
    let mut ordered: Vec<(Option<jiff::Timestamp>, &Record)> = records
        .values()
        // NOTE: an unreadable `lastUpdated` sorts first, as the oldest write, so the
        // record is still layered; no FHIR/SNOMED spec governs this: our own design.
        .map(|record| (record.last_modified.parse().ok(), record))
        .collect();
    ordered.sort_by(|(left_time, left), (right_time, right)| {
        left_time
            .cmp(right_time)
            .then_with(|| left.version_id.cmp(&right.version_id))
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered.into_iter().map(|(_, record)| record).collect()
}

/// One canonical that more than one resource of a type carries, with every id
/// that carries it and the one that answers for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DuplicateCanonical {
    /// The resource type.
    pub(super) resource_type: ResourceType,
    /// The `url|version` the resources share.
    pub(super) canonical: String,
    /// Every id that carries it, the loaded ones first, then the persisted
    /// ones oldest write first.
    pub(super) ids: Vec<String>,
    /// The id whose resource answers: the persisted record written last,
    /// which the served layer applies over every other.
    pub(super) answering: String,
}

/// Every canonical that a persisted record shares with another persisted
/// record or with a loaded resource of its type.
pub(super) fn duplicate_canonicals(
    records: &BTreeMap<(ResourceType, String), Record>,
    loaded: &[(ResourceType, &str, &str, Option<&str>)],
) -> Vec<DuplicateCanonical> {
    let mut held: BTreeMap<(ResourceType, String), Vec<String>> = BTreeMap::new();
    for (resource_type, id, url, version) in loaded {
        held.entry((*resource_type, canonical(url, *version)))
            .or_default()
            .push((*id).to_owned());
    }
    let mut written: BTreeMap<(ResourceType, String), Vec<String>> = BTreeMap::new();
    for record in in_write_order(records) {
        let (Some(resource_type), Some(url)) =
            (ResourceType::parse(&record.resource_type), &record.url)
        else {
            continue;
        };
        written
            .entry((resource_type, canonical(url, record.version.as_deref())))
            .or_default()
            .push(record.id.clone());
    }
    written
        .into_iter()
        .filter_map(|((resource_type, canonical), persisted)| {
            let answering = persisted.last()?.clone();
            let mut ids = held
                .remove(&(resource_type, canonical.clone()))
                .unwrap_or_default();
            ids.extend(persisted);
            (ids.len() > 1).then_some(DuplicateCanonical {
                resource_type,
                canonical,
                ids,
                answering,
            })
        })
        .collect()
}

#[cfg(test)]
mod duplicate_tests {
    use std::collections::BTreeMap;

    use super::{DuplicateCanonical, duplicate_canonicals, in_write_order, same_canonical};
    use crate::persistence::{Record, ResourceType};

    fn record(id: &str, url: &str, version: Option<&str>, written: &str) -> Record {
        Record {
            resource_type: String::from("ValueSet"),
            id: id.to_owned(),
            url: Some(url.to_owned()),
            version: version.map(str::to_owned),
            fhir_version: String::from("4.3.0"),
            version_id: 1,
            last_modified: written.to_owned(),
            resource: fhir_types::codec::Object::new(),
        }
    }

    fn records(held: Vec<Record>) -> BTreeMap<(ResourceType, String), Record> {
        held.into_iter()
            .map(|record| ((ResourceType::ValueSet, record.id.clone()), record))
            .collect()
    }

    #[test]
    fn an_empty_version_is_no_version() {
        assert!(same_canonical(("u", Some("")), ("u", None)));
        assert!(!same_canonical(("u", Some("1")), ("u", None)));
        assert!(!same_canonical(("u", Some("1")), ("v", Some("1"))));
    }

    #[test]
    fn records_layer_in_the_order_they_were_written() {
        // The id that sorts last was written first, and the fractional second
        // sorts after the whole one although its text sorts before it.
        let held = records(vec![
            record(
                "zz",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00Z",
            ),
            record(
                "aa",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00.5Z",
            ),
        ]);
        let order: Vec<&str> = in_write_order(&held)
            .into_iter()
            .map(|record| record.id.as_str())
            .collect();
        assert_eq!(order, ["zz", "aa"]);
    }

    #[test]
    fn a_shared_canonical_names_every_holder_and_the_latest_write() {
        let held = records(vec![
            record(
                "aa",
                "https://a.example/vs",
                Some("1"),
                "2026-02-01T00:00:00Z",
            ),
            record(
                "zz",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00Z",
            ),
            record(
                "other",
                "https://a.example/vs",
                Some("2"),
                "2026-01-01T00:00:00Z",
            ),
        ]);
        assert_eq!(
            duplicate_canonicals(&held, &[]),
            [DuplicateCanonical {
                resource_type: ResourceType::ValueSet,
                canonical: String::from("https://a.example/vs|1"),
                ids: vec![String::from("zz"), String::from("aa")],
                answering: String::from("aa"),
            }]
        );
    }

    #[test]
    fn a_persisted_record_over_a_loaded_canonical_is_named() {
        let held = records(vec![record(
            "written",
            "https://a.example/vs",
            Some("1"),
            "2026-01-01T00:00:00Z",
        )]);
        let loaded = [(
            ResourceType::ValueSet,
            "loaded",
            "https://a.example/vs",
            Some("1"),
        )];
        let found = duplicate_canonicals(&held, &loaded);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, ["loaded", "written"]);
        assert_eq!(found[0].answering, "written");
        assert!(
            duplicate_canonicals(&held, &[]).is_empty(),
            "one record alone is no duplicate"
        );
    }
}
