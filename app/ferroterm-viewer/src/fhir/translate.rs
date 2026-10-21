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

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::url::RequestUrl;

/// The `$translate` parameter naming the concept map to translate through.
const URL_PARAMETER: &str = "url";

/// The `$translate` parameter carrying the concept map's business version.
const MAP_VERSION_PARAMETER: &str = "conceptMapVersion";

/// The `$translate` parameter carrying a concept map inline.
const CONCEPT_MAP_PARAMETER: &str = "conceptMap";

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

    /// The `Parameters` body a `POST` run sends, carrying `map` inline.
    ///
    /// An operation invoked by `POST` carries every parameter in a
    /// `Parameters` body (<https://hl7.org/fhir/R4B/operations.html#request>),
    /// and `$translate` defines a `conceptMap` parameter that carries the map
    /// itself, "provided directly as part of the request"
    /// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>). That
    /// is what lets a map that has never been saved be translated through.
    /// `url` is left out: the two name the map two ways, and a server that
    /// took both would have to choose between them.
    pub(crate) fn body(&self, version: FhirVersion, map: &serde_json::Value) -> String {
        let names = spelling(version);
        let mut parameters = vec![json!({"name": CONCEPT_MAP_PARAMETER, "resource": map})];
        // Each `value[x]` is the type that version's OperationDefinition
        // declares for the parameter: the code is a `code`, a system is a
        // `uri`, and a system version is a `string`.
        for (name, kind, value) in [
            (names.code, "valueCode", &self.code),
            (names.system, "valueUri", &self.system),
            (names.system_version, "valueString", &self.system_version),
            (names.target_system, "valueUri", &self.target_system),
        ] {
            if !value.is_empty() {
                parameters.push(json!({"name": name, kind: value}));
            }
        }
        json!({"resourceType": tr(Msg::Parameters), "parameter": parameters}).to_string()
    }
}

/// The `Parameters` answer of `$translate`, as the screen reads it.
///
/// The operation answers a `Parameters` resource
/// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>), which is
/// a list of named parameters with nested parts, so it is read by name here
/// rather than modelled per version.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct TranslateAnswer {
    /// `Parameters.parameter`, in the order the server sent it.
    #[serde(default)]
    parameter: Vec<WireParameter>,
}

/// One `Parameters.parameter`, with the `value[x]` shapes the answer uses.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct WireParameter {
    /// `parameter.name`.
    name: Option<String>,
    /// `valueBoolean`.
    #[serde(rename = "valueBoolean")]
    boolean: Option<bool>,
    /// `valueString`.
    #[serde(rename = "valueString")]
    string: Option<String>,
    /// `valueCode`.
    #[serde(rename = "valueCode")]
    code: Option<String>,
    /// `valueUri`.
    #[serde(rename = "valueUri")]
    uri: Option<String>,
    /// `valueCanonical`.
    #[serde(rename = "valueCanonical")]
    canonical: Option<String>,
    /// `valueId`.
    #[serde(rename = "valueId")]
    id: Option<String>,
    /// `valueInteger`, which FHIR defines as 32-bit signed.
    #[serde(rename = "valueInteger")]
    integer: Option<i32>,
    /// `valueDateTime`.
    #[serde(rename = "valueDateTime")]
    date_time: Option<String>,
    /// `valueCoding`.
    #[serde(rename = "valueCoding")]
    coding: Option<WireCoding>,
    /// `parameter.part`, the nested parameters of a `match`.
    #[serde(default)]
    part: Vec<WireParameter>,
}

/// A `Coding` as the screen draws one.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Coding {
    /// `Coding.system`.
    pub(crate) system: Option<String>,
    /// `Coding.version`.
    pub(crate) version: Option<String>,
    /// `Coding.code`.
    pub(crate) code: Option<String>,
    /// `Coding.display`.
    pub(crate) display: Option<String>,
}

/// The wire shape of a `Coding`, which is the same shape the screen draws.
type WireCoding = Coding;

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

/// How a match states the way its target relates to the source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Relation {
    /// The element that carried it, which differs by FHIR version.
    pub(crate) label: Msg,
    /// The code, as the server wrote it.
    pub(crate) code: String,
}

impl TranslateAnswer {
    /// `result`, whether the server considers the code translated.
    pub(crate) fn result(&self) -> Option<bool> {
        self.first("result").and_then(|parameter| parameter.boolean)
    }

