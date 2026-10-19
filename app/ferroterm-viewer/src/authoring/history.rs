//! What the history screen shows: the versions of one resource, what two of
//! them differ by, and what a restore sends.
//!
//! Reading the versions takes whichever interaction the root declares. The
//! history interaction answers a `Bundle` of type `history` whose entries
//! carry the resource, `entry.request` and `entry.response`
//! (<https://hl7.org/fhir/R4B/http.html#history>); where a root declares only
//! the version read, the list is composed one `vread` at a time
//! (<https://hl7.org/fhir/R4B/http.html#vread>) from the version the resource
//! states down towards the first. A root that declares neither has no history
//! to show, and the screen says so.
//!
//! The difference between two versions is our own design: no FHIR
//! specification governs how a client renders one. It is a structural
//! comparison of the two JSON documents, path by path, so a reader sees which
//! elements changed rather than which bytes moved.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use serde_json::Map;
use serde_json::Value;

use crate::fhir::FhirClient;
use crate::fhir::capability::CapabilityStatement;
use crate::fhir::error::FhirError;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::StoredResource;
use crate::fhir::write::Version;

/// `Resource.meta`, whose two server-assigned elements a comparison drops.
const META: &str = "meta";

/// `meta.versionId`, which the server assigns on every write.
const VERSION_ID: &str = "versionId";

/// `meta.lastUpdated`, which the server assigns on every write.
const LAST_UPDATED: &str = "lastUpdated";

/// `Resource.id`, which a restore addresses the resource by.
const ID: &str = "id";

/// How many versions one screen reads back.
///
/// A root that answers only the version read costs one request per version,
/// so the list is bounded and the screen says when it stopped. A reader
/// comparing versions is looking at recent ones.
const DEPTH: u32 = 20;

/// How deep a comparison walks before it reports a subtree whole.
///
/// A resource nests a handful of levels; the bound is what keeps a document
/// this viewer did not write from driving the walk into the stack.
const MAX_DEPTH: u32 = 64;

/// How much of one value a difference row renders.
const VALUE_CHARS: usize = 200;

/// The interaction a root answers a resource's versions with.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Support {
    /// The root declares `history-instance`, so one request answers the list.
    History,
    /// The root declares `vread` alone, so the list is composed one version
    /// read at a time.
    VersionRead,
    /// The root declares neither, so there is no history to show.
    #[default]
    Neither,
}

impl Support {
    /// What `statement` says this root does with the versions of
    /// `resource_type`.
    pub(crate) fn of(statement: &CapabilityStatement, resource_type: &str) -> Self {
        if statement.declares_interaction(resource_type, "history-instance") {
            Self::History
        } else if statement.declares_interaction(resource_type, "vread") {
            Self::VersionRead
        } else {
            Self::Neither
        }
    }

    /// The sentence a screen puts beside an empty version list.
    pub(crate) fn why_empty(self) -> &'static str {
        match self {
            Self::History | Self::VersionRead => {
                "This server holds no earlier version of this resource."
            }
            Self::Neither => {
                "This server's capability statement declares neither the history interaction nor the version read for this resource type, so it offers no versions to read."
            }
        }
    }
}

/// Which resource one read was made for.
///
/// It travels with the answer rather than being read off the address a second
/// time. The address can change while the read for the previous one is still
/// in flight, and a control built from the new address over the old answer
/// would send one resource's document to another resource's id.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Subject {
    /// The resource type, as the RESTful API spells it in a path.
    pub(crate) resource_type: String,
    /// The logical id the interactions address.
    pub(crate) id: String,
}

/// One resource's history, as the screen draws it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Opened {
    /// The resource this answer was read for.
    pub(crate) subject: Subject,
    /// What the root declared about this type's versions.
    pub(crate) support: Support,
    /// The resource as the server holds it now.
    pub(crate) current: Option<Value>,
    /// Its versions, newest first.
    pub(crate) versions: Vec<Version>,
    /// Whether the list stopped short of the first version.
    pub(crate) truncated: bool,
}

