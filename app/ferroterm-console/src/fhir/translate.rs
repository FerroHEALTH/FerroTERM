// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$translate`: the request each version takes, and its answer.
//!
//! The operation is spelled differently in every release this server mounts,
//! and the four `OperationDefinition`s are the authority for which name goes
//! on the wire (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>,
//! <https://hl7.org/fhir/R5/conceptmap-operation-translate.html>). R4 and R4B
//! take `code`, `system`, `version`, and `targetsystem`; R5 renamed the code
//! to `sourceCode` and the target system to `targetSystem`; the R6 ballot
//! renamed `system` and `version` to `sourceSystem` and `sourceVersion`
//! (<https://hl7.org/fhir/6.0.0-ballot5/conceptmap-operation-translate.html>).
//!
//! The answer differs the same way. R4 and R4B report a `match.equivalence`
//! and R5 and the R6 ballot a `match.relationship`, over two different code
//! systems of values, so the reader below keeps them apart and never renders
//! one as the other.

use crate::fhir::parameters::{Coding, Parameter};
use crate::fhir::version::FhirVersion;
use crate::url::RequestUrl;

/// The `$translate` parameter naming the concept map to translate through.
const URL_PARAMETER: &str = "url";

/// The `$translate` parameter carrying the concept map's business version.
const MAP_VERSION_PARAMETER: &str = "conceptMapVersion";

/// The names one FHIR version gives the parameters of `$translate`.
///
/// Every name here is read from that version's own `OperationDefinition`, so
/// a request is what the version defines rather than what another one does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Spelling {
    /// The parameter carrying the code to translate.
    code: &'static str,
    /// The parameter carrying the code system that code belongs to.
    system: &'static str,
    /// The parameter carrying that code system's version.
    system_version: &'static str,
    /// The parameter naming the code system an answer is wanted in.
    target_system: &'static str,
}

/// How `version` spells the parameters of `$translate`.
fn spelling(version: FhirVersion) -> Spelling {
    match version {
        FhirVersion::R4 | FhirVersion::R4B => Spelling {
            code: "code",
            system: "system",
            system_version: "version",
            target_system: "targetsystem",
        },
        FhirVersion::R5 => Spelling {
            code: "sourceCode",
            system: "system",
            system_version: "version",
            target_system: "targetSystem",
        },
        FhirVersion::R6 => Spelling {
            code: "sourceCode",
            system: "sourceSystem",
            system_version: "sourceVersion",
            target_system: "targetSystem",
        },
    }
}

/// The parameters one run of `$translate` sends.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TranslateRequest {
    /// The concept map canonical, empty for the server's own choice of map.
    pub(crate) concept_map: String,
    /// The concept map's business version.
    pub(crate) concept_map_version: String,
    /// The code system the code belongs to.
    pub(crate) system: String,
    /// That code system's version.
    pub(crate) system_version: String,
    /// The code to translate.
    pub(crate) code: String,
    /// The code system an answer is wanted in.
    pub(crate) target_system: String,
}

impl TranslateRequest {
    /// Whether the request names enough for the operation to run.
    ///
    /// `code` and `system` travel together: the operation documents that "a
    /// system must be provided" with a code
    /// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>), so a
    /// half-named concept is not sent and refused, it is not sent at all.
    pub(crate) fn runnable(&self) -> bool {
        !self.code.is_empty() && !self.system.is_empty()
    }

    /// Appends every parameter that was set, under `version`'s own names.
    ///
    /// A parameter the reader left empty is left out, so the server's own
    /// default applies to it rather than an empty string the viewer invented.
    pub(crate) fn append(&self, url: RequestUrl, version: FhirVersion) -> RequestUrl {
        let names = spelling(version);
        let mut url = url;
        for (name, value) in [
            (URL_PARAMETER, &self.concept_map),
            (MAP_VERSION_PARAMETER, &self.concept_map_version),
            (names.system, &self.system),
            (names.system_version, &self.system_version),
            (names.code, &self.code),
            (names.target_system, &self.target_system),
        ] {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        url
    }
}

/// What `$translate` answered, read from the `Parameters` its release
/// decoded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TranslateAnswer {
    /// `result`, whether the server found a translation.
    result: Option<bool>,
    /// `message`, the server's own words about the answer.
    message: Option<String>,
    /// Each `used-conceptmap`, in the server's order.
    used_maps: Vec<String>,
    /// Each `match`, in the server's order.
    matches: Vec<TranslationMatch>,
}

