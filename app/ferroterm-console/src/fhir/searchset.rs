// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `searchset` `Bundle` a RESTful search answers.
//!
//! `GET [base]/{type}?…` is the search interaction and it answers a `Bundle`
//! of type `searchset` (<https://hl7.org/fhir/R4B/http.html#search>). The
//! envelope is the same whatever was searched for, so it is read once here and
//! each resource module carries only the elements it draws.

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

/// What a search answered, read from the `Bundle` its release decoded: what
/// the server counted, and the resources of one type it carried, in the
/// server's own order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Found<V> {
    /// `Bundle.total`, the number of matches the server counted.
    pub(crate) total: Option<u32>,
    /// The resources of the type searched for. An entry carrying another
    /// resource, such as the `OperationOutcome` a search may describe itself
    /// with (<https://hl7.org/fhir/R4B/http.html#search>), is not a match.
    found: Vec<V>,
}

impl<V> Found<V> {
    /// What a search answered: the server's count, and the matches in its
    /// order.
    pub(crate) fn new(total: Option<u32>, found: Vec<V>) -> Self {
        Self { total, found }
    }

    /// The resources the answer carries, in the server's own order, which a
    /// client never re-sorts.
    pub(crate) fn found(&self) -> Vec<&V> {
        self.found.iter().collect()
    }
}

impl<V> Default for Found<V> {
    fn default() -> Self {
        Self {
            total: None,
            found: Vec::new(),
        }
    }
}

/// Reads the `Bundle` of the `fhir-types` module `$module` into what a page
/// reads of each `$variant` it carries, as `$view`.
macro_rules! carries {
    ($module:ident, $variant:ident, $view:ty) => {
        impl crate::fhir::release::Reads<Found<$view>> for fhir_types::$module::bundle::Bundle {
            fn read(&self) -> Found<$view> {
                use fhir_types::$module::resource::Resource;
                Found::new(
                    self.total.as_ref().and_then(|total| total.value),
                    self.entry
                        .iter()
                        .filter_map(|entry| match &entry.resource {
                            Some(Resource::$variant(resource)) => Some(
                                crate::fhir::release::Reads::<$view>::read(resource.as_ref()),
                            ),
                            _ => None,
                        })
                        .collect(),
                )
            }
        }
    };
}

pub(crate) use carries;

#[cfg(test)]
mod tests {
    use crate::fhir::release::fixture::completed;
    use crate::fhir::searchset::Found;
    use crate::fhir::value_set::PublishedValueSet;
    use crate::fhir::version::FhirVersion;

    /// A searchset of `entries`, decoded at R4B.
    fn parse(total: u32, entries: &[&str]) -> Found<PublishedValueSet> {
        let entries: Vec<String> = entries
            .iter()
            .map(|resource| format!(r#"{{"resource":{resource}}}"#))
            .collect();
        FhirVersion::R4B
            .decode(&format!(
                r#"{{"resourceType":"Bundle","type":"searchset","total":{total},"entry":[{}]}}"#,
                entries.join(",")
            ))
            .expect("the fixture is a valid R4B searchset")
    }

    #[test]
    fn every_match_is_read_in_the_order_the_server_sent_it() {
        let b = completed("ValueSet", r#"{"id":"b"}"#);
        let a = completed("ValueSet", r#"{"id":"a"}"#);
        let found = parse(2, &[&b, &a]);
        let ids: Vec<Option<&str>> = found
            .found()
            .iter()
            .map(|value_set| value_set.id())
            .collect();
        assert_eq!(
            ids,
            [Some("b"), Some("a")],
            "the server's order is the answer's order"
        );
        assert_eq!(found.total, Some(2));
    }

    #[test]
    fn an_entry_carrying_another_resource_is_not_drawn_as_a_match() {
        let outcome = r#"{"resourceType":"OperationOutcome","issue":[
            {"severity":"information","code":"informational"}]}"#;
        let a = completed("ValueSet", r#"{"id":"a"}"#);
        let found = parse(1, &[outcome, &a]);
        assert_eq!(found.found().len(), 1, "the outcome describes the search");
        assert_eq!(
            found.found().first().and_then(|value_set| value_set.id()),
            Some("a")
        );
    }
}