impl Opened {
    /// The version the resource is at now, which an update states in
    /// `If-Match` (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) fn current_version_id(&self) -> String {
        self.current
            .as_ref()
            .map(StoredResource::of)
            .and_then(|read| read.meta)
            .and_then(|meta| meta.version_id)
            .unwrap_or_default()
    }

    /// The canonical the resource was published under, where it states one.
    pub(crate) fn canonical(&self) -> String {
        self.current
            .as_ref()
            .map(StoredResource::of)
            .and_then(|read| read.url)
            .unwrap_or_default()
    }

    /// The resource one version holds, by its version identifier.
    pub(crate) fn resource_at(&self, version_id: &str) -> Option<&Value> {
        self.versions
            .iter()
            .find(|version| version.id == version_id)
            .and_then(|version| version.resource.as_ref())
    }
}

/// Reads one resource and its versions.
///
/// # Errors
///
/// Returns the variant of [`FhirError`] describing what went wrong. A resource
/// the root does not hold, and a version it will not answer, are both a
/// refusal the screen renders whole.
pub(crate) async fn open(
    client: &FhirClient,
    version: FhirVersion,
    support: Support,
    subject: Subject,
) -> Result<Opened, FhirError> {
    let empty = Opened {
        subject: subject.clone(),
        support,
        ..Opened::default()
    };
    if subject.id.trim().is_empty() || support == Support::Neither {
        return Ok(empty);
    }
    let resource_type = subject.resource_type.as_str();
    let id = subject.id.as_str();
    let current = Box::pin(client.resource(version, resource_type, id)).await?;
    let (versions, truncated) = match support {
        Support::History => {
            let bundle = Box::pin(client.history(version, resource_type, id)).await?;
            (bundle.versions(), false)
        }
        Support::VersionRead => {
            Box::pin(version_reads(client, version, resource_type, id, &current)).await?
        }
        Support::Neither => (Vec::new(), false),
    };
    Ok(Opened {
        current: Some(current),
        versions,
        truncated,
        ..empty
    })
}

/// The versions of one resource, one version read at a time, newest first.
///
/// The newest version is the resource already in hand, so only the earlier
/// ones cost a request. The walk counts down from the version the resource
/// states, which is what a root that counts its versions answers; a root whose
/// version identifiers are not counted that way answers the one version it
/// stated and nothing earlier.
async fn version_reads(
    client: &FhirClient,
    version: FhirVersion,
    resource_type: &str,
    id: &str,
    current: &Value,
) -> Result<(Vec<Version>, bool), FhirError> {
    let newest = Version::of(Some(current.clone()), 0);
    let stated = newest.id.clone();
    let mut read = vec![newest];
    let Ok(counted) = stated.parse::<u32>() else {
        return Ok((read, false));
    };
    let floor = counted.saturating_sub(DEPTH.saturating_sub(1)).max(1);
    let mut wanted = counted.saturating_sub(1);
    while wanted >= floor && wanted > 0 {
        let at = wanted.to_string();
        let answered = Box::pin(client.version_read(version, resource_type, id, &at)).await;
        let resource = match answered {
            Ok(resource) => resource,
            // NOTE: <https://hl7.org/fhir/R4B/http.html#vread> answers `410`
            // for a deleted version and `404` for one this root does not hold,
            // so a refused earlier version is absent rather than defective.
            Err(refused) if refused.status().is_some() => return Ok((read, true)),
            Err(unreachable) => return Err(unreachable),
        };
        let mut held = Version::of(Some(resource), 0);
        // The address asked for one version, so that is the version this row
        // is, whatever `meta` the answer carried.
        held.id.clone_from(&at);
        read.push(held);
        wanted = wanted.saturating_sub(1);
    }
    Ok((read, floor > 1))
}

/// What one path of a resource did between two versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    /// The later version states the element and the earlier one does not.
    Added,
    /// The earlier version states the element and the later one does not.
    Removed,
    /// Both state it, with different values.
    Changed,
}

impl Change {
    /// The word a row carries, so the change is never a colour alone.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Changed => "changed",
        }
    }
}

