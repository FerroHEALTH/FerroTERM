// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The part of a published `ValueSet` the value set screen renders.
//!
//! `GET [base]/ValueSet` is the search interaction and `GET
//! [base]/ValueSet/{id}` the read (<https://hl7.org/fhir/R4B/http.html>). Both
//! answer the resource as its publisher wrote it, which says what the value
//! set *is*; expanding it is a separate operation and a separate screen.
//! Reading lives here, outside every component, so the screen renders a value
//! plain unit tests can pin.

use serde::{Deserialize, Serialize};

use crate::fhir::facts::Fact;
use crate::fhir::facts::fact;
use crate::fhir::facts::flag;
use crate::fhir::facts::named;
use crate::fhir::searchset::Published;
use crate::locale::Msg;

/// The `ValueSet` a root publishes, as the screen draws it.
///
/// Every field is optional so a server that omits one still renders. The
/// viewer carries the elements it draws and nothing else: it never mirrors the
/// whole resource, and it never expands one in the browser.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct PublishedValueSet {
    /// `resourceType`, which says what the entry actually carries.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// `ValueSet.id`, the id the read interaction addresses.
    id: Option<String>,
    /// `ValueSet.url`, the canonical the expansion runner takes.
    url: Option<String>,
    /// `ValueSet.version`, the business version of the resource.
    version: Option<String>,
    /// `ValueSet.name`, the computer-friendly name.
    name: Option<String>,
    /// `ValueSet.title`, the name written for a person.
    title: Option<String>,
    /// `ValueSet.status`, the publication status.
    status: Option<String>,
    /// `ValueSet.experimental`.
    experimental: Option<bool>,
    /// `ValueSet.date`, when the resource was last changed.
    date: Option<String>,
    /// `ValueSet.publisher`, who published it.
    publisher: Option<String>,
    /// `ValueSet.description`.
    description: Option<String>,
    /// `ValueSet.purpose`, why the value set was written.
    purpose: Option<String>,
    /// `ValueSet.immutable`, whether the content can never change.
    immutable: Option<bool>,
    /// `ValueSet.compose`, the selection the definition makes.
    compose: Option<Compose>,
}

/// `ValueSet.compose`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
struct Compose {
    /// `compose.include`, the selections drawn in.
    #[serde(default)]
    include: Vec<ComposeSet>,
    /// `compose.exclude`, the selections taken back out.
    #[serde(default)]
    exclude: Vec<ComposeSet>,
    /// `compose.inactive`, whether inactive codes are in the selection.
    inactive: Option<bool>,
    /// `compose.lockedDate`, the date the selection is fixed to.
    #[serde(rename = "lockedDate")]
    locked_date: Option<String>,
}

/// One `compose.include` or `compose.exclude`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ComposeSet {
    /// `system`, the code system the codes come from.
    system: Option<String>,
    /// `version`, the code system version the selection is pinned to.
    version: Option<String>,
    /// `valueSet`, the value sets this clause draws in whole.
    #[serde(default, rename = "valueSet")]
    value_set: Vec<String>,
    /// `concept`, the codes the clause names one by one.
    #[serde(default)]
    concept: Vec<ComposeConcept>,
    /// `filter`, the filters the clause selects with.
    #[serde(default)]
    filter: Vec<ComposeFilter>,
}

/// One `compose.include.concept`, counted rather than listed.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ComposeConcept {
    /// The code the clause names.
    code: Option<String>,
}

/// One `compose.include.filter`, as the screen states it.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ComposeFilter {
    /// The property the filter is over.
    property: Option<String>,
    /// The operator, for example `is-a` or `in`.
    op: Option<String>,
    /// The value the operator is applied with.
    value: Option<String>,
}

/// One clause of a value set's definition, as one line of the screen.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClauseRow {
    /// Whether the clause draws codes in or takes them back out.
    pub(crate) included: bool,
    /// The code system the clause is over, absent where it names none.
    pub(crate) system: Option<String>,
    /// The code system version the clause is pinned to.
    pub(crate) version: Option<String>,
    /// The value sets the clause draws in whole.
    pub(crate) value_sets: Vec<String>,
    /// How many codes the clause names one by one.
    pub(crate) codes: u32,
    /// The filters the clause selects with, each as one sentence.
    pub(crate) filters: Vec<String>,
}

impl Published for PublishedValueSet {
    const RESOURCE_TYPE: &str = "ValueSet";

    fn declared_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }
}

impl PublishedValueSet {
    /// The id the read interaction addresses, when the resource carries one.
    pub(crate) fn id(&self) -> Option<&str> {
        named(self.id.as_deref())
    }

    /// The canonical the expansion runner takes, when one is declared.
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

    /// The description, when it declared one.
    pub(crate) fn description(&self) -> Option<&str> {
        named(self.description.as_deref())
    }

