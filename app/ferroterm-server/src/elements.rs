// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `_elements` projection: the subset of a resource a client asked for.
//!
//! `_elements` is a search parameter every served version defines
//! (`http://hl7.org/fhir/SearchParameter/Resource-elements`, listed in the
//! base `CapabilityStatement` of each vendored package). A client names the
//! elements it wants and the server returns those, marking what it sends as a
//! subset (<https://hl7.org/fhir/R5/search.html#elements>).
//!
//! Three obligations come with honouring it, and each is a test below.
//!
//! The specification calls the list a hint: "the server SHOULD always return
//! mandatory elements whether they are requested or not". So `resourceType`
//! and `id` survive whatever was asked for, and so does `meta`, which is where
//! the tag goes.
//!
//! A resource returned as a subset carries the `SUBSETTED` tag, from
//! `http://terminology.hl7.org/CodeSystem/v3-ObservationValue`, so a client
//! cannot mistake what it received for the whole resource and store it back.
//!
//! The projection is over the wire representation rather than the typed model,
//! which is what lets one implementation serve all four FHIR versions: a
//! subset of a resource is not a resource, so it has no typed form to build.
//!
//! `_summary` (`http://hl7.org/fhir/SearchParameter/Resource-summary`) is the
//! other result parameter, with fixed views of a resource
//! (<https://hl7.org/fhir/R4B/search.html#summary>). Both apply to the read
//! interaction as well as to search (<https://hl7.org/fhir/R4B/http.html#read>).

use std::collections::BTreeMap;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::schema::{FieldSchema, Kind, Schemas, TypeSchema};
use http::StatusCode;

use crate::outcome::Failure;

/// The search parameter this module answers.
pub const PARAMETER: &str = "_elements";

/// The result parameter naming a summary view.
pub const SUMMARY: &str = "_summary";

/// A `_summary` view this server answers
/// (<https://hl7.org/fhir/R4B/valueset-search-summary.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    /// `true`: `id`, `meta`, and the top-level elements the resource's
    /// definition marks `isSummary`.
    True,
    /// `text`: `text`, `id`, `meta`, and the top-level mandatory elements.
    Text,
    /// `data`: the resource without `text`.
    Data,
    /// `count`: the number of matches and no resources, on search only.
    Count,
    /// `false`: the whole resource.
    False,
}

/// The interaction a projection is read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    /// The read of one resource.
    Read,
    /// A search.
    Search,
}

/// What a request asked a resource in the answer to carry: a `_summary` view
/// and an `_elements` list, both optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projection {
    /// The summary view, when one was named.
    summary: Option<Summary>,
    /// The elements named in `_elements`, the union of every occurrence.
    elements: Vec<String>,
}

impl Projection {
    /// The projection `query` asks for.
    ///
    /// A repeated `_elements` is one list: the union is what the client asked
    /// for. A repeated `_summary` naming one view is that view.
    ///
    /// # Errors
    ///
    /// Returns a `400` for a `_summary` value the search-summary code system
    /// does not define, for two different `_summary` values, for `count` on a
    /// read (the specification scopes it to search).
    pub fn of_query(query: &[(String, String)], interaction: Interaction) -> Result<Self, Failure> {
        let mut summary = None;
        for (_, value) in query.iter().filter(|(name, _)| name == SUMMARY) {
            let named = match value.as_str() {
                "text" => Summary::Text,
                "data" => Summary::Data,
                "count" if interaction == Interaction::Search => Summary::Count,
                "count" => {
                    return Err(invalid("`_summary=count` applies to a search, not a read"));
                }
                "false" => Summary::False,
                "true" => Summary::True,
                other => {
                    return Err(invalid(format!(
                        "`_summary={other}` is not one of `true`, `text`, `data`, `count` or `false`"
                    )));
                }
            };
            match summary {
                Some(held) if held != named => {
                    return Err(invalid("`_summary` names two different views"));
                }
                _ => summary = Some(named),
            }
        }
        let elements = query
            .iter()
            .filter(|(name, _)| name == PARAMETER)
            .flat_map(|(_, value)| requested(value))
            .collect();
        Ok(Self { summary, elements })
    }

    /// Whether the answer is the count of the matches alone.
    #[must_use]
    pub fn is_count(&self) -> bool {
        self.summary == Some(Summary::Count)
    }

    /// Whether the projection leaves every resource whole.
    #[must_use]
    pub fn is_whole(&self) -> bool {
        self.elements.is_empty() && matches!(self.summary, None | Some(Summary::False))
    }

