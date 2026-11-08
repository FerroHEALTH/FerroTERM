// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

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

use ferrobridge_term::outcome::{BatchOutcome, SubsumesOutcome, ValidateOutcome};
use ferrobridge_term::request::{
    CodeSystemValidation, Coded, Request, Subsumption, Target, Validation as Asked,
};

use crate::fhir::CODE_SYSTEM;
use crate::fhir::VALUE_SET;
use crate::fhir::error::FhirError;
use crate::fhir::operation::{Coding, ConceptEcho, mismatched};
use crate::fhir::outcome::{Classification, Outcome};
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr_with};

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

    /// The resource the run checks against: the stored one when the run
    /// names an id, the canonical otherwise.
    fn target(&self) -> Target {
        if self.instance() {
            Target::Instance(self.id.clone())
        } else {
            Target::Canonical(self.url.clone())
        }
    }

    /// The operation this run asks for, `None` for a value set run that
    /// names no system for its code.
    ///
    /// A parameter the reader left empty is left out, so the server's own
    /// default applies to it rather than an empty string the viewer invented.
    /// The canonical is left out of an instance-level run, where the address
    /// itself names the resource.
    pub(crate) fn request(&self) -> Option<Request> {
        match self.on {
            ValidateOn::CodeSystem => Some(Request::CodeSystemValidateCode(CodeSystemValidation {
                version: named(&self.resource_version),
                display: named(&self.display),
                display_language: named(&self.display_language),
                ..CodeSystemValidation::new(self.target(), self.code.as_str())
            })),
            ValidateOn::ValueSet => (!self.system.is_empty()).then(|| {
                Request::ValidateCode(Asked {
                    value_set: self.target(),
                    value_set_version: named(&self.resource_version),
                    date: None,
                    coded: Coded::Code {
                        system: self.system.clone(),
                        code: self.code.clone(),
                        version: named(&self.system_version),
                        display: named(&self.display),
                    },
                    display_language: named(&self.display_language),
                    active_only: None,
                })
            }),
        }
    }
}

/// `value`, unless the reader left it empty.
fn named(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
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

    /// The operation this run asks for.
    pub(crate) fn request(&self) -> Request {
        let code_system = if self.instance() {
            Target::Instance(self.id.clone())
        } else {
            Target::Canonical(self.system.clone())
        };
        Request::Subsumes(Subsumption {
            version: named(&self.version),
            ..Subsumption::on(code_system, self.code_a.as_str(), self.code_b.as_str())
        })
    }
}

/// What `$validate-code` answered: the answer, and the `issues` outcome a
/// reader is shown in their own language.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValidationAnswer {
    /// The answer, without its issues.
    read: Validation,
    /// The `issues` outcome, when the server sent one.
    issues: Option<Outcome>,
}

/// What `CodeSystem/$subsumes` answered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SubsumesAnswer {
    /// `outcome`, the relation the server found.
    outcome: Option<String>,
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
    pub(crate) concept: Option<ConceptEcho>,
    /// The itemised issues of the `issues` outcome.
    pub(crate) issues: Vec<ValidationIssue>,
}

impl ValidationAnswer {
    /// The answer `outcome` carries, the reading of the run sent to `url` at
    /// `version`.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Decode`] for an answer that is not a
    /// `$validate-code` answer.
    pub(crate) fn read(
        outcome: BatchOutcome,
        url: &str,
        version: FhirVersion,
    ) -> Result<Self, FhirError> {
        let (BatchOutcome::ValidateCode(outcome) | BatchOutcome::CodeSystemValidateCode(outcome)) =
            outcome
        else {
            return Err(mismatched(url, version));
        };
        let result = outcome.is_valid();
        let (display, message, issues, details) = match outcome {
            ValidateOutcome::Valid {
                display,
                message,
                outcome,
                details,
            }
            | ValidateOutcome::Invalid {
                display,
                message,
                outcome,
                details,
            } => (display, message, outcome, *details),
            _ => return Err(mismatched(url, version)),
        };
        // NOTE: both lists name a code system the server does not hold, the first for
        // the code checked and the second for a coding of a `codeableConcept`; a
        // reader needs the canonical either way, so they render as one.
        let mut unknown_systems = details.caused_by_unknown_systems;
        unknown_systems.extend(details.unknown_systems);
        Ok(Self {
            read: Validation {
                result: Some(result),
                message,
                display,
                code: details.code,
                normalized_code: details.normalized_code,
                system: details.system,
                version: details.version,
                inactive: details.inactive,
                status: details.status,
                unknown_systems,
                concept: details.codeable_concept.map(|concept| ConceptEcho {
                    codings: concept.coding.into_iter().map(Coding::from).collect(),
                    text: concept.text,
                }),
                issues: Vec::new(),
            },
            issues: issues.as_ref().map(Outcome::from),
        })
    }

