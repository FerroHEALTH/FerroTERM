// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated request and response contracts of the R5 family (R5 and the R6 ballot) mapped to and from the engine's neutral inputs and outcomes.

macro_rules! family_map {
    ($fhir:ident, $flavour:ident) => {
        pub mod map {
            //! The wire of the operations for one version of the R5 family.
            //!
            //! The generated request becomes the engine's neutral input, and the
            //! neutral outcome the generated response, so the version emits exactly what
            //! its `OperationDefinition`s declare
            //! (<https://hl7.org/fhir/R5/terminology-service.html>). R5 (5.0.0) and the
            //! R6 ballot (6.0.0-ballot5) declare the same result shapes; the few input
            //! differences are the `$flavour` arms below.

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
            use fhir_terminology::provider::{Designation, PropertyValue};
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

            /// A generated `Coding` as the engine's coding.
            #[must_use]
            pub fn coding_ref(coding: &Coding) -> CodingRef {
                CodingRef {
                    system: coding.system.as_ref().and_then(|v| v.value.clone()),
                    version: coding.version.as_ref().and_then(|v| v.value.clone()),
                    code: coding.code.as_ref().and_then(|v| v.value.clone()),
                    display: coding.display.as_ref().and_then(|v| v.value.clone()),
                }
            }

            /// A generated `CodeableConcept` as the engine's codeable concept.
            #[must_use]
            pub fn concept_ref(concept: &CodeableConcept) -> CodeableConceptRef {
                CodeableConceptRef {
                    coding: concept.coding.iter().map(coding_ref).collect(),
                    text: concept.text.as_ref().and_then(|v| v.value.clone()),
                }
            }

            fn canonicals(list: &[Canonical]) -> Vec<String> {
                list.iter().filter_map(|c| c.value.clone()).collect()
            }

            /// The `$lookup` request as the engine's input.
            #[must_use]
            pub fn lookup_input(request: &CodeSystemLookupRequest) -> LookupInput {
                LookupInput {
                    code: request.code.as_ref().and_then(|v| v.value.clone()),
                    system: request.system.as_ref().and_then(|v| v.value.clone()),
                    version: request.version.as_ref().and_then(|v| v.value.clone()),
                    coding: request.coding.as_ref().map(coding_ref),
                    display_language: request
                        .display_language
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    properties: request
                        .property
                        .iter()
                        .filter_map(|p| p.value.clone())
                        .collect(),
                    use_supplement: canonicals(&request.use_supplement),
                }
            }

            /// The `$lookup` outcome as the response: `name`, `version`, `display`,
            /// `definition`, `designation`, and `property` with its `subproperty` parts,
            /// then the ecosystem's `code`, `system`, and `abstract`.
            #[must_use]
            pub fn lookup_response(outcome: LookupOutcome) -> CodeSystemLookupResponse {
                CodeSystemLookupResponse {
                    name: outcome.name.into(),
                    version: outcome.version.map(Into::into),
                    display: outcome.display.into(),
                    definition: outcome.definition.map(Into::into),
                    designation: outcome.designations.into_iter().map(designation).collect(),
                    // NOTE: R5 answers `definition` as its own output parameter, and a property
                    // with a named parameter's name is returned there, not in `property`
                    // (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
                    property: outcome
                        .properties
                        .into_iter()
                        .filter(|property| property.code != "definition")
                        .map(|property| CodeSystemLookupResponseProperty {
                            code: property.code.into(),
                            value: Some(parameter_value(&property.value)),
                            description: property.description.map(Into::into),
                            source: None,
                            subproperty: property
                                .subproperties
                                .iter()
                                .map(|part| CodeSystemLookupResponsePropertySubproperty {
                                    code: part.code.as_str().into(),
                                    value: parameter_value(&part.value),
                                    description: part.description.as_deref().map(Into::into),
                                    source: None,
                                })
                                .collect(),
                        })
                        .collect(),
                    code: Some(outcome.code.into()),
                    system: Some(outcome.system.into()),
                    // NOTE: `abstract` is answered for an abstract concept only; the
                    // ecosystem's cases expect no `abstract = false`.
                    r#abstract: outcome.abstract_concept.then_some(true.into()),
                }
            }

            fn designation(d: Designation) -> CodeSystemLookupResponseDesignation {
                CodeSystemLookupResponseDesignation {
                    language: d.language.map(Into::into),
                    r#use: d.use_.map(|u| Coding {
                        system: Some(u.system.into()),
                        code: Some(u.code.into()),
                        display: u.display.map(Into::into),
                        ..Default::default()
                    }),
                    additional_use: Vec::new(),
                    value: d.value.into(),
                }
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

            /// The `$subsumes` request as the engine's input.
            #[must_use]
            pub fn subsumes_input(request: &CodeSystemSubsumesRequest) -> SubsumesInput {
                SubsumesInput {
                    code_a: request.code_a.as_ref().and_then(|v| v.value.clone()),
                    code_b: request.code_b.as_ref().and_then(|v| v.value.clone()),
                    system: request.system.as_ref().and_then(|v| v.value.clone()),
                    version: request.version.as_ref().and_then(|v| v.value.clone()),
                    coding_a: request.coding_a.as_ref().map(coding_ref),
                    coding_b: request.coding_b.as_ref().map(coding_ref),
                }
            }

            /// The `$subsumes` outcome as the response.
            #[must_use]
            pub fn subsumes_response(outcome: Outcome) -> CodeSystemSubsumesResponse {
                CodeSystemSubsumesResponse {
                    outcome: outcome.code().into(),
                }
            }

            /// The `CodeSystem/$validate-code` request as the engine's input.
            #[must_use]
            pub fn validate_code_input(request: &CodeSystemValidateCodeRequest) -> ValidateCodeInput {
                ValidateCodeInput {
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    version: request.version.as_ref().and_then(|v| v.value.clone()),
                    inline_code_system: request.code_system.is_some(),
                    code: request.code.as_ref().and_then(|v| v.value.clone()),
                    display: request.display.as_ref().and_then(|v| v.value.clone()),
                    coding: request.coding.as_ref().map(coding_ref),
                    codeable_concept: request
                        .codeable_concept
                        .as_ref()
                        .map(concept_ref),
                    display_language: request
                        .display_language
                        .as_ref()
                        .and_then(|v| v.value.clone()),

                    lenient_display: request
                        .lenient_display_validation
                        .as_ref()
                        .and_then(|b| b.value)
                        .unwrap_or(false),
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

            /// The `ValueSet/$validate-code` request as the engine's input; an inline
            /// `valueSet` is converted as a resource of the version.
            #[must_use]
            pub fn value_set_validate_input(request: &ValueSetValidateCodeRequest) -> ValueSetValidateInput {
                ValueSetValidateInput {
                    unsupported: $crate::version::map_r5::family_map!(@validate_unsupported $flavour, request),
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    value_set_version: request
                        .value_set_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_value_set: request
                        .value_set
                        .as_ref()
                        .map(valueset::convert::$fhir::convert),
                    use_supplement: canonicals(&request.use_supplement),
                    context: request.context.is_some(),
                    date: request.date.is_some(),
                    code: request.code.as_ref().and_then(|v| v.value.clone()),
                    system: request.system.as_ref().and_then(|v| v.value.clone()),
                    system_version: request
                        .system_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    display: request.display.as_ref().and_then(|v| v.value.clone()),
                    coding: request.coding.as_ref().map(coding_ref),
                    codeable_concept: request
                        .codeable_concept
                        .as_ref()
                        .map(concept_ref),
                    display_language: $crate::version::map_r5::family_map!(@vs_language $flavour, request),
                    abstract_ok: request.r#abstract.as_ref().and_then(|b| b.value),
                    default_system_version: canonicals(&request.system_version_canonical),
                    check_system_version: canonicals(&request.check_system_version),
                    force_system_version: canonicals(&request.force_system_version),
                    default_valueset_version: canonicals(&request.default_valueset_version),
                    check_valueset_version: canonicals(&request.check_valueset_version),
                    force_valueset_version: canonicals(&request.force_valueset_version),
                    infer_system: request.infer_system.as_ref().and_then(|b| b.value),
                    active_only: request.active_only.as_ref().and_then(|b| b.value),
                    membership_only: request
                        .valueset_membership_only
                        .as_ref()
                        .and_then(|b| b.value),
                    lenient_display_validation: request
                        .lenient_display_validation
                        .as_ref()
                        .and_then(|b| b.value),
                }
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

            /// The `$translate` request as the engine's input; an inline `conceptMap` is
            /// converted as a resource of the version.
            ///
            /// R5 renames the R4 inputs (`sourceCode`, `sourceScope`, `sourceCoding`,
            /// `sourceCodeableConcept`, `targetScope`, `targetSystem`) and replaces
            /// `reverse` with `targetCode`: a `targetCode` is the code to translate in
            /// reverse (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
            #[must_use]
            pub fn translate_input(request: &ConceptMapTranslateRequest) -> TranslateInput {
                let target_code = request.target_code.as_ref().and_then(|v| v.value.clone());
                // NOTE: a `target*` input names a code of the target system and reads the
                // map in reverse (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
                let targeted = target_code.is_some()
                    || request.target_coding.is_some()
                    || request.target_codeable_concept.is_some();
                let uri = |value: &Option<Uri>| value.as_ref().and_then(|v| v.value.clone());
                TranslateInput {
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    concept_map_version: request
                        .concept_map_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_concept_map: request
                        .concept_map
                        .as_ref()
                        .map(conceptmap::convert::$fhir::convert),
                    code: request
                        .source_code
                        .as_ref()
                        .and_then(|v| v.value.clone())
                        .or_else(|| target_code.clone()),
                    system: if targeted {
                        uri(&request.target_system)
                            .or_else(|| $crate::version::map_r5::family_map!(@source_system $flavour, request, uri))
                    } else {
                        $crate::version::map_r5::family_map!(@source_system $flavour, request, uri)
                    },
                    version: $crate::version::map_r5::family_map!(@source_version $flavour, request),
                    coding: request
                        .source_coding
                        .as_ref()
                        .or(request.target_coding.as_ref())
                        .map(coding_ref),
                    codeable_concept: request
                        .source_codeable_concept
                        .as_ref()
                        .or(request.target_codeable_concept.as_ref())
                        .map(concept_ref),
                    source: uri(&request.source_scope),
                    target: uri(&request.target_scope),
                    target_system: if targeted {
                        uri(&request.source_system)
                    } else {
                        uri(&request.target_system)
                    },
                    reverse: targeted.then_some(true),
                    target_input: targeted,
                    dependency: !request.dependency.is_empty(),
                }
            }

            /// The `ConceptMap` a `$closure` call answers with: one element per
            /// relationship the server is telling the client about.
            ///
            /// R5 and the R6 ballot state it as `relationship`
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
                        relationship: edge.relationship.relationship().into(),
                        ..Default::default()
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

            /// The `$translate` outcome as the `Parameters`.
            ///
            /// `result`, `message`, and each `match` with `relationship`, `concept`,
            /// `product` (an `attribute` and a `value`), and `originMap`, plus the
            /// overlay's `sourceConcept`, the comments, `noMap`, and `used-conceptmap`; a
            /// `noMap` match has no `relationship`.
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
                    r#match: translation
                        .matches
                        .iter()
                        .map(|m| ConceptMapTranslateResponseMatch {
                            relationship: (!m.origin.no_map).then(|| m.relationship.relationship().into()),
                            source_concept: m.origin.source_concept.as_ref().map(coding_of),
                            source_comment: m.origin.source_comment.as_deref().map(Into::into),
                            target_comment: m.origin.target_comment.as_deref().map(Into::into),
                            no_map: m.origin.no_map.then_some(true.into()),
                            concept: m.concept.as_ref().map(coding_of),
                            property: Vec::new(),
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
                        })
                        .collect(),
                }
                .to_parameters()
            }

            /// The `$expand` request as the engine's input; an inline `valueSet` is
            /// converted as a resource of the version.
            #[must_use]
            pub fn expand_input(request: &ValueSetExpandRequest) -> ExpandInput {
                let flag = |b: &Option<Boolean>| b.as_ref().and_then(|b| b.value);
                let number = |i: &Option<Integer>| i.as_ref().and_then(|i| i.value).map(i64::from);
                ExpandInput {
                    unsupported: $crate::version::map_r5::family_map!(@expand_unsupported $flavour, request),
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    value_set_version: request
                        .value_set_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_value_set: request
                        .value_set
                        .as_ref()
                        .map(valueset::convert::$fhir::convert),
                    context: request.context.is_some() || request.context_direction.is_some(),
                    date: request.date.is_some(),
                    filter: request.filter.as_ref().and_then(|v| v.value.clone()),
                    offset: number(&request.offset),
                    count: number(&request.count),
                    include_designations: flag(&request.include_designations),
                    designation: request
                        .designation
                        .iter()
                        .filter_map(|d| d.value.clone())
                        .collect(),
                    include_definition: flag(&request.include_definition),
                    active_only: flag(&request.active_only),
                    exclude_nested: flag(&request.exclude_nested),
                    exclude_not_for_ui: flag(&request.exclude_not_for_u_i),
                    exclude_post_coordinated: flag(&request.exclude_post_coordinated),
                    display_language: request
                        .display_language
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    exclude_system: canonicals(&request.exclude_system),
                    system_version: canonicals(&request.system_version),
                    check_system_version: canonicals(&request.check_system_version),
                    force_system_version: canonicals(&request.force_system_version),
                    default_valueset_version: canonicals(&request.default_valueset_version),
                    check_valueset_version: canonicals(&request.check_valueset_version),
                    force_valueset_version: canonicals(&request.force_valueset_version),
                    property: request
                        .property
                        .iter()
                        .filter_map(|p| p.value.clone())
                        .collect(),
                    use_supplement: canonicals(&request.use_supplement),
                    handle_unclosed_expansion: $crate::version::map_r5::family_map!(
                        @handle_unclosed $flavour, request, flag
                    ),
                }
            }

        }
    };
    // Only R6 declares `handle-unclosed-expansion`; R5 refuses the name as
    // undeclared (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
    (@handle_unclosed r5, $request:expr, $flag:ident) => {
        None
    };
    (@handle_unclosed r6, $request:expr, $flag:ident) => {
        $flag(&$request.handle_unclosed_expansion)
    };
    // R5 declares one `displayLanguage`; R6 declares a list, joined as a BCP 47
    // range list (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    (@vs_language r5, $request:expr) => {
        $request
            .display_language
            .as_ref()
            .and_then(|v| v.value.clone())
    };
    (@vs_language r6, $request:expr) => {{
        let languages: Vec<String> = $request
            .display_language
            .iter()
            .filter_map(|c| c.value.clone())
            .collect();
        (!languages.is_empty()).then(|| languages.join(","))
    }};
    // R5 keeps the R4 `system` and `version` beside the pre-adopted `sourceSystem`
    // and `sourceVersion`; R6 declares only the latter.
    (@source_system r5, $request:expr, $uri:ident) => {
        $uri(&$request.system).or_else(|| $uri(&$request.source_system))
    };
    (@source_system r6, $request:expr, $uri:ident) => {
        $uri(&$request.source_system)
    };
    (@source_version r5, $request:expr) => {
        $request
            .version
            .as_ref()
            .or($request.source_version.as_ref())
            .and_then(|v| v.value.clone())
    };
    (@source_version r6, $request:expr) => {
        $request
            .source_version
            .as_ref()
            .and_then(|v| v.value.clone())
    };
    // The R6 ballot declares parameters the server does not implement yet; a
    // request naming one is refused, never absorbed.
    (@expand_unsupported r5, $request:expr) => {
        Vec::new()
    };
    (@expand_unsupported r6, $request:expr) => {{
        let mut names: Vec<String> = Vec::new();
        if !$request.filter_property.is_empty() {
            names.push(String::from("filterProperty"));
        }
        if $request.manifest.is_some() {
            names.push(String::from("manifest"));
        }
        names
    }};
    (@validate_unsupported r5, $request:expr) => {
        Vec::new()
    };
    (@validate_unsupported r6, $request:expr) => {{
        let mut names: Vec<String> = Vec::new();
        if $request.manifest.is_some() {
            names.push(String::from("manifest"));
        }
        names
    }};
}

pub(crate) use family_map;
