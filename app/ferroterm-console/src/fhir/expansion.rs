// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ValueSet/$expand` request the runner sends, and the answer it reads.
//!
//! `$expand` is paged with `count` and `offset` and reports `expansion.total`
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). Reading the
//! answer lives here, outside every component, so the screen is a rendering of
//! a value that plain unit tests can pin.

use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::url::RequestUrl;

/// The `$expand` parameter naming the value set to expand.
pub(crate) const URL_PARAMETER: &str = "url";

/// The `$expand` parameter carrying the text filter.
pub(crate) const FILTER_PARAMETER: &str = "filter";

/// The `$expand` parameter carrying the page size.
pub(crate) const COUNT_PARAMETER: &str = "count";

/// The `$expand` parameter carrying the offset of the page.
pub(crate) const OFFSET_PARAMETER: &str = "offset";

/// The `$expand` parameter carrying the language displays are wanted in.
pub(crate) const DISPLAY_LANGUAGE_PARAMETER: &str = "displayLanguage";

/// The `$expand` parameter that drops inactive concepts.
pub(crate) const ACTIVE_ONLY_PARAMETER: &str = "activeOnly";

/// The `$expand` parameter that asks for the designations of every concept.
pub(crate) const INCLUDE_DESIGNATIONS_PARAMETER: &str = "includeDesignations";

/// The extension marking an expansion that leaves out codes the value set
/// admits (<https://hl7.org/fhir/R4B/extension-valueset-unclosed.html>).
const UNCLOSED_EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/valueset-unclosed";

/// The extension stating in words why an expansion is unclosed.
// NOTE: no published `StructureDefinition` defines it; the terminology
// ecosystem suite is the source (<https://github.com/HL7/fhir-tx-ecosystem-ig>).
const UNCLOSED_REASON_EXTENSION: &str =
    "http://hl7.org/fhir/StructureDefinition/valueset-unclosed-reason";

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
    /// Appends every parameter that was set to the operation's address.
    ///
    /// The canonical goes through the same encoder as everything else, which
    /// is what keeps an implicit form carrying its own query string inside the
    /// value it belongs to.
    pub(crate) fn append(&self, url: RequestUrl) -> RequestUrl {
        let mut url = url.query(URL_PARAMETER, &self.url);
        if let Some(filter) = &self.filter {
            url = url.query(FILTER_PARAMETER, filter);
        }
        if let Some(count) = self.count {
            url = url.query(COUNT_PARAMETER, &count.to_string());
        }
        if let Some(offset) = self.offset {
            url = url.query(OFFSET_PARAMETER, &offset.to_string());
        }
        if let Some(language) = &self.display_language {
            url = url.query(DISPLAY_LANGUAGE_PARAMETER, language);
        }
        if let Some(active_only) = self.active_only {
            url = url.query(ACTIVE_ONLY_PARAMETER, &active_only.to_string());
        }
        if let Some(designations) = self.include_designations {
            url = url.query(INCLUDE_DESIGNATIONS_PARAMETER, &designations.to_string());
        }
        url
    }
}

/// The `ValueSet` a `$expand` answers, as the viewer reads it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct ExpandedValueSet {
    /// `ValueSet.expansion`, absent from a resource that carries none.
    expansion: Option<WireExpansion>,
}

/// `ValueSet.expansion`, with the elements the runner draws.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireExpansion {
    /// `expansion.total`, the size of the whole selection.
    total: Option<i32>,
    /// `expansion.offset`, where this page starts.
    offset: Option<i32>,
    /// `expansion.parameter`, the parameters the server says it applied.
    #[serde(default)]
    parameter: Vec<WireParameter>,
    /// `expansion.contains`, the concepts of this page.
    #[serde(default)]
    contains: Vec<WireContains>,
    /// The extensions the server declared on the expansion.
    #[serde(default)]
    extension: Vec<WireExtension>,
}

/// One `expansion.parameter`, with every `value[x]` the element allows.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireParameter {
    /// `parameter.name`.
    name: Option<String>,
    /// `valueString`.
    #[serde(rename = "valueString")]
    value_string: Option<String>,
    /// `valueBoolean`.
    #[serde(rename = "valueBoolean")]
    value_boolean: Option<bool>,
    /// `valueInteger`.
    #[serde(rename = "valueInteger")]
    value_integer: Option<i32>,
    /// `valueDecimal`.
    #[serde(rename = "valueDecimal")]
    value_decimal: Option<Number>,
    /// `valueUri`.
    #[serde(rename = "valueUri")]
    value_uri: Option<String>,
    /// `valueCode`.
    #[serde(rename = "valueCode")]
    value_code: Option<String>,
    /// `valueDateTime`.
    #[serde(rename = "valueDateTime")]
    value_date_time: Option<String>,
}

