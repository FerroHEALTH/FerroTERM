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

use crate::fhir::facts::Fact;
use crate::fhir::facts::counted;
use crate::fhir::facts::fact;
use crate::fhir::facts::flag;
use crate::fhir::facts::named;
use crate::fhir::release::Reads;
use crate::fhir::searchset::{Found, carries};
use crate::locale::Msg;

/// The `ConceptMap` a root publishes, as the screen draws it, read from the
/// `ConceptMap` its release decoded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PublishedConceptMap {
    /// `ConceptMap.id`, the id the read interaction addresses.
    id: Option<String>,
    /// `meta.versionId`, which a resource written through the API carries.
    version_id: Option<String>,
    /// `ConceptMap.url`.
    url: Option<String>,
    /// `ConceptMap.version`.
    version: Option<String>,
    /// `ConceptMap.name`.
    name: Option<String>,
    /// `ConceptMap.title`.
    title: Option<String>,
    /// `ConceptMap.status`.
    status: Option<String>,
    /// `ConceptMap.experimental`.
    experimental: Option<bool>,
    /// `ConceptMap.date`.
    date: Option<String>,
    /// `ConceptMap.publisher`.
    publisher: Option<String>,
    /// `ConceptMap.description`.
    description: Option<String>,
    /// `ConceptMap.purpose`.
    purpose: Option<String>,
    /// The scope it translates from: `source[x]` up to R4B, `sourceScope[x]`
    /// from R5 (<https://hl7.org/fhir/R5/conceptmap.html>).
    source_scope: Option<String>,
    /// The scope it translates to: `target[x]` up to R4B, `targetScope[x]`
    /// from R5.
    target_scope: Option<String>,
    /// The groups, in the order the map declares them.
    groups: Vec<GroupRow>,
    /// Every mapping, a source code with no target among them.
    mappings: Vec<MappingRow>,
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

    /// The version the server holds of a resource written through the API;
    /// `None` for one the deployment loaded, which has no write path.
    pub(crate) fn stored(&self) -> Option<&str> {
        named(self.version_id.as_deref())
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
        named(self.source_scope.as_deref())
    }

    /// The scope the map translates to, whichever element its release names
    /// it in.
    pub(crate) fn target_scope(&self) -> Option<&str> {
        named(self.target_scope.as_deref())
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
        self.groups.clone()
    }

    /// Every mapping the map declares, in its own order: one row per
    /// target, and a row with no target for a source code the map leaves
    /// unmapped.
    pub(crate) fn mappings(&self) -> Vec<MappingRow> {
        self.mappings.clone()
    }

    fn group_count(&self) -> Option<u32> {
        u32::try_from(self.groups.len()).ok()
    }
}

/// One group's elements as `(code, display, targets)`, each target
/// `(code, display, relationship, comment)`.
type Elements = Vec<(Option<String>, Option<String>, Vec<Targeted>)>;

/// One target as `(code, display, relationship, comment)`.
type Targeted = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// The group row and the mappings of one group over `source` and `target`.
fn group(
    source: Option<&str>,
    target: Option<&str>,
    elements: &Elements,
) -> (GroupRow, Vec<MappingRow>) {
    let source_system = named(source).map(str::to_owned);
    let target_system = named(target).map(str::to_owned);
    let mut rows = Vec::new();
    for (code, display, targets) in elements {
        let Some(source_code) = named(code.as_deref()) else {
            continue;
        };
        let row = MappingRow {
            source_system: source_system.clone(),
            source_code: source_code.to_owned(),
            source_display: named(display.as_deref()).map(str::to_owned),
            target_system: target_system.clone(),
            ..MappingRow::default()
        };
        if targets.is_empty() {
            rows.push(row);
            continue;
        }
        for (code, display, relationship, comment) in targets {
            rows.push(MappingRow {
                target_code: named(code.as_deref()).map(str::to_owned),
                target_display: named(display.as_deref()).map(str::to_owned),
                relationship: named(relationship.as_deref()).map(str::to_owned),
                comment: named(comment.as_deref()).map(str::to_owned),
                ..row.clone()
            });
        }
    }
    let row = GroupRow {
        source: source_system,
        target: target_system,
        elements: u32::try_from(
            elements
                .iter()
                .filter(|(code, _, _)| code.is_some())
                .count(),
        )
        .unwrap_or(u32::MAX),
    };
    (row, rows)
}

