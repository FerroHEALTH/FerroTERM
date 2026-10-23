// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The part of a published `CodeSystem` the detail screen renders.
//!
//! `GET [base]/CodeSystem?url=` is the RESTful search interaction
//! (<https://hl7.org/fhir/R4B/http.html#search>) over the canonical every
//! definitional resource carries, and it answers a `searchset` `Bundle`. That
//! document says what the code system *is*, where
//! `TerminologyCapabilities` says what this server can *do* with it, so the
//! screen reads both and keeps them apart. Reading lives here, outside every
//! component, so the screen renders a value plain unit tests can pin.

use serde::{Deserialize, Serialize};

/// The resource type an entry must declare to be drawn as a code system.
///
/// `resourceType` is mandatory in FHIR JSON
/// (<https://hl7.org/fhir/R4B/json.html>), so an entry that declares another
/// type, or none, is something else the server put in the bundle.
const CODE_SYSTEM: &str = "CodeSystem";

/// What `GET [base]/CodeSystem?url=` answered.
///
/// Every field is optional so a server that omits one still renders. The
/// viewer carries the elements it draws and nothing else: it never mirrors the
/// whole resource.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct CodeSystemSearch {
    /// `Bundle.total`, the number of matches the server counted.
    total: Option<u32>,
    /// `Bundle.entry`, one per match.
    #[serde(default)]
    entry: Vec<SearchEntry>,
}

/// One `Bundle.entry`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct SearchEntry {
    /// The resource the entry carries.
    resource: Option<PublishedCodeSystem>,
}

/// The `CodeSystem` a root publishes, as the detail screen draws it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PublishedCodeSystem {
    /// `resourceType`, which says what the entry actually carries.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// `CodeSystem.id`, the id the read and the history address.
    id: Option<String>,
    /// `meta`, whose `versionId` says the resource was written through the
    /// API.
    meta: Option<crate::fhir::named::Meta>,
    /// `CodeSystem.property`, the properties the system declares.
    #[serde(default)]
    property: Vec<DeclaredProperty>,
    /// `CodeSystem.url`, the canonical the search asked for.
    url: Option<String>,
    /// `CodeSystem.version`, the business version of the resource.
    version: Option<String>,
    /// `CodeSystem.name`, the computer-friendly name.
    name: Option<String>,
    /// `CodeSystem.title`, the name written for a person.
    title: Option<String>,
    /// `CodeSystem.status`, the publication status.
    status: Option<String>,
    /// `CodeSystem.experimental`.
    experimental: Option<bool>,
    /// `CodeSystem.date`, when the resource was last changed.
    date: Option<String>,
    /// `CodeSystem.publisher`, who published it.
    publisher: Option<String>,
    /// `CodeSystem.description`.
    description: Option<String>,
    /// `CodeSystem.caseSensitive`.
    #[serde(rename = "caseSensitive")]
    case_sensitive: Option<bool>,
    /// `CodeSystem.hierarchyMeaning`, what a parent-child edge means here.
    #[serde(rename = "hierarchyMeaning")]
    hierarchy_meaning: Option<String>,
    /// `CodeSystem.compositional`, whether the system has a grammar.
    compositional: Option<bool>,
    /// `CodeSystem.versionNeeded`, whether a code means nothing on its own.
    #[serde(rename = "versionNeeded")]
    version_needed: Option<bool>,
    /// `CodeSystem.content`, how much of the system this resource carries.
    content: Option<String>,
    /// `CodeSystem.supplements`, the system this one supplements.
    supplements: Option<String>,
    /// `CodeSystem.count`, the number of concepts the system defines.
    count: Option<u32>,
}

/// One `CodeSystem.property`: a property the system declares for its
/// concepts.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct DeclaredProperty {
    /// `property.code`, how a concept names it.
    pub(crate) code: Option<String>,
    /// `property.type`, the type of its values.
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
    /// `property.description`, what it means.
    pub(crate) description: Option<String>,
    /// `property.uri`, the property it stands for.
    pub(crate) uri: Option<String>,
}

