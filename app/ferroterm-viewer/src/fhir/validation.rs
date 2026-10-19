//! `$validate-code` and `$subsumes`: the requests they take, and the answers.
//!
//! `CodeSystem/$validate-code` checks a code against a code system and
//! `ValueSet/$validate-code` checks it against a value set
//! (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>,
//! <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>). Both
//! answer a `Parameters`, and the interesting answers are the ones that say
//! `result` is false: a wrong display comes back corrected, an inactive
//! concept comes back marked, a code from a system the server does not hold
//! comes back with that system named.
//!
//! `CodeSystem/$subsumes` answers how two codes of one system relate
//! (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).
//!
//! Reading lives here, outside every component, so the screen renders values
//! plain unit tests can pin.

use serde::{Deserialize, Serialize};

use crate::fhir::CODE_SYSTEM;
use crate::fhir::VALUE_SET;
use crate::fhir::capability::CapabilityStatement;
use crate::fhir::concept::CODE_PARAMETER;
use crate::fhir::concept::SYSTEM_PARAMETER;
use crate::fhir::concept::VERSION_PARAMETER;
use crate::fhir::expansion::DISPLAY_LANGUAGE_PARAMETER;
use crate::fhir::expansion::URL_PARAMETER;
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::translate::Coding;
use crate::url::RequestUrl;

/// The `$validate-code` parameter carrying the display the client asserts.
pub(crate) const DISPLAY_PARAMETER: &str = "display";

/// The `ValueSet/$validate-code` parameter carrying the value set's version.
pub(crate) const VALUE_SET_VERSION_PARAMETER: &str = "valueSetVersion";

/// The `ValueSet/$validate-code` parameter carrying the code system's version.
///
/// A value set run names two versions: `valueSetVersion` for the value set and
/// `systemVersion` for the code system the code belongs to.
pub(crate) const SYSTEM_VERSION_PARAMETER: &str = "systemVersion";

/// The `$subsumes` parameter carrying the first code.
pub(crate) const CODE_A_PARAMETER: &str = "codeA";

/// The `$subsumes` parameter carrying the second code.
pub(crate) const CODE_B_PARAMETER: &str = "codeB";

/// The operation code of `$validate-code`, without the `$` a URL spells it with.
pub(crate) const VALIDATE_CODE_OPERATION: &str = "validate-code";

/// The operation code of `$subsumes`, without the `$` a URL spells it with.
pub(crate) const SUBSUMES_OPERATION: &str = "subsumes";

/// The invocation level a request against one stored resource is made at.
///
/// `OperationDefinition.instance` declares it
/// (<https://hl7.org/fhir/R4B/operationdefinition-definitions.html#OperationDefinition.instance>),
/// and the server states the levels it declares on each operation of its
/// `CapabilityStatement`.
const INSTANCE_LEVEL: &str = "instance";

/// Which resource type a `$validate-code` run is invoked on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ValidateOn {
    /// `CodeSystem/$validate-code`: the code is checked against a code system.
    #[default]
    CodeSystem,
    /// `ValueSet/$validate-code`: the code is checked against a value set.
    ValueSet,
}

impl ValidateOn {
    /// The resource type the operation is invoked on.
    pub(crate) fn resource_type(self) -> &'static str {
        match self {
            Self::CodeSystem => CODE_SYSTEM,
            Self::ValueSet => VALUE_SET,
        }
    }

    /// The word the viewer's own address carries this choice as.
    pub(crate) fn segment(self) -> &'static str {
        match self {
            Self::CodeSystem => "codesystem",
            Self::ValueSet => "valueset",
        }
    }

    /// The choice `segment` names, falling back to the code system form.
    ///
    /// An address a reader typed names anything at all, and a word that names
    /// no form reads as the default rather than wedging the screen.
    pub(crate) fn read(segment: &str) -> Self {
        if segment.trim() == Self::ValueSet.segment() {
            Self::ValueSet
        } else {
            Self::CodeSystem
        }
    }
}

/// What one root declares about running an operation on a resource type.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Offered {
    /// Whether the root declares the operation at all.
    pub(crate) declared: bool,
    /// Whether it declares the instance level, where a stored resource's id
    /// stands in for the canonical.
    pub(crate) instance: bool,
}

