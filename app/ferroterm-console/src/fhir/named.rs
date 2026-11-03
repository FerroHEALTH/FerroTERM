// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The name every published resource of one type carries.
//!
//! `GET [base]/{type}?_elements=url,name,title,version` is the RESTful search
//! interaction narrowed to four elements
//! (<https://hl7.org/fhir/R5/search.html#elements>). A published resource can
//! carry its whole content inline, so the whole search is a quarter of a
//! megabyte where this is four kilobytes, which is what makes it affordable
//! for a screen to offer a reader the resources this root holds.
//!
//! One type serves every definitional resource, because `url`, `name`,
//! `title` and `version` are the same four elements on `CodeSystem`,
//! `ValueSet` and `ConceptMap`.

use std::collections::BTreeMap;

use crate::fhir::release::Reads;
use crate::fhir::write::Stamp;

/// What the narrowed search answered: the stamp of each published resource
/// it carried, read from the `Bundle` its release decoded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NamedSearch {
    /// Each `CodeSystem`, `ValueSet` or `ConceptMap` the entries carry, in
    /// the server's order.
    published: Vec<Stamp>,
}

/// One published resource as a listing shows it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Listed {
    /// The logical id, empty when the server sent none.
    pub(crate) id: String,
    /// The canonical, empty when the resource carries none.
    pub(crate) url: String,
    /// The name a reader reads: the title, else the name, else the canonical.
    pub(crate) label: String,
    /// The business version, when the resource states one.
    pub(crate) version: Option<String>,
    /// The publication status, when the resource states one.
    pub(crate) status: Option<String>,
    /// The version the server holds of a resource written through the API;
    /// `None` for one the deployment loaded, which has no write path.
    pub(crate) stored: Option<String>,
}

/// One offer a picker makes: what a reader reads, and what the run carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Choice {
    /// The canonical the run sends.
    pub(crate) canonical: String,
    /// The name the reader picks it by.
    pub(crate) label: String,
}

impl NamedSearch {
    /// The name each canonical was published under.
    ///
    /// A resource the root published without a name of any kind is absent, so
    /// a caller that needs one for every canonical falls back to the canonical
    /// itself.
    pub(crate) fn names(&self) -> BTreeMap<String, String> {
        self.published()
            .filter_map(|published| {
                let canonical = published.url.as_deref()?;
                let label = named(published.title.as_deref())
                    .or_else(|| named(published.name.as_deref()))?;
                Some((canonical.to_owned(), label.to_owned()))
            })
            .collect()
    }

    /// Every published resource, as the offers a picker makes.
    ///
    /// One canonical is offered once however many versions of it the root
    /// holds, because the canonical is what a run carries and the version is a
    /// parameter of its own. The order is the name a reader reads, case-folded,
    /// so a list of twenty is walked rather than searched.
    pub(crate) fn choices(&self) -> Vec<Choice> {
        let mut offered: BTreeMap<String, String> = BTreeMap::new();
        for published in self.published() {
            let Some(canonical) = published.url.as_deref().filter(|url| !url.is_empty()) else {
                continue;
            };
            let label = named(published.title.as_deref())
                .or_else(|| named(published.name.as_deref()))
                .unwrap_or(canonical);
            offered
                .entry(canonical.to_owned())
                .or_insert_with(|| label.to_owned());
        }
        let mut choices: Vec<Choice> = offered
            .into_iter()
            .map(|(canonical, label)| Choice { canonical, label })
            .collect();
        choices.sort_by(|left, right| {
            left.label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.canonical.cmp(&right.canonical))
        });
        choices
    }

    /// The versions of `canonical` this root published, as picker offers.
    ///
    /// A resource published once with no version of its own offers nothing:
    /// there is no version to send, and the server resolves the request
    /// itself. The offer's label is the version, because a version has no
    /// name a reader knows it by.
    pub(crate) fn versions(&self, canonical: &str) -> Vec<Choice> {
        let mut held: Vec<String> = self
            .published()
            .filter(|published| published.url.as_deref() == Some(canonical))
            .filter_map(|published| named(published.version.as_deref()))
            .map(str::to_owned)
            .collect();
        held.sort();
        held.dedup();
        held.into_iter()
            .map(|version| Choice {
                label: version.clone(),
                canonical: version,
            })
            .collect()
    }

    /// Every published resource as a listing shows it, in the order the
    /// name a reader reads sorts, case-folded.
    pub(crate) fn listed(&self) -> Vec<Listed> {
        let mut listed: Vec<Listed> = self
            .published()
            .map(|published| {
                let url = published.url.clone().unwrap_or_default();
                let label = named(published.title.as_deref())
                    .or_else(|| named(published.name.as_deref()))
                    .map_or_else(|| url.clone(), str::to_owned);
                Listed {
                    id: published.id.clone().unwrap_or_default(),
                    label,
                    url,
                    version: named(published.version.as_deref()).map(str::to_owned),
                    status: named(published.status.as_deref()).map(str::to_owned),
                    stored: named(published.version_id.as_deref()).map(str::to_owned),
                }
            })
            .collect();
        listed.sort_by(|left, right| {
            left.label
                .to_lowercase()
                .cmp(&right.label.to_lowercase())
                .then_with(|| left.url.cmp(&right.url))
        });
        listed
    }

    /// The resources the answer carried.
    fn published(&self) -> impl Iterator<Item = &Stamp> {
        self.published.iter()
    }
}

