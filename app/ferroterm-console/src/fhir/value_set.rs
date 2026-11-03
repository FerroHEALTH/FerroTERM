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

use crate::fhir::facts::Fact;
use crate::fhir::facts::fact;
use crate::fhir::facts::flag;
use crate::fhir::facts::named;
use crate::fhir::release::Reads;
use crate::fhir::searchset::{Found, carries};
use crate::locale::Msg;

/// The `ValueSet` a root publishes, as the screen draws it, read from the
/// `ValueSet` its release decoded.
///
/// The screen reads these elements and nothing else, and never expands a
/// value set itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PublishedValueSet {
    /// `ValueSet.id`, the id the read interaction addresses.
    id: Option<String>,
    /// `meta.versionId`, which a resource written through the API carries.
    version_id: Option<String>,
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
    /// `compose.inactive`, whether inactive codes are in the selection.
    inactive: Option<bool>,
    /// `compose.lockedDate`, the date the selection is fixed to.
    locked_date: Option<String>,
    /// The clauses of `compose`, includes before excludes; empty for a value
    /// set the server holds as a built selection, which carries no
    /// `compose`.
    clauses: Vec<ClauseRow>,
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
    /// The codes the clause names, each with the display it gives.
    pub(crate) concepts: Vec<(String, Option<String>)>,
    /// The filters the clause selects with, each its property, operator and
    /// value.
    pub(crate) filter_parts: Vec<(String, String, String)>,
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
            fact(Msg::InactiveCodesSelected, flag(self.inactive)),
            fact(Msg::LockedToDate, self.locked_date.as_deref()),
        ]
    }

    /// The clauses the definition is made of, includes before excludes.
    ///
    /// A value set the server holds as a built selection carries no `compose`
    /// at all, which is an ordinary answer rather than a missing one.
    pub(crate) fn clauses(&self) -> Vec<ClauseRow> {
        self.clauses.clone()
    }
}

/// One clause read off `compose`: the parts of its filters, which a filter
/// at R4 and R4B always states and one from R5 on may leave without a value.
fn clause(
    included: bool,
    system: Option<&str>,
    version: Option<&str>,
    value_sets: &[String],
    concepts: &[(Option<String>, Option<String>)],
    filters: &[(Option<String>, Option<String>, Option<String>)],
) -> ClauseRow {
    let text = |value: &Option<String>| named(value.as_deref()).map(str::to_owned);
    ClauseRow {
        included,
        system: named(system).map(str::to_owned),
        version: named(version).map(str::to_owned),
        value_sets: value_sets
            .iter()
            .filter_map(|url| named(Some(url)))
            .map(str::to_owned)
            .collect(),
        codes: u32::try_from(concepts.iter().filter(|(code, _)| code.is_some()).count())
            .unwrap_or(u32::MAX),
        // NOTE: no specification governs this: our own design; a filter missing a
        // part selects nothing a reader can predict, so the part is stated as `?`.
        filters: filters
            .iter()
            .map(|(property, op, value)| {
                let part =
                    |value: &Option<String>| text(value).unwrap_or_else(|| String::from("?"));
                format!("{} {} {}", part(property), part(op), part(value))
            })
            .collect(),
        concepts: concepts
            .iter()
            .filter_map(|(code, display)| Some((text(code)?, text(display))))
            .collect(),
        filter_parts: filters
            .iter()
            .map(|(property, op, value)| {
                let part = |value: &Option<String>| text(value).unwrap_or_default();
                (part(property), part(op), part(value))
            })
            .collect(),
    }
}