/// What a comparison of two chosen versions came to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Comparison {
    /// Neither side is chosen yet.
    #[default]
    Unasked,
    /// One of the two versions carries no resource to compare, because it was
    /// a delete or because this root would not answer it.
    Unread,
    /// The elements the two versions disagree about, empty where they agree.
    Differences(Vec<Difference>),
}

impl Comparison {
    /// What the two chosen versions differ by, where both were read.
    ///
    /// `before` and `after` are the resources the two versions hold, absent
    /// where the version carries none.
    pub(crate) fn of(before: Option<&Value>, after: Option<&Value>) -> Self {
        match (before, after) {
            (Some(before), Some(after)) => Self::Differences(differences(before, after)),
            _unread => Self::Unread,
        }
    }

    /// The sentence the screen announces about the comparison.
    pub(crate) fn sentence(&self) -> String {
        match self {
            Self::Unasked => String::from("Choose two versions above to compare them."),
            Self::Unread => String::from(
                "One of these two versions carries no resource, so there is nothing to compare.",
            ),
            Self::Differences(found) => match found.len() {
                0 => String::from("These two versions state the same elements."),
                1 => String::from("1 element differs."),
                more => format!("{more} elements differ."),
            },
        }
    }

    /// The rows the difference table draws, empty where there are none.
    pub(crate) fn rows(&self) -> &[Difference] {
        match self {
            Self::Differences(found) => found,
            Self::Unasked | Self::Unread => &[],
        }
    }
}

/// One element two versions of a resource disagree about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Difference {
    /// The element's path in the resource, as a reader reads it.
    pub(crate) path: String,
    /// What happened to it.
    pub(crate) change: Change,
    /// What the earlier version held, empty where it held nothing.
    pub(crate) before: String,
    /// What the later version holds, empty where it holds nothing.
    pub(crate) after: String,
}

/// What two versions of one resource differ by, path by path.
///
/// No FHIR specification governs this: it is our own design. The comparison is
/// structural, so the rows name elements rather than bytes, and arrays are
/// compared by position because that is the order a resource states them in.
/// `meta.versionId` and `meta.lastUpdated` are left out: the server assigns
/// both on every write (<https://hl7.org/fhir/R4B/http.html#update>), so they
/// differ between any two versions and say nothing about the edit.
pub(crate) fn differences(before: &Value, after: &Value) -> Vec<Difference> {
    let mut found = Vec::new();
    walk("", &comparable(before), &comparable(after), 0, &mut found);
    found
}

/// `resource` with the two elements the server assigns on every write removed.
///
/// `meta` is left in place, empty when nothing else was in it, so a version
/// that carries another `meta` element compares element by element rather than
/// as one whole object appearing out of nowhere.
fn comparable(resource: &Value) -> Value {
    let mut held = resource.clone();
    let Some(object) = held.as_object_mut() else {
        return held;
    };
    let mut meta = object
        .get(META)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    meta.remove(VERSION_ID);
    meta.remove(LAST_UPDATED);
    object.insert(META.to_owned(), Value::Object(meta));
    held
}

/// Compares one path of two documents and records what differs below it.
fn walk(path: &str, before: &Value, after: &Value, depth: u32, into: &mut Vec<Difference>) {
    if before == after {
        return;
    }
    if depth >= MAX_DEPTH {
        into.push(changed(path, before, after));
        return;
    }
    match (before, after) {
        (Value::Object(earlier), Value::Object(later)) => {
            let names: BTreeSet<&String> = earlier.keys().chain(later.keys()).collect();
            for name in names {
                let below = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}.{name}")
                };
                compare(&below, earlier.get(name), later.get(name), depth, into);
            }
        }
        (Value::Array(earlier), Value::Array(later)) => {
            let reach = earlier.len().max(later.len());
            for index in 0..reach {
                let below = format!("{path}[{index}]");
                compare(&below, earlier.get(index), later.get(index), depth, into);
            }
        }
        (_earlier, _later) => into.push(changed(path, before, after)),
    }
}

