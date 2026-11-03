// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated request and response contracts of the R4 family (R4 and R4B) mapped to and from the engine's neutral inputs and outcomes.

macro_rules! family_map {
    ($fhir:ident) => {
        pub mod map {
            //! The wire of the operations for one version.
            //!
            //! The generated request becomes the engine's neutral input, and the
            //! neutral outcome the generated response, so the version emits exactly what
            //! its `OperationDefinition`s declare. R4 (4.0.1) and R4B (4.3.0) declare the
            //! same parameters, so one macro serves both.

            use concept_graph::subsumption::Outcome;
            use fhir_terminology::operations::expand::ExpandInput;
            use fhir_terminology::operations::lookup::{LookupInput, LookupOutcome};
            use fhir_terminology::operations::subsumes::SubsumesInput;
            use fhir_terminology::operations::translate::{TranslateInput, Translation};
            use fhir_terminology::operations::validate_code::{
                ValidateCodeInput, ValidationOutcome,
            };
            use fhir_terminology::operations::value_set_validate_code::{
                TX_ISSUE_TYPE, Validation, ValueSetValidateInput,
            };
            use fhir_terminology::operations::{
                CodeableConceptRef, CodingRef, Issue, MESSAGE_ID_URL,
            };
            use fhir_terminology::provider::{Designation, PropertyValue};
            use fhir_terminology::valueset::convert;
            use fhir_types::$fhir::codeable_concept::CodeableConcept;
            use fhir_types::$fhir::coding::Coding;
            use fhir_types::$fhir::extension::{Extension, ExtensionValue};
            use fhir_types::$fhir::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
            use fhir_types::$fhir::operations::code_system_lookup::{
                CodeSystemLookupRequest, CodeSystemLookupResponse,
                CodeSystemLookupResponseDesignation, CodeSystemLookupResponseProperty,
                CodeSystemLookupResponsePropertySubproperty,
            };
            use fhir_types::$fhir::operations::code_system_subsumes::{
                CodeSystemSubsumesRequest, CodeSystemSubsumesResponse,
            };
            use fhir_types::$fhir::operations::code_system_validate_code::{
                CodeSystemValidateCodeRequest, CodeSystemValidateCodeResponse,
            };
            use fhir_types::$fhir::operations::concept_map_translate::{
                ConceptMapTranslateRequest, ConceptMapTranslateResponse,
                ConceptMapTranslateResponseMatch, ConceptMapTranslateResponseMatchProduct,
            };
            use fhir_types::$fhir::operations::value_set_expand::ValueSetExpandRequest;
            use fhir_types::$fhir::operations::value_set_validate_code::{
                ValueSetValidateCodeRequest, ValueSetValidateCodeResponse,
            };
            use fhir_types::$fhir::parameters::Parameters;
            use fhir_types::$fhir::parameters::ParametersParameterValue;

            /// A generated `Coding` as the engine names one.
            #[must_use]
            pub fn coding_ref(coding: &Coding) -> CodingRef {
                CodingRef {
                    system: coding.system.as_ref().and_then(|v| v.value.clone()),
                    version: coding.version.as_ref().and_then(|v| v.value.clone()),
                    code: coding.code.as_ref().and_then(|v| v.value.clone()),
                    display: coding.display.as_ref().and_then(|v| v.value.clone()),
                }
            }

            /// A generated `CodeableConcept` as the engine names one.
            #[must_use]
            pub fn concept_ref(concept: &CodeableConcept) -> CodeableConceptRef {
                CodeableConceptRef {
                    coding: concept.coding.iter().map(coding_ref).collect(),
                    text: concept.text.as_ref().and_then(|v| v.value.clone()),
                }
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

            /// The `$lookup` outcome as the version's response: `name`, `version`, `display`,
            /// `designation`, and `property` with its `subproperty` parts, then the
            /// ecosystem's `code`, `system`, and `abstract`.
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
                        .map(|property| CodeSystemLookupResponseProperty {
                            code: property.code.into(),
                            value: Some(parameter_value(&property.value)),
                            description: property.description.map(Into::into),
                            subproperty: property
                                .subproperties
                                .iter()
                                .map(|part| CodeSystemLookupResponsePropertySubproperty {
                                    code: part.code.as_str().into(),
                                    value: parameter_value(&part.value),
                                    description: part.description.as_deref().map(Into::into),
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
                    value: d.value.into(),
                }
            }

            /// A property value as a `Parameters.parameter.value[x]`.
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
                    PropertyValue::DateTime(d) => {
                        ParametersParameterValue::DateTime(d.as_str().into())
                    }
                    PropertyValue::Decimal(d) => {
                        ParametersParameterValue::Decimal(d.as_str().into())
                    }
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

            /// The `$subsumes` outcome as the version's response.
            #[must_use]
            pub fn subsumes_response(outcome: Outcome) -> CodeSystemSubsumesResponse {
                CodeSystemSubsumesResponse {
                    outcome: outcome.code().into(),
                }
            }

            /// The `CodeSystem/$validate-code` request as the engine's input.
            #[must_use]
            pub fn validate_code_input(
                request: &CodeSystemValidateCodeRequest,
            ) -> ValidateCodeInput {
                ValidateCodeInput {
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    version: request.version.as_ref().and_then(|v| v.value.clone()),
                    inline_code_system: request.code_system.is_some(),
                    code: request.code.as_ref().and_then(|v| v.value.clone()),
                    display: request.display.as_ref().and_then(|v| v.value.clone()),
                    coding: request.coding.as_ref().map(coding_ref),
                    codeable_concept: request.codeable_concept.as_ref().map(concept_ref),
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

            /// The `CodeSystem/$validate-code` outcome as the version's response.
            ///
            /// `result`, `message`, and `display`, then the ecosystem overlay's validated
            /// `code`, `system`, `version`, the itemised `issues`, and
            /// `x-caused-by-unknown-system`.
            #[must_use]
            pub fn validate_code_response(
                outcome: ValidationOutcome,
            ) -> CodeSystemValidateCodeResponse {
                CodeSystemValidateCodeResponse {
                    result: outcome.result.into(),
                    message: outcome.message.map(Into::into),
                    display: outcome.display.map(Into::into),
                    code: outcome.code.map(Into::into),
                    normalized_code: outcome.normalized_code.map(Into::into),
                    system: outcome.system.map(Into::into),
                    version: outcome.version.map(Into::into),
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
                    codeable_concept: outcome.codeable_concept.as_ref().map(concept_of),
                    inactive: outcome.inactive.map(Into::into),
                    status: outcome.status.map(Into::into),
                }
            }

            /// A neutral `codeableConcept` echoed as the version's `CodeableConcept`.
            fn concept_of(concept: &CodeableConceptRef) -> CodeableConcept {
                CodeableConcept {
                    coding: concept.coding.iter().map(coding_of).collect(),
                    text: concept.text.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// The version's `Coding` of a `tx-issue-type` code, for `issue.details.coding`.
            fn tx_issue_coding(kind: &str) -> Coding {
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
            pub fn value_set_validate_input(
                request: &ValueSetValidateCodeRequest,
            ) -> ValueSetValidateInput {
                ValueSetValidateInput {
                    unsupported: Vec::new(),
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    value_set_version: request
                        .value_set_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_value_set: request.value_set.as_ref().map(convert::$fhir::convert),
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
                    codeable_concept: request.codeable_concept.as_ref().map(concept_ref),
                    display_language: request
                        .display_language
                        .as_ref()
                        .and_then(|v| v.value.clone()),
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

            /// The `ValueSet/$validate-code` outcome as the version's `Parameters`.
            ///
            /// `result`, `message`, and `display`, then the ecosystem overlay's validated
            /// `code`, `system`, `version`, the itemised `issues`, and
            /// `x-caused-by-unknown-system`.
            #[must_use]
            pub fn value_set_validation_parameters(validation: &Validation) -> Parameters {
                let response = ValueSetValidateCodeResponse {
                    result: validation.result.into(),
                    message: validation.message.as_deref().map(Into::into),
                    display: validation.display.as_deref().map(Into::into),
                    code: validation.code.as_deref().map(Into::into),
                    normalized_code: validation.normalized_code.as_deref().map(Into::into),
                    system: validation.system.as_deref().map(Into::into),
                    version: validation.version.as_deref().map(Into::into),
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
                    codeable_concept: validation.codeable_concept.as_ref().map(concept_of),
                    inactive: validation.inactive.map(Into::into),
                    status: validation.status.as_deref().map(Into::into),
                };
                response.to_parameters()
            }

            /// A neutral coding as the version's `Coding`.
            fn coding_of(coding: &CodingRef) -> Coding {
                Coding {
                    system: coding.system.as_deref().map(Into::into),
                    version: coding.version.as_deref().map(Into::into),
                    code: coding.code.as_deref().map(Into::into),
                    display: coding.display.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// The `$translate` request as the engine's input.
            ///
            /// An inline `conceptMap` is converted as a resource of the version. The R6
            /// names `sourceSystem` and `sourceVersion` (pre-adopted) are accepted beside
            /// `system` and `version`.
            #[must_use]
            pub fn translate_input(request: &ConceptMapTranslateRequest) -> TranslateInput {
                // NOTE: the R6 names are pre-adopted beside the version's own; a
                // `target*` input reads the map in reverse, as R5 and R6 define it
                // (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
                let targeted = request.target_code.is_some()
                    || request.target_coding.is_some()
                    || request.target_codeable_concept.is_some();
                let text = |value: &Option<fhir_types::$fhir::primitives::Code>| {
                    value.as_ref().and_then(|v| v.value.clone())
                };
                let uri = |value: &Option<fhir_types::$fhir::primitives::Uri>| {
                    value.as_ref().and_then(|v| v.value.clone())
                };
                TranslateInput {
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    concept_map_version: request
                        .concept_map_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_concept_map: request
                        .concept_map
                        .as_ref()
                        .map(fhir_terminology::conceptmap::convert::$fhir::convert),
                    code: text(&request.code)
                        .or_else(|| text(&request.source_code))
                        .or_else(|| text(&request.target_code)),
                    system: if targeted {
                        uri(&request.target_system).or_else(|| uri(&request.system))
                    } else {
                        uri(&request.system).or_else(|| uri(&request.source_system))
                    },
                    version: request
                        .version
                        .as_ref()
                        .or(request.source_version.as_ref())
                        .and_then(|v| v.value.clone()),
                    coding: request
                        .coding
                        .as_ref()
                        .or(request.source_coding.as_ref())
                        .or(request.target_coding.as_ref())
                        .map(coding_ref),
                    codeable_concept: request
                        .codeable_concept
                        .as_ref()
                        .or(request.source_codeable_concept.as_ref())
                        .or(request.target_codeable_concept.as_ref())
                        .map(concept_ref),
                    source: uri(&request.source).or_else(|| uri(&request.source_scope)),
                    target: uri(&request.target).or_else(|| uri(&request.target_scope)),
                    target_system: if targeted {
                        uri(&request.source_system)
                    } else {
                        uri(&request.targetsystem).or_else(|| uri(&request.target_system))
                    },
                    reverse: request
                        .reverse
                        .as_ref()
                        .and_then(|b| b.value)
                        .or(targeted.then_some(true)),
                    target_input: targeted,
                    dependency: !request.dependency.is_empty(),
                }
            }

            /// The `$translate` outcome as the version's `Parameters`.
            ///
            /// The `ConceptMap` a `$closure` call answers with: one element per
            /// relationship the server is telling the client about.
            ///
            /// R4 and R4B state the relationship as `equivalence`
            /// (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>).
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
                        equivalence: edge.relationship.equivalence().into(),
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
            /// `result`, `message`, and each `match` with `equivalence`, `concept`, `product`,
            /// and `source`, the declared shape, plus the overlay's `originMap`,
            /// `sourceConcept`, the comments, `noMap`, and `used-conceptmap`; a `noMap`
            /// match has no `equivalence`.
            #[must_use]
            pub fn translation_parameters(translation: &Translation) -> Parameters {
                let response = ConceptMapTranslateResponse {
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
                            equivalence: (!m.origin.no_map)
                                .then(|| m.relationship.equivalence().into()),
                            origin_map: Some(m.origin.origin_map.as_str().into()),
                            source_concept: m.origin.source_concept.as_ref().map(coding_of),
                            source_comment: m.origin.source_comment.as_deref().map(Into::into),
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
                        })
                        .collect(),
                };
                response.to_parameters()
            }

            /// The canonicals of a repeated `canonical` parameter, as text.
            fn canonicals(list: &[fhir_types::$fhir::primitives::Canonical]) -> Vec<String> {
                list.iter().filter_map(|c| c.value.clone()).collect()
            }

            /// The `$expand` request as the engine's input; an inline `valueSet` is
            /// converted as a resource of the version. R4 and R4B declare no `property` or
            /// `useSupplement`, so those stay empty.
            #[must_use]
            pub fn expand_input(request: &ValueSetExpandRequest) -> ExpandInput {
                let flag = |b: &Option<fhir_types::$fhir::primitives::Boolean>| {
                    b.as_ref().and_then(|b| b.value)
                };
                let number = |i: &Option<fhir_types::$fhir::primitives::Integer>| {
                    i.as_ref().and_then(|i| i.value).map(i64::from)
                };
                ExpandInput {
                    unsupported: Vec::new(),
                    url: request.url.as_ref().and_then(|v| v.value.clone()),
                    value_set_version: request
                        .value_set_version
                        .as_ref()
                        .and_then(|v| v.value.clone()),
                    inline_value_set: request.value_set.as_ref().map(convert::$fhir::convert),
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
                    // Only R6 declares `handle-unclosed-expansion`
                    // (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
                    handle_unclosed_expansion: None,
                }
            }
        }
    };
}

pub(crate) use family_map;