impl CodeSystemSearch {
    /// The published code systems the answer carries, in the server's order.
    ///
    /// An entry carrying something other than a `CodeSystem` is passed over
    /// rather than drawn as one: a `searchset` may also carry an
    /// `OperationOutcome` describing the search itself
    /// (<https://hl7.org/fhir/R4B/http.html#search>).
    pub(crate) fn published(&self) -> Vec<PublishedCodeSystem> {
        self.carried().cloned().collect()
    }

    /// The `CodeSystem` resources the entries carry, borrowed.
    fn carried(&self) -> impl Iterator<Item = &PublishedCodeSystem> {
        self.entry
            .iter()
            .filter_map(|entry| entry.resource.as_ref())
            .filter(|resource| resource.resource_type.as_deref() == Some(CODE_SYSTEM))
    }
}

impl PublishedCodeSystem {
    /// The name a person reads: the title, else the computer-friendly name.
    pub(crate) fn title(&self) -> Option<&str> {
        named(self.title.as_deref()).or_else(|| named(self.name.as_deref()))
    }

    /// The publication status, when it declared one.
    pub(crate) fn status(&self) -> Option<&str> {
        named(self.status.as_deref())
    }

    /// The logical id, when the server sent one.
    pub(crate) fn id(&self) -> Option<&str> {
        named(self.id.as_deref())
    }

    /// The version the server holds of a resource written through the API;
    /// `None` for one the deployment loaded, which has no write path.
    pub(crate) fn stored(&self) -> Option<&str> {
        self.meta
            .as_ref()
            .and_then(crate::fhir::named::Meta::stored)
    }

    /// The properties the system declares for its concepts.
    pub(crate) fn properties(&self) -> &[DeclaredProperty] {
        &self.property
    }

    /// The computer-friendly name, when it declared one.
    pub(crate) fn name(&self) -> Option<&str> {
        named(self.name.as_deref())
    }

    /// Whether it is experimental, when it said.
    pub(crate) fn experimental(&self) -> Option<bool> {
        self.experimental
    }

    /// Who published it, when it said.
    pub(crate) fn publisher(&self) -> Option<&str> {
        named(self.publisher.as_deref())
    }

    /// When it was last changed, when it said.
    pub(crate) fn date(&self) -> Option<&str> {
        named(self.date.as_deref())
    }

    /// Whether its codes are case sensitive, when it said.
    pub(crate) fn case_sensitive(&self) -> Option<bool> {
        self.case_sensitive
    }

    /// What a parent-child edge means, when it said.
    pub(crate) fn hierarchy_meaning(&self) -> Option<&str> {
        named(self.hierarchy_meaning.as_deref())
    }

    /// Whether it has a compositional grammar, when it said.
    pub(crate) fn compositional(&self) -> Option<bool> {
        self.compositional
    }

    /// Whether a code means nothing without its version, when it said.
    pub(crate) fn version_needed(&self) -> Option<bool> {
        self.version_needed
    }

    /// How much of the system the resource carries, when it said.
    pub(crate) fn content(&self) -> Option<&str> {
        named(self.content.as_deref())
    }

    /// The system it supplements, when it is a supplement.
    pub(crate) fn supplements(&self) -> Option<&str> {
        named(self.supplements.as_deref())
    }

    /// How many concepts the system defines, when it said.
    pub(crate) fn count(&self) -> Option<u32> {
        self.count
    }

    /// The description, when it declared one.
    pub(crate) fn description(&self) -> Option<&str> {
        named(self.description.as_deref())
    }

    /// The business version of the resource, when it declared one.
    pub(crate) fn version(&self) -> Option<&str> {
        named(self.version.as_deref())
    }
}

