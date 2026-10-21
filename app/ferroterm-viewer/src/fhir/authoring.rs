// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CodeSystem` an editor reads back, in the elements it authors.
//!
//! The editor screen needs more of the resource than the detail screen shows:
//! the logical id and `meta.versionId` an update states in `If-Match`
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>), the declared properties,
//! and every concept with its designations and property values
//! (<https://hl7.org/fhir/R4B/codesystem.html>). Those elements are read here,
//! outside every component, so the screen renders a value plain unit tests can
//! pin.
//!
//! This is still the minimum the screen draws. The viewer never mirrors a
//! whole FHIR resource.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The resource type an entry must declare to be read as a code system.
const CODE_SYSTEM: &str = "CodeSystem";

/// What `GET [base]/CodeSystem?url=` answered, read for authoring.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct AuthoredSearch {
    /// `Bundle.entry`, one per match.
    #[serde(default)]
    entry: Vec<SearchEntry>,
}

/// One `Bundle.entry`.
///
/// The resource is kept as the server sent it. An update replaces the whole
/// resource (<https://hl7.org/fhir/R4B/http.html#update>), so every element
/// the editor does not draw still has to reach the server again, and an
/// element this viewer has never heard of cannot survive a typed read.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct SearchEntry {
    /// The resource the entry carries.
    resource: Option<Value>,
}

impl AuthoredSearch {
    /// The first `CodeSystem` the answer carries, in the server's own order.
    ///
    /// A `searchset` may also carry an `OperationOutcome` describing the search
    /// (<https://hl7.org/fhir/R4B/http.html#search>), so an entry declaring
    /// another type is passed over rather than read as a code system.
    pub(crate) fn first(&self) -> Option<Value> {
        self.first_of(CODE_SYSTEM)
    }

    /// The first resource of `resource_type` the answer carries.
    ///
    /// The envelope is the same whatever was searched for, so one reader
    /// serves every authoring screen and each one names the type it drew.
    pub(crate) fn first_of(&self, resource_type: &str) -> Option<Value> {
        self.entry
            .iter()
            .filter_map(|entry| entry.resource.as_ref())
            .find(|resource| {
                resource.get("resourceType").and_then(Value::as_str) == Some(resource_type)
            })
            .cloned()
    }
}

/// A `CodeSystem` as the editor reads it back off the server.
///
/// This is the elements the screen draws, and no more. The resource itself
/// travels beside it, so what the screen does not draw is still what is sent
/// back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredCodeSystem {
    /// `id`, the logical id an update addresses.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// `meta`, which carries the version an update states.
    #[serde(default)]
    pub(crate) meta: Option<Meta>,
    /// `CodeSystem.url`, the canonical.
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// `CodeSystem.version`, the business version.
    #[serde(default)]
    pub(crate) version: Option<String>,
    /// `CodeSystem.status`, a code of the publication status value set.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `CodeSystem.content`, a code of the content mode value set.
    #[serde(default)]
    pub(crate) content: Option<String>,
    /// `CodeSystem.caseSensitive`.
    #[serde(default, rename = "caseSensitive")]
    pub(crate) case_sensitive: Option<bool>,
    /// `CodeSystem.property`, the properties this system declares.
    #[serde(default)]
    pub(crate) property: Vec<StoredProperty>,
    /// `CodeSystem.concept`, the concepts it defines.
    #[serde(default)]
    pub(crate) concept: Vec<StoredConcept>,
}

/// The `Resource.meta` elements the editor reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Meta {
    /// `meta.versionId`, the version an `If-Match` states.
    #[serde(default, rename = "versionId")]
    pub(crate) version_id: Option<String>,
}

/// One `CodeSystem.property` declaration.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredProperty {
    /// `property.code`, the name a concept's value refers to.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// `property.uri`, the formal identifier of the property.
    #[serde(default)]
    pub(crate) uri: Option<String>,
    /// `property.type`, a code of the concept property type value set.
    #[serde(default, rename = "type")]
    pub(crate) kind: Option<String>,
}

/// One `CodeSystem.concept`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredConcept {
    /// `concept.code`.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// `concept.display`.
    #[serde(default)]
    pub(crate) display: Option<String>,
    /// `concept.definition`.
    #[serde(default)]
    pub(crate) definition: Option<String>,
    /// `concept.designation`.
    #[serde(default)]
    pub(crate) designation: Vec<StoredDesignation>,
    /// `concept.property`, the values this concept carries.
    #[serde(default)]
    pub(crate) property: Vec<StoredValue>,
}

/// One `concept.designation`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredDesignation {
    /// `designation.language`, a BCP 47 tag.
    #[serde(default)]
    pub(crate) language: Option<String>,
    /// `designation.use`, the coding that says what kind of term this is.
    #[serde(default, rename = "use")]
    pub(crate) usage: Option<StoredCoding>,
    /// `designation.value`, the term itself.
    #[serde(default)]
    pub(crate) value: Option<String>,
}