/// What `statement` declares about `operation` on `resource_type`.
///
/// An affordance appears only where the statement declares it, so the screen
/// never offers a run this root then refuses, and the instance-level field
/// appears only where the version's own `OperationDefinition` declares that
/// level. Nothing here is a table of versions: the document answers.
pub(crate) fn offered(
    statement: &CapabilityStatement,
    resource_type: &str,
    operation: &str,
) -> Offered {
    let wanted = operation.trim_start_matches('$');
    let instance = statement
        .declarations()
        .into_iter()
        .filter(|declared| declared.resource == resource_type && declared.code == wanted)
        .any(|declared| declared.levels.iter().any(|level| level == INSTANCE_LEVEL));
    Offered {
        declared: statement.declares_operation(resource_type, operation),
        instance,
    }
}

/// The parameters one `$validate-code` run sends.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValidateRequest {
    /// Which resource type the operation is invoked on.
    pub(crate) on: ValidateOn,
    /// The canonical of the resource the code is checked against.
    pub(crate) url: String,
    /// The id of the stored resource, for the instance-level form.
    pub(crate) id: String,
    /// The business version of that resource.
    pub(crate) resource_version: String,
    /// The code system the code belongs to, which only a value set run names.
    pub(crate) system: String,
    /// That code system's version.
    pub(crate) system_version: String,
    /// The code to check.
    pub(crate) code: String,
    /// The display the client asserts, which the server checks.
    pub(crate) display: String,
    /// The BCP 47 tag the display is asserted in.
    pub(crate) display_language: String,
}

impl ValidateRequest {
    /// Whether the request names enough for the operation to run.
    ///
    /// One of `url` or `codeSystem` must be given unless the operation is
    /// called at the instance level
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>),
    /// and a value set run names the code system the code belongs to as well,
    /// because the code alone does not say which system it is from. A
    /// half-named run is not sent and refused, it is not sent at all.
    pub(crate) fn runnable(&self) -> bool {
        if self.code.is_empty() || (self.url.is_empty() && self.id.is_empty()) {
            return false;
        }
        match self.on {
            ValidateOn::CodeSystem => true,
            ValidateOn::ValueSet => !self.system.is_empty(),
        }
    }

    /// Whether this run addresses one stored resource rather than a canonical.
    pub(crate) fn instance(&self) -> bool {
        !self.id.is_empty()
    }

    /// Appends every parameter that was set, under the operation's own names.
    ///
    /// A parameter the reader left empty is left out, so the server's own
    /// default applies to it rather than an empty string the viewer invented.
    /// The canonical is left out of an instance-level run, where the address
    /// itself names the resource.
    pub(crate) fn append(&self, url: RequestUrl) -> RequestUrl {
        let mut url = url;
        let named: [(&str, &str); 6] = match self.on {
            ValidateOn::CodeSystem => [
                (URL_PARAMETER, &self.url),
                (VERSION_PARAMETER, &self.resource_version),
                ("", ""),
                ("", ""),
                (CODE_PARAMETER, &self.code),
                (DISPLAY_PARAMETER, &self.display),
            ],
            ValidateOn::ValueSet => [
                (URL_PARAMETER, &self.url),
                (VALUE_SET_VERSION_PARAMETER, &self.resource_version),
                (SYSTEM_PARAMETER, &self.system),
                (SYSTEM_VERSION_PARAMETER, &self.system_version),
                (CODE_PARAMETER, &self.code),
                (DISPLAY_PARAMETER, &self.display),
            ],
        };
        for (name, value) in named {
            if name.is_empty() || value.is_empty() {
                continue;
            }
            if name == URL_PARAMETER && self.instance() {
                continue;
            }
            url = url.query(name, value);
        }
        if !self.display_language.is_empty() {
            url = url.query(DISPLAY_LANGUAGE_PARAMETER, &self.display_language);
        }
        url
    }
}

/// The parameters one `CodeSystem/$subsumes` run sends.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SubsumesRequest {
    /// The code system both codes belong to.
    pub(crate) system: String,
    /// That code system's version.
    pub(crate) version: String,
    /// The id of the stored `CodeSystem`, for the instance-level form.
    pub(crate) id: String,
    /// The first code.
    pub(crate) code_a: String,
    /// The second code.
    pub(crate) code_b: String,
}