    /// The name a reader recognises: the title, else the name.
    ///
    /// `title` is "a short, descriptive, user-friendly title" and `name` is
    /// the computer-friendly one
    /// (<https://hl7.org/fhir/R4B/valueset.html>), so the title wins where the
    /// publisher wrote one.
    pub(crate) fn label(&self) -> Option<&str> {
        named(self.title.as_deref()).or_else(|| named(self.name.as_deref()))
    }

    /// The facts the screen draws, in a fixed order.
    ///
    /// The order is written here rather than derived from the document, so two
    /// published resources render their terms in the same rows and a fact the
    /// server omitted still gets a row that states the absence.
    pub(crate) fn facts(&self) -> Vec<Fact> {
        let compose = self.compose.clone().unwrap_or_default();
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
            fact(Msg::Immutable, flag(self.immutable)),
            fact(Msg::InactiveCodesSelected, flag(compose.inactive)),
            fact(Msg::LockedToDate, compose.locked_date.as_deref()),
        ]
    }

    /// The clauses the definition is made of, includes before excludes.
    ///
    /// A value set the server holds as a built selection carries no `compose`
    /// at all, which is an ordinary answer rather than a missing one.
    pub(crate) fn clauses(&self) -> Vec<ClauseRow> {
        let Some(compose) = self.compose.as_ref() else {
            return Vec::new();
        };
        let read = |set: &ComposeSet, included: bool| ClauseRow {
            included,
            system: named(set.system.as_deref()).map(str::to_owned),
            version: named(set.version.as_deref()).map(str::to_owned),
            value_sets: set
                .value_set
                .iter()
                .filter_map(|url| named(Some(url)))
                .map(str::to_owned)
                .collect(),
            codes: u32::try_from(set.concept.iter().filter(|c| c.code.is_some()).count())
                .unwrap_or(u32::MAX),
            filters: set.filter.iter().map(filter_sentence).collect(),
        };
        compose
            .include
            .iter()
            .map(|set| read(set, true))
            .chain(compose.exclude.iter().map(|set| read(set, false)))
            .collect()
    }
}

