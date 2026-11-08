// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ValueSet/$expand` request the runner sends, and the answer it reads.
//!
//! `$expand` is paged with `count` and `offset` and reports `expansion.total`
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). Reading the
//! answer lives here, outside every component, so the screen is a rendering of
//! a value that plain unit tests can pin.

use ferrobridge_term::outcome::{BatchOutcome, ExpandOutcome, Expanded, ExpansionContains};
use ferrobridge_term::request::{Expansion as Asked, Request};

use crate::fhir::concept::designation;
use crate::fhir::error::FhirError;
use crate::fhir::operation::{mismatched, primitive_text};
use crate::fhir::version::FhirVersion;

/// The parameters one run of `$expand` sends.
///
/// Every optional field is sent only when the reader set it, so the request is
/// the one they asked for and the server's own defaults apply to the rest.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExpandRequest {
    /// The canonical of the value set to expand, implicit forms included.
    pub(crate) url: String,
    /// The text filter, when the reader typed one.
    pub(crate) filter: Option<String>,
    /// How many concepts one page holds, absent for an unpaged run.
    pub(crate) count: Option<u32>,
    /// Where the page starts, absent for the start of the result.
    pub(crate) offset: Option<u32>,
    /// The BCP 47 tag displays are wanted in.
    pub(crate) display_language: Option<String>,
    /// Whether inactive concepts are left out.
    pub(crate) active_only: Option<bool>,
    /// Whether every concept carries its designations.
    pub(crate) include_designations: Option<bool>,
}

impl ExpandRequest {
    /// The operation this run asks for.
    ///
    /// The canonical goes through the same encoder as everything else, which
    /// is what keeps an implicit form carrying its own query string inside the
    /// value it belongs to.
    pub(crate) fn request(&self) -> Request {
        Request::Expand(Asked {
            filter: self.filter.clone(),
            count: self.count,
            offset: self.offset,
            display_language: self.display_language.clone(),
            active_only: self.active_only,
            include_designations: self.include_designations,
            ..Asked::new(self.url.as_str())
        })
    }
}

/// What `ValueSet/$expand` answered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExpandedValueSet {
    /// `ValueSet.expansion`.
    expansion: Expansion,
}

/// One expansion, as the runner draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Expansion {
    /// `expansion.total`, absent when the server declared none.
    pub(crate) total: Option<u32>,
    /// `expansion.offset`, absent when the server declared none.
    pub(crate) offset: Option<u32>,
    /// The parameters the server says it applied, in the order it sent them.
    pub(crate) parameters: Vec<ParameterLine>,
    /// The concepts of this page, nested entries flattened in document order.
    pub(crate) concepts: Vec<ConceptRow>,
    /// Why the list is not the whole story, when the server said it is not.
    pub(crate) unclosed: Option<Unclosed>,
}

impl Expansion {
    /// How many concepts this answer lists, nested entries included.
    ///
    /// `expansion.total` is "the total number of concepts in the expansion",
    /// measured against "the number of concept nodes in this resource"
    /// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.total>),
    /// so a nested entry counts the same as a top-level one and this number is
    /// also the number of rows a reader can count.
    pub(crate) fn listed(&self) -> u32 {
        u32::try_from(self.concepts.len()).unwrap_or(u32::MAX)
    }
}

/// The mark saying the value set admits codes this expansion does not list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Unclosed {
    /// The reasons the server stated, in its own words and its own order.
    pub(crate) reasons: Vec<String>,
}

/// One echoed `expansion.parameter`, flattened to the two strings shown.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ParameterLine {
    /// The parameter name.
    pub(crate) name: String,
    /// The value, whichever `value[x]` carried it.
    pub(crate) value: String,
}