/// Records what one element did, where each side may not state it at all.
fn compare(
    path: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    depth: u32,
    into: &mut Vec<Difference>,
) {
    match (before, after) {
        (Some(earlier), Some(later)) => walk(path, earlier, later, depth.saturating_add(1), into),
        (Some(earlier), None) => into.push(Difference {
            path: path.to_owned(),
            change: Change::Removed,
            before: rendered(earlier),
            after: String::new(),
        }),
        (None, Some(later)) => into.push(Difference {
            path: path.to_owned(),
            change: Change::Added,
            before: String::new(),
            after: rendered(later),
        }),
        (None, None) => {}
    }
}

/// One row saying both versions state the path with different values.
fn changed(path: &str, before: &Value, after: &Value) -> Difference {
    Difference {
        path: path.to_owned(),
        change: Change::Changed,
        before: rendered(before),
        after: rendered(after),
    }
}

/// One value as a difference row shows it, bounded so a cell stays readable.
fn rendered(value: &Value) -> String {
    let whole = match value {
        Value::String(text) => text.clone(),
        Value::Null => String::from("null"),
        other => other.to_string(),
    };
    if whole.chars().count() <= VALUE_CHARS {
        return whole;
    }
    let kept: String = whole.chars().take(VALUE_CHARS).collect();
    format!("{kept}\u{2026}")
}

/// The update one restore sends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Restore {
    /// The body, which is the chosen version's whole resource.
    pub(crate) body: String,
    /// The version the update replaces, which travels in `If-Match`
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) version_id: String,
}

