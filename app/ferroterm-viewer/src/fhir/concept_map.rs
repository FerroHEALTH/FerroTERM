// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The part of a published `ConceptMap` the concept map screen renders.
//!
//! `GET [base]/ConceptMap` is the search interaction and `GET
//! [base]/ConceptMap/{id}` the read (<https://hl7.org/fhir/R4B/http.html>).
//! The scope elements were renamed between FHIR versions: R4 and R4B carry
//! `source[x]` and `target[x]`, and R5 renamed them `sourceScope[x]` and
//! `targetScope[x]` (<https://hl7.org/fhir/R5/conceptmap.html>). Both spellings
//! are read here, so one screen draws a map from any of the four roots without
//! claiming one version's element name for another.

use serde::{Deserialize, Serialize};

use crate::fhir::facts::Fact;
use crate::fhir::facts::counted;
use crate::fhir::facts::fact;
use crate::fhir::facts::first_named;
use crate::fhir::facts::flag;
use crate::fhir::facts::named;
use crate::fhir::searchset::Published;
use crate::locale::Msg;

/// The `ConceptMap` a root publishes, as the screen draws it.
///
/// Every field is optional so a server that omits one still renders. The
/// viewer carries the elements it draws and nothing else: it never mirrors the
/// whole resource, and it never translates a code in the browser.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PublishedConceptMap {
    /// `resourceType`, which says what the entry actually carries.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// `ConceptMap.id`, the id the read interaction addresses.
    id: Option<String>,
    /// `ConceptMap.url`, the canonical `$translate` takes as `url`.
    url: Option<String>,
    /// `ConceptMap.version`, the business version of the resource.
    version: Option<String>,
    /// `ConceptMap.name`, the computer-friendly name.
    name: Option<String>,
    /// `ConceptMap.title`, the name written for a person.
    title: Option<String>,
    /// `ConceptMap.status`, the publication status.
    status: Option<String>,
    /// `ConceptMap.experimental`.
    experimental: Option<bool>,
    /// `ConceptMap.date`, when the resource was last changed.
    date: Option<String>,
    /// `ConceptMap.publisher`, who published it.
    publisher: Option<String>,
    /// `ConceptMap.description`.
    description: Option<String>,
    /// `ConceptMap.purpose`, why the map was written.
    purpose: Option<String>,
    /// `source[x]` as a plain URI, which R4 and R4B name.
    #[serde(rename = "sourceUri")]
    source_uri: Option<String>,
    /// `source[x]` as a canonical, which R4 and R4B name.
    #[serde(rename = "sourceCanonical")]
    source_canonical: Option<String>,
    /// `sourceScope[x]` as a plain URI, which R5 and the R6 ballot name.
    #[serde(rename = "sourceScopeUri")]
    source_scope_uri: Option<String>,
    /// `sourceScope[x]` as a canonical, which R5 and the R6 ballot name.
    #[serde(rename = "sourceScopeCanonical")]
    source_scope_canonical: Option<String>,
    /// `target[x]` as a plain URI, which R4 and R4B name.
    #[serde(rename = "targetUri")]
    target_uri: Option<String>,
    /// `target[x]` as a canonical, which R4 and R4B name.
    #[serde(rename = "targetCanonical")]
    target_canonical: Option<String>,
    /// `targetScope[x]` as a plain URI, which R5 and the R6 ballot name.
    #[serde(rename = "targetScopeUri")]
    target_scope_uri: Option<String>,
    /// `targetScope[x]` as a canonical, which R5 and the R6 ballot name.
    #[serde(rename = "targetScopeCanonical")]
    target_scope_canonical: Option<String>,
    /// `ConceptMap.group`, one per source and target code system pair.
    #[serde(default)]
    group: Vec<Group>,
}

/// One `ConceptMap.group`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Group {
    /// The code system the mapped codes come from.
    source: Option<String>,
    /// The code system the mapped codes go to.
    target: Option<String>,
    /// The mappings, counted rather than listed.
    #[serde(default)]
    element: Vec<GroupElement>,
}