    /// `resource` as this projection shows it; `schemas` is the served
    /// version's element table, which names the mandatory and the summary
    /// elements.
    ///
    /// The summary view applies first and `_elements` narrows what it kept; a
    /// resource that lost an element carries the `SUBSETTED` tag.
    #[must_use]
    pub fn apply(&self, resource: &Object, schemas: &Schemas) -> Object {
        let viewed = match self.summary {
            Some(Summary::True) => summary_view(resource, schemas),
            Some(Summary::Text) => text_view(resource, schemas),
            Some(Summary::Data) => keep(resource, |name| name != "text"),
            None | Some(Summary::False | Summary::Count) => resource.clone(),
        };
        project(&viewed, &self.elements)
    }

    /// Every matched `entry.resource` of a searchset `bundle`, projected.
    ///
    /// The bundle itself is not a resource a client asked elements of: the
    /// parameters name elements of the resources a search matched, so the
    /// envelope keeps every field it had. An entry that is not a match keeps
    /// its resource whole: an `OperationOutcome` describing the search is not
    /// one of the matched resources, and its `issue` is mandatory
    /// (<https://hl7.org/fhir/R4B/http.html#search>).
    pub fn apply_bundle(&self, bundle: &mut Object, schemas: &Schemas) {
        if self.is_whole() {
            return;
        }
        for resource in matched_resources(bundle) {
            let projected = self.apply(resource, schemas);
            *resource = projected;
        }
    }
}

/// The `400` of a `_summary` the specification does not define.
fn invalid(diagnostics: impl Into<String>) -> Failure {
    Failure::new(StatusCode::BAD_REQUEST, "invalid", diagnostics)
}

/// The `_summary=true` view of `resource`: `id`, `meta`, and the top-level
/// elements its definition marks `isSummary`
/// (<https://hl7.org/fhir/R4B/search.html#summary>).
fn summary_view(resource: &Object, schemas: &Schemas) -> Object {
    let schema = resource_schema(resource, schemas);
    keep(resource, |name| {
        MANDATORY.contains(&name) || defined_with(schema, name, |field| field.is_summary)
    })
}

/// The `_summary=text` view of `resource`: `text`, `id`, `meta`, and the
/// top-level elements its definition makes mandatory (`min` of 1 or more).
fn text_view(resource: &Object, schemas: &Schemas) -> Object {
    let schema = resource_schema(resource, schemas);
    keep(resource, |name| {
        MANDATORY.contains(&name)
            || name == "text"
            || defined_with(schema, name, |field| field.min > 0)
    })
}

/// The element table of the type `resource` names in `resourceType`.
fn resource_schema(resource: &Object, schemas: &Schemas) -> Option<&'static TypeSchema> {
    match resource.get("resourceType") {
        Some(Value::String(name)) => schemas.type_named(name),
        _ => None,
    }
}

/// Whether the JSON member `name` is a top-level element of `schema` that
/// `flagged` accepts; a choice element answers for each of its typed names
/// (<https://hl7.org/fhir/R4B/formats.html#choice>).
fn defined_with(
    schema: Option<&TypeSchema>,
    name: &str,
    flagged: impl Fn(&FieldSchema) -> bool,
) -> bool {
    schema.is_some_and(|schema| {
        schema.fields.iter().any(|field| {
            flagged(field)
                && match field.kind {
                    Kind::Choice(alternatives) => name
                        .strip_prefix(field.name)
                        .is_some_and(|suffix| alternatives.iter().any(|(s, _)| *s == suffix)),
                    _ => name == field.name,
                }
        })
    })
}

/// `resource` with the elements `wanted` accepts, the sibling `_name` of a
/// primitive going with its element (<https://hl7.org/fhir/R4B/json.html#primitive>),
/// and marked as a subset when anything was left out.
fn keep(resource: &Object, wanted: impl Fn(&str) -> bool) -> Object {
    let mut kept: Object = resource
        .iter()
        .filter(|(name, _)| wanted(name.strip_prefix('_').unwrap_or(name)))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    if kept.len() < resource.len() {
        kept.insert("meta".to_owned(), subsetted(resource.get("meta")));
    }
    kept
}

/// The elements a resource keeps whatever a client asked for.
///
/// `resourceType` is what makes the JSON a resource at all, `id` is what makes
/// it addressable, and `meta` carries the tag saying it is a subset.
const MANDATORY: [&str; 3] = ["resourceType", "id", "meta"];

