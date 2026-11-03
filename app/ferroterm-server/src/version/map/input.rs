// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated requests of every served version as the engine's neutral
//! inputs: the half of [`super::family_map`] that reads a request.
//!
//! `family_map!` expands this inside each version's `map` module, so the
//! functions share its imports; `$flavour` names the family as there.

macro_rules! family_input {
    ($fhir:ident, $flavour:ident) => {
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

            /// The `ValueSet/$validate-code` request as the engine's input; an inline
            /// `valueSet` is converted as a resource of the version.
            #[must_use]
            pub fn value_set_validate_input(request: &ValueSetValidateCodeRequest) -> ValueSetValidateInput {
                ValueSetValidateInput {
                    unsupported: $crate::version::map::input::family_input!(@validate_unsupported $flavour, request),
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
                    display_language: $crate::version::map::input::family_input!(@vs_language $flavour, request),
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

            /// The `$translate` request as the engine's input; an inline `conceptMap` is
            /// converted as a resource of the version.
            #[must_use]
            pub fn translate_input(request: &ConceptMapTranslateRequest) -> TranslateInput {
                let targeted = targeted(request);
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
                    target_input: targeted,
                    dependency: !request.dependency.is_empty(),
                    ..translate_inputs(request, targeted)
                }
            }

            /// Whether a `$translate` request names a `target*` input, which reads the
            /// map in reverse (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
            fn targeted(request: &ConceptMapTranslateRequest) -> bool {
                request.target_code.is_some()
                    || request.target_coding.is_some()
                    || request.target_codeable_concept.is_some()
            }

            /// The text of a `uri` parameter.
            fn uri(value: Option<&Uri>) -> Option<String> {
                value.and_then(|v| v.value.clone())
            }

            /// The `$expand` request as the engine's input; an inline `valueSet` is
            /// converted as a resource of the version.
            #[must_use]
            pub fn expand_input(request: &ValueSetExpandRequest) -> ExpandInput {
                let flag = |b: &Option<Boolean>| b.as_ref().and_then(|b| b.value);
                let number = |i: &Option<Integer>| i.as_ref().and_then(|i| i.value).map(i64::from);
                ExpandInput {
                    unsupported: $crate::version::map::input::family_input!(@expand_unsupported $flavour, request),
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
                    handle_unclosed_expansion: $crate::version::map::input::family_input!(
                        @handle_unclosed $flavour, request, flag
                    ),
                }
            }

            $crate::version::map::input::family_input!(@translate_inputs $fhir, $flavour);
    };
    // Only R6 declares `handle-unclosed-expansion`; R4 and R5 refuse the name
    // as undeclared (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
    (@handle_unclosed r6, $request:expr, $flag:ident) => {
        $flag(&$request.handle_unclosed_expansion)
    };
    (@handle_unclosed $flavour:ident, $request:expr, $flag:ident) => {
        None
    };
    // R4 and R5 declare one `displayLanguage`; R6 declares a list, joined as a BCP 47
    // range list (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    (@vs_language r6, $request:expr) => {{
        let languages: Vec<String> = $request
            .display_language
            .iter()
            .filter_map(|c| c.value.clone())
            .collect();
        (!languages.is_empty()).then(|| languages.join(","))
    }};
    (@vs_language $flavour:ident, $request:expr) => {
        $request
            .display_language
            .as_ref()
            .and_then(|v| v.value.clone())
    };
    // R5 keeps the R4 `system` and `version` beside the pre-adopted `sourceSystem`
    // and `sourceVersion`; R6 declares only the latter.
    (@source_system r5, $request:expr) => {
        uri($request.system.as_ref()).or_else(|| uri($request.source_system.as_ref()))
    };
    (@source_system r6, $request:expr) => {
        uri($request.source_system.as_ref())
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
    (@expand_unsupported $flavour:ident, $request:expr) => {
        Vec::new()
    };
    (@validate_unsupported r6, $request:expr) => {{
        let mut names: Vec<String> = Vec::new();
        if $request.manifest.is_some() {
            names.push(String::from("manifest"));
        }
        names
    }};
    (@validate_unsupported $flavour:ident, $request:expr) => {
        Vec::new()
    };
    // `$translate`: R5 renames the R4 inputs.
    (@translate_inputs $fhir:ident, r4) => {
            /// The family's own `$translate` inputs: the code, its system and version,
            /// the scope, and the direction. R4 keeps its names beside the pre-adopted
            /// R6 `sourceSystem` and `sourceVersion`.
            fn translate_inputs(request: &ConceptMapTranslateRequest, targeted: bool) -> TranslateInput {
                // NOTE: the R6 names are pre-adopted beside the version's own; a
                // `target*` input reads the map in reverse, as R5 and R6 define it
                // (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
                let text = |value: &Option<fhir_types::$fhir::primitives::Code>| {
                    value.as_ref().and_then(|v| v.value.clone())
                };
                TranslateInput {
                    code: text(&request.code)
                        .or_else(|| text(&request.source_code))
                        .or_else(|| text(&request.target_code)),
                    system: if targeted {
                        uri(request.target_system.as_ref()).or_else(|| uri(request.system.as_ref()))
                    } else {
                        uri(request.system.as_ref()).or_else(|| uri(request.source_system.as_ref()))
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
                    source: uri(request.source.as_ref()).or_else(|| uri(request.source_scope.as_ref())),
                    target: uri(request.target.as_ref()).or_else(|| uri(request.target_scope.as_ref())),
                    target_system: if targeted {
                        uri(request.source_system.as_ref())
                    } else {
                        uri(request.targetsystem.as_ref()).or_else(|| uri(request.target_system.as_ref()))
                    },
                    reverse: request
                        .reverse
                        .as_ref()
                        .and_then(|b| b.value)
                        .or(targeted.then_some(true)),
                    ..TranslateInput::default()
                }
            }
    };
    (@translate_inputs $fhir:ident, $flavour:ident) => {
            /// The family's own `$translate` inputs.
            ///
            /// R5 renames the R4 inputs (`sourceCode`, `sourceScope`, `sourceCoding`,
            /// `sourceCodeableConcept`, `targetScope`, `targetSystem`) and replaces
            /// `reverse` with `targetCode`: a `targetCode` is the code to translate in
            /// reverse (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
            fn translate_inputs(request: &ConceptMapTranslateRequest, targeted: bool) -> TranslateInput {
                let target_code = request.target_code.as_ref().and_then(|v| v.value.clone());
                TranslateInput {
                    code: request
                        .source_code
                        .as_ref()
                        .and_then(|v| v.value.clone())
                        .or(target_code),
                    system: if targeted {
                        uri(request.target_system.as_ref())
                            .or_else(|| $crate::version::map::input::family_input!(@source_system $flavour, request))
                    } else {
                        $crate::version::map::input::family_input!(@source_system $flavour, request)
                    },
                    version: $crate::version::map::input::family_input!(@source_version $flavour, request),
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
                    source: uri(request.source_scope.as_ref()),
                    target: uri(request.target_scope.as_ref()),
                    target_system: if targeted {
                        uri(request.source_system.as_ref())
                    } else {
                        uri(request.target_system.as_ref())
                    },
                    reverse: targeted.then_some(true),
                    ..TranslateInput::default()
                }
            }
    };
}

pub(crate) use family_input;