    /// The `$validate-code` answer, its issues worded for `locale`.
    pub(crate) fn validation(&self, locale: &ferrohealth_ui::i18n::Locale) -> Validation {
        Validation {
            issues: self
                .issues
                .as_ref()
                .map(|outcome| itemised(locale, outcome))
                .unwrap_or_default(),
            ..self.read.clone()
        }
    }
}

impl SubsumesAnswer {
    /// The answer `outcome` carries, the reading of the run sent to `url` at
    /// `version`.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Decode`] for an answer that is not a `$subsumes`
    /// answer.
    pub(crate) fn read(
        outcome: &BatchOutcome,
        url: &str,
        version: FhirVersion,
    ) -> Result<Self, FhirError> {
        match outcome {
            BatchOutcome::Subsumes(subsumption) => Ok(Self {
                outcome: Some(SubsumesOutcome::as_str(*subsumption).to_owned()),
            }),
            _ => Err(mismatched(url, version)),
        }
    }

    /// `outcome`, the `$subsumes` answer, when the server sent one.
    pub(crate) fn subsumption(&self) -> Option<String> {
        self.outcome.clone()
    }
}

/// The issues of the `issues` outcome, in the order the server wrote them.
fn itemised(locale: &ferrohealth_ui::i18n::Locale, outcome: &Outcome) -> Vec<ValidationIssue> {
    outcome
        .issues()
        .iter()
        .map(|issue| ValidationIssue {
            severity: issue.severity_word(),
            classifications: issue.classifications.clone(),
            issue_code: issue.code.clone().unwrap_or_else(|| "unknown".to_owned()),
            text: issue.text(locale),
            expressions: issue.expressions.clone(),
        })
        .collect()
}

/// What one `ConceptSubsumptionOutcome` code says about two codes.
///
/// The four codes are `equivalent`, `subsumes`, `subsumed-by`, and
/// `not-subsumed` (<https://hl7.org/fhir/R4B/valueset-concept-subsumption-outcome.html>).
/// A code outside that set is stated verbatim rather than guessed at, so a
/// server that answers something new is not misreported.
pub(crate) fn subsumption_sentence(
    locale: &ferrohealth_ui::i18n::Locale,
    outcome: &str,
    code_a: &str,
    code_b: &str,
) -> String {
    match outcome {
        "equivalent" => tr_with(
            locale,
            Msg::SameConcept,
            &[
                ("code_a", (code_a).to_string().into()),
                ("code_b", (code_b).to_string().into()),
            ],
        ),
        "subsumes" => tr_with(
            locale,
            Msg::SubsumesDescendant,
            &[
                ("code_a", (code_a).to_string().into()),
                ("code_b", (code_b).to_string().into()),
            ],
        ),
        "subsumed-by" => tr_with(
            locale,
            Msg::SubsumedDescendant,
            &[
                ("code_a", (code_a).to_string().into()),
                ("code_b", (code_b).to_string().into()),
            ],
        ),
        "not-subsumed" => tr_with(
            locale,
            Msg::NeitherNorSubsumesOther,
            &[
                ("code_a", (code_a).to_string().into()),
                ("code_b", (code_b).to_string().into()),
            ],
        ),
        other => tr_with(
            locale,
            Msg::ServerAnsweredOutcomeWhichNotOne,
            &[("other", (other).to_string().into())],
        ),
    }
}

#[cfg(test)]
mod tests;