/// One `match` of a translation, with the elements the answer carried.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TranslationMatch {
    /// `match.equivalence`, which R4 and R4B answer.
    pub(crate) equivalence: Option<String>,
    /// `match.relationship`, which R5 and the R6 ballot answer.
    pub(crate) relationship: Option<String>,
    /// `match.noMap`, set where the map states that the code maps to nothing.
    pub(crate) no_map: bool,
    /// `match.concept`, the target the code translates to.
    pub(crate) target: Option<Coding>,
    /// `match.sourceConcept`, the source the match was made from.
    pub(crate) source_concept: Option<Coding>,
    /// The concept map this match came from, from `originMap` or R4's `source`.
    pub(crate) origin_map: Option<String>,
    /// `match.sourceComment`.
    pub(crate) source_comment: Option<String>,
    /// `match.targetComment`.
    pub(crate) target_comment: Option<String>,
    /// `match.product`, the values the mapping also produces.
    pub(crate) products: Vec<NamedValue>,
    /// `match.dependsOn`, which R5 and the R6 ballot answer.
    pub(crate) depends_on: Vec<NamedValue>,
    /// `match.property`, which R5 and the R6 ballot answer.
    pub(crate) properties: Vec<NamedValue>,
}

/// One named value of a `match`: what it is about, and what it says.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NamedValue {
    /// The attribute the value is about.
    pub(crate) name: String,
    /// The value, absent where the server sent a shape this viewer draws no
    /// text for.
    pub(crate) value: Option<String>,
}

impl TranslateAnswer {
    /// `result`, whether the server found a translation.
    pub(crate) fn result(&self) -> Option<bool> {
        self.result
    }

    /// `message`, the server's own words about the answer.
    pub(crate) fn message(&self) -> Option<String> {
        self.message.clone()
    }

    /// Every `used-conceptmap` the server names.
    pub(crate) fn used_maps(&self) -> Vec<String> {
        self.used_maps.clone()
    }

    /// Every `match`, in the server's order.
    pub(crate) fn matches(&self) -> Vec<TranslationMatch> {
        self.matches.clone()
    }
}

/// The answer the top-level parameters `read` make.
fn answer(read: &[Parameter]) -> TranslateAnswer {
    let first = |name: &str| read.iter().find(|parameter| parameter.name == name);
    TranslateAnswer {
        result: first("result").and_then(|parameter| parameter.flag),
        message: first("message").and_then(|parameter| parameter.text.clone()),
        used_maps: read
            .iter()
            .filter(|parameter| parameter.name == "used-conceptmap")
            .filter_map(|parameter| parameter.text.clone())
            .collect(),
        matches: read
            .iter()
            .filter(|parameter| parameter.name == "match")
            .map(read_match)
            .collect(),
    }
}

fn read_match(parameter: &Parameter) -> TranslationMatch {
    let mut found = TranslationMatch::default();
    for part in &parameter.parts {
        match part.name.as_str() {
            "equivalence" => found.equivalence.clone_from(&part.text),
            "relationship" => found.relationship.clone_from(&part.text),
            "noMap" => found.no_map = part.flag.unwrap_or_default(),
            "concept" => found.target.clone_from(&part.coding),
            "sourceConcept" => found.source_concept.clone_from(&part.coding),
            // NOTE: <https://hl7.org/fhir/R5/conceptmap-operation-translate.html>: R4 and R4B
            // name it `source` and R5 renamed it `originMap`, both the map's canonical.
            "originMap" | "source" => found.origin_map.clone_from(&part.text),
            "sourceComment" => found.source_comment.clone_from(&part.text),
            "targetComment" => found.target_comment.clone_from(&part.text),
            "product" => found.products.push(read_named_value(part)),
            "dependsOn" => found.depends_on.push(read_named_value(part)),
            "property" => found.properties.push(read_named_value(part)),
            _other => {}
        }
    }
    found
}