/// A name the resource actually carries, or `None` for one it left empty.
fn named(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|name| !name.is_empty())
}

/// Reads the narrowed `searchset` `Bundle` of the `fhir-types` module
/// `$module`.
///
/// An entry that carries no `CodeSystem`, `ValueSet` or `ConceptMap`, such as
/// the `OperationOutcome` a `searchset` may carry about the search itself
/// (<https://hl7.org/fhir/R4B/http.html#search>), names no resource.
macro_rules! reads_named {
    ($module:ident) => {
        impl Reads<NamedSearch> for fhir_types::$module::bundle::Bundle {
            fn read(&self) -> NamedSearch {
                NamedSearch {
                    published: self
                        .entry
                        .iter()
                        .filter_map(|entry| entry.resource.as_ref())
                        .map(Reads::<Stamp>::read)
                        .filter(|stamp| stamp.resource_type.is_some())
                        .collect(),
                }
            }
        }
    };
}

reads_named!(r4);
reads_named!(r4b);
reads_named!(r5);
reads_named!(r6);

#[cfg(test)]
mod tests {
    use super::*;

    /// One answer, built the way a server writes it.
    fn answered(rows: &[(&str, Option<&str>, Option<&str>)]) -> NamedSearch {
        NamedSearch {
            published: rows
                .iter()
                .map(|(url, name, title)| Stamp {
                    resource_type: Some("ValueSet".to_owned()),
                    url: Some((*url).to_owned()),
                    name: name.map(str::to_owned),
                    title: title.map(str::to_owned),
                    ..Stamp::default()
                })
                .collect(),
        }
    }

    /// One answer carrying a version per entry.
    fn versioned(rows: &[(&str, &str)]) -> NamedSearch {
        NamedSearch {
            published: rows
                .iter()
                .map(|(url, version)| Stamp {
                    resource_type: Some("ValueSet".to_owned()),
                    url: Some((*url).to_owned()),
                    version: Some((*version).to_owned()),
                    ..Stamp::default()
                })
                .collect(),
        }
    }

    #[test]
    fn the_title_is_the_name_a_reader_picks_by_and_the_name_is_the_fallback() {
        let search = answered(&[
            ("urn:a", Some("TheComputerName"), Some("A title")),
            ("urn:b", Some("TheComputerName"), None),
        ]);
        let names = search.names();
        assert_eq!(names.get("urn:a").map(String::as_str), Some("A title"));
        assert_eq!(
            names.get("urn:b").map(String::as_str),
            Some("TheComputerName"),
            "a resource published without a title is known by its name"
        );
    }

    #[test]
    fn a_resource_published_with_no_name_is_offered_by_its_canonical() {
        let choices = answered(&[("urn:c", None, None)]).choices();
        assert_eq!(
            choices,
            [Choice {
                canonical: "urn:c".to_owned(),
                label: "urn:c".to_owned(),
            }],
            "a picker with a blank row in it offers nothing a reader can pick"
        );
    }

    #[test]
    fn one_canonical_is_offered_once_however_many_versions_the_root_holds() {
        let choices = answered(&[
            ("urn:sct", None, Some("SNOMED CT")),
            ("urn:sct", None, Some("SNOMED CT")),
        ])
        .choices();
        assert_eq!(
            choices.len(),
            1,
            "the canonical is what a run carries, and the version is a parameter of its own"
        );
    }