/// One `ConceptMap.group.element`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct GroupElement {
    /// The source code the element maps.
    code: Option<String>,
    /// The source code's display.
    display: Option<String>,
    /// What the code maps to.
    #[serde(default)]
    target: Vec<ElementTarget>,
}

/// One `ConceptMap.group.element.target`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ElementTarget {
    /// The target code.
    code: Option<String>,
    /// The target code's display.
    display: Option<String>,
    /// `equivalence`, which R4 and R4B name.
    equivalence: Option<String>,
    /// `relationship`, which R5 renamed it to
    /// (<https://hl7.org/fhir/R5/conceptmap.html>).
    relationship: Option<String>,
    /// The publisher's comment on the mapping.
    comment: Option<String>,
}

/// One mapping of a concept map: a source code, what it maps to and how.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MappingRow {
    /// The code system the source code belongs to.
    pub(crate) source_system: Option<String>,
    /// The source code.
    pub(crate) source_code: String,
    /// The source code's display, when the map wrote one.
    pub(crate) source_display: Option<String>,
    /// The code system the target code belongs to.
    pub(crate) target_system: Option<String>,
    /// The target code, absent for a code the map says maps to nothing.
    pub(crate) target_code: Option<String>,
    /// The target code's display, when the map wrote one.
    pub(crate) target_display: Option<String>,
    /// How the two relate, under whichever element the version names.
    pub(crate) relationship: Option<String>,
    /// The publisher's comment, when it wrote one.
    pub(crate) comment: Option<String>,
}

/// One group of a concept map, as one row of the screen.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GroupRow {
    /// The code system the mapped codes come from.
    pub(crate) source: Option<String>,
    /// The code system the mapped codes go to.
    pub(crate) target: Option<String>,
    /// How many source codes the group maps.
    pub(crate) elements: u32,
}

impl Published for PublishedConceptMap {
    const RESOURCE_TYPE: &'static str = "ConceptMap";

    fn declared_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }
}

impl PublishedConceptMap {
    /// The id the read interaction addresses, when the resource carries one.
    pub(crate) fn id(&self) -> Option<&str> {
        named(self.id.as_deref())
    }

    /// The canonical `$translate` takes as `url`, when one is declared.
    pub(crate) fn url(&self) -> Option<&str> {
        named(self.url.as_deref())
    }

    /// The business version of the resource, when it declared one.
    pub(crate) fn version(&self) -> Option<&str> {
        named(self.version.as_deref())
    }

    /// The publication status, when it declared one.
    pub(crate) fn status(&self) -> Option<&str> {
        named(self.status.as_deref())
    }

    /// The name a reader recognises: the title, else the name.
    pub(crate) fn label(&self) -> Option<&str> {
        named(self.title.as_deref()).or_else(|| named(self.name.as_deref()))
    }

    /// What the map translates from, under whichever element names it.
    pub(crate) fn source_scope(&self) -> Option<&str> {
        first_named(&[
            self.source_scope_uri.as_deref(),
            self.source_scope_canonical.as_deref(),
            self.source_uri.as_deref(),
            self.source_canonical.as_deref(),
        ])
    }

    /// What the map translates to, under whichever element names it.
    pub(crate) fn target_scope(&self) -> Option<&str> {
        first_named(&[
            self.target_scope_uri.as_deref(),
            self.target_scope_canonical.as_deref(),
            self.target_uri.as_deref(),
            self.target_canonical.as_deref(),
        ])
    }

    /// The code system the translate form is prefilled from, when there is one.
    ///
    /// A map whose groups all name one source system says which system its
    /// codes come from; one that spans several does not, so the form is left
    /// for the reader rather than filled with a guess.
    pub(crate) fn only_source_system(&self) -> Option<String> {
        let mut systems = self.groups().into_iter().filter_map(|group| group.source);
        let first = systems.next()?;
        systems.all(|other| other == first).then_some(first)
    }

