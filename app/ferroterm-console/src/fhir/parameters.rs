// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Parameters` an operation answers, as the operation readers walk it.
//!
//! `$lookup`, `$validate-code`, `$subsumes` and `$translate` each answer a
//! `Parameters`, a list of named parameters whose names the operation
//! defines (<https://hl7.org/fhir/R4B/operations.html#response>). The list is
//! decoded through the `fhir-types` module of the version that answered, and
//! each reader picks its parameters by name from the walk below.

use crate::fhir::outcome::Outcome;
use crate::fhir::release::Reads;

/// One parameter as the readers walk it: its name, its value in the shapes
/// the readers draw, and its parts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Parameter {
    /// `name`.
    pub(crate) name: String,
    /// A textual value: a string, a code, a URI, a canonical, an id or a
    /// date and time.
    pub(crate) text: Option<String>,
    /// `valueBoolean`.
    pub(crate) flag: Option<bool>,
    /// `valueInteger`.
    pub(crate) number: Option<i32>,
    /// `valueDecimal`, in the lexical form the server wrote.
    pub(crate) decimal: Option<String>,
    /// `valueCoding`.
    pub(crate) coding: Option<Coding>,
    /// `valueCodeableConcept`.
    pub(crate) concept: Option<ConceptEcho>,
    /// `resource`, where it is the `OperationOutcome` of the answer.
    pub(crate) outcome: Option<Outcome>,
    /// `part`.
    pub(crate) parts: Vec<Parameter>,
}

/// A `Coding` as the screens draw one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

/// The `CodeableConcept` an answer echoes, as the screen draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConceptEcho {
    /// Each coding, with the elements the answer carried.
    pub(crate) codings: Vec<Coding>,
    /// The wording the client sent with the concept.
    pub(crate) text: Option<String>,
}

impl Parameter {
    /// The value as one line of text, whichever type carried it.
    pub(crate) fn rendered(&self) -> Option<String> {
        self.text
            .clone()
            .or_else(|| self.flag.map(|flag| flag.to_string()))
            .or_else(|| self.number.map(|number| number.to_string()))
            .or_else(|| self.decimal.clone())
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

/// Walks the `Parameters` of the `fhir-types` module `$module`.
macro_rules! reads_parameters {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::coding;
            use fhir_types::$module::parameters::{
                Parameters, ParametersParameter, ParametersParameterValue,
            };
            use fhir_types::$module::resource::Resource;

            use super::{Coding, ConceptEcho, Parameter, Reads};

            /// One `Coding`, as the screens draw it.
            fn coding(coding: &coding::Coding) -> Coding {
                Coding {
                    system: coding
                        .system
                        .as_ref()
                        .and_then(|system| system.value.clone()),
                    version: coding
                        .version
                        .as_ref()
                        .and_then(|version| version.value.clone()),
                    code: coding.code.as_ref().and_then(|code| code.value.clone()),
                    display: coding
                        .display
                        .as_ref()
                        .and_then(|display| display.value.clone()),
                }
            }

            /// One parameter and its parts, as the readers walk them.
            fn read(parameter: &ParametersParameter) -> Parameter {
                let mut read = Parameter {
                    name: parameter.name.value.clone().unwrap_or_default(),
                    parts: parameter.part.iter().map(read).collect(),
                    ..Parameter::default()
                };
                match &parameter.value {
                    Some(ParametersParameterValue::String(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Code(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Uri(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Canonical(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Id(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::DateTime(value)) => {
                        read.text.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Boolean(value)) => read.flag = value.value,
                    Some(ParametersParameterValue::Integer(value)) => read.number = value.value,
                    Some(ParametersParameterValue::Decimal(value)) => {
                        read.decimal.clone_from(&value.value);
                    }
                    Some(ParametersParameterValue::Coding(value)) => {
                        read.coding = Some(coding(value));
                    }
                    Some(ParametersParameterValue::CodeableConcept(value)) => {
                        read.concept = Some(ConceptEcho {
                            codings: value.coding.iter().map(coding).collect(),
                            text: value.text.as_ref().and_then(|text| text.value.clone()),
                        });
                    }
                    _ => {}
                }
                if let Some(Resource::OperationOutcome(outcome)) = &parameter.resource {
                    read.outcome = Some(outcome.read());
                }
                read
            }

            impl Reads<Vec<Parameter>> for Parameters {
                fn read(&self) -> Vec<Parameter> {
                    self.parameter.iter().map(read).collect()
                }
            }
        }
    };
}

reads_parameters!(r4);
reads_parameters!(r4b);
reads_parameters!(r5);
reads_parameters!(r6);