    #[test]
    fn the_offers_are_ordered_by_the_name_a_reader_reads() {
        let choices = answered(&[
            ("urn:z", None, Some("apples")),
            ("urn:a", None, Some("Bananas")),
            ("urn:m", None, Some("Cherries")),
        ])
        .choices();
        let labels: Vec<&str> = choices.iter().map(|choice| choice.label.as_str()).collect();
        assert_eq!(
            labels,
            ["apples", "Bananas", "Cherries"],
            "case is a spelling, not an order"
        );
    }

    #[test]
    fn a_resource_the_answer_carried_without_a_canonical_is_not_offered() {
        let search = NamedSearch {
            published: vec![Stamp {
                resource_type: Some("ValueSet".to_owned()),
                name: Some("Nameless".to_owned()),
                ..Stamp::default()
            }],
        };
        assert!(
            search.choices().is_empty(),
            "a run needs the canonical, so an offer without one cannot be run"
        );
    }

    #[test]
    fn the_versions_offered_are_the_ones_that_canonical_was_published_in() {
        let search = versioned(&[("urn:a", "2.0"), ("urn:a", "1.0"), ("urn:b", "9.9")]);
        let offered: Vec<String> = search
            .versions("urn:a")
            .into_iter()
            .map(|choice| choice.canonical)
            .collect();
        assert_eq!(
            offered,
            ["1.0", "2.0"],
            "a version of another resource is not an offer"
        );
        assert!(
            search.versions("urn:never").is_empty(),
            "a canonical this root does not publish has no versions to offer"
        );
    }

    #[test]
    fn a_resource_published_without_a_version_offers_none() {
        assert!(
            answered(&[("urn:a", None, Some("A"))])
                .versions("urn:a")
                .is_empty(),
            "there is nothing to send, and the server resolves the request itself"
        );
    }

    #[test]
    fn an_empty_answer_offers_nothing() {
        assert_eq!(NamedSearch::default().choices(), Vec::<Choice>::new());
        assert!(NamedSearch::default().names().is_empty());
    }

    /// A searchset as the server answers `_elements=url,name,title,version`.
    const LISTING: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[
        {"resource":{"resourceType":"ValueSet","id":"b","url":"https://terminology.example/b",
          "name":"Bravo","status":"active","meta":{"versionId":"3"}}},
        {"resource":{"resourceType":"ValueSet","id":"a","url":"https://terminology.example/a",
          "title":"  ","name":"alpha","version":" 1.0 ","status":"draft"}},
        {"resource":{"resourceType":"ValueSet","id":"c","url":"https://terminology.example/c",
          "status":"active"}},
        {"search":{"mode":"outcome"},"resource":{"resourceType":"OperationOutcome",
          "issue":[{"severity":"information","code":"informational"}]}}]}"#;

    /// `LISTING`, decoded at R4B.
    fn listing() -> NamedSearch {
        crate::fhir::version::FhirVersion::R4B
            .decode(LISTING)
            .expect("the server's own answer decodes at R4B")
    }

    #[test]
    fn a_listing_names_each_resource_by_title_else_name_else_canonical_in_folded_order() {
        let search = listing();
        let listed = search.listed();
        assert_eq!(
            vec!["alpha", "Bravo", "https://terminology.example/c"],
            listed
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            "a blank title gives way to the name, and case does not decide the order"
        );
        assert_eq!(
            Listed {
                id: String::from("a"),
                url: String::from("https://terminology.example/a"),
                label: String::from("alpha"),
                version: Some(String::from("1.0")),
                status: Some(String::from("draft")),
                stored: None,
            },
            listed[0]
        );
    }

    #[test]
    fn only_a_resource_written_through_the_api_lists_a_stored_version() {
        let search = listing();
        let stored: Vec<Option<String>> =
            search.listed().into_iter().map(|row| row.stored).collect();
        assert_eq!(
            vec![None, Some(String::from("3")), None],
            stored,
            "a resource the deployment loaded states no version"
        );
    }

    #[test]
    fn meta_names_the_stored_version_only_when_it_carries_one() {
        let stored = |version_id: Option<&str>| {
            NamedSearch {
                published: vec![Stamp {
                    resource_type: Some("ValueSet".to_owned()),
                    version_id: version_id.map(str::to_owned),
                    ..Stamp::default()
                }],
            }
            .listed()
            .into_iter()
            .map(|row| row.stored)
            .collect::<Vec<_>>()
        };
        assert_eq!(stored(Some("7")), vec![Some(String::from("7"))]);
        assert_eq!(
            stored(Some(" ")),
            vec![None],
            "a blank versionId is no version"
        );
        assert_eq!(stored(None), vec![None]);
    }
}