    /// The facts the screen draws, in a fixed order.
    pub(crate) fn facts(&self) -> Vec<Fact> {
        vec![
            fact(Msg::Name, self.name.as_deref()),
            fact(Msg::Title, self.title.as_deref()),
            fact(Msg::Version, self.version.as_deref()),
            fact(Msg::Status, self.status.as_deref()),
            fact(Msg::Experimental, flag(self.experimental)),
            fact(Msg::Publisher, self.publisher.as_deref()),
            fact(Msg::LastChanged, self.date.as_deref()),
            fact(Msg::Description2, self.description.as_deref()),
            fact(Msg::Purpose, self.purpose.as_deref()),
            fact(Msg::TranslatesFrom, self.source_scope()),
            fact(Msg::TranslatesTo, self.target_scope()),
            fact(Msg::Groups, counted(self.group_count()).as_deref()),
        ]
    }

    /// The groups the map is made of, in the order it declares them.
    pub(crate) fn groups(&self) -> Vec<GroupRow> {
        self.group
            .iter()
            .map(|group| GroupRow {
                source: named(group.source.as_deref()).map(str::to_owned),
                target: named(group.target.as_deref()).map(str::to_owned),
                elements: u32::try_from(
                    group
                        .element
                        .iter()
                        .filter(|element| element.code.is_some())
                        .count(),
                )
                .unwrap_or(u32::MAX),
            })
            .collect()
    }

    /// Every mapping the map writes, one row per target, in the order the
    /// groups and elements declare them; a source code with no target is one
    /// row saying so.
    pub(crate) fn mappings(&self) -> Vec<MappingRow> {
        let mut rows = Vec::new();
        for group in &self.group {
            let source_system = named(group.source.as_deref()).map(str::to_owned);
            let target_system = named(group.target.as_deref()).map(str::to_owned);
            for element in &group.element {
                let Some(source_code) = named(element.code.as_deref()) else {
                    continue;
                };
                let source = MappingRow {
                    source_system: source_system.clone(),
                    source_code: source_code.to_owned(),
                    source_display: named(element.display.as_deref()).map(str::to_owned),
                    target_system: target_system.clone(),
                    ..MappingRow::default()
                };
                if element.target.is_empty() {
                    rows.push(source);
                    continue;
                }
                for target in &element.target {
                    rows.push(MappingRow {
                        target_code: named(target.code.as_deref()).map(str::to_owned),
                        target_display: named(target.display.as_deref()).map(str::to_owned),
                        relationship: first_named(&[
                            target.relationship.as_deref(),
                            target.equivalence.as_deref(),
                        ])
                        .map(str::to_owned),
                        comment: named(target.comment.as_deref()).map(str::to_owned),
                        ..source.clone()
                    });
                }
            }
        }
        rows
    }

    /// The description, when it declared one.
    pub(crate) fn description(&self) -> Option<&str> {
        named(self.description.as_deref())
    }