impl SubsumesRequest {
    /// Whether the request names enough for the operation to run.
    ///
    /// The operation takes two codes and the system they are from
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>); an
    /// instance-level run takes the system from the address instead.
    pub(crate) fn runnable(&self) -> bool {
        !self.code_a.is_empty()
            && !self.code_b.is_empty()
            && (!self.system.is_empty() || !self.id.is_empty())
    }

    /// Whether this run addresses one stored resource rather than a canonical.
    pub(crate) fn instance(&self) -> bool {
        !self.id.is_empty()
    }

    /// Appends every parameter that was set, under the operation's own names.
    pub(crate) fn append(&self, url: RequestUrl) -> RequestUrl {
        let mut url = url;
        if !self.instance() && !self.system.is_empty() {
            url = url.query(SYSTEM_PARAMETER, &self.system);
        }
        for (name, value) in [
            (VERSION_PARAMETER, &self.version),
            (CODE_A_PARAMETER, &self.code_a),
            (CODE_B_PARAMETER, &self.code_b),
        ] {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        url
    }
}

/// The `Parameters` answer of one operation, as the screen reads it.
///
/// Both operations answer a `Parameters` resource, which is a list of named
/// parameters (<https://hl7.org/fhir/R4B/operations.html#response>), so it is
/// read by name here rather than modelled per version.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct ParametersAnswer {
    /// `Parameters.parameter`, in the order the server sent it.
    #[serde(default)]
    parameter: Vec<WireParameter>,
}

/// One `Parameters.parameter`, with the shapes these two answers use.
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
    /// `valueCodeableConcept`, the concept a `codeableConcept` run echoes.
    #[serde(rename = "valueCodeableConcept")]
    concept: Option<WireConcept>,
    /// `parameter.resource`, which carries the `issues` outcome.
    resource: Option<OperationOutcome>,
}

/// The `CodeableConcept` an answer echoes, as the screen draws it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct WireConcept {
    /// `CodeableConcept.coding`, each with the elements the answer carried.
    #[serde(default)]
    pub(crate) coding: Vec<Coding>,
    /// `CodeableConcept.text`, the wording the client sent with the concept.
    pub(crate) text: Option<String>,
}

/// One issue of the `issues` outcome, as the screen renders it.
///
/// `details.coding` leads, because the terminology ecosystem binds it to its
/// own `tx-issue-type` code system as a `SHALL` while leaving `issue.code`
/// unbound (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>). A
/// reader deciding what to do next reads the classification the server was
/// required to state, and `issue.code` beside it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValidationIssue {
    /// `issue.severity`, as the word the server wrote.
    pub(crate) severity: String,
    /// The codes of `issue.details.coding`, in the order they arrived.
    pub(crate) classifications: Vec<Classification>,
    /// `issue.code`, the `IssueType` code, which the binding leaves open.
    pub(crate) issue_code: String,
    /// `details.text`, or `diagnostics` where the server sent no details.
    pub(crate) text: String,
    /// `issue.expression`, the paths of the input the issue is about.
    pub(crate) expressions: Vec<String>,
}

/// One `issue.details.coding`: the classification, and what defines it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Classification {
    /// The code, as the server wrote it.
    pub(crate) code: String,
    /// The system that defines the code.
    pub(crate) system: String,
}

/// The answer of one `$validate-code` run, as the screen renders it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Validation {
    /// `result`, whether the code is valid where it was checked.
    pub(crate) result: Option<bool>,
    /// `message`, the server's own sentence about the run.
    pub(crate) message: Option<String>,
    /// `display`, the display the system prefers for the code.
    pub(crate) display: Option<String>,
    /// `code`, the code that was validated.
    pub(crate) code: Option<String>,
    /// `normalized-code`, the code as the system spells it.
    pub(crate) normalized_code: Option<String>,
    /// `system`, the code system it was validated against.
    pub(crate) system: Option<String>,
    /// `version`, the code system version it was validated in.
    pub(crate) version: Option<String>,
    /// `inactive`, whether the concept is inactive in its code system.
    pub(crate) inactive: Option<bool>,
    /// `status`, the concept's status where its system states one.
    pub(crate) status: Option<String>,
    /// The canonicals of the code systems the server does not hold.
    pub(crate) unknown_systems: Vec<String>,
    /// `codeableConcept`, the concept the run echoed back.
    pub(crate) concept: Option<WireConcept>,
    /// The itemised issues of the `issues` outcome.
    pub(crate) issues: Vec<ValidationIssue>,
}