fn read_named_value(parameter: &Parameter) -> NamedValue {
    let mut found = NamedValue::default();
    for part in &parameter.parts {
        match part.name.as_str() {
            "element" | "attribute" | "uri" => found.name = part.text.clone().unwrap_or_default(),
            "concept" | "value" => found.value = part.rendered(),
            _other => {}
        }
    }
    found
}

impl From<Vec<Parameter>> for TranslateAnswer {
    fn from(parameters: Vec<Parameter>) -> Self {
        answer(&parameters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `json`, decoded as the R4B `Parameters` it completes with its
    /// `resourceType`.
    fn parse(json: &str) -> TranslateAnswer {
        parse_at(FhirVersion::R4B, json)
    }

    /// `json`, decoded as the `Parameters` of `version` it completes with its
    /// `resourceType`.
    fn parse_at(version: FhirVersion, json: &str) -> TranslateAnswer {
        let mut document: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(json).expect("the fixture is a JSON object");
        document
            .entry("resourceType")
            .or_insert_with(|| serde_json::Value::from("Parameters"));
        version
            .decode(&serde_json::Value::Object(document).to_string())
            .expect("the fixture is a valid Parameters at its version")
    }

    fn request() -> TranslateRequest {
        TranslateRequest {
            concept_map: "https://terminology.example/ConceptMap/m".to_owned(),
            concept_map_version: String::new(),
            system: "https://terminology.example/a".to_owned(),
            system_version: "2031".to_owned(),
            code: "x".to_owned(),
            target_system: "https://terminology.example/b".to_owned(),
        }
    }

    fn address(version: FhirVersion) -> String {
        request().append(RequestUrl::default(), version).render("")
    }

    /// An R4 or R4B answer, as `map_r4` writes one.
    const R4_ANSWER: &str = r#"{"resourceType":"Parameters","parameter":[
        {"name":"result","valueBoolean":true},
        {"name":"match","part":[
          {"name":"equivalence","valueCode":"wider"},
          {"name":"concept","valueCoding":{"system":"https://terminology.example/b",
            "code":"y","display":"Why"}},
          {"name":"product","part":[
            {"name":"element","valueUri":"https://terminology.example/attr"},
            {"name":"concept","valueCoding":{"code":"p"}}]},
          {"name":"source","valueCanonical":"https://terminology.example/ConceptMap/m|1"}]},
        {"name":"used-conceptmap","valueUri":"https://terminology.example/ConceptMap/m"}]}"#;