/// A `Coding`, in the two elements the editor authors.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredCoding {
    /// `Coding.system`.
    #[serde(default)]
    pub(crate) system: Option<String>,
    /// `Coding.code`.
    #[serde(default)]
    pub(crate) code: Option<String>,
}

/// One `concept.property`, read as the text the editor shows.
///
/// `property.value[x]` is a choice of seven types
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.concept.property.value_x_>),
/// and the editor draws one control per value whose shape the declared type
/// decides, so every arm is read into the text that control holds.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredValue {
    /// `property.code`, naming the declared property.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// `valueCode`.
    #[serde(default, rename = "valueCode")]
    value_code: Option<String>,
    /// `valueCoding`.
    #[serde(default, rename = "valueCoding")]
    value_coding: Option<StoredCoding>,
    /// `valueString`.
    #[serde(default, rename = "valueString")]
    value_string: Option<String>,
    /// `valueInteger`, which FHIR defines as 32-bit signed
    /// (<https://hl7.org/fhir/R4B/datatypes.html#primitive>).
    #[serde(default, rename = "valueInteger")]
    value_integer: Option<i32>,
    /// `valueBoolean`.
    #[serde(default, rename = "valueBoolean")]
    value_boolean: Option<bool>,
    /// `valueDateTime`.
    #[serde(default, rename = "valueDateTime")]
    value_date_time: Option<String>,
    /// `valueDecimal`, read as the number's own text.
    #[serde(default, rename = "valueDecimal")]
    value_decimal: Option<serde_json::Number>,
}

impl StoredCodeSystem {
    /// The elements the editor draws, read out of `resource`.
    ///
    /// Every field is optional, so any JSON object reads; a document that is
    /// not an object reads as the empty resource, which the screen then shows
    /// as a code system it found nothing in.
    pub(crate) fn of(resource: &Value) -> Self {
        serde_json::from_value(resource.clone()).unwrap_or_default()
    }
}

/// The separator between a coding's system and its code, in one field.
///
/// FHIR search spells a coding `system|code`
/// (<https://hl7.org/fhir/R4B/search.html#token>), so the editor takes one
/// field in that syntax rather than two controls for one value.
pub(crate) const CODING_SEPARATOR: char = '|';

impl StoredValue {
    /// The value as the one text the editor's control holds.
    pub(crate) fn text(&self) -> String {
        if let Some(code) = &self.value_code {
            return code.clone();
        }
        if let Some(coding) = &self.value_coding {
            let system = coding.system.clone().unwrap_or_default();
            let code = coding.code.clone().unwrap_or_default();
            return format!("{system}{CODING_SEPARATOR}{code}");
        }
        if let Some(text) = &self.value_string {
            return text.clone();
        }
        if let Some(number) = self.value_integer {
            return number.to_string();
        }
        if let Some(flag) = self.value_boolean {
            return flag.to_string();
        }
        if let Some(moment) = &self.value_date_time {
            return moment.clone();
        }
        self.value_decimal
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    }
}

/// A `ConceptMap` as the editor reads it back off the server.
///
/// Every element that is spelled differently per version is read under both
/// spellings, because a resource is read from whichever root the reader is
/// looking through and the form holds one fact rather than one per release
/// (<https://hl7.org/fhir/R4B/conceptmap.html>,
/// <https://hl7.org/fhir/R5/conceptmap.html>). The resource itself travels
/// beside this, so what the screen does not draw is still what is sent back.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredConceptMap {
    /// `id`, the logical id an update addresses.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// `meta`, which carries the version an update states.
    #[serde(default)]
    pub(crate) meta: Option<Meta>,
    /// `ConceptMap.url`, the canonical.
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// `ConceptMap.version`, the business version.
    #[serde(default)]
    pub(crate) version: Option<String>,
    /// `ConceptMap.status`, a code of the publication status value set.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// `sourceUri`, which R4 and R4B scope a map with.
    #[serde(default, rename = "sourceUri")]
    source_uri: Option<String>,
    /// `sourceCanonical`, the other arm of that choice.
    #[serde(default, rename = "sourceCanonical")]
    source_canonical: Option<String>,
    /// `sourceScopeUri`, which R5 and R6 scope a map with.
    #[serde(default, rename = "sourceScopeUri")]
    source_scope_uri: Option<String>,
    /// `sourceScopeCanonical`, the other arm of that choice.
    #[serde(default, rename = "sourceScopeCanonical")]
    source_scope_canonical: Option<String>,
    /// `targetUri`.
    #[serde(default, rename = "targetUri")]
    target_uri: Option<String>,
    /// `targetCanonical`.
    #[serde(default, rename = "targetCanonical")]
    target_canonical: Option<String>,
    /// `targetScopeUri`.
    #[serde(default, rename = "targetScopeUri")]
    target_scope_uri: Option<String>,
    /// `targetScopeCanonical`.
    #[serde(default, rename = "targetScopeCanonical")]
    target_scope_canonical: Option<String>,
    /// `ConceptMap.group`, the pairs of systems it maps between.
    #[serde(default)]
    pub(crate) group: Vec<StoredMapGroup>,
}