/// One concept of an expansion, as a row.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConceptRow {
    /// The system the code belongs to, empty when the server named none.
    pub(crate) system: String,
    /// The system version, sent only where codes need distinguishing.
    pub(crate) version: Option<String>,
    /// The code, empty when the server named none.
    pub(crate) code: String,
    /// The display the server sent for it.
    pub(crate) display: Option<String>,
    /// Whether the concept is inactive in its system.
    pub(crate) inactive: bool,
    /// Whether the concept cannot be used as a selectable code.
    pub(crate) abstract_concept: bool,
    /// The designations, when the request asked for them.
    pub(crate) designations: Vec<DesignationRow>,
    /// How deep the entry was nested, zero at the top.
    pub(crate) depth: u32,
}

/// One designation of a concept, as a line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DesignationRow {
    /// The BCP 47 tag, absent when the server sent none.
    pub(crate) language: Option<String>,
    /// What the term is for, as the server's display or its code.
    pub(crate) usage: Option<String>,
    /// The term itself.
    pub(crate) value: String,
}

impl ExpandedValueSet {
    /// The expansion `outcome` carries, the reading of the run sent to `url`
    /// at `version`.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Decode`] for an answer that is not an `$expand`
    /// answer.
    pub(crate) fn read(
        outcome: BatchOutcome,
        url: &str,
        version: FhirVersion,
    ) -> Result<Self, FhirError> {
        match outcome {
            BatchOutcome::Expand(ExpandOutcome::Expanded(expanded)) => Ok(Self {
                expansion: expansion(expanded),
            }),
            _ => Err(mismatched(url, version)),
        }
    }

    /// The expansion the answer carried.
    pub(crate) fn expansion(&self) -> Expansion {
        self.expansion.clone()
    }
}

/// The expansion as a page draws it.
fn expansion(expanded: Expanded) -> Expansion {
    Expansion {
        total: expanded.total.and_then(|total| u32::try_from(total).ok()),
        offset: expanded
            .offset
            .and_then(|offset| u32::try_from(offset).ok()),
        parameters: expanded
            .parameters
            .into_iter()
            .map(|parameter| ParameterLine {
                value: parameter
                    .value
                    .as_ref()
                    .and_then(primitive_text)
                    .unwrap_or_default(),
                name: parameter.name,
            })
            .collect(),
        concepts: rows(expanded.contains),
        unclosed: expanded.unclosed.map(|unclosed| Unclosed {
            reasons: unclosed.reasons,
        }),
    }
}