    /// How many groups the map declares.
    fn group_count(&self) -> Option<u32> {
        u32::try_from(self.group.len()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::searchset::SearchSet;

    #[test]
    fn every_target_is_a_mapping_under_either_name_of_the_relationship() {
        let r4 = parse(
            r#"{"resourceType":"ConceptMap","group":[{"source":"urn:a","target":"urn:b",
              "element":[{"code":"x","display":"Ex","target":[
                {"code":"p","equivalence":"equivalent"},{"code":"q","equivalence":"wider"}]},
                {"code":"y"}]}]}"#,
        );
        let rows = r4.mappings();
        assert_eq!(rows.len(), 3, "two targets and one unmapped code");
        assert_eq!(rows[0].target_code.as_deref(), Some("p"));
        assert_eq!(rows[0].relationship.as_deref(), Some("equivalent"));
        assert_eq!(rows[0].source_display.as_deref(), Some("Ex"));
        assert_eq!(rows[2].target_code, None);
        let r5 = parse(
            r#"{"resourceType":"ConceptMap","group":[{"element":[{"code":"x","target":[
                {"code":"p","relationship":"equivalent"}]}]}]}"#,
        );
        assert_eq!(r5.mappings()[0].relationship.as_deref(), Some("equivalent"));
    }

    fn parse(json: &str) -> PublishedConceptMap {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    fn value_of(resource: &PublishedConceptMap, label: &str) -> Option<String> {
        resource
            .facts()
            .into_iter()
            .find(|fact| crate::locale::tr(fact.label) == label)
            .expect("the screen draws a row for every fact it names")
            .value
    }

    /// A map written the way an R4 or R4B publisher writes one.
    const R4_SHAPE: &str = r#"{"resourceType":"ConceptMap","id":"map-1",
        "url":"https://terminology.example/ConceptMap/animals","version":"1",
        "name":"AnimalMap","title":"Animal map","status":"active","experimental":false,
        "date":"2031-01-01","publisher":"Example Publisher","description":"Maps animals.",
        "sourceUri":"https://terminology.example/ValueSet/a",
        "targetUri":"https://terminology.example/ValueSet/b",
        "group":[{"source":"https://terminology.example/a","target":"https://terminology.example/b",
          "element":[{"code":"x"},{"code":"y"}]}]}"#;

    /// The same map written the way an R5 or R6 publisher writes one.
    const R5_SHAPE: &str = r#"{"resourceType":"ConceptMap","id":"map-2",
        "url":"https://terminology.example/ConceptMap/animals","status":"active",
        "sourceScopeCanonical":"https://terminology.example/ValueSet/a",
        "targetScopeCanonical":"https://terminology.example/ValueSet/b",
        "group":[{"source":"https://terminology.example/a","element":[{"code":"x"}]}]}"#;

    #[test]
    fn the_resource_is_read_by_the_id_the_read_interaction_addresses() {
        let resource = parse(R4_SHAPE);
        assert_eq!(resource.id(), Some("map-1"));
        assert_eq!(
            resource.url(),
            Some("https://terminology.example/ConceptMap/animals")
        );
        assert_eq!(resource.version(), Some("1"));
        assert_eq!(resource.status(), Some("active"));
        assert_eq!(resource.label(), Some("Animal map"));
    }

    #[test]
    fn the_r4_scope_elements_and_the_r5_ones_read_the_same_way() {
        let r4 = parse(R4_SHAPE);
        let r5 = parse(R5_SHAPE);
        assert_eq!(
            r4.source_scope(),
            Some("https://terminology.example/ValueSet/a"),
            "R4 and R4B name it source[x]"
        );
        assert_eq!(
            r5.source_scope(),
            Some("https://terminology.example/ValueSet/a"),
            "R5 and the R6 ballot renamed it sourceScope[x]"
        );
        assert_eq!(r4.target_scope(), r5.target_scope());
    }

    #[test]
    fn every_fact_the_screen_names_is_read_from_the_resource() {
        let resource = parse(R4_SHAPE);
        assert_eq!(value_of(&resource, "Title"), Some("Animal map".into()));
        assert_eq!(value_of(&resource, "Experimental"), Some("no".into()));
        assert_eq!(value_of(&resource, "Groups"), Some("1".into()));
        assert_eq!(
            value_of(&resource, "Translates to"),
            Some("https://terminology.example/ValueSet/b".into())
        );
    }

    #[test]
    fn a_fact_the_resource_omits_still_gets_its_row() {
        let resource = parse(R5_SHAPE);
        assert_eq!(value_of(&resource, "Purpose"), None);
        assert!(
            resource
                .facts()
                .iter()
                .any(|fact| crate::locale::tr(fact.label) == "Purpose"),
            "the row is drawn so the screen can state the absence"
        );
    }

    #[test]
    fn the_facts_render_in_the_same_order_for_every_resource() {
        let rich: Vec<Msg> = parse(R4_SHAPE)
            .facts()
            .into_iter()
            .map(|f| f.label)
            .collect();
        let bare: Vec<Msg> = PublishedConceptMap::default()
            .facts()
            .into_iter()
            .map(|f| f.label)
            .collect();
        assert_eq!(
            rich, bare,
            "two resources put the same term in the same row"
        );
        assert!(!bare.is_empty(), "a passing comparison means something");
    }

    #[test]
    fn the_groups_are_read_in_the_order_the_map_declares_them() {
        let resource = parse(
            r#"{"resourceType":"ConceptMap","group":[
                {"source":"https://x.example/b","element":[{"code":"1"}]},
                {"source":"https://x.example/a","target":"https://x.example/c",
                 "element":[{"code":"1"},{"code":"2"},{"code":"3"}]}]}"#,
        );
        let groups = resource.groups();
        assert_eq!(
            groups.first().and_then(|group| group.source.clone()),
            Some("https://x.example/b".to_owned()),
            "a client that re-ordered the groups would hide the map's own order"
        );
        assert_eq!(groups.get(1).map(|group| group.elements), Some(3));
        assert_eq!(
            groups.first().and_then(|group| group.target.clone()),
            None,
            "a group that names no target says so rather than borrowing another's"
        );
    }

    #[test]
    fn a_map_whose_groups_share_one_source_system_names_it_for_the_form() {
        assert_eq!(
            parse(R4_SHAPE).only_source_system(),
            Some("https://terminology.example/a".to_owned())
        );
    }

    #[test]
    fn a_map_spanning_several_source_systems_names_none_rather_than_guessing() {
        let resource = parse(
            r#"{"resourceType":"ConceptMap","group":[
                {"source":"https://x.example/a"},{"source":"https://x.example/b"}]}"#,
        );
        assert_eq!(
            resource.only_source_system(),
            None,
            "the form is left for the reader rather than filled with one of two answers"
        );
    }

    #[test]
    fn a_map_with_no_group_names_no_source_system() {
        assert_eq!(
            parse(r#"{"resourceType":"ConceptMap"}"#).only_source_system(),
            None
        );
    }

    #[test]
    fn an_invented_map_renders_with_no_change_here() {
        let resource = parse(
            r#"{"resourceType":"ConceptMap","url":"https://invented.example/cm",
                "status":"draft","group":[{"source":"https://invented.example/cs",
                  "target":"https://invented.example/other","element":[{"code":"q"}]}]}"#,
        );
        assert_eq!(value_of(&resource, "Status"), Some("draft".into()));
        assert_eq!(resource.groups().len(), 1);
    }

    #[test]
    fn the_rest_of_the_resource_is_ignored_rather_than_re_modelled() {
        let resource = parse(
            r#"{"resourceType":"ConceptMap","url":"https://x.example/cm",
                "group":[{"source":"https://x.example/a","unmapped":{"mode":"fixed","code":"z"},
                  "element":[{"code":"x","target":[{"code":"y","equivalence":"equivalent"}]}]}]}"#,
        );
        assert_eq!(resource.url(), Some("https://x.example/cm"));
        assert_eq!(
            resource.groups().first().map(|group| group.elements),
            Some(1),
            "the viewer carries the fields it renders and nothing more"
        );
    }

    #[test]
    fn a_searchset_of_concept_maps_reads_through_the_shared_envelope() {
        let found: SearchSet<PublishedConceptMap> = serde_json::from_str(&format!(
            r#"{{"resourceType":"Bundle","type":"searchset","total":1,
                "entry":[{{"resource":{R4_SHAPE}}}]}}"#
        ))
        .expect("the fixture is valid JSON");
        assert_eq!(found.matched(), 1);
        assert_eq!(
            found.found().first().and_then(|resource| resource.id()),
            Some("map-1")
        );
    }
}