/// One `compose` filter, as the sentence the screen shows.
///
/// The three parts are rendered as the publisher wrote them; a part the
/// resource omits is stated rather than guessed, because a filter missing its
/// operator selects nothing a reader can predict.
fn filter_sentence(filter: &ComposeFilter) -> String {
    let part = |value: Option<&str>| named(value).unwrap_or("?").to_owned();
    format!(
        "{} {} {}",
        part(filter.property.as_deref()),
        part(filter.op.as_deref()),
        part(filter.value.as_deref())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::searchset::SearchSet;

    fn parse(json: &str) -> PublishedValueSet {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    fn value_of(resource: &PublishedValueSet, label: &str) -> Option<String> {
        resource
            .facts()
            .into_iter()
            .find(|fact| crate::locale::tr(fact.label) == label)
            .expect("the screen draws a row for every fact it names")
            .value
    }

    /// A published value set shaped the way a publisher writes one.
    const ONE: &str = r#"{"resourceType":"ValueSet","id":"example-vs",
        "url":"https://terminology.example/ValueSet/animals","version":"2031-01-01",
        "name":"AnimalCodes","title":"Animal codes","status":"active",
        "experimental":false,"date":"2031-01-01","publisher":"Example Publisher",
        "description":"Codes for animals.","purpose":"Exercising the reader.",
        "immutable":true,
        "compose":{"inactive":false,"lockedDate":"2031-01-01",
          "include":[{"system":"https://terminology.example/animals","version":"1",
            "concept":[{"code":"a"},{"code":"b"}],
            "filter":[{"property":"concept","op":"is-a","value":"root"}]}],
          "exclude":[{"valueSet":["https://terminology.example/ValueSet/retired"]}]}}"#;

    #[test]
    fn the_resource_is_read_by_the_id_the_read_interaction_addresses() {
        let resource = parse(ONE);
        assert_eq!(resource.id(), Some("example-vs"));
        assert_eq!(
            resource.url(),
            Some("https://terminology.example/ValueSet/animals")
        );
        assert_eq!(resource.version(), Some("2031-01-01"));
        assert_eq!(resource.status(), Some("active"));
    }

    #[test]
    fn the_label_prefers_the_title_a_person_reads() {
        assert_eq!(parse(ONE).label(), Some("Animal codes"));
        assert_eq!(
            parse(r#"{"resourceType":"ValueSet","name":"AnimalCodes"}"#).label(),
            Some("AnimalCodes"),
            "a resource with no title falls back to the computer-friendly name"
        );
        assert_eq!(
            parse(r#"{"resourceType":"ValueSet","title":"  "}"#).label(),
            None,
            "a blank title names nothing, so the screen states the absence"
        );
    }

    #[test]
    fn every_fact_the_screen_names_is_read_from_the_resource() {
        let resource = parse(ONE);
        assert_eq!(value_of(&resource, "Title"), Some("Animal codes".into()));
        assert_eq!(value_of(&resource, "Status"), Some("active".into()));
        assert_eq!(value_of(&resource, "Experimental"), Some("no".into()));
        assert_eq!(value_of(&resource, "Immutable"), Some("yes".into()));
        assert_eq!(
            value_of(&resource, "Purpose"),
            Some("Exercising the reader.".into())
        );
        assert_eq!(
            value_of(&resource, "Inactive codes selected"),
            Some("no".into())
        );
        assert_eq!(
            value_of(&resource, "Locked to date"),
            Some("2031-01-01".into())
        );
    }

    #[test]
    fn a_fact_the_resource_omits_reads_as_absent_rather_than_as_false() {
        let resource = parse(r#"{"resourceType":"ValueSet","url":"https://x.example/v"}"#);
        assert_eq!(
            value_of(&resource, "Immutable"),
            None,
            "an undeclared boolean is not the same claim as a declared `no`"
        );
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
        let rich: Vec<Msg> = parse(ONE).facts().into_iter().map(|f| f.label).collect();
        let bare: Vec<Msg> = PublishedValueSet::default()
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
    fn the_definition_reads_as_clauses_with_the_includes_first() {
        let clauses = parse(ONE).clauses();
        assert_eq!(clauses.len(), 2);
        let first = clauses.first().expect("the definition includes one clause");
        assert!(first.included, "an include is drawn before an exclude");
        assert_eq!(
            first.system.as_deref(),
            Some("https://terminology.example/animals")
        );
        assert_eq!(first.version.as_deref(), Some("1"));
        assert_eq!(first.codes, 2, "the named codes are counted, not listed");
        assert_eq!(first.filters, ["concept is-a root".to_owned()]);
        let second = clauses.get(1).expect("the definition excludes one clause");
        assert!(!second.included);
        assert_eq!(
            second.value_sets,
            ["https://terminology.example/ValueSet/retired".to_owned()]
        );
    }

    #[test]
    fn a_filter_missing_a_part_says_so_rather_than_reading_as_complete() {
        let resource = parse(
            r#"{"resourceType":"ValueSet","compose":{"include":[
                {"system":"https://x.example/s","filter":[{"property":"concept"}]}]}}"#,
        );
        assert_eq!(
            resource
                .clauses()
                .first()
                .map(|clause| clause.filters.clone()),
            Some(vec!["concept ? ?".to_owned()]),
            "a filter with no operator selects nothing a reader can predict"
        );
    }

    #[test]
    fn a_value_set_with_no_compose_reads_as_no_clause_rather_than_as_empty_ones() {
        let resource = parse(r#"{"resourceType":"ValueSet","url":"https://x.example/v"}"#);
        assert!(
            resource.clauses().is_empty(),
            "a value set the server holds as a built selection carries no compose"
        );
    }

    #[test]
    fn an_invented_value_set_renders_with_no_change_here() {
        let resource = parse(
            r#"{"resourceType":"ValueSet","url":"https://invented.example/vs",
                "status":"draft","compose":{"include":[
                  {"system":"https://invented.example/cs",
                   "filter":[{"property":"parent","op":"in","value":"x,y"}]}]}}"#,
        );
        assert_eq!(value_of(&resource, "Status"), Some("draft".into()));
        assert_eq!(
            resource
                .clauses()
                .first()
                .map(|clause| clause.filters.len()),
            Some(1),
            "a system this server has never served before needs no code here"
        );
    }

    #[test]
    fn the_rest_of_the_resource_is_ignored_rather_than_re_modelled() {
        let resource = parse(
            r#"{"resourceType":"ValueSet","url":"https://x.example/v","status":"active",
                "expansion":{"total":9,"contains":[{"code":"a"}]},
                "text":{"status":"generated","div":"<p/>"}}"#,
        );
        assert_eq!(value_of(&resource, "Status"), Some("active".into()));
        assert_eq!(
            resource.url(),
            Some("https://x.example/v"),
            "the viewer carries the fields it renders and nothing more"
        );
    }

    #[test]
    fn a_searchset_of_value_sets_reads_through_the_shared_envelope() {
        let found: SearchSet<PublishedValueSet> = serde_json::from_str(&format!(
            r#"{{"resourceType":"Bundle","type":"searchset","total":1,
                "entry":[{{"resource":{ONE}}}]}}"#
        ))
        .expect("the fixture is valid JSON");
        assert_eq!(found.matched(), 1);
        assert_eq!(
            found.found().first().and_then(|resource| resource.id()),
            Some("example-vs")
        );
    }
}