/// Every entry, nested ones included, in the order the server wrote them.
fn rows(contains: Vec<ExpansionContains>) -> Vec<ConceptRow> {
    // NOTE: no specification governs this: our own design; the walk keeps its
    // own stack, so a deeply nested answer cannot exhaust the thread's.
    let mut rows = Vec::with_capacity(contains.len());
    let mut pending: Vec<(ExpansionContains, u32)> = contains
        .into_iter()
        .rev()
        .map(|entry| (entry, 0_u32))
        .collect();
    while let Some((mut entry, depth)) = pending.pop() {
        let nested = std::mem::take(&mut entry.contains);
        pending.extend(
            nested
                .into_iter()
                .rev()
                .map(|nested| (nested, depth.saturating_add(1))),
        );
        rows.push(ConceptRow {
            system: entry.system.unwrap_or_default(),
            version: entry.version,
            code: entry.code.unwrap_or_default(),
            display: entry.display,
            inactive: entry.inactive.unwrap_or_default(),
            abstract_concept: entry.r#abstract.unwrap_or_default(),
            designations: entry.designations.into_iter().map(designation).collect(),
            depth,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The extension marking an expansion that leaves out codes the value
    /// set admits (<https://hl7.org/fhir/R4B/extension-valueset-unclosed.html>).
    const UNCLOSED_EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/valueset-unclosed";

    /// The extension stating in words why an expansion is unclosed.
    const UNCLOSED_REASON_EXTENSION: &str =
        "http://hl7.org/fhir/StructureDefinition/valueset-unclosed-reason";

    fn parse(json: &str) -> ExpandedValueSet {
        read(
            FhirVersion::R4B,
            &crate::fhir::release::fixture::completed("ValueSet", json),
        )
        .expect("the fixture is a valid R4B ValueSet")
    }

    /// `json`, the `ValueSet` an `$expand` answered at `version`, read as the
    /// console reads it.
    fn read(version: FhirVersion, json: &str) -> Result<ExpandedValueSet, FhirError> {
        let request = ExpandRequest::default().request();
        let outcome = crate::fhir::operation::tests::read(version, &request, json)?;
        ExpandedValueSet::read(outcome, "", version)
    }

    /// The query `request` is sent with at R4B.
    fn query(request: &ExpandRequest) -> String {
        crate::fhir::operation::tests::query(FhirVersion::R4B, request.request())
    }

    #[test]
    fn a_request_sends_only_the_parameters_the_reader_set() {
        let request = ExpandRequest {
            url: "https://terminology.example/ValueSet/all".to_owned(),
            count: Some(20),
            ..ExpandRequest::default()
        };
        assert_eq!(
            query(&request),
            "?url=https%3A%2F%2Fterminology.example%2FValueSet%2Fall&count=20",
            "an unset parameter leaves the server's own default in force"
        );
    }

    #[test]
    fn every_parameter_the_runner_offers_is_sent_when_it_is_set() {
        let request = ExpandRequest {
            url: "https://terminology.example/ValueSet/all".to_owned(),
            filter: Some("fever".to_owned()),
            count: Some(20),
            offset: Some(40),
            display_language: Some("nl-NL".to_owned()),
            active_only: Some(true),
            include_designations: Some(false),
        };
        assert_eq!(
            query(&request),
            "?url=https%3A%2F%2Fterminology.example%2FValueSet%2Fall&filter=fever&offset=40\
             &count=20&includeDesignations=false&activeOnly=true&displayLanguage=nl-NL",
            "the boolean parameters are sent as the FHIR literals"
        );
    }

    #[test]
    fn an_implicit_canonical_carrying_a_query_string_stays_one_parameter() {
        // An implicit value set canonical has its own `?` and `=`, and an
        // unencoded one would truncate the request it is embedded in.
        let request = ExpandRequest {
            url: "http://snomed.info/sct?fhir_vs=isa/404684003".to_owned(),
            count: Some(20),
            ..ExpandRequest::default()
        };
        let rendered = query(&request);
        assert_eq!(
            rendered, "?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F404684003&count=20",
            "the canonical's own query string is escaped into the value"
        );
        assert_eq!(
            rendered.matches('?').count(),
            1,
            "the request has one query string, and it is the one the client built"
        );
    }

    #[test]
    fn a_filter_carrying_an_ampersand_cannot_add_a_parameter() {
        let request = ExpandRequest {
            url: "https://terminology.example/ValueSet/all".to_owned(),
            filter: Some("a&count=1".to_owned()),
            ..ExpandRequest::default()
        };
        assert_eq!(
            query(&request),
            "?url=https%3A%2F%2Fterminology.example%2FValueSet%2Fall&filter=a%26count%3D1",
            "a typed value cannot smuggle in a parameter of its own"
        );
    }

    #[test]
    fn the_page_and_its_total_are_read() {
        let expansion = parse(
            r#"{"resourceType":"ValueSet","expansion":{"identifier":"urn:uuid:1",
                "timestamp":"2026-09-07T00:00:00Z","total":173,"offset":20,
                "contains":[{"system":"https://terminology.example/x","code":"a","display":"A"}]}}"#,
        )
        .expansion();
        assert_eq!((expansion.total, expansion.offset), (Some(173), Some(20)));
        assert_eq!(expansion.concepts.len(), 1);
    }

    #[test]
    fn a_value_set_with_no_expansion_is_no_answer_at_all() {
        let unread = read(
            FhirVersion::R4B,
            r#"{"resourceType":"ValueSet","status":"active"}"#,
        );
        assert!(
            matches!(unread, Err(FhirError::Decode { .. })),
            "`$expand` returns the value set with its expansion, so an answer without one \
             is refused: {unread:?}"
        );
    }

    #[test]
    fn a_concept_carries_its_flags_and_its_designations() {
        let expansion = parse(
            r#"{"expansion":{"contains":[{"system":"https://terminology.example/x",
                "version":"2.0","code":"a","display":"A","inactive":true,"abstract":true,
                "designation":[{"language":"nl","use":{"system":"https://terminology.example/u",
                  "code":"900","display":"Preferred"},"value":"Aa"},
                 {"language":"cy","value":"Ah"}]}]}}"#,
        )
        .expansion();
        let concept = expansion.concepts.first().expect("one concept was sent");
        assert_eq!(concept.version.as_deref(), Some("2.0"));
        assert!(
            concept.inactive && concept.abstract_concept,
            "both flags the server set are read"
        );
        assert_eq!(
            concept
                .designations
                .iter()
                .map(|designation| (designation.language.clone(), designation.usage.clone()))
                .collect::<Vec<_>>(),
            vec![
                (Some("nl".to_owned()), Some("Preferred".to_owned())),
                (Some("cy".to_owned()), None),
            ],
            "a designation with no use reads as one, and the use shows its display"
        );
    }

    #[test]
    fn a_use_without_a_display_falls_back_to_its_code() {
        let expansion = parse(
            r#"{"expansion":{"contains":[{"code":"a",
                "designation":[{"use":{"code":"900"},"value":"Aa"}]}]}}"#,
        )
        .expansion();
        assert_eq!(
            expansion
                .concepts
                .first()
                .and_then(|concept| concept.designations.first())
                .and_then(|designation| designation.usage.clone()),
            Some("900".to_owned()),
            "the code is what the server named it by"
        );
    }

    #[test]
    fn nested_entries_are_flattened_in_document_order_with_their_depth() {
        let expansion = parse(
            r#"{"expansion":{"contains":[
                {"code":"a","contains":[{"code":"a1"},{"code":"a2","contains":[{"code":"a2i"}]}]},
                {"code":"b"}]}}"#,
        )
        .expansion();
        assert_eq!(
            expansion
                .concepts
                .iter()
                .map(|concept| (concept.code.clone(), concept.depth))
                .collect::<Vec<_>>(),
            vec![
                ("a".to_owned(), 0),
                ("a1".to_owned(), 1),
                ("a2".to_owned(), 1),
                ("a2i".to_owned(), 2),
                ("b".to_owned(), 0),
            ],
            "a hierarchy renders as rows that say how deep they sit"
        );
        assert_eq!(
            expansion.listed(),
            5,
            "`expansion.total` counts concept nodes, so every rendered row counts"
        );
        assert!(
            expansion.concepts.iter().any(|concept| concept.depth > 0),
            "an answer with a nested entry is not a page of a flat expansion"
        );
    }

    #[test]
    fn a_flat_answer_is_counted_the_same_way_and_says_it_is_flat() {
        let expansion =
            parse(r#"{"expansion":{"total":2,"contains":[{"code":"a"},{"code":"b"}]}}"#)
                .expansion();
        assert_eq!(expansion.listed(), 2);
        assert!(
            !expansion.concepts.iter().any(|concept| concept.depth > 0),
            "nothing is nested, so the page window the request asked for applies"
        );
    }

    #[test]
    fn an_unclosed_expansion_is_marked_and_states_its_reasons_in_order() {
        let expansion = parse(&format!(
            r#"{{"expansion":{{"total":2,"extension":[
                {{"url":"{UNCLOSED_EXTENSION}","valueBoolean":true}},
                {{"url":"{UNCLOSED_REASON_EXTENSION}","valueString":"the grammar admits more"}},
                {{"url":"{UNCLOSED_REASON_EXTENSION}","valueString":"the fragment holds part"}}]}}}}"#
        ))
        .expansion();
        assert_eq!(
            expansion.unclosed,
            Some(Unclosed {
                reasons: vec![
                    "the grammar admits more".to_owned(),
                    "the fragment holds part".to_owned(),
                ],
            }),
            "the reader sees the server's own wording, in the server's order"
        );
    }

    #[test]
    fn a_closed_expansion_carries_no_mark() {
        let expansion = parse(r#"{"expansion":{"total":2,"contains":[{"code":"a"}]}}"#).expansion();
        assert_eq!(
            expansion.unclosed, None,
            "an expansion that lists everything says nothing about being unclosed"
        );
    }

    #[test]
    fn a_reason_without_the_mark_is_not_read_as_unclosed() {
        let expansion = parse(&format!(
            r#"{{"expansion":{{"extension":[
                {{"url":"{UNCLOSED_REASON_EXTENSION}","valueString":"orphaned"}}]}}}}"#
        ))
        .expansion();
        assert_eq!(
            expansion.unclosed, None,
            "the mark is the boolean, and a reason alone claims nothing"
        );
    }

    #[test]
    fn a_mark_set_to_false_leaves_the_expansion_closed() {
        let expansion = parse(&format!(
            r#"{{"expansion":{{"extension":[
                {{"url":"{UNCLOSED_EXTENSION}","valueBoolean":false}}]}}}}"#
        ))
        .expansion();
        assert_eq!(
            expansion.unclosed, None,
            "the mark is false, so the expansion lists everything"
        );
    }

    #[test]
    fn every_echoed_parameter_is_read_whichever_value_carried_it() {
        let expansion = parse(
            r#"{"expansion":{"parameter":[
                {"name":"filter","valueString":"fever"},
                {"name":"count","valueInteger":20},
                {"name":"activeOnly","valueBoolean":true},
                {"name":"displayLanguage","valueCode":"nl-NL"},
                {"name":"used-codesystem","valueUri":"https://terminology.example/x|2.0"},
                {"name":"score","valueDecimal":0.5},
                {"name":"date","valueDateTime":"2026-09-07"},
                {"name":"version"}]}}"#,
        )
        .expansion();
        assert_eq!(
            expansion
                .parameters
                .iter()
                .map(|line| (line.name.clone(), line.value.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("filter".to_owned(), "fever".to_owned()),
                ("count".to_owned(), "20".to_owned()),
                ("activeOnly".to_owned(), "true".to_owned()),
                ("displayLanguage".to_owned(), "nl-NL".to_owned()),
                (
                    "used-codesystem".to_owned(),
                    "https://terminology.example/x|2.0".to_owned(),
                ),
                ("score".to_owned(), "0.5".to_owned()),
                ("date".to_owned(), "2026-09-07".to_owned()),
                ("version".to_owned(), String::new()),
            ],
            "the echo is the server's, so a parameter with no value still shows"
        );
    }

    #[test]
    fn a_negative_total_reads_as_absent_rather_than_wrapping() {
        let expansion = parse(r#"{"expansion":{"total":-1,"offset":-5}}"#).expansion();
        assert_eq!(
            (expansion.total, expansion.offset),
            (None, None),
            "a count below zero names no page, and the screen states the absence"
        );
    }

    #[test]
    fn an_answer_with_elements_the_reader_does_not_draw_still_reads() {
        let answer = r#"{"resourceType":"ValueSet","id":"x","status":"active",
            "meta":{"lastUpdated":"2026-09-07T00:00:00Z"},
            "expansion":{"identifier":"urn:uuid:2","timestamp":"2026-09-07T00:00:00Z",
            "property":[{"code":"parent"}],"contains":[{"code":"a"}]}}"#;
        let expansion = read(FhirVersion::R5, answer)
            .expect("R5 defines `expansion.property`")
            .expansion();
        assert_eq!(
            expansion.concepts.len(),
            1,
            "the reader ignores the elements it does not draw"
        );
        let refused =
            read(FhirVersion::R4B, answer).expect_err("R4B defines no `expansion.property`");
        assert!(refused.to_string().contains("property"), "{refused}");
    }
}
