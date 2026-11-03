// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The logical ids the loaded resources answer on.

use std::collections::BTreeMap;

use crate::persistence::ResourceType;
use crate::state::LoadError;
use crate::state::instance_id;

/// `url|version` split into its parts.
pub(super) fn split_canonical(canonical: &str) -> (&str, Option<&str>) {
    match canonical.split_once('|') {
        Some((url, version)) => (url, Some(version)),
        None => (canonical, None),
    }
}
/// `wanted`, or `wanted` with a numeric suffix when the reduced id is taken.
pub(super) fn unique_id(taken: &BTreeMap<String, (String, String)>, wanted: String) -> String {
    unique_id_of(taken, wanted)
}
/// The `url|version` canonical of a resource, for a diagnostic.
pub(super) fn canonical(url: &str, version: Option<&str>) -> String {
    match version.filter(|version| !version.is_empty()) {
        Some(version) => format!("{url}|{version}"),
        None => url.to_owned(),
    }
}
/// Every loaded resource of one type carries a distinct authored id.
///
/// # Errors
///
/// Returns [`LoadError::DuplicateId`] naming both canonicals. A server does not
/// rename a resource it loads (<https://hl7.org/fhir/R4B/resource.html#id>), so
/// the load fails instead of picking one.
pub(super) fn check_distinct_ids(
    resource_type: ResourceType,
    authored: impl Iterator<Item = (String, String)>,
) -> Result<(), LoadError> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (id, canonical) in authored {
        if let Some(first) = seen.get(&id) {
            return Err(LoadError::DuplicateId {
                resource_type: resource_type.name(),
                id,
                first: first.clone(),
                second: canonical,
            });
        }
        seen.insert(id, canonical);
    }
    Ok(())
}
/// The id a loaded value set or concept map is registered under.
///
/// The id the resource was authored with, so a client reads it at the id it
/// knows (<https://hl7.org/fhir/R4B/resource.html#id>); a resource without one
/// keeps the id the server mints from its canonical, which no specification
/// governs: our own design.
///
/// # Errors
///
/// Returns [`LoadError::DuplicateId`] when another loaded resource of the type
/// already answers on the authored id.
pub(super) fn registered_id(
    resource_type: ResourceType,
    authored: Option<&str>,
    url: &str,
    version: Option<&str>,
    taken: &BTreeMap<String, (String, Option<String>)>,
) -> Result<String, LoadError> {
    let Some(authored) = authored else {
        return Ok(unique_id_of(
            taken,
            instance_id(url, version.unwrap_or_default()),
        ));
    };
    if let Some((held_url, held_version)) = taken.get(authored) {
        return Err(LoadError::DuplicateId {
            resource_type: resource_type.name(),
            id: authored.to_owned(),
            first: canonical(held_url, held_version.as_deref()),
            second: canonical(url, version),
        });
    }
    Ok(authored.to_owned())
}
/// `wanted`, or `wanted` with a numeric suffix when the reduced id is a key
/// of `taken`.
pub(super) fn unique_id_of<V>(taken: &BTreeMap<String, V>, wanted: String) -> String {
    if !taken.contains_key(&wanted) {
        return wanted;
    }
    let stem: String = wanted.chars().take(60).collect();
    (2..=taken.len().saturating_add(2))
        .map(|n| format!("{stem}-{n}"))
        .find(|candidate| !taken.contains_key(candidate))
        .unwrap_or(wanted)
}