/// Reads the `ValueSet` of the `fhir-types` module `$module` as published;
/// `$value` reads a filter's `value`, which R4 and R4B require and R5 and R6
/// make optional.
macro_rules! reads_value_set {
    ($module:ident, $value:expr) => {
        impl Reads<PublishedValueSet> for fhir_types::$module::value_set::ValueSet {
            fn read(&self) -> PublishedValueSet {
                use fhir_types::$module::value_set::{
                    ValueSetComposeInclude, ValueSetComposeIncludeFilter,
                };
                let filter_value: fn(&ValueSetComposeIncludeFilter) -> Option<String> = $value;
                let text = |value: Option<&fhir_types::$module::primitives::String>| {
                    value.and_then(|value| value.value.clone())
                };
                let read = |set: &ValueSetComposeInclude, included: bool| {
                    clause(
                        included,
                        set.system
                            .as_ref()
                            .and_then(|system| system.value.as_deref()),
                        set.version
                            .as_ref()
                            .and_then(|version| version.value.as_deref()),
                        &set.value_set
                            .iter()
                            .filter_map(|url| url.value.clone())
                            .collect::<Vec<_>>(),
                        &set.concept
                            .iter()
                            .map(|concept| {
                                (concept.code.value.clone(), text(concept.display.as_ref()))
                            })
                            .collect::<Vec<_>>(),
                        &set.filter
                            .iter()
                            .map(|filter| {
                                (
                                    filter.property.value.clone(),
                                    filter.op.value.clone(),
                                    filter_value(filter),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                };
                let compose = self.compose.as_ref();
                PublishedValueSet {
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
                    immutable: self.immutable.as_ref().and_then(|flag| flag.value),
                    inactive: compose
                        .and_then(|compose| compose.inactive.as_ref())
                        .and_then(|flag| flag.value),
                    locked_date: compose
                        .and_then(|compose| compose.locked_date.as_ref())
                        .and_then(|date| date.value.clone()),
                    clauses: compose
                        .map(|compose| {
                            compose
                                .include
                                .iter()
                                .map(|set| read(set, true))
                                .chain(compose.exclude.iter().map(|set| read(set, false)))
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            }
        }

        carries!($module, ValueSet, PublishedValueSet);
    };
}

reads_value_set!(r4, |filter| filter.value.value.clone());
reads_value_set!(r4b, |filter| filter.value.value.clone());
reads_value_set!(r5, |filter| filter.value.value.clone());
reads_value_set!(r6, |filter| filter
    .value
    .as_ref()
    .and_then(|value| value.value.clone()));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::release::fixture::completed;
    use crate::fhir::version::FhirVersion;

    /// `json`, decoded as the R4B `ValueSet` its required elements complete.
    fn parse(json: &str) -> PublishedValueSet {
        FhirVersion::R4B
            .decode(&completed("ValueSet", json))
            .expect("the fixture is a valid R4B ValueSet")
    }

    fn value_of(resource: &PublishedValueSet, label: &str) -> Option<String> {
        resource
            .facts()
            .into_iter()
            .find(|fact| {
                crate::locale::tr(&ferrohealth_ui::i18n::Locale::default(), fact.label) == label
            })
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
            resource.facts().iter().any(|fact| crate::locale::tr(
                &ferrohealth_ui::i18n::Locale::default(),
                fact.label
            ) == "Purpose"),
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
        let resource: PublishedValueSet = FhirVersion::R6
            .decode(&completed(
                "ValueSet",
                r#"{"compose":{"include":[
                    {"system":"https://x.example/s","filter":[{"property":"concept","op":"exists"}]}]}}"#,
            ))
            .expect("R6 lets a filter leave out its value");
        assert_eq!(
            resource
                .clauses()
                .first()
                .map(|clause| clause.filters.clone()),
            Some(vec!["concept exists ?".to_owned()]),
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
                "text":{"status":"generated","div":"<div xmlns=\"http://www.w3.org/1999/xhtml\"><p/></div>"}}"#,
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
        let found: Found<PublishedValueSet> = FhirVersion::R4B
            .decode(&format!(
                r#"{{"resourceType":"Bundle","type":"searchset","total":1,
                    "entry":[{{"resource":{ONE}}}]}}"#
            ))
            .expect("the fixture is a valid R4B searchset");
        assert_eq!(found.found().len(), 1);
        assert_eq!(
            found.found().first().and_then(|resource| resource.id()),
            Some("example-vs")
        );
    }
}