    /// `message`, the server's own sentence about the run.
    pub(crate) fn message(&self) -> Option<String> {
        self.first("message").and_then(WireParameter::text)
    }

    /// `used-conceptmap`, the maps the server says it translated through.
    pub(crate) fn used_maps(&self) -> Vec<String> {
        self.parameter
            .iter()
            .filter(|parameter| parameter.name.as_deref() == Some("used-conceptmap"))
            .filter_map(WireParameter::text)
            .collect()
    }

    /// The matches, in the order the server reported them.
    ///
    /// The order is the server's and is never re-sorted here: a reverse
    /// translation reports its matches in the edition's own concept order, and
    /// a client that re-ordered them would hide that.
    pub(crate) fn matches(&self) -> Vec<TranslationMatch> {
        self.parameter
            .iter()
            .filter(|parameter| parameter.name.as_deref() == Some("match"))
            .map(read_match)
            .collect()
    }

    /// The first parameter named `name`, when the answer carries one.
    fn first(&self, name: &str) -> Option<&WireParameter> {
        self.parameter
            .iter()
            .find(|parameter| parameter.name.as_deref() == Some(name))
    }
}

impl TranslationMatch {
    /// How this match states its relation, under the element that carried it.
    ///
    /// R4 and R4B answer a `ConceptMapEquivalence` code and R5 and the R6
    /// ballot a `ConceptMapRelationship` code, and the two code sets are not
    /// the same. Both are reported under their own name, so a reader is never
    /// shown one version's answer as though it were another's.
    pub(crate) fn relations(&self) -> Vec<Relation> {
        let mut stated = Vec::new();
        if let Some(code) = self.equivalence.clone() {
            stated.push(Relation {
                label: Msg::Equivalence,
                code,
            });
        }
        if let Some(code) = self.relationship.clone() {
            stated.push(Relation {
                label: Msg::Relationship,
                code,
            });
        }
        stated
    }
}

/// One `match` parameter, read into the elements the screen draws.
fn read_match(parameter: &WireParameter) -> TranslationMatch {
    let mut found = TranslationMatch::default();
    for part in &parameter.part {
        match part.name.as_deref().unwrap_or_default() {
            "equivalence" => found.equivalence = part.text(),
            "relationship" => found.relationship = part.text(),
            "noMap" => found.no_map = part.boolean.unwrap_or_default(),
            "concept" => found.target.clone_from(&part.coding),
            "sourceConcept" => found.source_concept.clone_from(&part.coding),
            // R4 and R4B name it `source` and R5 renamed it `originMap`; both
            // carry the canonical of the map the mapping came from.
            "originMap" | "source" => found.origin_map = part.text(),
            "sourceComment" => found.source_comment = part.text(),
            "targetComment" => found.target_comment = part.text(),
            "product" => found.products.push(read_named_value(part)),
            "dependsOn" => found.depends_on.push(read_named_value(part)),
            "property" => found.properties.push(read_named_value(part)),
            _other => {}
        }
    }
    found
}

/// One `product`, `dependsOn`, or `property` part, as a name and a value.
///
/// R4 and R4B name the attribute `element` and carry the value as a `Coding`
/// under `concept`; R5 and the R6 ballot name it `attribute` and carry a
/// `value[x]`, and `property` names it `uri`. Every spelling is read, so the
/// row is drawn whichever version answered.
fn read_named_value(parameter: &WireParameter) -> NamedValue {
    let mut found = NamedValue::default();
    for part in &parameter.part {
        match part.name.as_deref().unwrap_or_default() {
            "element" | "attribute" | "uri" => found.name = part.text().unwrap_or_default(),
            "concept" | "value" => found.value = part.rendered(),
            _other => {}
        }
    }
    found
}

impl WireParameter {
    /// The value as plain text, for the `value[x]` shapes that are one string.
    fn text(&self) -> Option<String> {
        self.string
            .clone()
            .or_else(|| self.code.clone())
            .or_else(|| self.uri.clone())
            .or_else(|| self.canonical.clone())
            .or_else(|| self.id.clone())
            .or_else(|| self.date_time.clone())
    }