impl ParametersAnswer {
    /// The `$validate-code` answer, read in one pass over the parameters.
    ///
    /// A repeated parameter keeps its first value, except the two that are
    /// declared as repeating, which keep every one in the order they arrived.
    pub(crate) fn validation(&self) -> Validation {
        let mut read = Validation::default();
        for parameter in &self.parameter {
            match parameter.name.as_deref().unwrap_or_default() {
                "result" => set(&mut read.result, parameter.boolean),
                "message" => set(&mut read.message, parameter.text()),
                "display" => set(&mut read.display, parameter.text()),
                "code" => set(&mut read.code, parameter.text()),
                "normalized-code" => set(&mut read.normalized_code, parameter.text()),
                "system" => set(&mut read.system, parameter.text()),
                "version" => set(&mut read.version, parameter.text()),
                "inactive" => set(&mut read.inactive, parameter.boolean),
                "status" => set(&mut read.status, parameter.text()),
                // Both name a code system the server does not hold: the first
                // for the code that was checked, the second for a coding of a
                // `codeableConcept` input. A reader needs the canonical either
                // way, so the two lists render as one.
                "x-caused-by-unknown-system" | "x-unknown-system" => {
                    read.unknown_systems.extend(parameter.text());
                }
                "codeableConcept" => set(&mut read.concept, parameter.concept.clone()),
                "issues" if read.issues.is_empty() => {
                    read.issues = parameter
                        .resource
                        .as_ref()
                        .map(itemised)
                        .unwrap_or_default();
                }
                _other => {}
            }
        }
        read
    }

    /// `outcome`, the `$subsumes` answer, when the server sent one.
    pub(crate) fn subsumption(&self) -> Option<String> {
        self.parameter
            .iter()
            .find(|parameter| parameter.name.as_deref() == Some("outcome"))
            .and_then(WireParameter::text)
    }
}

/// Keeps the first value a repeated parameter carried.
fn set<T>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() {
        *slot = value;
    }
}