    /// An R5 or R6 answer, as `map_r5` writes one.
    const R5_ANSWER: &str = r#"{"resourceType":"Parameters","parameter":[
        {"name":"result","valueBoolean":true},
        {"name":"message","valueString":"one map was consulted"},
        {"name":"match","part":[
          {"name":"relationship","valueCode":"source-is-narrower-than-target"},
          {"name":"concept","valueCoding":{"system":"https://terminology.example/b",
            "code":"y","display":"Why"}},
          {"name":"product","part":[
            {"name":"attribute","valueUri":"https://terminology.example/attr"},
            {"name":"value","valueCoding":{"code":"p"}}]},
          {"name":"originMap","valueCanonical":"https://terminology.example/ConceptMap/m|1"}]}]}"#;

    #[test]
    fn r4_and_r4b_send_the_names_their_operation_definition_declares() {
        let expected = "?url=https%3A%2F%2Fterminology.example%2FConceptMap%2Fm\
             &system=https%3A%2F%2Fterminology.example%2Fa&version=2031&code=x\
             &targetsystem=https%3A%2F%2Fterminology.example%2Fb";
        assert_eq!(address(FhirVersion::R4), expected);
        assert_eq!(
            address(FhirVersion::R4B),
            expected,
            "R4 and R4B declare the same parameters"
        );
    }

    #[test]
    fn r5_renames_the_code_and_the_target_system() {
        assert_eq!(
            address(FhirVersion::R5),
            "?url=https%3A%2F%2Fterminology.example%2FConceptMap%2Fm\
             &system=https%3A%2F%2Fterminology.example%2Fa&version=2031&sourceCode=x\
             &targetSystem=https%3A%2F%2Fterminology.example%2Fb",
            "R5 keeps `system` and `version` and renames `code` and `targetsystem`"
        );
    }

    #[test]
    fn the_r6_ballot_renames_the_system_and_its_version_too() {
        assert_eq!(
            address(FhirVersion::R6),
            "?url=https%3A%2F%2Fterminology.example%2FConceptMap%2Fm\
             &sourceSystem=https%3A%2F%2Fterminology.example%2Fa&sourceVersion=2031&sourceCode=x\
             &targetSystem=https%3A%2F%2Fterminology.example%2Fb",
            "the R6 ballot renamed `system` and `version`"
        );
    }

    #[test]
    fn a_parameter_the_reader_left_empty_is_not_sent_at_all() {
        let sparse = TranslateRequest {
            system: "https://terminology.example/a".to_owned(),
            code: "x".to_owned(),
            ..TranslateRequest::default()
        };
        assert_eq!(
            sparse
                .append(RequestUrl::default(), FhirVersion::R5)
                .render(""),
            "?system=https%3A%2F%2Fterminology.example%2Fa&sourceCode=x",
            "the server's own default applies to a parameter the reader did not set"
        );
    }

    #[test]
    fn a_request_missing_the_system_of_its_code_is_not_sent() {
        let half = TranslateRequest {
            code: "x".to_owned(),
            ..TranslateRequest::default()
        };
        assert!(
            !half.runnable(),
            "the operation requires a system with a code"
        );
        assert!(request().runnable());
    }

    #[test]
    fn both_shapes_carry_the_same_target_coding() {
        let target = |json: &str| {
            parse(json)
                .matches()
                .first()
                .and_then(|found| found.target.clone())
        };
        let expected = Some(Coding {
            system: Some("https://terminology.example/b".to_owned()),
            version: None,
            code: Some("y".to_owned()),
            display: Some("Why".to_owned()),
        });
        assert_eq!(target(R4_ANSWER), expected);
        assert_eq!(
            target(R5_ANSWER),
            expected,
            "`match.concept` is the target in every version"
        );
    }

    #[test]
    fn a_product_reads_under_both_spellings_of_its_parts() {
        let product = |json: &str| {
            parse(json)
                .matches()
                .first()
                .and_then(|found| found.products.first().cloned())
        };
        let expected = Some(NamedValue {
            name: "https://terminology.example/attr".to_owned(),
            value: Some("p".to_owned()),
        });
        assert_eq!(
            product(R4_ANSWER),
            expected,
            "R4 names the parts `element` and `concept`"
        );
        assert_eq!(
            product(R5_ANSWER),
            expected,
            "R5 names them `attribute` and `value`"
        );
    }

    #[test]
    fn the_map_a_match_came_from_reads_under_both_spellings() {
        let origin = |json: &str| {
            parse(json)
                .matches()
                .first()
                .and_then(|found| found.origin_map.clone())
        };
        assert_eq!(
            origin(R4_ANSWER),
            Some("https://terminology.example/ConceptMap/m|1".to_owned()),
            "R4 names it `source`"
        );
        assert_eq!(
            origin(R5_ANSWER),
            Some("https://terminology.example/ConceptMap/m|1".to_owned()),
            "R5 names it `originMap`"
        );
    }

    #[test]
    fn the_top_level_parameters_are_read_by_name() {
        let answer = parse(R5_ANSWER);
        assert_eq!(answer.result(), Some(true));
        assert_eq!(answer.message(), Some("one map was consulted".to_owned()));
        assert_eq!(
            parse(R4_ANSWER).used_maps(),
            ["https://terminology.example/ConceptMap/m".to_owned()]
        );
    }

    #[test]
    fn a_match_that_maps_to_nothing_states_it_rather_than_drawing_an_empty_row() {
        let answer = parse(
            r#"{"parameter":[{"name":"result","valueBoolean":false},
                {"name":"match","part":[{"name":"noMap","valueBoolean":true},
                  {"name":"sourceConcept","valueCoding":{"code":"x"}}]}]}"#,
        );
        let matches = answer.matches();
        let first = matches.first().expect("the answer carries one match");
        assert!(first.no_map);
        assert!(
            first.relationship.is_none() && first.equivalence.is_none(),
            "a no-map match states no relation, which is what the server sent"
        );
        assert_eq!(answer.result(), Some(false));
    }

    #[test]
    fn the_matches_are_kept_in_the_order_the_server_reported_them() {
        let answer = parse(
            r#"{"parameter":[
                {"name":"match","part":[{"name":"concept","valueCoding":{"code":"b"}}]},
                {"name":"match","part":[{"name":"concept","valueCoding":{"code":"a"}}]}]}"#,
        );
        let codes: Vec<Option<String>> = answer
            .matches()
            .into_iter()
            .map(|found| found.target.and_then(|coding| coding.code))
            .collect();
        assert_eq!(
            codes,
            [Some("b".to_owned()), Some("a".to_owned())],
            "a reverse translation reports the edition's own concept order"
        );
    }

    #[test]
    fn a_dependency_and_a_property_are_read_where_a_version_answers_them() {
        let answer = parse(
            r#"{"parameter":[{"name":"match","part":[
                {"name":"dependsOn","part":[{"name":"attribute","valueUri":"https://x.example/d"},
                  {"name":"value","valueString":"left"}]},
                {"name":"property","part":[{"name":"uri","valueUri":"https://x.example/p"},
                  {"name":"value","valueBoolean":true}]}]}]}"#,
        );
        let matches = answer.matches();
        let first = matches.first().expect("the answer carries one match");
        assert_eq!(
            first.depends_on,
            [NamedValue {
                name: "https://x.example/d".to_owned(),
                value: Some("left".to_owned()),
            }]
        );
        assert_eq!(
            first.properties,
            [NamedValue {
                name: "https://x.example/p".to_owned(),
                value: Some("true".to_owned()),
            }]
        );
    }

    #[test]
    fn a_value_shape_the_viewer_draws_no_text_for_states_the_absence() {
        let answer = parse(
            r#"{"parameter":[{"name":"match","part":[
                {"name":"product","part":[{"name":"attribute","valueUri":"https://x.example/q"},
                  {"name":"value","valueQuantity":{"value":3,"unit":"mg"}}]}]}]}"#,
        );
        assert_eq!(
            answer
                .matches()
                .first()
                .and_then(|found| found.products.first().cloned()),
            Some(NamedValue {
                name: "https://x.example/q".to_owned(),
                value: None,
            }),
            "the screen says the value was not drawn rather than showing an empty cell"
        );
    }

    #[test]
    fn a_coding_renders_the_system_the_code_and_the_display() {
        assert_eq!(
            Coding {
                system: Some("https://x.example/s".to_owned()),
                version: None,
                code: Some("c".to_owned()),
                display: Some("See".to_owned()),
            }
            .rendered(),
            "https://x.example/s|c (See)"
        );
        assert_eq!(
            Coding {
                code: Some("c".to_owned()),
                ..Coding::default()
            }
            .rendered(),
            "c",
            "a coding with no system renders the code it does carry"
        );
        assert_eq!(Coding::default().rendered(), "");
    }

    #[test]
    fn an_answer_carrying_no_parameter_reads_as_no_match() {
        let answer = parse(r#"{"resourceType":"Parameters"}"#);
        assert_eq!(answer.matches(), Vec::<TranslationMatch>::new());
        assert_eq!(
            answer.result(),
            None,
            "the screen states the absence rather than reading it as a refusal"
        );
    }
}