/// The trimmed text, or `None` when it names nothing.
fn named(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|trimmed| !trimmed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the screen reads for the fact `label` names, as text.
    fn value_of(resource: &PublishedCodeSystem, label: &str) -> Option<String> {
        let word = |flag: Option<bool>| {
            flag.map(|flag| {
                if flag {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                }
            })
        };
        match label {
            "Title" => resource.title().map(str::to_owned),
            "Status" => resource.status().map(str::to_owned),
            "Publisher" => resource.publisher().map(str::to_owned),
            "Content" => resource.content().map(str::to_owned),
            "A code needs its version" => word(resource.version_needed()),
            "Experimental" => word(resource.experimental()),
            "Compositional grammar" => word(resource.compositional()),
            "Concepts defined" => resource.count().map(|count| count.to_string()),
            "Version" => resource.version().map(str::to_owned),
            "Hierarchy meaning" => resource.hierarchy_meaning().map(str::to_owned),
            "Supplements" => resource.supplements().map(str::to_owned),
            other => panic!("no fact is named {other}"),
        }
    }

    fn parse(json: &str) -> CodeSystemSearch {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    /// A `searchset` shaped the way the server answers one.
    const ONE_MATCH: &str = r#"{"resourceType":"Bundle","type":"searchset","total":1,
        "entry":[{"fullUrl":"CodeSystem/x","search":{"mode":"match"},
          "resource":{"resourceType":"CodeSystem",
            "url":"https://terminology.example/x","version":"2031-01-01",
            "name":"ExampleSystem","title":"The example system","status":"active",
            "experimental":false,"date":"2031-01-01","publisher":"Example Publisher",
            "description":"A system used to exercise the reader.",
            "caseSensitive":true,"hierarchyMeaning":"is-a","compositional":false,
            "versionNeeded":true,"content":"not-present","count":42}}]}"#;

    #[test]
    fn a_published_resource_is_read_from_the_searchset_entry() {
        let search = parse(ONE_MATCH);
        assert_eq!(search.total, Some(1));
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(
            resource.url.as_deref(),
            Some("https://terminology.example/x")
        );
        assert_eq!(resource.version(), Some("2031-01-01"));
    }

    #[test]
    fn every_fact_the_screen_names_is_read_from_the_resource() {
        let published = parse(ONE_MATCH).published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(
            value_of(resource, "Title"),
            Some("The example system".into())
        );
        assert_eq!(value_of(resource, "Status"), Some("active".into()));
        assert_eq!(
            value_of(resource, "Publisher"),
            Some("Example Publisher".into())
        );
        assert_eq!(value_of(resource, "Content"), Some("not-present".into()));
        assert_eq!(
            value_of(resource, "A code needs its version"),
            Some("yes".into()),
            "a declared boolean renders as a word, never as a colour alone"
        );
        assert_eq!(value_of(resource, "Experimental"), Some("no".into()));
        assert_eq!(value_of(resource, "Concepts defined"), Some("42".into()));
    }

    #[test]
    fn a_fact_the_resource_omits_reads_as_absent_rather_than_as_false() {
        let search = parse(
            r#"{"entry":[{"resource":{"resourceType":"CodeSystem",
                "url":"https://terminology.example/bare","status":"active"}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(
            value_of(resource, "Compositional grammar"),
            None,
            "an undeclared boolean is not the same claim as a declared `no`"
        );
        assert_eq!(value_of(resource, "Publisher"), None);
        assert_eq!(value_of(resource, "Concepts defined"), None);
    }

    #[test]
    fn an_entry_that_carries_another_resource_is_not_drawn_as_a_code_system() {
        let search = parse(
            r#"{"resourceType":"Bundle","type":"searchset","total":0,
                "entry":[{"search":{"mode":"outcome"},
                  "resource":{"resourceType":"OperationOutcome",
                    "issue":[{"severity":"warning","code":"processing"}]}}]}"#,
        );
        assert!(
            search.published().is_empty(),
            "the screen then states that the root publishes no resource"
        );
        assert_eq!(search.total, Some(0));
    }

    #[test]
    fn the_count_the_live_region_reads_is_the_number_of_resources_drawn() {
        let search = parse(ONE_MATCH);
        assert_eq!(
            search.published().len(),
            search.published().len(),
            "the announced count and the drawn blocks are the same answer"
        );
    }

    #[test]
    fn an_empty_searchset_reads_as_no_published_resource() {
        let search = parse(r#"{"resourceType":"Bundle","type":"searchset","total":0}"#);
        assert_eq!(search.published(), Vec::<PublishedCodeSystem>::new());
        assert_eq!(search.total, Some(0));
    }

    #[test]
    fn a_bundle_that_counts_nothing_still_renders_its_entries() {
        let search = parse(
            r#"{"resourceType":"Bundle","type":"searchset",
                "entry":[{"resource":{"resourceType":"CodeSystem",
                  "url":"https://terminology.example/y"}}]}"#,
        );
        assert_eq!(
            search.total, None,
            "the screen states the absence rather than inventing a count"
        );
        assert_eq!(search.published().len(), 1);
    }

    #[test]
    fn a_version_declared_as_the_empty_string_names_nothing() {
        let search = parse(
            r#"{"entry":[{"resource":{"resourceType":"CodeSystem",
                "url":"https://terminology.example/w","version":"  "}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(
            resource.version(),
            None,
            "the screen states the absence rather than drawing an empty heading"
        );
        assert_eq!(value_of(resource, "Version"), None);
    }

    #[test]
    fn an_invented_code_system_renders_with_no_change_here() {
        let search = parse(
            r#"{"resourceType":"Bundle","type":"searchset","total":1,"entry":[
                {"resource":{"resourceType":"CodeSystem",
                  "url":"https://terminology.example/invented","status":"draft",
                  "content":"fragment","hierarchyMeaning":"grouped-by",
                  "supplements":"https://terminology.example/base"}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(value_of(resource, "Status"), Some("draft".into()));
        assert_eq!(
            value_of(resource, "Hierarchy meaning"),
            Some("grouped-by".into()),
            "a system this server has never served before needs no code here"
        );
        assert_eq!(
            value_of(resource, "Supplements"),
            Some("https://terminology.example/base".into())
        );
    }

    #[test]
    fn the_rest_of_the_resource_is_ignored_rather_than_re_modelled() {
        let search = parse(
            r#"{"entry":[{"resource":{"resourceType":"CodeSystem",
                "url":"https://terminology.example/v","status":"active",
                "concept":[{"code":"a","display":"A"}],
                "property":[{"code":"p","type":"code"}],
                "filter":[{"code":"concept","operator":["is-a"],"value":"a code"}]}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(
            resource.url.as_deref(),
            Some("https://terminology.example/v")
        );
        assert_eq!(
            value_of(resource, "Status"),
            Some("active".into()),
            "the viewer carries the fields it renders and nothing more"
        );
    }

    #[test]
    fn the_header_facts_and_declared_properties_are_read_from_the_resource() {
        let search = parse(
            r#"{"entry":[{"resource":{"resourceType":"CodeSystem","id":"colours",
                "url":"https://terminology.example/colours","name":"Colours",
                "date":"2031-02-03","caseSensitive":false,"hierarchyMeaning":"part-of",
                "meta":{"versionId":"4"},
                "property":[{"code":"hue","type":"string","description":"The hue.",
                  "uri":"https://terminology.example/hue"}]}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(Some("colours"), resource.id());
        assert_eq!(Some("Colours"), resource.name());
        assert_eq!(
            Some("Colours"),
            resource.title(),
            "a system with no title is read by its name"
        );
        assert_eq!(Some("2031-02-03"), resource.date());
        assert_eq!(Some(false), resource.case_sensitive());
        assert_eq!(Some("part-of"), resource.hierarchy_meaning());
        assert_eq!(
            Some("4"),
            resource.stored(),
            "a system written through the API names the version the server holds"
        );
        assert_eq!(
            vec![DeclaredProperty {
                code: Some(String::from("hue")),
                kind: Some(String::from("string")),
                description: Some(String::from("The hue.")),
                uri: Some(String::from("https://terminology.example/hue")),
            }],
            resource.properties()
        );
    }

    #[test]
    fn a_loaded_system_has_no_stored_version_and_blank_facts_read_as_absent() {
        let search = parse(
            r#"{"entry":[{"resource":{"resourceType":"CodeSystem","id":" ",
                "url":"https://terminology.example/loaded","name":" ","date":""}}]}"#,
        );
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(None, resource.stored());
        assert_eq!(None, resource.id());
        assert_eq!(None, resource.name());
        assert_eq!(None, resource.date());
        assert_eq!(None, resource.case_sensitive());
        assert_eq!(Vec::<DeclaredProperty>::new(), resource.properties());
    }
}
