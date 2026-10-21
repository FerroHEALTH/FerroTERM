// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `searchset` `Bundle` a RESTful search answers.
//!
//! `GET [base]/{type}?…` is the search interaction and it answers a `Bundle`
//! of type `searchset` (<https://hl7.org/fhir/R4B/http.html#search>). The
//! envelope is the same whatever was searched for, so it is read once here and
//! each resource module carries only the elements it draws.

use serde::{Deserialize, Serialize};

/// A resource type this viewer draws out of a `searchset`.
///
/// A `searchset` may carry an `OperationOutcome` describing the search itself
/// beside the matches (<https://hl7.org/fhir/R4B/http.html#search>), so an
/// entry has to say what it is before it is drawn as a match.
pub(crate) trait Published {
    /// The `resourceType` a match declares.
    const RESOURCE_TYPE: &'static str;

    /// The `resourceType` this entry declared, when it declared one.
    ///
    /// `resourceType` is mandatory in FHIR JSON
    /// (<https://hl7.org/fhir/R4B/json.html>), so an entry declaring none is
    /// not a resource this viewer draws.
    fn declared_type(&self) -> Option<&str>;
}

/// The search parameters a definitional-resource search sends.
///
/// `url` and `version` are the two search parameters every definitional
/// resource defines (<https://hl7.org/fhir/R4B/valueset.html#search>). Each is
/// sent only when the reader named it, so an empty filter asks for everything
/// the root holds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SearchFilter {
    /// `url`, matched against the resource's canonical.
    pub(crate) url: String,
    /// `version`, matched against the resource's business version.
    pub(crate) version: String,
}

/// What a search answered: what the server counted, and what it sent.
///
/// The container carries the default, so a `Bundle` that states neither
/// element reads as an empty answer rather than as a decode error, and a
/// resource type that has no `Default` of its own can still be searched for.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default)]
pub(crate) struct SearchSet<T> {
    /// `Bundle.total`, the number of matches the server counted.
    total: Option<u32>,
    /// `Bundle.entry`, one per match.
    entry: Vec<Entry<T>>,
}

impl<T> Default for SearchSet<T> {
    fn default() -> Self {
        Self {
            total: None,
            entry: Vec::new(),
        }
    }
}

/// One `Bundle.entry`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Entry<T> {
    /// The resource the entry carries.
    resource: Option<T>,
}

impl<T: Published> SearchSet<T> {
    /// The number of matches the server counted, when it counted them.
    pub(crate) fn total(&self) -> Option<u32> {
        self.total
    }

    /// How many resources of this type the answer carries.
    pub(crate) fn matched(&self) -> usize {
        self.carried().count()
    }

    /// The resources the answer carries, in the server's own order.
    ///
    /// The order is the server's and is never re-sorted here: a search states
    /// its own order, and a client that re-orders one hides it.
    pub(crate) fn found(&self) -> Vec<&T> {
        self.carried().collect()
    }

    /// The entries that carry a resource of this type.
    fn carried(&self) -> impl Iterator<Item = &T> {
        self.entry
            .iter()
            .filter_map(|entry| entry.resource.as_ref())
            .filter(|resource| resource.declared_type() == Some(T::RESOURCE_TYPE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in resource, so the envelope is tested without a resource
    /// module's own reading rules.
    #[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
    struct Thing {
        #[serde(rename = "resourceType")]
        resource_type: Option<String>,
        id: Option<String>,
    }

    impl Published for Thing {
        const RESOURCE_TYPE: &'static str = "Thing";

        fn declared_type(&self) -> Option<&str> {
            self.resource_type.as_deref()
        }
    }

    fn parse(json: &str) -> SearchSet<Thing> {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    #[test]
    fn every_match_is_read_in_the_order_the_server_sent_it() {
        let found = parse(
            r#"{"resourceType":"Bundle","type":"searchset","total":2,"entry":[
                {"resource":{"resourceType":"Thing","id":"b"}},
                {"resource":{"resourceType":"Thing","id":"a"}}]}"#,
        );
        let ids: Vec<Option<String>> = found.found().iter().map(|thing| thing.id.clone()).collect();
        assert_eq!(
            ids,
            [Some("b".to_owned()), Some("a".to_owned())],
            "the server's order is the answer's order"
        );
        assert_eq!(found.total(), Some(2));
    }

    #[test]
    fn an_entry_carrying_another_resource_is_not_drawn_as_a_match() {
        let found = parse(
            r#"{"entry":[
                {"search":{"mode":"outcome"},
                 "resource":{"resourceType":"OperationOutcome"}},
                {"resource":{"resourceType":"Thing","id":"a"}}]}"#,
        );
        assert_eq!(
            found.matched(),
            1,
            "an outcome describing the search is not one of its matches"
        );
    }

    #[test]
    fn an_answer_that_counts_nothing_still_renders_what_it_carries() {
        let found = parse(r#"{"entry":[{"resource":{"resourceType":"Thing"}}]}"#);
        assert_eq!(
            found.total(),
            None,
            "the screen states the absence rather than inventing a count"
        );
        assert_eq!(found.matched(), 1);
    }

    #[test]
    fn an_empty_searchset_reads_as_no_match_rather_than_as_a_decode_error() {
        let found = parse(r#"{"resourceType":"Bundle","type":"searchset","total":0}"#);
        assert_eq!(found.found(), Vec::<&Thing>::new());
        assert_eq!(found.total(), Some(0));
    }
}