/// One `expansion.contains`, which may itself carry nested entries.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireContains {
    /// `contains.system`.
    system: Option<String>,
    /// `contains.version`, sent when one system answers from several versions.
    version: Option<String>,
    /// `contains.code`.
    code: Option<String>,
    /// `contains.display`.
    display: Option<String>,
    /// `contains.inactive`.
    inactive: Option<bool>,
    /// `contains.abstract`, whose wire name is a Rust keyword.
    #[serde(rename = "abstract")]
    abstract_concept: Option<bool>,
    /// `contains.designation`.
    #[serde(default)]
    designation: Vec<WireDesignation>,
    /// The entries nested under this one.
    #[serde(default)]
    contains: Vec<WireContains>,
}

/// One `contains.designation`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireDesignation {
    /// The BCP 47 tag of the term.
    language: Option<String>,
    /// What the term is for, as a `Coding`.
    #[serde(rename = "use")]
    usage: Option<WireCoding>,
    /// The term itself.
    value: Option<String>,
}

/// The parts of a `Coding` the runner renders.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireCoding {
    /// The code.
    code: Option<String>,
    /// The display the server sent for it.
    display: Option<String>,
}

/// One extension on the expansion, read for the unclosed mark and its reason.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireExtension {
    /// The canonical that says what the extension means.
    url: Option<String>,
    /// `valueBoolean`, which is how the mark itself is carried.
    #[serde(rename = "valueBoolean")]
    value_boolean: Option<bool>,
    /// `valueString`, which is how a reason is carried.
    #[serde(rename = "valueString")]
    value_string: Option<String>,
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

    /// Whether the server nested entries under one another.
    ///
    /// "Paging only applies to flat expansions: servers ignore paging if the
    /// expansion is not flat"
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, the `count`
    /// parameter), so a nested answer is the whole selection rather than a
    /// page of one.
    pub(crate) fn hierarchical(&self) -> bool {
        self.concepts.iter().any(|concept| concept.depth > 0)
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
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DesignationRow {
    /// The BCP 47 tag, absent when the server sent none.
    pub(crate) language: Option<String>,
    /// What the term is for, as the server's display or its code.
    pub(crate) usage: Option<String>,
    /// The term itself.
    pub(crate) value: String,
}

impl ExpandedValueSet {
    /// The expansion the answer carried, or `None` when it carried none.
    ///
    /// A `ValueSet` with no `expansion` is a legal resource and a useless
    /// answer to `$expand`, so the screen states that rather than drawing an
    /// empty table.
    pub(crate) fn expansion(&self) -> Option<Expansion> {
        let wire = self.expansion.as_ref()?;
        Some(Expansion {
            total: wire.total.and_then(|total| u32::try_from(total).ok()),
            offset: wire.offset.and_then(|offset| u32::try_from(offset).ok()),
            parameters: wire
                .parameter
                .iter()
                .map(|parameter| ParameterLine {
                    name: parameter.name.clone().unwrap_or_default(),
                    value: parameter.value().unwrap_or_default(),
                })
                .collect(),
            concepts: rows(&wire.contains),
            unclosed: unclosed_of(&wire.extension),
        })
    }
}

impl WireParameter {
    /// The value, from whichever `value[x]` the server used.
    fn value(&self) -> Option<String> {
        self.value_string
            .clone()
            .or_else(|| self.value_uri.clone())
            .or_else(|| self.value_code.clone())
            .or_else(|| self.value_date_time.clone())
            .or_else(|| self.value_boolean.map(|flag| flag.to_string()))
            .or_else(|| self.value_integer.map(|number| number.to_string()))
            .or_else(|| self.value_decimal.as_ref().map(ToString::to_string))
    }
}

impl WireContains {
    /// This entry as a row at `depth`, without its nested entries.
    fn row(&self, depth: u32) -> ConceptRow {
        ConceptRow {
            system: self.system.clone().unwrap_or_default(),
            version: self.version.clone(),
            code: self.code.clone().unwrap_or_default(),
            display: self.display.clone(),
            inactive: self.inactive.unwrap_or_default(),
            abstract_concept: self.abstract_concept.unwrap_or_default(),
            designations: self
                .designation
                .iter()
                .map(|designation| DesignationRow {
                    language: designation.language.clone(),
                    usage: designation.usage.as_ref().and_then(WireCoding::label),
                    value: designation.value.clone().unwrap_or_default(),
                })
                .collect(),
            depth,
        }
    }
}

impl WireCoding {
    /// The display the server sent, or the bare code when it sent none.
    fn label(&self) -> Option<String> {
        self.display.clone().or_else(|| self.code.clone())
    }
}

/// Every entry, nested ones included, in the order the server wrote them.
///
/// The walk carries its own stack rather than recursing, so a deeply nested
/// answer cannot exhaust the browser's.
fn rows(entries: &[WireContains]) -> Vec<ConceptRow> {
    let mut rows = Vec::with_capacity(entries.len());
    let mut pending: Vec<(&WireContains, u32)> =
        entries.iter().rev().map(|entry| (entry, 0_u32)).collect();
    while let Some((entry, depth)) = pending.pop() {
        rows.push(entry.row(depth));
        pending.extend(
            entry
                .contains
                .iter()
                .rev()
                .map(|nested| (nested, depth.saturating_add(1))),
        );
    }
    rows
}

/// The unclosed mark and its stated reasons, when the server sent the mark.
///
/// A reason without the mark says nothing on its own, so the reasons are read
/// only once the mark is there.
fn unclosed_of(extensions: &[WireExtension]) -> Option<Unclosed> {
    let marked = extensions.iter().any(|extension| {
        extension.url.as_deref() == Some(UNCLOSED_EXTENSION)
            && extension.value_boolean.unwrap_or_default()
    });
    marked.then(|| Unclosed {
        reasons: extensions
            .iter()
            .filter(|extension| extension.url.as_deref() == Some(UNCLOSED_REASON_EXTENSION))
            .filter_map(|extension| extension.value_string.clone())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> ExpandedValueSet {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    #[test]
    fn a_request_sends_only_the_parameters_the_reader_set() {
        let request = ExpandRequest {
            url: "https://terminology.example/ValueSet/all".to_owned(),
            count: Some(20),
            ..ExpandRequest::default()
        };
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
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
            request.append(RequestUrl::new()).render(""),
            "?url=https%3A%2F%2Fterminology.example%2FValueSet%2Fall&filter=fever&count=20\
             &offset=40&displayLanguage=nl-NL&activeOnly=true&includeDesignations=false",
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
        let rendered = request.append(RequestUrl::new()).render("");
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
            request.append(RequestUrl::new()).render(""),
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
        .expansion()
        .expect("the answer carries an expansion");
        assert_eq!((expansion.total, expansion.offset), (Some(173), Some(20)));
        assert_eq!(expansion.concepts.len(), 1);
    }

    #[test]
    fn a_value_set_with_no_expansion_reads_as_no_expansion() {
        assert_eq!(
            parse(r#"{"resourceType":"ValueSet","status":"active"}"#).expansion(),
            None,
            "the screen states that rather than drawing an empty table"
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
        .expansion()
        .expect("the answer carries an expansion");
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
        .expansion()
        .expect("the answer carries an expansion");
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
        .expansion()
        .expect("the answer carries an expansion");
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
            expansion.hierarchical(),
            "an answer with a nested entry is not a page of a flat expansion"
        );
    }

    #[test]
    fn a_flat_answer_is_counted_the_same_way_and_says_it_is_flat() {
        let expansion =
            parse(r#"{"expansion":{"total":2,"contains":[{"code":"a"},{"code":"b"}]}}"#)
                .expansion()
                .expect("the answer carries an expansion");
        assert_eq!(expansion.listed(), 2);
        assert!(
            !expansion.hierarchical(),
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
        .expansion()
        .expect("the answer carries an expansion");
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
        let expansion = parse(r#"{"expansion":{"total":2,"contains":[{"code":"a"}]}}"#)
            .expansion()
            .expect("the answer carries an expansion");
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
        .expansion()
        .expect("the answer carries an expansion");
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
        .expansion()
        .expect("the answer carries an expansion");
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
        .expansion()
        .expect("the answer carries an expansion");
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
        let expansion = parse(r#"{"expansion":{"total":-1,"offset":-5}}"#)
            .expansion()
            .expect("the answer carries an expansion");
        assert_eq!(
            (expansion.total, expansion.offset),
            (None, None),
            "a count below zero names no page, and the screen states the absence"
        );
    }

    #[test]
    fn an_answer_with_unknown_elements_still_reads() {
        let expansion = parse(
            r#"{"resourceType":"ValueSet","id":"x","meta":{"lastUpdated":"2026-09-07"},
                "expansion":{"identifier":"urn:uuid:2","timestamp":"2026-09-07T00:00:00Z",
                "property":[{"code":"parent"}],"contains":[{"code":"a"}]}}"#,
        )
        .expansion()
        .expect("the answer carries an expansion");
        assert_eq!(
            expansion.concepts.len(),
            1,
            "the reader ignores the elements it does not draw"
        );
    }
}