/// The issues of the `issues` outcome, in the order the server wrote them.
fn itemised(outcome: &OperationOutcome) -> Vec<ValidationIssue> {
    outcome
        .issue
        .iter()
        .map(|issue| ValidationIssue {
            severity: issue.severity.clone().unwrap_or_else(|| "error".to_owned()),
            classifications: issue
                .details
                .as_ref()
                .map(|details| {
                    details
                        .coding
                        .iter()
                        .map(|coding| Classification {
                            code: coding.code.clone().unwrap_or_default(),
                            system: coding.system.clone().unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            issue_code: issue.code.clone().unwrap_or_else(|| "unknown".to_owned()),
            text: issue
                .details
                .as_ref()
                .and_then(|details| details.text.clone())
                .or_else(|| issue.diagnostics.clone())
                .unwrap_or_else(|| "the server sent no diagnostic".to_owned()),
            expressions: issue.expression.clone(),
        })
        .collect()
}

impl WireParameter {
    /// The value as plain text, for the `value[x]` shapes that are one string.
    fn text(&self) -> Option<String> {
        self.string
            .clone()
            .or_else(|| self.code.clone())
            .or_else(|| self.uri.clone())
            .or_else(|| self.canonical.clone())
    }
}

impl Validation {
    /// The one sentence a live region announces about this answer.
    ///
    /// The result is a word rather than a tint, and an inactive concept is
    /// announced as an answer rather than as a failure
    /// (<https://www.w3.org/TR/WCAG22/#use-of-color>).
    pub(crate) fn sentence(&self) -> String {
        let mut said = match self.result {
            Some(true) => String::from("The server answered result true: the code is valid."),
            Some(false) => String::from("The server answered result false: the code is not valid."),
            None => String::from("The server answered no result."),
        };
        if self.inactive == Some(true) {
            said.push_str(" The concept is inactive in its code system.");
        }
        if let Some(message) = &self.message {
            said.push(' ');
            said.push_str(message);
        }
        said
    }
}

/// What one `ConceptSubsumptionOutcome` code says about two codes.
///
/// The four codes are `equivalent`, `subsumes`, `subsumed-by`, and
/// `not-subsumed` (<https://hl7.org/fhir/R4B/valueset-concept-subsumption-outcome.html>).
/// A code outside that set is stated verbatim rather than guessed at, so a
/// server that answers something new is not misreported.
pub(crate) fn subsumption_sentence(outcome: &str, code_a: &str, code_b: &str) -> String {
    match outcome {
        "equivalent" => format!("{code_a} and {code_b} are the same concept."),
        "subsumes" => {
            format!("{code_a} subsumes {code_b}: {code_b} is a descendant of {code_a}.")
        }
        "subsumed-by" => {
            format!("{code_a} is subsumed by {code_b}: {code_a} is a descendant of {code_b}.")
        }
        "not-subsumed" => {
            format!("Neither {code_a} nor {code_b} subsumes the other.")
        }
        other => format!(
            "The server answered the outcome `{other}`, which is not one of the four this viewer has wording for."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `CapabilityStatement` `GET /r4b/metadata` answered.
    const R4B: &str = include_str!("../../fixtures/capability-statement-r4b.json");

    fn parse(json: &str) -> ParametersAnswer {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    fn statement() -> CapabilityStatement {
        serde_json::from_str(R4B).expect("the recorded statement is valid JSON")
    }

    fn code_system_run() -> ValidateRequest {
        ValidateRequest {
            on: ValidateOn::CodeSystem,
            url: "https://terminology.example/animals".to_owned(),
            resource_version: "2.0".to_owned(),
            code: "cat".to_owned(),
            display: "Dog".to_owned(),
            ..ValidateRequest::default()
        }
    }

    #[test]
    fn a_code_system_run_sends_the_names_that_operation_declares() {
        assert_eq!(
            code_system_run().append(RequestUrl::new()).render(""),
            "?url=https%3A%2F%2Fterminology.example%2Fanimals&version=2.0&code=cat&display=Dog",
            "the code system's own version travels as `version`"
        );
    }

    #[test]
    fn a_value_set_run_names_two_versions_under_their_own_parameters() {
        let request = ValidateRequest {
            on: ValidateOn::ValueSet,
            url: "https://terminology.example/vs".to_owned(),
            resource_version: "1.2".to_owned(),
            system: "https://terminology.example/animals".to_owned(),
            system_version: "2.0".to_owned(),
            code: "cat".to_owned(),
            display_language: "nl-NL".to_owned(),
            ..ValidateRequest::default()
        };
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
            "?url=https%3A%2F%2Fterminology.example%2Fvs&valueSetVersion=1.2\
             &system=https%3A%2F%2Fterminology.example%2Fanimals&systemVersion=2.0\
             &code=cat&displayLanguage=nl-NL",
            "the value set's version and the code system's version are two parameters"
        );
    }

    #[test]
    fn an_instance_run_leaves_the_canonical_out_of_the_query() {
        let request = ValidateRequest {
            id: "animals".to_owned(),
            ..code_system_run()
        };
        assert!(request.instance());
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
            "?version=2.0&code=cat&display=Dog",
            "the address names the resource, so naming it again in `url` would be a second claim"
        );
    }

    #[test]
    fn a_half_named_run_is_not_sent_at_all() {
        assert!(!ValidateRequest::default().runnable(), "nothing was named");
        assert!(
            !ValidateRequest {
                code: String::new(),
                ..code_system_run()
            }
            .runnable(),
            "a system with no code asks nothing"
        );
        assert!(
            !ValidateRequest {
                on: ValidateOn::ValueSet,
                system: String::new(),
                ..code_system_run()
            }
            .runnable(),
            "a code alone does not say which system it is from"
        );
        assert!(code_system_run().runnable());
    }

    #[test]
    fn a_subsumes_run_sends_both_codes_and_the_system_they_are_from() {
        let request = SubsumesRequest {
            system: "https://terminology.example/animals".to_owned(),
            version: "2.0".to_owned(),
            code_a: "cat".to_owned(),
            code_b: "kitten".to_owned(),
            ..SubsumesRequest::default()
        };
        assert!(request.runnable());
        assert_eq!(
            request.append(RequestUrl::new()).render(""),
            "?system=https%3A%2F%2Fterminology.example%2Fanimals&version=2.0&codeA=cat&codeB=kitten"
        );
        assert!(
            !SubsumesRequest {
                code_b: String::new(),
                ..request.clone()
            }
            .runnable(),
            "one code is not a comparison"
        );
        assert_eq!(
            SubsumesRequest {
                id: "animals".to_owned(),
                system: String::new(),
                ..request
            }
            .append(RequestUrl::new())
            .render(""),
            "?version=2.0&codeA=cat&codeB=kitten",
            "an instance run takes the system from the address"
        );
    }

    #[test]
    fn the_levels_an_affordance_needs_are_read_from_the_recorded_statement() {
        let statement = statement();
        assert_eq!(
            offered(&statement, CODE_SYSTEM, VALIDATE_CODE_OPERATION),
            Offered {
                declared: true,
                instance: true,
            },
            "this root declares the operation and the instance level of it"
        );
        assert_eq!(
            offered(&statement, CODE_SYSTEM, SUBSUMES_OPERATION),
            Offered {
                declared: true,
                instance: true,
            }
        );
        assert!(offered(&statement, VALUE_SET, VALIDATE_CODE_OPERATION).declared);
        assert_eq!(
            offered(&statement, VALUE_SET, SUBSUMES_OPERATION),
            Offered::default(),
            "an operation no resource type declares is offered nowhere"
        );
    }

    #[test]
    fn a_root_that_declares_nothing_offers_nothing() {
        assert_eq!(
            offered(
                &CapabilityStatement::default(),
                CODE_SYSTEM,
                VALIDATE_CODE_OPERATION
            ),
            Offered::default(),
            "a statement the viewer could not read offers no affordance at all"
        );
    }

    #[test]
    fn the_choice_of_resource_type_survives_the_address() {
        for on in [ValidateOn::CodeSystem, ValidateOn::ValueSet] {
            assert_eq!(ValidateOn::read(on.segment()), on);
        }
        assert_eq!(
            ValidateOn::read("neither"),
            ValidateOn::CodeSystem,
            "an address a reader typed cannot wedge the screen"
        );
    }

    #[test]
    fn a_wrong_display_comes_back_corrected_with_its_classification() {
        let answer = parse(
            r#"{"resourceType":"Parameters","parameter":[
                {"name":"result","valueBoolean":false},
                {"name":"message","valueString":"Wrong display name"},
                {"name":"display","valueString":"Cat"},
                {"name":"code","valueCode":"cat"},
                {"name":"system","valueUri":"https://terminology.example/animals"},
                {"name":"version","valueString":"2.0"},
                {"name":"issues","resource":{"resourceType":"OperationOutcome","issue":[
                  {"severity":"error","code":"invalid",
                   "details":{"coding":[{"system":"https://example.org/tx-issue-type",
                     "code":"invalid-display"}],"text":"Wrong display name"},
                   "expression":["Coding.display"]}]}}]}"#,
        );
        let read = answer.validation();
        assert_eq!(read.result, Some(false));
        assert_eq!(
            read.display.as_deref(),
            Some("Cat"),
            "the display the system prefers comes back beside the refusal"
        );
        assert_eq!(read.version.as_deref(), Some("2.0"));
        assert_eq!(
            read.issues,
            vec![ValidationIssue {
                severity: "error".to_owned(),
                classifications: vec![Classification {
                    code: "invalid-display".to_owned(),
                    system: "https://example.org/tx-issue-type".to_owned(),
                }],
                issue_code: "invalid".to_owned(),
                text: "Wrong display name".to_owned(),
                expressions: vec!["Coding.display".to_owned()],
            }],
            "the coding leads, and the IssueType code travels beside it"
        );
        assert_eq!(
            read.sentence(),
            "The server answered result false: the code is not valid. Wrong display name"
        );
    }

    #[test]
    fn an_inactive_concept_is_an_answer_rather_than_a_failure() {
        let read = parse(
            r#"{"parameter":[{"name":"result","valueBoolean":true},
                {"name":"code","valueCode":"dodo"},
                {"name":"inactive","valueBoolean":true},
                {"name":"status","valueCode":"retired"},
                {"name":"message","valueString":"The concept is retired."}]}"#,
        )
        .validation();
        assert_eq!(read.result, Some(true));
        assert_eq!(read.inactive, Some(true));
        assert_eq!(read.status.as_deref(), Some("retired"));
        assert_eq!(
            read.sentence(),
            "The server answered result true: the code is valid. \
             The concept is inactive in its code system. The concept is retired.",
            "the inactivity is announced as part of the answer"
        );
    }

    #[test]
    fn every_unknown_system_the_answer_names_is_kept_in_order() {
        let read = parse(
            r#"{"parameter":[{"name":"result","valueBoolean":false},
                {"name":"x-caused-by-unknown-system","valueCanonical":"https://a.example"},
                {"name":"x-unknown-system","valueCanonical":"https://b.example"},
                {"name":"x-caused-by-unknown-system","valueCanonical":"https://c.example"}]}"#,
        )
        .validation();
        assert_eq!(
            read.unknown_systems,
            vec![
                "https://a.example".to_owned(),
                "https://b.example".to_owned(),
                "https://c.example".to_owned(),
            ],
            "a validator is told every system this server does not hold"
        );
    }

    #[test]
    fn a_codeable_concept_answer_keeps_its_text_and_its_codings() {
        let read = parse(
            r#"{"parameter":[{"name":"result","valueBoolean":true},
                {"name":"codeableConcept","valueCodeableConcept":{
                   "text":"a cat",
                   "coding":[{"system":"https://terminology.example/animals","version":"2.0",
                              "code":"cat","display":"Cat"}]}}]}"#,
        )
        .validation();
        let concept = read.concept.expect("the answer echoed the concept");
        assert_eq!(concept.text.as_deref(), Some("a cat"));
        assert_eq!(
            concept.coding.first().map(Coding::rendered),
            Some("https://terminology.example/animals|cat (Cat)".to_owned())
        );
        assert_eq!(
            concept
                .coding
                .first()
                .and_then(|coding| coding.version.clone()),
            Some("2.0".to_owned()),
            "the version the coding was validated in is kept"
        );
    }

    #[test]
    fn an_answer_with_no_issues_reads_as_an_empty_list_rather_than_a_failure() {
        let read = parse(r#"{"parameter":[{"name":"result","valueBoolean":true}]}"#).validation();
        assert!(read.issues.is_empty());
        assert_eq!(
            read.sentence(),
            "The server answered result true: the code is valid."
        );
    }

    #[test]
    fn an_answer_the_viewer_cannot_read_states_that_rather_than_inventing_one() {
        let read = parse(r#"{"resourceType":"Parameters"}"#).validation();
        assert_eq!(read, Validation::default());
        assert_eq!(read.sentence(), "The server answered no result.");
    }

    #[test]
    fn an_issue_with_no_wording_still_produces_a_line() {
        let read = parse(
            r#"{"parameter":[{"name":"issues","resource":{"issue":[{"code":"processing"}]}}]}"#,
        )
        .validation();
        assert_eq!(
            read.issues,
            vec![ValidationIssue {
                severity: "error".to_owned(),
                classifications: Vec::new(),
                issue_code: "processing".to_owned(),
                text: "the server sent no diagnostic".to_owned(),
                expressions: Vec::new(),
            }],
            "a refusal never renders as nothing"
        );
    }

    #[test]
    fn every_subsumption_outcome_is_a_sentence_as_well_as_a_code() {
        assert_eq!(
            subsumption_sentence("equivalent", "a", "b"),
            "a and b are the same concept."
        );
        assert_eq!(
            subsumption_sentence("subsumes", "a", "b"),
            "a subsumes b: b is a descendant of a."
        );
        assert_eq!(
            subsumption_sentence("subsumed-by", "a", "b"),
            "a is subsumed by b: a is a descendant of b."
        );
        assert_eq!(
            subsumption_sentence("not-subsumed", "a", "b"),
            "Neither a nor b subsumes the other."
        );
        assert_eq!(
            subsumption_sentence("mystery", "a", "b"),
            "The server answered the outcome `mystery`, which is not one of the four this viewer has wording for.",
            "an answer the viewer does not know is stated rather than guessed at"
        );
    }

    #[test]
    fn the_subsumption_outcome_is_read_from_the_parameter_that_carries_it() {
        assert_eq!(
            parse(r#"{"parameter":[{"name":"outcome","valueCode":"subsumes"}]}"#).subsumption(),
            Some("subsumes".to_owned())
        );
        assert_eq!(
            parse(r#"{"parameter":[{"name":"result","valueBoolean":true}]}"#).subsumption(),
            None,
            "an answer carrying no outcome is stated rather than read as one"
        );
    }
}