/// One `ConceptMap.group`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredMapGroup {
    /// `group.source`, a `uri` in R4 and R4B and a `canonical` in R5 and R6.
    #[serde(default)]
    pub(crate) source: Option<String>,
    /// `group.sourceVersion`, which R4 and R4B state beside the system.
    #[serde(default, rename = "sourceVersion")]
    pub(crate) source_version: Option<String>,
    /// `group.target`.
    #[serde(default)]
    pub(crate) target: Option<String>,
    /// `group.targetVersion`.
    #[serde(default, rename = "targetVersion")]
    pub(crate) target_version: Option<String>,
    /// `group.element`, the codes this group maps.
    #[serde(default)]
    pub(crate) element: Vec<StoredMapElement>,
}

/// One `group.element`: a source code and what it maps to.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredMapElement {
    /// `element.code`.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// `element.display`.
    #[serde(default)]
    pub(crate) display: Option<String>,
    /// `element.noMap`, which R5 and R6 define and R4 and R4B do not.
    #[serde(default, rename = "noMap")]
    pub(crate) no_map: Option<bool>,
    /// `element.target`.
    #[serde(default)]
    pub(crate) target: Vec<StoredMapTarget>,
}

/// One `element.target`: a code and its relationship to the source.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct StoredMapTarget {
    /// `target.code`.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// `target.display`.
    #[serde(default)]
    pub(crate) display: Option<String>,
    /// `target.equivalence`, which R4 and R4B state the relation in.
    #[serde(default)]
    pub(crate) equivalence: Option<String>,
    /// `target.relationship`, which R5 and R6 state it in.
    #[serde(default)]
    pub(crate) relationship: Option<String>,
    /// `target.comment`.
    #[serde(default)]
    pub(crate) comment: Option<String>,
}

impl StoredConceptMap {
    /// The elements the editor draws, read out of `resource`.
    pub(crate) fn of(resource: &Value) -> Self {
        serde_json::from_value(resource.clone()).unwrap_or_default()
    }

    /// The version an update of this resource states in `If-Match`.
    pub(crate) fn version_id(&self) -> Option<String> {
        self.meta.as_ref()?.version_id.clone()
    }

    /// What the map's source codes are drawn from, whichever arm stated it.
    pub(crate) fn source_scope(&self) -> Option<&str> {
        first_stated([
            &self.source_scope_canonical,
            &self.source_scope_uri,
            &self.source_canonical,
            &self.source_uri,
        ])
    }

    /// What the map's target codes are drawn from, whichever arm stated it.
    pub(crate) fn target_scope(&self) -> Option<&str> {
        first_stated([
            &self.target_scope_canonical,
            &self.target_scope_uri,
            &self.target_canonical,
            &self.target_uri,
        ])
    }
}