/// The system of the tag that marks a subset.
const TAG_SYSTEM: &str = "http://terminology.hl7.org/CodeSystem/v3-ObservationValue";

/// The code of that tag.
const TAG_CODE: &str = "SUBSETTED";

/// Its display, as the code system states it.
const TAG_DISPLAY: &str = "subsetted";

/// The elements a client asked for, read from one `_elements` value.
///
/// The value is a comma-separated list. An empty name is dropped rather than
/// refused: a trailing comma is a client's slip, not a request for an element
/// with no name.
#[must_use]
pub fn requested(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `resource`, reduced to `wanted` plus the elements every resource keeps, and
/// marked as a subset.
///
/// The sibling `_name` of a primitive goes with its element
/// (<https://hl7.org/fhir/R4B/json.html#primitive>). Returns the resource untouched when `wanted` is empty, because a client
/// that named nothing asked for everything.
#[must_use]
pub fn project(resource: &Object, wanted: &[String]) -> Object {
    if wanted.is_empty() {
        return resource.clone();
    }
    let mut kept: Object = resource
        .iter()
        .filter(|(name, _)| {
            let element = name.strip_prefix('_').unwrap_or(name);
            MANDATORY.contains(&element) || wanted.iter().any(|asked| asked == element)
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    kept.insert("meta".to_owned(), subsetted(resource.get("meta")));
    kept
}

/// `meta` with the `SUBSETTED` tag added, keeping whatever it already held.
fn subsetted(existing: Option<&Value>) -> Value {
    let mut meta = match existing {
        Some(Value::Object(held)) => held.clone(),
        _ => BTreeMap::new(),
    };
    let mut tags = match meta.get("tag") {
        Some(Value::Array(held)) => held.clone(),
        _ => Vec::new(),
    };
    if !tags.iter().any(is_subsetted) {
        tags.push(Value::Object(BTreeMap::from([
            ("system".to_owned(), Value::String(TAG_SYSTEM.to_owned())),
            ("code".to_owned(), Value::String(TAG_CODE.to_owned())),
            ("display".to_owned(), Value::String(TAG_DISPLAY.to_owned())),
        ])));
    }
    meta.insert("tag".to_owned(), Value::Array(tags));
    Value::Object(meta)
}

/// Whether one `meta.tag` entry is already the subsetted mark.
fn is_subsetted(tag: &Value) -> bool {
    let Value::Object(coding) = tag else {
        return false;
    };
    coding.get("system") == Some(&Value::String(TAG_SYSTEM.to_owned()))
        && coding.get("code") == Some(&Value::String(TAG_CODE.to_owned()))
}

/// The resource of every `match` entry of a searchset `bundle`.
fn matched_resources(bundle: &mut Object) -> impl Iterator<Item = &mut Object> {
    let entries = match bundle.get_mut("entry") {
        Some(Value::Array(entries)) => entries.as_mut_slice(),
        _ => &mut [],
    };
    entries.iter_mut().filter_map(|entry| {
        let Value::Object(entry) = entry else {
            return None;
        };
        let matched = match entry.get("search") {
            Some(Value::Object(search)) => search
                .get("mode")
                .is_none_or(|mode| mode.as_str() == Some("match")),
            _ => true,
        };
        match entry.get_mut("resource") {
            Some(Value::Object(resource)) if matched => Some(resource),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `CodeSystem` as a search would have rendered it.
    fn code_system() -> Object {
        BTreeMap::from([
            (
                "resourceType".to_owned(),
                Value::String("CodeSystem".to_owned()),
            ),
            ("id".to_owned(), Value::String("loinc".to_owned())),
            (
                "url".to_owned(),
                Value::String("http://loinc.org".to_owned()),
            ),
            ("title".to_owned(), Value::String("LOINC".to_owned())),
            ("version".to_owned(), Value::String("2.83".to_owned())),
            ("status".to_owned(), Value::String("active".to_owned())),
            ("concept".to_owned(), Value::Array(vec![Value::Null; 900])),
        ])
    }

    /// The tag entries a projected resource carries.
    fn tags(projected: &Object) -> Vec<String> {
        let Some(Value::Object(meta)) = projected.get("meta") else {
            return Vec::new();
        };
        let Some(Value::Array(tags)) = meta.get("tag") else {
            return Vec::new();
        };
        tags.iter()
            .filter_map(|tag| match tag {
                Value::Object(coding) => match coding.get("code") {
                    Some(Value::String(code)) => Some(code.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_comma_separated_value_is_the_list_of_elements() {
        assert_eq!(requested("url,name,title"), ["url", "name", "title"]);
        assert_eq!(
            requested(" url , title "),
            ["url", "title"],
            "space around a name is a client's formatting, not part of it"
        );
        assert_eq!(
            requested("url,,title,"),
            ["url", "title"],
            "an empty name is a slip, not a request for an element with no name"
        );
        assert!(requested("").is_empty());
    }

    #[test]
    fn naming_nothing_asks_for_everything() {
        let whole = code_system();
        assert_eq!(
            project(&whole, &[]),
            whole,
            "a search with no _elements is the search it always was"
        );
    }

    #[test]
    fn only_the_named_elements_and_the_mandatory_ones_survive() {
        let projected = project(&code_system(), &requested("url,title"));
        let mut names: Vec<&str> = projected.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["id", "meta", "resourceType", "title", "url"]);
        assert!(
            !projected.contains_key("concept"),
            "the element a client did not ask for is what this parameter exists to leave out"
        );
    }

    #[test]
    fn a_mandatory_element_survives_whether_it_was_asked_for_or_not() {
        let projected = project(&code_system(), &requested("title"));
        assert_eq!(
            projected.get("resourceType"),
            Some(&Value::String("CodeSystem".to_owned())),
            "the specification says a server returns mandatory elements whether they are requested or not"
        );
        assert_eq!(
            projected.get("id"),
            Some(&Value::String("loinc".to_owned())),
            "a resource with no id cannot be read back by the address the search gave"
        );
    }

    #[test]
    fn a_subset_says_it_is_one() {
        let projected = project(&code_system(), &requested("url"));
        assert_eq!(
            tags(&projected),
            ["SUBSETTED"],
            "a client must not mistake a subset for the whole resource and store it back"
        );
    }

    #[test]
    fn the_tag_is_added_once_however_often_a_resource_is_projected() {
        let once = project(&code_system(), &requested("url"));
        let twice = project(&once, &requested("url"));
        assert_eq!(tags(&twice), ["SUBSETTED"]);
    }

    #[test]
    fn a_tag_the_resource_already_carried_is_kept() {
        let mut whole = code_system();
        whole.insert(
            "meta".to_owned(),
            Value::Object(BTreeMap::from([(
                "tag".to_owned(),
                Value::Array(vec![Value::Object(BTreeMap::from([
                    (
                        "system".to_owned(),
                        Value::String("http://example.org/tags".to_owned()),
                    ),
                    ("code".to_owned(), Value::String("local".to_owned())),
                ]))]),
            )])),
        );
        let projected = project(&whole, &requested("url"));
        assert_eq!(
            tags(&projected),
            ["local", "SUBSETTED"],
            "the server adds its mark rather than replacing what the resource held"
        );
    }

    #[test]
    fn an_element_the_resource_does_not_have_is_not_an_error() {
        let projected = project(&code_system(), &requested("url,copyright"));
        assert!(
            !projected.contains_key("copyright"),
            "an absent element is absent, not null"
        );
        assert!(
            projected.contains_key("url"),
            "the elements it does have still come back"
        );
    }

    #[test]
    fn a_primitive_keeps_its_extension_member_and_an_unnamed_one_does_not() {
        let mut whole = code_system();
        whole.insert("_title".to_owned(), Value::Object(BTreeMap::new()));
        whole.insert("_version".to_owned(), Value::Object(BTreeMap::new()));
        let projected = project(&whole, &requested("title"));
        let mut names: Vec<&str> = projected.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["_title", "id", "meta", "resourceType", "title"],
            "a primitive's extension member goes with its element"
        );
    }

    #[test]
    fn every_entry_of_a_searchset_is_projected_and_the_envelope_is_not() {
        let mut bundle: Object = BTreeMap::from([
            (
                "resourceType".to_owned(),
                Value::String("Bundle".to_owned()),
            ),
            ("type".to_owned(), Value::String("searchset".to_owned())),
            ("total".to_owned(), Value::String("2".to_owned())),
            (
                "entry".to_owned(),
                Value::Array(vec![
                    Value::Object(BTreeMap::from([(
                        "resource".to_owned(),
                        Value::Object(code_system()),
                    )])),
                    Value::Object(BTreeMap::from([(
                        "resource".to_owned(),
                        Value::Object(code_system()),
                    )])),
                ]),
            ),
        ]);
        projection(&[("_elements", "url")])
            .apply_bundle(&mut bundle, &fhir_types::r4b::schema::SCHEMAS);
        assert_eq!(
            bundle.get("total"),
            Some(&Value::String("2".to_owned())),
            "the parameter names elements of the resources, not of the envelope"
        );
        let Some(Value::Array(entries)) = bundle.get("entry") else {
            panic!("the bundle keeps its entries");
        };
        for entry in entries {
            let Value::Object(entry) = entry else {
                panic!("an entry is an object");
            };
            let Some(Value::Object(resource)) = entry.get("resource") else {
                panic!("an entry carries its resource");
            };
            assert!(!resource.contains_key("concept"));
            assert_eq!(tags(resource), ["SUBSETTED"]);
        }
    }

    fn query(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn projection(pairs: &[(&str, &str)]) -> Projection {
        Projection::of_query(&query(pairs), Interaction::Search).expect("a projection")
    }

    #[test]
    fn the_summary_views_read_from_the_query() {
        assert!(projection(&[]).is_whole());
        assert!(projection(&[("_summary", "false")]).is_whole());
        assert!(projection(&[("_summary", "count")]).is_count());
        assert!(!projection(&[("_summary", "data")]).is_whole());
        assert!(!projection(&[("_summary", "text"), ("_summary", "text")]).is_whole());
        let read = Projection::of_query(&query(&[("_summary", "count")]), Interaction::Read);
        assert_eq!(read.map_err(|f| f.code), Err("invalid"));
        let summary = Projection::of_query(&query(&[("_summary", "true")]), Interaction::Read);
        assert!(
            summary.is_ok_and(|projection| !projection.is_whole()),
            "`true` is a view the search-summary code system defines, answered on a read"
        );
    }

    #[test]
    fn the_text_view_keeps_the_narrative_and_the_mandatory_elements() {
        let mut whole = code_system();
        whole.insert("content".to_owned(), Value::String("complete".to_owned()));
        whole.insert("_status".to_owned(), Value::Object(BTreeMap::new()));
        whole.insert("text".to_owned(), Value::Object(BTreeMap::new()));
        let viewed =
            projection(&[("_summary", "text")]).apply(&whole, &fhir_types::r4b::schema::SCHEMAS);
        let mut names: Vec<&str> = viewed.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "_status",
                "content",
                "id",
                "meta",
                "resourceType",
                "status",
                "text"
            ],
            "a primitive's extension member goes with its element"
        );
        assert_eq!(tags(&viewed), ["SUBSETTED"]);
    }

    #[test]
    fn the_summary_view_keeps_the_elements_the_definition_flags() {
        let mut whole = code_system();
        whole.insert("name".to_owned(), Value::String("LOINC".to_owned()));
        whole.insert("_version".to_owned(), Value::Object(BTreeMap::new()));
        whole.insert("text".to_owned(), Value::Object(BTreeMap::new()));
        whole.insert(
            "copyright".to_owned(),
            Value::String("Regenstrief".to_owned()),
        );
        let viewed =
            projection(&[("_summary", "true")]).apply(&whole, &fhir_types::r4b::schema::SCHEMAS);
        let mut names: Vec<&str> = viewed.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "_version",
                "id",
                "meta",
                "name",
                "resourceType",
                "status",
                "title",
                "url",
                "version"
            ],
            "`concept`, `text` and `copyright` are not summary elements of an R4B CodeSystem"
        );
        assert_eq!(tags(&viewed), ["SUBSETTED"]);
    }

    #[test]
    fn the_data_view_tags_only_when_it_dropped_the_narrative() {
        let schemas = &fhir_types::r5::schema::SCHEMAS;
        let data = projection(&[("_summary", "data")]);
        assert_eq!(data.apply(&code_system(), schemas), code_system());
        let mut whole = code_system();
        whole.insert("text".to_owned(), Value::Object(BTreeMap::new()));
        let viewed = data.apply(&whole, schemas);
        assert!(!viewed.contains_key("text"));
        assert_eq!(tags(&viewed), ["SUBSETTED"]);
    }

    #[test]
    fn elements_narrow_what_a_summary_kept() {
        let mut whole = code_system();
        whole.insert("text".to_owned(), Value::Object(BTreeMap::new()));
        let viewed = projection(&[("_summary", "data"), ("_elements", "url,text")])
            .apply(&whole, &fhir_types::r5::schema::SCHEMAS);
        let mut names: Vec<&str> = viewed.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["id", "meta", "resourceType", "url"]);
    }
}