/// The update that puts `chosen` back as the current version of `id`.
///
/// The body is the chosen version's resource whole, so every element the
/// screen never drew comes back too. It states no `meta.versionId` and no
/// `meta.lastUpdated`, because the server assigns both
/// (<https://hl7.org/fhir/R4B/http.html#update>), and the version being
/// replaced is the CURRENT one rather than the chosen one: `If-Match` is what
/// makes a concurrent edit a refusal rather than an overwrite.
pub(crate) fn restore(chosen: &Value, id: &str, current_version_id: &str) -> Restore {
    let mut body = chosen.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert(ID.to_owned(), Value::String(id.to_owned()));
        if let Some(meta) = object.get_mut(META).and_then(Value::as_object_mut) {
            meta.remove(VERSION_ID);
            meta.remove(LAST_UPDATED);
        }
    }
    Restore {
        body: body.to_string(),
        version_id: current_version_id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One shaped resource, as the server holds it.
    fn resource(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    /// The rows a comparison produced, as `path:change` pairs.
    fn rows(before: &Value, after: &Value) -> Vec<String> {
        differences(before, after)
            .into_iter()
            .map(|row| format!("{}:{}", row.path, row.change.word()))
            .collect()
    }

    #[test]
    fn two_identical_resources_differ_in_nothing() {
        let held = resource(r#"{"resourceType":"CodeSystem","id":"c","status":"draft"}"#);
        assert!(differences(&held, &held).is_empty());
    }

    #[test]
    fn the_two_elements_the_server_assigns_are_never_a_difference() {
        let before = resource(
            r#"{"resourceType":"CodeSystem","id":"c","meta":{"versionId":"1","lastUpdated":"2026-09-23T10:00:00Z"}}"#,
        );
        let after = resource(
            r#"{"resourceType":"CodeSystem","id":"c","meta":{"versionId":"2","lastUpdated":"2026-09-24T10:00:00Z"}}"#,
        );
        assert!(
            differences(&before, &after).is_empty(),
            "both differ on every write and say nothing about the edit"
        );
    }

    #[test]
    fn a_meta_element_the_server_does_not_assign_is_still_a_difference() {
        let before = resource(r#"{"resourceType":"CodeSystem","meta":{"versionId":"1"}}"#);
        let after = resource(
            r#"{"resourceType":"CodeSystem","meta":{"versionId":"2","source":"https://sync.example/run/9"}}"#,
        );
        assert_eq!(rows(&before, &after), ["meta.source:added"]);
    }

    #[test]
    fn an_element_one_version_states_and_the_other_does_not() {
        let before = resource(r#"{"resourceType":"ValueSet","status":"draft"}"#);
        let after = resource(r#"{"resourceType":"ValueSet","status":"draft","title":"Palette"}"#);
        let added = differences(&before, &after);
        assert_eq!(rows(&before, &after), ["title:added"]);
        assert_eq!(
            added.first().map(|row| row.after.as_str()),
            Some("Palette"),
            "the row says what the later version holds"
        );
        assert_eq!(rows(&after, &before), ["title:removed"]);
    }

    #[test]
    fn a_changed_value_carries_both_sides() {
        let before = resource(r#"{"resourceType":"CodeSystem","status":"draft"}"#);
        let after = resource(r#"{"resourceType":"CodeSystem","status":"active"}"#);
        let found = differences(&before, &after);
        let row = found.first().expect("one element changed");
        assert_eq!(row.path, "status");
        assert_eq!(row.change, Change::Changed);
        assert_eq!(row.before, "draft");
        assert_eq!(row.after, "active");
    }

    #[test]
    fn a_nested_element_is_named_by_its_whole_path() {
        let before = resource(r#"{"concept":[{"code":"red","display":"Red"}]}"#);
        let after = resource(r#"{"concept":[{"code":"red","display":"Crimson"}]}"#);
        assert_eq!(rows(&before, &after), ["concept[0].display:changed"]);
    }

    #[test]
    fn an_array_is_compared_by_position() {
        let before = resource(r#"{"concept":[{"code":"a"},{"code":"b"}]}"#);
        let after = resource(r#"{"concept":[{"code":"a"}]}"#);
        assert_eq!(rows(&before, &after), ["concept[1]:removed"]);
        assert_eq!(rows(&after, &before), ["concept[1]:added"]);
    }

    #[test]
    fn a_value_that_changed_type_is_one_changed_row() {
        let before = resource(r#"{"caseSensitive":true}"#);
        let after = resource(r#"{"caseSensitive":"yes"}"#);
        let found = differences(&before, &after);
        assert_eq!(rows(&before, &after), ["caseSensitive:changed"]);
        assert_eq!(found.first().map(|row| row.before.as_str()), Some("true"));
    }

    #[test]
    fn every_element_that_changed_is_a_row_of_its_own() {
        let before = resource(
            r#"{"resourceType":"CodeSystem","status":"draft","version":"1",
                "concept":[{"code":"a","display":"A"}]}"#,
        );
        let after = resource(
            r#"{"resourceType":"CodeSystem","status":"active","version":"1",
                "concept":[{"code":"a","display":"A","definition":"the first"},{"code":"b"}]}"#,
        );
        assert_eq!(
            rows(&before, &after),
            [
                "concept[0].definition:added",
                "concept[1]:added",
                "status:changed",
            ],
            "the rows are in the resource's own key order, which is stable"
        );
    }

    #[test]
    fn a_long_value_is_bounded_so_a_cell_stays_readable() {
        let long = "x".repeat(VALUE_CHARS + 50);
        let before = resource(r#"{"description":""}"#);
        let after = serde_json::json!({ "description": long });
        let found = differences(&before, &after);
        let row = found.first().expect("the description changed");
        assert_eq!(row.after.chars().count(), VALUE_CHARS + 1, "{}", row.after);
        assert!(row.after.ends_with('\u{2026}'));
    }

    #[test]
    fn a_restore_sends_the_chosen_version_and_states_the_current_one() {
        let chosen = resource(
            r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"1","lastUpdated":"2026-09-23T10:00:00Z"},
                "url":"https://terminology.example/colours","status":"draft","extra":{"kept":true}}"#,
        );
        let sent = restore(&chosen, "colours", "3");
        assert_eq!(
            sent.version_id, "3",
            "If-Match states the version being replaced, never the one being restored"
        );
        let body: Value = serde_json::from_str(&sent.body).expect("the body is JSON");
        assert_eq!(body.get("status").and_then(Value::as_str), Some("draft"));
        assert_eq!(
            body.pointer("/extra/kept").and_then(Value::as_bool),
            Some(true),
            "an element the screen never drew still reaches the server"
        );
        assert_eq!(body.get(ID).and_then(Value::as_str), Some("colours"));
        assert_eq!(
            body.pointer("/meta/versionId"),
            None,
            "the server assigns the version, so the body states none"
        );
        assert_eq!(body.pointer("/meta/lastUpdated"), None);
    }

    #[test]
    fn a_restore_addresses_the_resource_the_screen_has_open() {
        let chosen = resource(r#"{"resourceType":"ValueSet","id":"old","status":"draft"}"#);
        let sent = restore(&chosen, "current", "2");
        let body: Value = serde_json::from_str(&sent.body).expect("the body is JSON");
        assert_eq!(
            body.get(ID).and_then(Value::as_str),
            Some("current"),
            "an update addresses the instance it replaces"
        );
    }

    #[test]
    fn a_restore_of_a_document_that_is_not_an_object_sends_it_unchanged() {
        let sent = restore(&Value::Null, "c", "1");
        assert_eq!(sent.body, "null");
        assert_eq!(sent.version_id, "1");
    }

    #[test]
    fn an_empty_version_list_says_why_it_is_empty() {
        assert!(
            Support::Neither
                .why_empty()
                .contains("capability statement")
        );
        assert!(Support::History.why_empty().contains("no earlier version"));
        assert_eq!(
            Support::VersionRead.why_empty(),
            Support::History.why_empty()
        );
    }

    #[test]
    fn the_current_version_and_canonical_come_off_the_resource_the_server_holds() {
        let opened = Opened {
            current: Some(resource(
                r#"{"resourceType":"CodeSystem","id":"c","meta":{"versionId":"4"},
                    "url":"https://terminology.example/colours"}"#,
            )),
            ..Opened::default()
        };
        assert_eq!(opened.current_version_id(), "4");
        assert_eq!(opened.canonical(), "https://terminology.example/colours");
        assert_eq!(Opened::default().current_version_id(), "");
        assert_eq!(Opened::default().canonical(), "");
    }

    #[test]
    fn a_version_that_carries_no_resource_is_never_reported_as_agreeing() {
        let held = resource(r#"{"resourceType":"CodeSystem","status":"draft"}"#);
        let unread = Comparison::of(None, Some(&held));
        assert_eq!(unread, Comparison::Unread);
        assert!(unread.rows().is_empty());
        assert!(
            unread.sentence().contains("nothing to compare"),
            "a deleted version says so rather than agreeing: `{}`",
            unread.sentence()
        );
        assert_eq!(Comparison::of(Some(&held), None), Comparison::Unread);
    }

    #[test]
    fn a_comparison_counts_what_it_found() {
        let before = resource(r#"{"status":"draft","version":"1"}"#);
        let after = resource(r#"{"status":"active","version":"2"}"#);
        assert_eq!(
            Comparison::of(Some(&before), Some(&after)).sentence(),
            "2 elements differ."
        );
        assert_eq!(
            Comparison::of(Some(&before), Some(&before)).sentence(),
            "These two versions state the same elements."
        );
        assert_eq!(
            Comparison::of(
                Some(&before),
                Some(&resource(r#"{"status":"active","version":"1"}"#))
            )
            .sentence(),
            "1 element differs."
        );
        assert_eq!(
            Comparison::Unasked.sentence(),
            "Choose two versions above to compare them.",
            "the region is in the document before the first comparison, saying what to do"
        );
    }

    #[test]
    fn a_version_is_found_by_its_identifier_and_never_by_its_row() {
        let opened = Opened {
            current: None,
            versions: vec![
                Version {
                    id: String::from("2"),
                    resource: Some(resource(r#"{"status":"active"}"#)),
                    ..Version::default()
                },
                Version {
                    id: String::from("1"),
                    resource: Some(resource(r#"{"status":"draft"}"#)),
                    ..Version::default()
                },
            ],
            ..Opened::default()
        };
        assert_eq!(
            opened
                .resource_at("1")
                .and_then(|held| held.get("status"))
                .and_then(Value::as_str),
            Some("draft")
        );
        assert!(opened.resource_at("9").is_none());
    }
}