/// The first of `arms` the resource stated, in the order given.
///
/// A `value[x]` choice carries at most one arm
/// (<https://hl7.org/fhir/R4B/formats.html#choice>), so the order only decides
/// which release's spelling is preferred when a document carries two.
fn first_stated(arms: [&Option<String>; 4]) -> Option<&str> {
    arms.into_iter().flatten().map(String::as_str).next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_code_system_reads_the_elements_an_update_needs() {
        let answer: AuthoredSearch = serde_json::from_str(
            r#"{"resourceType":"Bundle","entry":[
                 {"resource":{"resourceType":"OperationOutcome"}},
                 {"resource":{"resourceType":"CodeSystem","id":"colours",
                   "meta":{"versionId":"4"},
                   "url":"https://terminology.example/colours","version":"1.0.0",
                   "status":"draft","content":"complete","caseSensitive":true,
                   "property":[{"code":"status","uri":"http://hl7.org/fhir/concept-properties#status","type":"code"}],
                   "concept":[{"code":"red","display":"Red","definition":"The colour red",
                     "designation":[{"language":"nl-NL","use":{"system":"https://terms.example/use","code":"syn"},"value":"Rood"}],
                     "property":[{"code":"status","valueCode":"active"}]}]}}]}"#,
        )
        .expect("the server's own answer parses");
        let stored =
            StoredCodeSystem::of(&answer.first().expect("the bundle carries a code system"));
        assert_eq!(stored.id.as_deref(), Some("colours"));
        assert_eq!(
            stored.meta.and_then(|meta| meta.version_id).as_deref(),
            Some("4"),
            "the version an If-Match states comes off the resource"
        );
        assert_eq!(stored.case_sensitive, Some(true));
        assert_eq!(stored.property.len(), 1);
        assert_eq!(stored.concept.len(), 1);
    }

    #[test]
    fn an_answer_carrying_no_code_system_reads_as_none() {
        let answer: AuthoredSearch = serde_json::from_str(
            r#"{"resourceType":"Bundle","entry":[{"resource":{"resourceType":"OperationOutcome"}}]}"#,
        )
        .expect("the server's own answer parses");
        assert_eq!(answer.first(), None);
        assert_eq!(AuthoredSearch::default().first(), None);
    }

    /// Every arm of `property.value[x]` reads back as the text its control
    /// holds, so a value the editor did not write is not dropped on a save.
    #[test]
    fn every_property_value_type_reads_as_the_text_its_control_holds() {
        for (json, expected) in [
            (r#"{"code":"p","valueCode":"active"}"#, "active"),
            (
                r#"{"code":"p","valueCoding":{"system":"https://x.example/s","code":"c"}}"#,
                "https://x.example/s|c",
            ),
            (r#"{"code":"p","valueString":"a phrase"}"#, "a phrase"),
            (r#"{"code":"p","valueInteger":-3}"#, "-3"),
            (r#"{"code":"p","valueBoolean":true}"#, "true"),
            (r#"{"code":"p","valueDateTime":"2026-09-24"}"#, "2026-09-24"),
            (r#"{"code":"p","valueDecimal":1.5}"#, "1.5"),
            (r#"{"code":"p"}"#, ""),
        ] {
            let value: StoredValue = serde_json::from_str(json).expect("the arm parses");
            assert_eq!(value.text(), expected, "{json}");
        }
    }

    #[test]
    fn a_searchset_answers_the_first_resource_of_the_type_that_was_searched_for() {
        let answer: AuthoredSearch = serde_json::from_str(
            r#"{"resourceType":"Bundle","entry":[
                 {"resource":{"resourceType":"OperationOutcome"}},
                 {"resource":{"resourceType":"ConceptMap","id":"m"}}]}"#,
        )
        .expect("the server's own answer parses");
        assert_eq!(
            answer
                .first_of("ConceptMap")
                .and_then(|held| held.get("id").and_then(Value::as_str).map(str::to_owned)),
            Some(String::from("m"))
        );
        assert_eq!(
            answer.first(),
            None,
            "a search for one type does not answer another"
        );
    }

    /// A map's scope is `source[x]` on R4 and R4B and `sourceScope[x]` on R5
    /// and R6 (<https://hl7.org/fhir/R4B/conceptmap.html>,
    /// <https://hl7.org/fhir/R5/conceptmap.html>), so the reader takes
    /// whichever arm the resource stated.
    #[test]
    fn a_stored_concept_map_reads_the_scope_under_either_release_s_spelling() {
        let r4b: StoredConceptMap = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","id":"m","meta":{"versionId":"2"},
                "url":"https://terminology.example/m","version":"1","status":"active",
                "sourceUri":"https://terminology.example/vs-a",
                "targetCanonical":"https://terminology.example/vs-b",
                "group":[{"source":"https://terminology.example/a","sourceVersion":"1.0",
                  "target":"https://terminology.example/b",
                  "element":[{"code":"x","display":"Ex","target":[
                    {"code":"y","display":"Why","equivalence":"wider","comment":"c"}]}]}]}"#,
        )
        .expect("the server's own answer parses");
        assert_eq!(r4b.version_id().as_deref(), Some("2"));
        assert_eq!(r4b.source_scope(), Some("https://terminology.example/vs-a"));
        assert_eq!(r4b.target_scope(), Some("https://terminology.example/vs-b"));
        let group = r4b.group.first().expect("the map carries one group");
        assert_eq!(group.source_version.as_deref(), Some("1.0"));
        let element = group.element.first().expect("the group carries one code");
        assert_eq!(element.no_map, None, "R4B defines no `noMap`");
        let target = element.target.first().expect("the code carries one target");
        assert_eq!(target.equivalence.as_deref(), Some("wider"));
        assert_eq!(target.relationship, None);

        let r5: StoredConceptMap = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","url":"https://terminology.example/m","status":"draft",
                "sourceScopeCanonical":"https://terminology.example/vs-a",
                "group":[{"source":"https://terminology.example/a|1.0",
                  "target":"https://terminology.example/b",
                  "element":[{"code":"x","noMap":true}]}]}"#,
        )
        .expect("the server's own answer parses");
        assert_eq!(r5.source_scope(), Some("https://terminology.example/vs-a"));
        assert_eq!(r5.target_scope(), None);
        assert_eq!(
            r5.group
                .first()
                .and_then(|group| group.element.first())
                .and_then(|element| element.no_map),
            Some(true)
        );
    }
}