    /// The value as the text a reader is shown, `Coding`s included.
    ///
    /// A shape this viewer draws no text for reads as `None`, which the screen
    /// states rather than rendering as an empty cell.
    fn rendered(&self) -> Option<String> {
        self.text()
            .or_else(|| self.boolean.map(|flag| flag.to_string()))
            .or_else(|| self.integer.map(|number| number.to_string()))
            .or_else(|| self.coding.as_ref().map(Coding::rendered))
    }
}

impl Coding {
    /// The coding as one line: the code, its system, and its display.
    pub(crate) fn rendered(&self) -> String {
        let code = match (self.system.as_deref(), self.code.as_deref()) {
            (Some(system), Some(code)) => format!("{system}|{code}"),
            (None, Some(code)) => code.to_owned(),
            (Some(system), None) => system.to_owned(),
            (None, None) => String::new(),
        };
        match self.display.as_deref() {
            Some(display) if code.is_empty() => display.to_owned(),
            Some(display) => format!("{code} ({display})"),
            None => code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TranslateAnswer {
        serde_json::from_str(json).expect("the fixture is valid JSON")
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
        request().append(RequestUrl::new(), version).render("")
    }

    /// The `conceptMap` parameter carries the map itself, and `url` is left
    /// out, because the two name the map two ways
    /// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).
    #[test]
    fn a_post_body_carries_the_map_inline_under_each_version_s_own_names() {
        let map = serde_json::json!({"resourceType": "ConceptMap", "status": "draft"});
        let r4b = request().body(FhirVersion::R4B, &map);
        assert_eq!(
            r4b,
            r#"{"parameter":[{"name":"conceptMap","resource":{"resourceType":"ConceptMap","status":"draft"}},{"name":"code","valueCode":"x"},{"name":"system","valueUri":"https://terminology.example/a"},{"name":"version","valueString":"2031"},{"name":"targetsystem","valueUri":"https://terminology.example/b"}],"resourceType":"Parameters"}"#
        );
        let r5 = request().body(FhirVersion::R5, &map);
        assert!(
            r5.contains(r#"{"name":"sourceCode","valueCode":"x"}"#)
                && r5.contains(r#"{"name":"system","valueUri""#)
                && r5.contains(r#"{"name":"targetSystem","valueUri""#),
            "R5 renamed the code and the target system and kept `system`: {r5}"
        );
        let r6 = request().body(FhirVersion::R6, &map);
        assert!(
            r6.contains(r#"{"name":"sourceSystem","valueUri""#)
                && r6.contains(r#"{"name":"sourceVersion","valueString""#),
            "the R6 ballot renamed the source system and its version: {r6}"
        );
        for body in [r4b, r5, r6] {
            assert!(
                !body.contains(r#""name":"url""#),
                "a map sent inline is never also named by canonical: {body}"
            );
        }
    }

    /// A parameter the reader left empty is left out, so the server's own
    /// default applies to it rather than an empty string the viewer invented.
    #[test]
    fn a_post_body_leaves_out_every_parameter_that_was_not_set() {
        let map = serde_json::json!({"resourceType": "ConceptMap"});
        let request = TranslateRequest {
            system: "https://terminology.example/a".to_owned(),
            code: "x".to_owned(),
            ..TranslateRequest::default()
        };
        let body = request.body(FhirVersion::R4B, &map);
        assert!(!body.contains("targetsystem"), "{body}");
        assert!(!body.contains(r#""name":"version""#), "{body}");
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
            sparse.append(RequestUrl::new(), FhirVersion::R5).render(""),
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
    fn an_r4_answer_reports_an_equivalence_and_never_a_relationship() {
        let matches = parse(R4_ANSWER).matches();
        let first = matches.first().expect("the answer carries one match");
        assert_eq!(
            first.relations(),
            vec![Relation {
                label: Msg::Equivalence,
                code: "wider".to_owned(),
            }],
            "`wider` is a ConceptMapEquivalence code and means nothing as a relationship"
        );
        assert_eq!(first.relationship, None);
    }

    #[test]
    fn an_r5_answer_reports_a_relationship_and_never_an_equivalence() {
        let matches = parse(R5_ANSWER).matches();
        let first = matches.first().expect("the answer carries one match");
        assert_eq!(
            first.relations(),
            vec![Relation {
                label: Msg::Relationship,
                code: "source-is-narrower-than-target".to_owned(),
            }]
        );
        assert_eq!(first.equivalence, None);
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
            first.relations().is_empty(),
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