/// Reads the `ConceptMap` of the `fhir-types` module `$module`: `$scope`
/// reads its source and target scopes, `source[x]` and `target[x]` up to R4B
/// and `sourceScope[x]` and `targetScope[x]` from R5, and `$relationship`
/// names a target's `equivalence` up to R4B and its `relationship` from R5.
macro_rules! reads_concept_map {
    ($module:ident, $scope:expr, $relationship:ident) => {
        impl Reads<PublishedConceptMap> for fhir_types::$module::concept_map::ConceptMap {
            fn read(&self) -> PublishedConceptMap {
                use fhir_types::$module::concept_map::ConceptMap;
                let scope: fn(&ConceptMap) -> (Option<String>, Option<String>) = $scope;
                let text = |value: Option<&fhir_types::$module::primitives::String>| {
                    value.and_then(|value| value.value.clone())
                };
                let (source_scope, target_scope) = scope(self);
                let mut groups = Vec::new();
                let mut mappings = Vec::new();
                for declared in &self.group {
                    let elements: Elements = declared
                        .element
                        .iter()
                        .map(|element| {
                            (
                                element.code.as_ref().and_then(|code| code.value.clone()),
                                text(element.display.as_ref()),
                                element
                                    .target
                                    .iter()
                                    .map(|target| {
                                        (
                                            target
                                                .code
                                                .as_ref()
                                                .and_then(|code| code.value.clone()),
                                            text(target.display.as_ref()),
                                            target.$relationship.value.clone(),
                                            text(target.comment.as_ref()),
                                        )
                                    })
                                    .collect(),
                            )
                        })
                        .collect();
                    let (row, rows) = group(
                        declared
                            .source
                            .as_ref()
                            .and_then(|source| source.value.as_deref()),
                        declared
                            .target
                            .as_ref()
                            .and_then(|target| target.value.as_deref()),
                        &elements,
                    );
                    groups.push(row);
                    mappings.extend(rows);
                }
                PublishedConceptMap {
                    id: self.id.clone(),
                    version_id: self
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.version_id.as_ref())
                        .and_then(|id| id.value.clone()),
                    url: self.url.as_ref().and_then(|url| url.value.clone()),
                    version: text(self.version.as_ref()),
                    name: text(self.name.as_ref()),
                    title: text(self.title.as_ref()),
                    status: self.status.value.clone(),
                    experimental: self.experimental.as_ref().and_then(|flag| flag.value),
                    date: self.date.as_ref().and_then(|date| date.value.clone()),
                    publisher: text(self.publisher.as_ref()),
                    description: self
                        .description
                        .as_ref()
                        .and_then(|text| text.value.clone()),
                    purpose: self.purpose.as_ref().and_then(|text| text.value.clone()),
                    source_scope,
                    target_scope,
                    groups,
                    mappings,
                }
            }
        }

        carries!($module, ConceptMap, PublishedConceptMap);
    };
}

/// The scopes of an R4 or R4B map, `source[x]` and `target[x]`.
macro_rules! r4_scope {
    ($module:ident) => {
        |map| {
            use fhir_types::$module::concept_map::{ConceptMapSource, ConceptMapTarget};
            (
                match &map.source {
                    Some(ConceptMapSource::Uri(uri)) => uri.value.clone(),
                    Some(ConceptMapSource::Canonical(canonical)) => canonical.value.clone(),
                    None => None,
                },
                match &map.target {
                    Some(ConceptMapTarget::Uri(uri)) => uri.value.clone(),
                    Some(ConceptMapTarget::Canonical(canonical)) => canonical.value.clone(),
                    None => None,
                },
            )
        }
    };
}

/// The scopes of an R5 or R6 map, `sourceScope[x]` and `targetScope[x]`.
macro_rules! r5_scope {
    ($module:ident) => {
        |map| {
            use fhir_types::$module::concept_map::{ConceptMapSourceScope, ConceptMapTargetScope};
            (
                match &map.source_scope {
                    Some(ConceptMapSourceScope::Uri(uri)) => uri.value.clone(),
                    Some(ConceptMapSourceScope::Canonical(canonical)) => canonical.value.clone(),
                    None => None,
                },
                match &map.target_scope {
                    Some(ConceptMapTargetScope::Uri(uri)) => uri.value.clone(),
                    Some(ConceptMapTargetScope::Canonical(canonical)) => canonical.value.clone(),
                    None => None,
                },
            )
        }
    };
}

reads_concept_map!(r4, r4_scope!(r4), equivalence);
reads_concept_map!(r4b, r4_scope!(r4b), equivalence);
reads_concept_map!(r5, r5_scope!(r5), relationship);
reads_concept_map!(r6, r5_scope!(r6), relationship);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::release::fixture::completed;
    use crate::fhir::version::FhirVersion;

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
        let r5 = parse_r5(
            r#"{"resourceType":"ConceptMap","group":[{"element":[{"code":"x","target":[
                {"code":"p","relationship":"equivalent"}]}]}]}"#,
        );
        assert_eq!(r5.mappings()[0].relationship.as_deref(), Some("equivalent"));
    }

    /// `json`, decoded as the R4B `ConceptMap` its required elements
    /// complete.
    fn parse(json: &str) -> PublishedConceptMap {
        FhirVersion::R4B
            .decode(&completed("ConceptMap", json))
            .expect("the fixture is a valid R4B ConceptMap")
    }

    /// `json`, decoded as the R5 `ConceptMap` its required elements complete.
    fn parse_r5(json: &str) -> PublishedConceptMap {
        FhirVersion::R5
            .decode(&completed("ConceptMap", json))
            .expect("the fixture is a valid R5 ConceptMap")
    }

    fn value_of(resource: &PublishedConceptMap, label: &str) -> Option<String> {
        resource
            .facts()
            .into_iter()
            .find(|fact| {
                crate::locale::tr(&ferrohealth_ui::i18n::Locale::default(), fact.label) == label
            })
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
        let r5 = parse_r5(R5_SHAPE);
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
        let resource = parse_r5(R5_SHAPE);
        assert_eq!(value_of(&resource, "Purpose"), None);
        assert!(
            resource.facts().iter().any(|fact| crate::locale::tr(
                &ferrohealth_ui::i18n::Locale::default(),
                fact.label
            ) == "Purpose"),
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
                {"source":"https://x.example/a","element":[{"code":"x"}]},
                {"source":"https://x.example/b","element":[{"code":"y"}]}]}"#,
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
        let found: Found<PublishedConceptMap> = FhirVersion::R4B
            .decode(&format!(
                r#"{{"resourceType":"Bundle","type":"searchset","total":1,
                    "entry":[{{"resource":{R4_SHAPE}}}]}}"#
            ))
            .expect("the fixture is a valid R4B searchset");
        assert_eq!(found.found().len(), 1);
        assert_eq!(
            found.found().first().and_then(|resource| resource.id()),
            Some("map-1")
        );
    }
}
