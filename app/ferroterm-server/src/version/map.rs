// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated request and response contracts of every served version mapped
//! to and from the engine's neutral inputs and outcomes.
//!
//! One macro serves R4, R4B, R5 and the R6 ballot. `$flavour` names the
//! family (`r4` for R4 and R4B, `r5` and `r6` for the R5 family), and the
//! arms below the body state only what the generated contracts make
//! different: R5 renames the `$translate` inputs and states `relationship`
//! where R4 states `equivalence`, answers `definition` on `$lookup`, and
//! carries `source` and `additionalUse`; the R6 ballot adds a few inputs.

pub(crate) mod input;

macro_rules! family_map {
    ($fhir:ident, $flavour:ident) => {
        pub mod map {
            //! The wire of the operations for one version.
            //!
            //! The generated request becomes the engine's neutral input, and the
            //! neutral outcome the generated response, so the version emits exactly what
            //! its `OperationDefinition`s declare
            //! (<https://hl7.org/fhir/R5/terminology-service.html>); what one family
            //! declares and the other does not is a `$flavour` arm of the macro.

            use concept_graph::subsumption::Outcome;
            use fhir_terminology::operations::expand::ExpandInput;
            use fhir_terminology::operations::lookup::{LookupInput, LookupOutcome};
            use fhir_terminology::operations::subsumes::SubsumesInput;
            use fhir_terminology::operations::translate::{TranslateInput, Translation};
            use fhir_terminology::operations::validate_code::{ValidateCodeInput, ValidationOutcome};
            use fhir_terminology::operations::value_set_validate_code::{
                TX_ISSUE_TYPE, Validation, ValueSetValidateInput,
            };
            use fhir_terminology::operations::{CodeableConceptRef, CodingRef, Issue, MESSAGE_ID_URL};
            use fhir_terminology::operations::translate::Match;
            use fhir_terminology::provider::{Designation, DesignationUse, Property, PropertyValue, Subproperty};
            use fhir_terminology::{conceptmap, valueset};
            use fhir_types::$fhir::codeable_concept::CodeableConcept;
            use fhir_types::$fhir::coding::Coding;
            use fhir_types::$fhir::extension::{Extension, ExtensionValue};
            use fhir_types::$fhir::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
            use fhir_types::$fhir::operations::code_system_lookup::{
                CodeSystemLookupRequest, CodeSystemLookupResponse, CodeSystemLookupResponseDesignation,
                CodeSystemLookupResponseProperty, CodeSystemLookupResponsePropertySubproperty,
            };
            use fhir_types::$fhir::operations::code_system_subsumes::{
                CodeSystemSubsumesRequest, CodeSystemSubsumesResponse,
            };
            use fhir_types::$fhir::operations::code_system_validate_code::{
                CodeSystemValidateCodeRequest, CodeSystemValidateCodeResponse,
            };
            use fhir_types::$fhir::operations::concept_map_translate::{
                ConceptMapTranslateRequest, ConceptMapTranslateResponse, ConceptMapTranslateResponseMatch,
                ConceptMapTranslateResponseMatchProduct,
            };
            use fhir_types::$fhir::operations::value_set_expand::ValueSetExpandRequest;
            use fhir_types::$fhir::operations::value_set_validate_code::{
                ValueSetValidateCodeRequest, ValueSetValidateCodeResponse,
            };
            use fhir_types::$fhir::parameters::{Parameters, ParametersParameterValue};
            use fhir_types::$fhir::primitives::{Boolean, Canonical, Integer, Uri};

            $crate::version::map::input::family_input!($fhir, $flavour);

            /// The `$lookup` parameters this version answers on their own rather than
            /// as a `property`.
            const OWN_PARAMETERS: &[&str] = $crate::version::map::family_map!(@own_parameters $flavour);

            $crate::version::map::family_map!(@update $flavour,
            /// The `$lookup` outcome as the version's response: `name`, `version`,
            /// `display`, R5's `definition`, `designation`, and `property` with its
            /// `subproperty` parts, then the ecosystem's `code`, `system`, and
            /// `abstract`.
            #[must_use]
            pub fn lookup_response(outcome: LookupOutcome) -> CodeSystemLookupResponse {
                CodeSystemLookupResponse {
                    name: outcome.name.into(),
                    version: outcome.version.map(Into::into),
                    display: outcome.display.into(),
                    designation: outcome.designations.into_iter().map(designation).collect(),
                    property: outcome
                        .properties
                        .into_iter()
                        .filter(|property| !OWN_PARAMETERS.contains(&property.code.as_str()))
                        .map(property)
                        .collect(),
                    code: Some(outcome.code.into()),
                    system: Some(outcome.system.into()),
                    r#abstract: abstract_of(outcome.abstract_concept),
                    ..$crate::version::map::family_map!(@definition $flavour, outcome.definition)
                }
            }
            );

            $crate::version::map::family_map!(@update $flavour,
            /// A `$lookup` property as the response's; R5 adds a `source`, which
            /// stays empty.
            fn property(property: Property) -> CodeSystemLookupResponseProperty {
                CodeSystemLookupResponseProperty {
                    code: property.code.into(),
                    value: Some(parameter_value(&property.value)),
                    description: property.description.map(Into::into),
                    subproperty: property.subproperties.iter().map(subproperty).collect(),
                    ..Default::default()
                }
            }
            );

            $crate::version::map::family_map!(@subproperty $flavour);

            $crate::version::map::family_map!(@update $flavour,
            /// A designation of the `$lookup` response; R5 adds an `additionalUse`,
            /// which stays empty.
            fn designation(d: Designation) -> CodeSystemLookupResponseDesignation {
                CodeSystemLookupResponseDesignation {
                    language: d.language.map(Into::into),
                    r#use: d.use_.map(use_coding),
                    value: d.value.into(),
                    ..Default::default()
                }
            }
            );

            /// The use of a designation as a `Coding` of the version.
            fn use_coding(designation_use: DesignationUse) -> Coding {
                Coding {
                    system: Some(designation_use.system.into()),
                    code: Some(designation_use.code.into()),
                    display: designation_use.display.map(Into::into),
                    ..Default::default()
                }
            }

            /// `abstract`, answered for an abstract concept only.
            // NOTE: the ecosystem's cases expect no `abstract = false`.
            fn abstract_of(abstract_concept: bool) -> Option<fhir_types::$fhir::primitives::Boolean> {
                abstract_concept.then_some(true.into())
            }

            /// A property value as a `Parameters` value.
            #[must_use]
            pub fn parameter_value(value: &PropertyValue) -> ParametersParameterValue {
                match value {
                    PropertyValue::Code(c) => ParametersParameterValue::Code(c.as_str().into()),
                    PropertyValue::Uri(u) => ParametersParameterValue::Uri(u.as_str().into()),
                    PropertyValue::Coding {
                        system,
                        code,
                        display,
                    } => ParametersParameterValue::Coding(Box::new(Coding {
                        system: Some(system.as_str().into()),
                        code: Some(code.as_str().into()),
                        display: display.as_ref().map(|d| d.as_str().into()),
                        ..Default::default()
                    })),
                    PropertyValue::String(s) => ParametersParameterValue::String(s.as_str().into()),
                    PropertyValue::Integer(i) => match i32::try_from(*i) {
                        Ok(i) => ParametersParameterValue::Integer(i.into()),
                        Err(_) => ParametersParameterValue::String(i.to_string().into()),
                    },
                    PropertyValue::Boolean(b) => ParametersParameterValue::Boolean((*b).into()),
                    PropertyValue::DateTime(d) => ParametersParameterValue::DateTime(d.as_str().into()),
                    PropertyValue::Decimal(d) => ParametersParameterValue::Decimal(d.as_str().into()),
                }
            }

            /// The `$subsumes` outcome as the response.
            #[must_use]
            pub fn subsumes_response(outcome: Outcome) -> CodeSystemSubsumesResponse {
                CodeSystemSubsumesResponse {
                    outcome: outcome.code().into(),
                }
            }

            /// The `CodeSystem/$validate-code` outcome as the response: `result`,
            /// `message`, `display`, the validated `code`, `system`, and `version`, and
            /// the itemised `issues`.
            #[must_use]
            pub fn validate_code_response(outcome: ValidationOutcome) -> CodeSystemValidateCodeResponse {
                CodeSystemValidateCodeResponse {
                    result: outcome.result.into(),
                    message: outcome.message.map(Into::into),
                    display: outcome.display.map(Into::into),
                    code: outcome.code.map(Into::into),
                    normalized_code: outcome.normalized_code.map(Into::into),
                    system: outcome.system.map(Into::into),
                    version: outcome.version.map(Into::into),
                    codeable_concept: outcome.codeable_concept.as_ref().map(concept_of),
                    issues: issues(&outcome.issues),
                    x_caused_by_unknown_system: outcome
                        .unknown_systems
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    x_unknown_system: outcome
                        .x_unknown_systems
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    inactive: outcome.inactive.map(Into::into),
                    status: outcome.status.map(Into::into),
                }
            }

            /// A neutral `codeableConcept` echoed as the `CodeableConcept` of the version.
            fn concept_of(concept: &CodeableConceptRef) -> CodeableConcept {
                CodeableConcept {
                    coding: concept.coding.iter().map(coding_of).collect(),
                    text: concept.text.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// The `Coding` of a `tx-issue-type` code, for `issue.details.coding`.
            #[must_use]
            pub fn tx_issue_coding(kind: &str) -> Coding {
                Coding {
                    system: Some(TX_ISSUE_TYPE.into()),
                    code: Some(kind.into()),
                    ..Default::default()
                }
            }

            /// The itemised issues as the `issues` `OperationOutcome`; none when empty.
            fn issues(list: &[Issue]) -> Option<OperationOutcome> {
                if list.is_empty() {
                    return None;
                }
                Some(OperationOutcome {
                    issue: list
                        .iter()
                        .map(|issue| OperationOutcomeIssue {
                            extension: vec![Extension {
                                url: String::from(MESSAGE_ID_URL),
                                value: Some(ExtensionValue::String(issue.message_id().into())),
                                ..Default::default()
                            }],
                            severity: issue.severity.into(),
                            code: issue.code.into(),
                            details: Some(CodeableConcept {
                                coding: vec![tx_issue_coding(issue.kind)],
                                text: Some(issue.text.as_str().into()),
                                ..Default::default()
                            }),
                            expression: issue
                                .expression
                                .as_deref()
                                .map(Into::into)
                                .into_iter()
                                .collect(),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                })
            }

            /// The `ValueSet/$validate-code` outcome as the `Parameters`: `result`,
            /// `message`, `display`, the validated `code`, `system`, and `version`, and
            /// the itemised `issues`.
            #[must_use]
            pub fn value_set_validation_parameters(validation: &Validation) -> Parameters {
                ValueSetValidateCodeResponse {
                    result: validation.result.into(),
                    message: validation.message.as_deref().map(Into::into),
                    display: validation.display.as_deref().map(Into::into),
                    code: validation.code.as_deref().map(Into::into),
                    normalized_code: validation.normalized_code.as_deref().map(Into::into),
                    system: validation.system.as_deref().map(Into::into),
                    version: validation.version.as_deref().map(Into::into),
                    codeable_concept: validation.codeable_concept.as_ref().map(concept_of),
                    issues: issues(&validation.issues),
                    x_caused_by_unknown_system: validation
                        .unknown_systems
                        .iter()
                        .map(|s| s.as_str().into())
                        .collect(),
                    x_unknown_system: validation
                        .x_unknown_systems
                        .iter()
                        .map(|s| s.as_str().into())
                        .collect(),
                    inactive: validation.inactive.map(Into::into),
                    status: validation.status.as_deref().map(Into::into),
                }
                .to_parameters()
            }

            /// A neutral coding as a `Coding` of the version.
            fn coding_of(coding: &CodingRef) -> Coding {
                Coding {
                    system: coding.system.as_deref().map(Into::into),
                    version: coding.version.as_deref().map(Into::into),
                    code: coding.code.as_deref().map(Into::into),
                    display: coding.display.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// The `ConceptMap` a `$closure` call answers with: one element per
            /// relationship the server is telling the client about.
            ///
            /// R4 and R4B state the relationship as `equivalence`
            /// (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>), R5 and
            /// the R6 ballot as `relationship`
            /// (<https://hl7.org/fhir/R5/conceptmap-operation-closure.html>).
            #[must_use]
            pub fn closure_map(
                id: &str,
                name: &str,
                version: u32,
                edges: &[fhir_terminology::operations::closure::Edge],
            ) -> fhir_types::$fhir::concept_map::ConceptMap {
                use fhir_types::$fhir::concept_map::{
                    ConceptMap, ConceptMapGroup, ConceptMapGroupElement,
                    ConceptMapGroupElementTarget,
                };
                let mut groups: Vec<ConceptMapGroup> = Vec::new();
                for edge in edges {
                    let target = ConceptMapGroupElementTarget {
                        code: Some(edge.target.code.as_str().into()),
                        ..$crate::version::map::family_map!(@closure_target $flavour, edge)
                    };
                    let element = ConceptMapGroupElement {
                        code: Some(edge.source.code.as_str().into()),
                        target: vec![target],
                        ..Default::default()
                    };
                    match groups.iter_mut().find(|held| {
                        held.source.as_ref().map(|s| s.value.as_deref())
                            == Some(Some(edge.source.system.as_str()))
                            && held.target.as_ref().map(|t| t.value.as_deref())
                                == Some(Some(edge.target.system.as_str()))
                    }) {
                        Some(held) => held.element.push(element),
                        None => groups.push(ConceptMapGroup {
                            source: Some(edge.source.system.as_str().into()),
                            target: Some(edge.target.system.as_str().into()),
                            element: vec![element],
                            ..Default::default()
                        }),
                    }
                }
                ConceptMap {
                    id: Some(id.to_owned()),
                    name: Some(name.into()),
                    status: "active".into(),
                    experimental: Some(true.into()),
                    version: Some(version.to_string().as_str().into()),
                    group: groups,
                    ..Default::default()
                }
            }

            $crate::version::map::family_map!(@translation_match $flavour);

            /// The `$translate` outcome as the `Parameters`: `result`, `message`, each
            /// `match`, and the overlay's `used-conceptmap`.
            #[must_use]
            pub fn translation_parameters(translation: &Translation) -> Parameters {
                ConceptMapTranslateResponse {
                    result: translation.result.into(),
                    message: translation.message.as_deref().map(Into::into),
                    used_conceptmap: translation
                        .used_concept_maps
                        .iter()
                        .map(|c| c.as_str().into())
                        .collect(),
                    used_system: Vec::new(),
                    r#match: translation.matches.iter().map(translation_match).collect(),
                }
                .to_parameters()
            }

        }
    };
    // R4 and R4B answer the closure relationship as `equivalence`, R5 and the R6
    // ballot as `relationship`.
    (@closure_target r4, $edge:ident) => {
        ConceptMapGroupElementTarget {
            equivalence: $edge.relationship.equivalence().into(),
            ..Default::default()
        }
    };
    (@closure_target $flavour:ident, $edge:ident) => {
        ConceptMapGroupElementTarget {
            relationship: $edge.relationship.relationship().into(),
            ..Default::default()
        }
    };
    // R4 and R4B declare every field a shared item sets where R5 declares one
    // more, which the item's struct update leaves empty.
    (@update r4, $item:item) => {
        #[expect(
            clippy::needless_update,
            reason = "R4 and R4B declare every field this sets; the update fills the field only R5 declares"
        )]
        $item
    };
    (@update $flavour:ident, $item:item) => {
        $item
    };
    // A part of a structured `$lookup` property; R5 adds a `source`, which stays
    // empty.
    (@subproperty r4) => {
        fn subproperty(part: &Subproperty) -> CodeSystemLookupResponsePropertySubproperty {
            CodeSystemLookupResponsePropertySubproperty {
                code: part.code.as_str().into(),
                value: parameter_value(&part.value),
                description: part.description.as_deref().map(Into::into),
            }
        }
    };
    (@subproperty $flavour:ident) => {
        fn subproperty(part: &Subproperty) -> CodeSystemLookupResponsePropertySubproperty {
            CodeSystemLookupResponsePropertySubproperty {
                code: part.code.as_str().into(),
                source: None,
                value: parameter_value(&part.value),
                description: part.description.as_deref().map(Into::into),
            }
        }
    };
    // R5 answers `definition` as its own output parameter, and a property with a
    // named parameter's name is returned there, not in `property`
    // (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>); R4 names none.
    (@own_parameters r4) => {
        &[]
    };
    (@own_parameters $flavour:ident) => {
        &["definition"]
    };
    (@definition r4, $definition:expr) => {
        CodeSystemLookupResponse::default()
    };
    (@definition $flavour:ident, $definition:expr) => {
        CodeSystemLookupResponse {
            definition: $definition.map(Into::into),
            ..Default::default()
        }
    };
    // `$translate`: R5 renames the R4 inputs and states `relationship` where R4
    // states `equivalence`.
    (@translation_match r4) => {
            /// One `match` of the `$translate` response: `equivalence`, `concept`,
            /// `product`, and `source`, the declared shape, plus the overlay's
            /// `originMap`, `sourceConcept`, the comments, and `noMap`; a `noMap` match
            /// has no `equivalence`.
            fn translation_match(m: &Match) -> ConceptMapTranslateResponseMatch {
                ConceptMapTranslateResponseMatch {
                    equivalence: (!m.origin.no_map).then(|| m.relationship.equivalence().into()),
                    source_concept: m.origin.source_concept.as_ref().map(coding_of),
                    source_comment: m.origin.source_comment.as_deref().map(Into::into),
                    origin_map: Some(m.origin.origin_map.as_str().into()),
                    target_comment: m.origin.target_comment.as_deref().map(Into::into),
                    no_map: m.origin.no_map.then_some(true.into()),
                    concept: m.concept.as_ref().map(coding_of),
                    product: m
                        .products
                        .iter()
                        .map(|p| ConceptMapTranslateResponseMatchProduct {
                            element: Some(p.element.as_str().into()),
                            concept: Some(coding_of(&p.concept)),
                        })
                        .collect(),
                    source: m.source.as_deref().map(Into::into),
                }
            }
    };
    (@translation_match $flavour:ident) => {
            /// One `match` of the `$translate` response: `relationship`, `concept`,
            /// `product` (an `attribute` and a `value`), and `originMap`, plus the
            /// overlay's `sourceConcept`, the comments, and `noMap`; a `noMap` match
            /// has no `relationship`.
            fn translation_match(m: &Match) -> ConceptMapTranslateResponseMatch {
                ConceptMapTranslateResponseMatch {
                    relationship: (!m.origin.no_map).then(|| m.relationship.relationship().into()),
                    source_concept: m.origin.source_concept.as_ref().map(coding_of),
                    source_comment: m.origin.source_comment.as_deref().map(Into::into),
                    target_comment: m.origin.target_comment.as_deref().map(Into::into),
                    no_map: m.origin.no_map.then_some(true.into()),
                    property: Vec::new(),
                    concept: m.concept.as_ref().map(coding_of),
                    product: m
                        .products
                        .iter()
                        .map(|p| ConceptMapTranslateResponseMatchProduct {
                            attribute: p.element.as_str().into(),
                            value: ParametersParameterValue::Coding(Box::new(coding_of(&p.concept))),
                        })
                        .collect(),
                    depends_on: Vec::new(),
                    origin_map: Some(m.origin.origin_map.as_str().into()),
                }
            }
    };
}

pub(crate) use family_map;
