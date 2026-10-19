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

/// One fact of a published resource: a term and the value beside it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Fact {
    /// The term a reader reads.
    pub(crate) label: &'static str,
    /// The value, absent when the resource declared none.
    pub(crate) value: Option<String>,
}

impl CodeSystemSearch {
    /// The number of matches the server counted, when it counted them.
    pub(crate) fn total(&self) -> Option<u32> {
        self.total
    }

    /// The published code systems the answer carries, in the server's order.
    ///
    /// An entry carrying something other than a `CodeSystem` is passed over
    /// rather than drawn as one: a `searchset` may also carry an
    /// `OperationOutcome` describing the search itself
    /// (<https://hl7.org/fhir/R4B/http.html#search>).
    pub(crate) fn published(&self) -> Vec<PublishedCodeSystem> {
        self.carried().cloned().collect()
    }

    /// How many published code systems the answer carries.
    ///
    /// The live region needs the count alone, so it is counted rather than
    /// cloned out of the bundle a second time on every settle.
    pub(crate) fn matched(&self) -> usize {
        self.carried().count()
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

    /// The description, when it declared one.
    pub(crate) fn description(&self) -> Option<&str> {
        named(self.description.as_deref())
    }

    /// The business version of the resource, when it declared one.
    pub(crate) fn version(&self) -> Option<&str> {
        named(self.version.as_deref())
    }

    /// The canonical the resource declares, when it declared one.
    #[cfg(test)]
    pub(crate) fn url(&self) -> Option<&str> {
        named(self.url.as_deref())
    }

    /// The facts the screen draws, in a fixed order.
    ///
    /// The order is written here rather than derived from the document, so two
    /// published resources render their terms in the same rows and a fact the
    /// server omitted still gets a row that states the absence.
    pub(crate) fn facts(&self) -> Vec<Fact> {
        vec![
            fact("Name", self.name.as_deref()),
            fact("Title", self.title.as_deref()),
            fact("Version", self.version.as_deref()),
            fact("Status", self.status.as_deref()),
            fact("Experimental", flag(self.experimental)),
            fact("Publisher", self.publisher.as_deref()),
            fact("Last changed", self.date.as_deref()),
            fact("Description", self.description.as_deref()),
            fact("Content", self.content.as_deref()),
            fact("A code needs its version", flag(self.version_needed)),
            fact("Case sensitive", flag(self.case_sensitive)),
            fact("Hierarchy meaning", self.hierarchy_meaning.as_deref()),
            fact("Compositional grammar", flag(self.compositional)),
            fact("Supplements", self.supplements.as_deref()),
            fact("Concepts defined", counted(self.count).as_deref()),
        ]
    }
}

/// One fact, with a value that names nothing read as absent.
fn fact(label: &'static str, value: Option<&str>) -> Fact {
    Fact {
        label,
        value: named(value).map(str::to_owned),
    }
}

/// A declared count as the text a reader reads.
fn counted(count: Option<u32>) -> Option<String> {
    count.map(|count| count.to_string())
}

/// A declared boolean as the word a reader reads.
///
/// The word carries the meaning, so nothing here depends on a colour or a
/// glyph (<https://www.w3.org/TR/WCAG22/#use-of-color>).
fn flag(declared: Option<bool>) -> Option<&'static str> {
    declared.map(|value| if value { "yes" } else { "no" })
}

/// The trimmed text, or `None` when it names nothing.
fn named(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|trimmed| !trimmed.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> CodeSystemSearch {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    fn value_of(resource: &PublishedCodeSystem, label: &str) -> Option<String> {
        resource
            .facts()
            .into_iter()
            .find(|fact| fact.label == label)
            .expect("the screen draws a row for every fact it names")
            .value
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
        assert_eq!(search.total(), Some(1));
        let published = search.published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(resource.url(), Some("https://terminology.example/x"));
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
    fn a_fact_the_recorded_resource_omits_still_gets_its_row() {
        let published = parse(ONE_MATCH).published();
        let resource = published.first().expect("the bundle carries one match");
        assert_eq!(value_of(resource, "Supplements"), None);
        assert!(
            resource
                .facts()
                .iter()
                .any(|fact| fact.label == "Supplements"),
            "the row is drawn so the screen can state the absence"
        );
    }

    #[test]
    fn the_facts_render_in_the_same_order_for_every_resource() {
        let rich: Vec<&'static str> = parse(ONE_MATCH)
            .published()
            .iter()
            .flat_map(PublishedCodeSystem::facts)
            .map(|fact| fact.label)
            .collect();
        let bare: Vec<&'static str> = PublishedCodeSystem::default()
            .facts()
            .into_iter()
            .map(|fact| fact.label)
            .collect();
        assert_eq!(
            rich, bare,
            "two resources put the same term in the same row"
        );
        assert!(!bare.is_empty(), "a passing comparison means something");
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
        assert_eq!(search.total(), Some(0));
    }

    #[test]
    fn the_count_the_live_region_reads_is_the_number_of_resources_drawn() {
        let search = parse(ONE_MATCH);
        assert_eq!(
            search.matched(),
            search.published().len(),
            "the announced count and the drawn blocks are the same answer"
        );
    }

    #[test]
    fn an_empty_searchset_reads_as_no_published_resource() {
        let search = parse(r#"{"resourceType":"Bundle","type":"searchset","total":0}"#);
        assert!(search.published().is_empty());
        assert_eq!(search.total(), Some(0));
    }

    #[test]
    fn a_bundle_that_counts_nothing_still_renders_its_entries() {
        let search = parse(
            r#"{"resourceType":"Bundle","type":"searchset",
                "entry":[{"resource":{"resourceType":"CodeSystem",
                  "url":"https://terminology.example/y"}}]}"#,
        );
        assert_eq!(
            search.total(),
            None,
            "the screen states the absence rather than inventing a count"
        );
        assert_eq!(search.published().len(), 1);
    }

    #[test]
    fn two_versions_of_one_canonical_each_keep_their_own_facts() {
        let search = parse(
            r#"{"resourceType":"Bundle","type":"searchset","total":2,"entry":[
                {"resource":{"resourceType":"CodeSystem",
                  "url":"https://terminology.example/z","version":"1","status":"retired"}},
                {"resource":{"resourceType":"CodeSystem",
                  "url":"https://terminology.example/z","version":"2","status":"active"}}]}"#,
        );
        let published = search.published();
        let versions: Vec<Option<&str>> =
            published.iter().map(PublishedCodeSystem::version).collect();
        assert_eq!(
            versions,
            [Some("1"), Some("2")],
            "the server's order is kept, so the two blocks do not swap between reads"
        );
        assert_eq!(
            published
                .first()
                .and_then(|resource| value_of(resource, "Status")),
            Some("retired".into())
        );
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
        assert_eq!(resource.url(), Some("https://terminology.example/v"));
        assert_eq!(
            value_of(resource, "Status"),
            Some("active".into()),
            "the viewer carries the fields it renders and nothing more"
        );
    }
}
