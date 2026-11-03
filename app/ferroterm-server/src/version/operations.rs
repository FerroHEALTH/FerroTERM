// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operation handlers of one version: GET and POST, type and instance level, over the engine.

pub(crate) mod handlers;

macro_rules! operations {
    ($fhir:ident) => {
        pub mod operations {
            //! The `CodeSystem`, `ValueSet`, and `ConceptMap` operation handlers of one version.
            //!
            //! Each handler turns the wire form into the generated request type, runs the
            //! operation in the request's [`Scope`], and writes the generated response.
            //! `tx-resource` parameters and the `X-Cache-Id` header are peeled off before
            //! the generated request refuses what the operation does not declare.

            use std::sync::Arc;

            use axum::body::Bytes;
            use axum::extract::{Path as UrlPath, Query, RawQuery, State};
            use axum::response::{IntoResponse, Response};
            use fhir_terminology::operations::{
                Invocation, OperationError, expand, lookup, subsumes, translate, validate_code,
                value_set_validate_code,
            };
            use fhir_terminology::valueset::render;
            use fhir_types::$fhir::operations::code_system_lookup::{
                CODE_SYSTEM_LOOKUP, CodeSystemLookupRequest,
            };
            use fhir_types::$fhir::operations::code_system_subsumes::{
                CODE_SYSTEM_SUBSUMES, CodeSystemSubsumesRequest,
            };
            use fhir_types::$fhir::operations::code_system_validate_code::{
                CODE_SYSTEM_VALIDATE_CODE, CodeSystemValidateCodeRequest,
            };
            use fhir_types::$fhir::operations::concept_map_translate::{
                CONCEPT_MAP_TRANSLATE, ConceptMapTranslateRequest,
            };
            use fhir_types::$fhir::operations::value_set_expand::{
                VALUE_SET_EXPAND, ValueSetExpandRequest,
            };
            use fhir_types::$fhir::operations::value_set_validate_code::{
                VALUE_SET_VALIDATE_CODE, ValueSetValidateCodeRequest,
            };
            use fhir_types::$fhir::parameters::{Parameters, ParametersParameter};
            use fhir_types::$fhir::resource::Resource;
            use fhir_types::codec::Json;
            use http::{HeaderMap, StatusCode};

            use super::resources::{split_resources, split_supplied};
            use super::{map, parameters};
            use crate::outcome::{Failure, Origin};
            use crate::scope::{Scope, scope_of};
            use crate::state::AppState;
            use crate::wire::{Wire, without_format};

            /// What one operation produced: this version's `Parameters`, or the
            /// `ValueSet` an expansion answers with.
            ///
            /// Every terminology operation answers `200` with a resource, so an
            /// invocation over HTTP and one inside a `Bundle` entry differ only in how
            /// the resource is delivered: the response writes it, the entry holds it.
            pub(super) enum Answer {
                /// The output values of an operation.
                Parameters(Box<Parameters>),
                /// The expansion of `$expand`.
                ValueSet(Box<fhir_types::$fhir::value_set::ValueSet>),
            }

            impl Answer {
                /// The `200` response in `wire`, or the failure that stopped it.
                pub(super) fn respond(&self, wire: Wire) -> Response {
                    match self {
                        Self::Parameters(parameters) => {
                            parameters::respond(parameters.as_ref(), wire)
                        }
                        Self::ValueSet(value_set) => {
                            parameters::respond_resource(value_set.as_ref(), wire)
                        }
                    }
                    .unwrap_or_else(|failure| failure.respond(wire))
                }

                /// The resource itself, for a `Bundle` entry.
                pub(super) fn resource(self) -> Resource {
                    match self {
                        Self::Parameters(parameters) => Resource::Parameters(parameters),
                        Self::ValueSet(value_set) => Resource::ValueSet(value_set),
                    }
                }
            }

            /// What one operation produced, or the failure it answered with.
            type Handled = Result<Answer, Failure>;

            fn instance(state: &AppState, id: &str) -> Result<Invocation, Failure> {
                state.instance(id).map(Invocation::Instance).ok_or_else(|| {
                    Failure::new(
                        StatusCode::NOT_FOUND,
                        "not-found",
                        format!("no CodeSystem with id `{id}`"),
                    )
                })
            }

            /// The canonical of the value set or concept map an instance-level
            /// invocation runs on.
            #[derive(Debug, Clone)]
            struct Bound {
                /// The resource's `url`.
                url: String,
                /// Its `version`, when it states one.
                version: Option<String>,
            }

            /// The value set `ValueSet/{id}/$expand` runs on.
            fn value_set_instance(state: &AppState, id: &str) -> Result<Bound, Failure> {
                let model = state.value_set_instance(id);
                bound_to(model.as_ref().map(|m| (&m.url, &m.version)), "ValueSet", id)
            }

            /// The concept map `ConceptMap/{id}/$translate` runs on.
            fn concept_map_instance(state: &AppState, id: &str) -> Result<Bound, Failure> {
                let model = state.concept_map_instance(id);
                bound_to(model.as_ref().map(|m| (&m.url, &m.version)), "ConceptMap", id)
            }

            /// The canonical of the `kind` resource with `id`, which `found` holds when
            /// the server has one.
            fn bound_to(
                found: Option<(&String, &Option<String>)>,
                kind: &str,
                id: &str,
            ) -> Result<Bound, Failure> {
                found
                    .map(|(url, version)| Bound {
                        url: url.clone(),
                        version: version.clone(),
                    })
                    .ok_or_else(|| {
                        Failure::new(
                            StatusCode::NOT_FOUND,
                            "not-found",
                            format!("no {kind} with id `{id}`"),
                        )
                    })
            }

            /// Refuses a request that names another resource than the instance
            /// it was invoked on.
            ///
            /// The instance form runs the operation on that resource
            /// (<https://hl7.org/fhir/R4B/operations.html#request>), so an
            /// input naming a different canonical, a different version, or an
            /// inline resource contradicts the invocation. No operation page
            /// states what a server does with such an input, so refusing it
            /// rather than silently preferring one side is our own design.
            fn binds(
                bound: &Bound,
                kind: &str,
                url: Option<&str>,
                version: Option<&str>,
                inline: bool,
                inline_name: &str,
            ) -> Result<(), Failure> {
                let invalid = |text: String| Failure::new(StatusCode::BAD_REQUEST, "invalid", text);
                // NOTE: a canonical carries its version after a `|`
                // (<https://hl7.org/fhir/R4B/references.html#canonical>), so the two
                // halves are compared against the instance separately.
                let (named, embedded) = match url {
                    Some(url) => match url.split_once('|') {
                        Some((url, version)) => (Some(url), Some(version)),
                        None => (Some(url), None),
                    },
                    None => (None, None),
                };
                let version = version.or(embedded);
                if let Some(url) = named
                    && url != bound.url
                {
                    return Err(invalid(format!(
                        "the request names {kind} `{url}`, the instance is `{}`",
                        bound.url
                    )));
                }
                if let Some(version) = version
                    && Some(version) != bound.version.as_deref()
                {
                    return Err(invalid(format!(
                        "the request names version `{version}`, the instance is `{}`",
                        bound.version.as_deref().unwrap_or("unversioned")
                    )));
                }
                if inline {
                    return Err(invalid(format!(
                        "an instance-level invocation runs on the {kind} it names, so `{inline_name}` is not accepted"
                    )));
                }
                Ok(())
            }

            /// A `GET` invocation: the query parameters, in the scope the headers name.
            fn from_query(
                state: &AppState,
                operation: &fhir_types::operation::Operation,
                headers: &HeaderMap,
                query: &[(String, String)],
            ) -> Result<(Scope, Parameters), Failure> {
                let own: Vec<(String, String)> = query
                    .iter()
                    .filter(|(name, _)| name != crate::scope::UUID)
                    .cloned()
                    .collect();
                let mut parameters = parameters::parameters_from_query(operation, &own)?;
                parameters::apply_accept_language(operation, headers, &mut parameters);
                Ok((scope_of(state, super::metadata::FHIR_VERSION, headers, Vec::new())?, parameters))
            }

            /// A `POST` invocation: the body's `Parameters` less its `tx-resource`s, in
            /// the scope those resources and the headers form.
            fn from_body(
                state: &AppState,
                operation: &fhir_types::operation::Operation,
                headers: &HeaderMap,
                body: &Bytes,
            ) -> Result<(Scope, Parameters), Failure> {
                from_body_accepting(state, operation, &[], headers, body)
            }

            /// A `POST` invocation that also takes the names in `also`.
            ///
            /// `$batch-validate-code` is the only caller: no `OperationDefinition`
            /// declares it, so its `validation` parameter is declared by nothing
            /// and would otherwise be refused as undeclared.
            fn from_body_accepting(
                state: &AppState,
                operation: &fhir_types::operation::Operation,
                also: &[&str],
                headers: &HeaderMap,
                body: &Bytes,
            ) -> Result<(Scope, Parameters), Failure> {
                from_body_placed(state, operation, also, headers, body)
                    .map(|(scope, parameters, _)| (scope, parameters))
            }

            /// A `POST` invocation, with where the body wrote each parameter.
            ///
            /// The body's own order is taken before its `tx-resource`s are split off,
            /// so a refusal names a parameter by the index the client sent it at.
            fn from_body_placed(
                state: &AppState,
                operation: &fhir_types::operation::Operation,
                also: &[&str],
                headers: &HeaderMap,
                body: &Bytes,
            ) -> Result<(Scope, Parameters, Origin), Failure> {
                let object = parameters::object_from_body(headers, body)?;
                let origin = Origin::of_body(&object);
                let (mut parameters, resources) = split_supplied(object)?;
                parameters::refuse_undeclared(operation, also, &parameters)?;
                parameters::apply_accept_language(operation, headers, &mut parameters);
                Ok((
                    scope_of(state, super::metadata::FHIR_VERSION, headers, resources)?,
                    parameters,
                    origin,
                ))
            }

            /// The failure of a value set operation: a refused value pointed at
            /// where `origin` shows the request wrote it, anything else as the
            /// request's scope answers it.
            fn value_set_refused(scope: &Scope, error: OperationError, origin: &Origin) -> Failure {
                if error.refused().is_some() {
                    Failure::placed(error, origin)
                } else {
                    scope.refused(error)
                }
            }

            fn run_lookup(
                scope: &Scope,
                invocation: &Invocation,
                parameters: &Parameters,
            ) -> Handled {
                let request = CodeSystemLookupRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let outcome =
                    lookup::lookup(scope.registry(), invocation, &map::lookup_input(&request))
                        .map_err(|e| scope.refused(e))?;
                Ok(Answer::Parameters(Box::new(
                    map::lookup_response(outcome).to_parameters(),
                )))
            }

            fn run_validate_code(
                scope: &Scope,
                invocation: &Invocation,
                parameters: &Parameters,
            ) -> Handled {
                let request = CodeSystemValidateCodeRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let outcome = validate_code::validate_code(
                    scope.registry(),
                    invocation,
                    &map::validate_code_input(&request),
                )
                .map_err(|e| scope.refused(e))?;
                Ok(Answer::Parameters(Box::new(
                    map::validate_code_response(outcome).to_parameters(),
                )))
            }

            fn run_subsumes(
                scope: &Scope,
                invocation: &Invocation,
                parameters: &Parameters,
            ) -> Handled {
                let request = CodeSystemSubsumesRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let outcome = subsumes::subsumes(
                    scope.registry(),
                    invocation,
                    &map::subsumes_input(&request),
                )
                .map_err(|e| scope.refused(e))?;
                Ok(Answer::Parameters(Box::new(
                    map::subsumes_response(outcome).to_parameters(),
                )))
            }

            /// Runs a `ValueSet` operation on the instance it was invoked on, when it
            /// was: the input's value set becomes the instance's, after [`binds`]
            /// refuses one naming another.
            fn bind_value_set(
                instance: Option<&Bound>,
                url: &mut Option<String>,
                version: &mut Option<String>,
                inline: bool,
            ) -> Result<(), Failure> {
                let Some(bound) = instance else {
                    return Ok(());
                };
                binds(bound, "ValueSet", url.as_deref(), version.as_deref(), inline, "valueSet")?;
                *url = Some(bound.url.clone());
                version.clone_from(&bound.version);
                Ok(())
            }

            fn run_expand(
                scope: &Scope,
                instance: Option<&Bound>,
                parameters: &Parameters,
                origin: &Origin,
            ) -> Handled {
                let request = ValueSetExpandRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let mut input = map::expand_input(&request);
                bind_value_set(
                    instance,
                    &mut input.url,
                    &mut input.value_set_version,
                    input.inline_value_set.is_some(),
                )?;
                let outcome = expand::expand(&scope.sources(), &input)
                    .map_err(|e| value_set_refused(scope, e, origin))?;
                Ok(Answer::ValueSet(Box::new(render::$fhir::expansion(
                    &outcome,
                ))))
            }

            fn run_value_set_validate_code(
                scope: &Scope,
                instance: Option<&Bound>,
                parameters: &Parameters,
                origin: &Origin,
            ) -> Handled {
                Ok(Answer::Parameters(Box::new(value_set_validation(
                    scope, instance, parameters, origin,
                )?)))
            }

            /// One `ValueSet/$validate-code` as this version's `Parameters`.
            fn value_set_validation(
                scope: &Scope,
                instance: Option<&Bound>,
                parameters: &Parameters,
                origin: &Origin,
            ) -> Result<Parameters, Failure> {
                let request = ValueSetValidateCodeRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let mut input = map::value_set_validate_input(&request);
                bind_value_set(
                    instance,
                    &mut input.url,
                    &mut input.value_set_version,
                    input.inline_value_set.is_some(),
                )?;
                let validation =
                    value_set_validate_code::validate_code(&scope.sources(), &input)
                        .map_err(|e| value_set_refused(scope, e, origin))?;
                Ok(map::value_set_validation_parameters(&validation))
            }

            /// The name of one validation in a batch, in and out.
            const VALIDATION: &str = "validation";

            /// `$batch-validate-code`: many validations against one value set, in one
            /// request.
            ///
            /// No `OperationDefinition` declares this operation, in any core package or
            /// in the terminology ecosystem IG, so the contract is the IG's own test
            /// cases (`batch/batch-validate`, `batch/batch-validate-bad`) plus the
            /// ecosystem's `$validate-code` semantics: the request carries the shared
            /// inputs once and a `validation` parameter per validation, each a
            /// `Parameters` of that validation's own inputs, and the answer repeats
            /// `validation` in the same order.
            fn run_batch_validate(scope: &Scope, parameters: &Parameters) -> Handled {
                let mut shared = Vec::new();
                let mut validations = Vec::new();
                for parameter in &parameters.parameter {
                    if parameter.name.value.as_deref() == Some(VALIDATION) {
                        validations.push(parameter);
                    } else {
                        shared.push(parameter.clone());
                    }
                }
                let answered: Vec<ParametersParameter> = validations
                    .into_iter()
                    .map(|validation| answer(scope, &shared, validation))
                    .collect();
                Ok(Answer::Parameters(Box::new(Parameters {
                    parameter: answered,
                    ..Default::default()
                })))
            }

            /// One validation of a batch, answered in its own slot.
            ///
            /// A validation the server cannot run answers an `OperationOutcome` in that
            /// slot and leaves the others alone, the way a batch `Bundle` entry does
            /// (<https://hl7.org/fhir/R4B/http.html#transaction>).
            fn answer(
                scope: &Scope,
                shared: &[ParametersParameter],
                validation: &ParametersParameter,
            ) -> ParametersParameter {
                let resource = match own(validation) {
                    Ok(own) => {
                        let merged = merge(shared, own);
                        match value_set_validation(scope, None, &merged, &Origin::Unplaced) {
                            Ok(answered) => Some(Resource::Parameters(Box::new(answered))),
                            Err(failure) => outcome_resource(&failure),
                        }
                    }
                    Err(failure) => outcome_resource(&failure),
                };
                ParametersParameter {
                    name: VALIDATION.into(),
                    resource,
                    ..Default::default()
                }
            }

            /// The `Parameters` one `validation` carries.
            fn own(validation: &ParametersParameter) -> Result<&Parameters, Failure> {
                match &validation.resource {
                    Some(Resource::Parameters(parameters)) => Ok(parameters),
                    _ => Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "invalid",
                        format!("each `{VALIDATION}` carries a `Parameters` resource"),
                    )),
                }
            }

            /// The shared inputs with one validation's own on top: a name the
            /// validation states is the validation's, not the request's.
            fn merge(shared: &[ParametersParameter], own: &Parameters) -> Parameters {
                let mut parameter: Vec<ParametersParameter> = shared
                    .iter()
                    .filter(|kept| {
                        !own.parameter
                            .iter()
                            .any(|stated| stated.name.value == kept.name.value)
                    })
                    .cloned()
                    .collect();
                parameter.extend(own.parameter.iter().cloned());
                Parameters {
                    parameter,
                    ..Default::default()
                }
            }

            /// A failure as the `OperationOutcome` resource of one slot.
            pub(super) fn outcome_resource(failure: &Failure) -> Option<Resource> {
                failure.outcome().to_json().ok().and_then(|object| {
                    Resource::from_json(
                        &object,
                        &mut fhir_types::codec::Path::root("OperationOutcome"),
                    )
                    .ok()
                })
            }

            /// The refusal of `reverse` on a version that does not declare it.
            ///
            /// R5 replaced `reverse` with the `target*` inputs
            /// (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>); the
            /// ecosystem wants `not-supported` naming them.
            fn reverse_refusal<'a>(mut names: impl Iterator<Item = &'a str>) -> Result<(), Failure> {
                if CONCEPT_MAP_TRANSLATE
                    .parameter(fhir_types::operation::ParameterUse::In, "reverse")
                    .is_none()
                    && names.any(|name| name == "reverse")
                {
                    return Err(Failure::new(
                        axum::http::StatusCode::BAD_REQUEST,
                        "not-supported",
                        format!(
                            "The 'reverse' parameter is not defined in {}: name the target concept with targetCode, targetCoding or targetCodeableConcept instead",
                            stringify!($fhir).to_uppercase()
                        ),
                    )
                    .kind("not-supported"));
                }
                Ok(())
            }

            fn run_translate(
                scope: &Scope,
                instance: Option<&Bound>,
                parameters: &Parameters,
            ) -> Handled {
                reverse_refusal(
                    parameters
                        .parameter
                        .iter()
                        .filter_map(|p| p.name.value.as_deref()),
                )?;
                let request = ConceptMapTranslateRequest::from_parameters(parameters)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let mut input = map::translate_input(&request);
                if let Some(bound) = instance {
                    binds(
                        bound,
                        "ConceptMap",
                        input.url.as_deref(),
                        input.concept_map_version.as_deref(),
                        input.inline_concept_map.is_some(),
                        "conceptMap",
                    )?;
                    input.url = Some(bound.url.clone());
                    input.concept_map_version.clone_from(&bound.version);
                }
                let translation =
                    translate::translate(&scope.sources(), &input).map_err(|e| scope.refused(e))?;
                Ok(Answer::Parameters(Box::new(map::translation_parameters(
                    &translation,
                ))))
            }

            /// Which operation a `Bundle` entry's URL names.
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            enum Which {
                /// `CodeSystem/$lookup`.
                Lookup,
                /// `CodeSystem/$validate-code`.
                ValidateCode,
                /// `CodeSystem/$subsumes`.
                Subsumes,
                /// `ValueSet/$expand`.
                Expand,
                /// `ValueSet/$validate-code`.
                ValueSetValidateCode,
                /// `ConceptMap/$translate`.
                Translate,
            }

            /// What an entry's path invoked the operation on: the type, or the
            /// instance of one of the three resource types.
            #[derive(Debug, Clone, PartialEq, Eq)]
            enum At {
                /// `[base]/{Type}/$op`.
                Type,
                /// `[base]/CodeSystem/{id}/$op`.
                CodeSystem(String),
                /// `[base]/ValueSet/{id}/$op`.
                ValueSet(String),
                /// `[base]/ConceptMap/{id}/$op`.
                ConceptMap(String),
            }

            /// The operation the path `segments` name, with the descriptor that
            /// declares its parameters and the level the path invokes it at.
            fn route(
                segments: &[&str],
            ) -> Option<(Which, &'static fhir_types::operation::Operation, At)> {
                let (which, operation, id) = match segments {
                    ["CodeSystem", "$lookup"] => (Which::Lookup, &CODE_SYSTEM_LOOKUP, At::Type),
                    // NOTE: R5 and the R6 ballot declare `$lookup` at the instance level
                    // and R4 and R4B do not, so the batch surface follows the version's
                    // own definition (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
                    ["CodeSystem", id, "$lookup"] if CODE_SYSTEM_LOOKUP.instance => (
                        Which::Lookup,
                        &CODE_SYSTEM_LOOKUP,
                        At::CodeSystem((*id).to_owned()),
                    ),
                    ["CodeSystem", "$validate-code"] => {
                        (Which::ValidateCode, &CODE_SYSTEM_VALIDATE_CODE, At::Type)
                    }
                    ["CodeSystem", id, "$validate-code"] => (
                        Which::ValidateCode,
                        &CODE_SYSTEM_VALIDATE_CODE,
                        At::CodeSystem((*id).to_owned()),
                    ),
                    ["CodeSystem", "$subsumes"] => {
                        (Which::Subsumes, &CODE_SYSTEM_SUBSUMES, At::Type)
                    }
                    ["CodeSystem", id, "$subsumes"] => (
                        Which::Subsumes,
                        &CODE_SYSTEM_SUBSUMES,
                        At::CodeSystem((*id).to_owned()),
                    ),
                    ["ValueSet", "$expand"] => (Which::Expand, &VALUE_SET_EXPAND, At::Type),
                    ["ValueSet", id, "$expand"] => (
                        Which::Expand,
                        &VALUE_SET_EXPAND,
                        At::ValueSet((*id).to_owned()),
                    ),
                    ["ValueSet", "$validate-code"] => (
                        Which::ValueSetValidateCode,
                        &VALUE_SET_VALIDATE_CODE,
                        At::Type,
                    ),
                    ["ValueSet", id, "$validate-code"] => (
                        Which::ValueSetValidateCode,
                        &VALUE_SET_VALIDATE_CODE,
                        At::ValueSet((*id).to_owned()),
                    ),
                    ["ConceptMap", "$translate"] => {
                        (Which::Translate, &CONCEPT_MAP_TRANSLATE, At::Type)
                    }
                    ["ConceptMap", id, "$translate"] => (
                        Which::Translate,
                        &CONCEPT_MAP_TRANSLATE,
                        At::ConceptMap((*id).to_owned()),
                    ),
                    _ => return None,
                };
                Some((which, operation, id))
            }

            /// Runs the operation `path` names, for a `Bundle` entry.
            ///
            /// `parameters` carries a `POST` entry's `Parameters` resource; a `GET`
            /// entry passes `None` and its inputs arrive in `query`.
            ///
            /// # Errors
            ///
            /// A path that names no operation of this server is a `404`, and every
            /// refusal the operation itself makes is the entry's own.
            pub(super) fn invoke(
                state: &AppState,
                headers: &HeaderMap,
                path: &str,
                query: &[(String, String)],
                sent: Option<Parameters>,
            ) -> Handled {
                let segments: Vec<&str> = path.split('/').collect();
                let Some((which, operation, id)) = route(&segments) else {
                    return Err(Failure::new(
                        StatusCode::NOT_FOUND,
                        "not-found",
                        format!("`{path}` is not an operation of this server"),
                    ));
                };
                let mut invocation = Invocation::Type;
                let mut bound = None;
                match &id {
                    At::Type => {}
                    At::CodeSystem(id) => invocation = instance(state, id)?,
                    At::ValueSet(id) => bound = Some(value_set_instance(state, id)?),
                    At::ConceptMap(id) => bound = Some(concept_map_instance(state, id)?),
                }
                let (scope, parameters) = match sent {
                    Some(sent) => {
                        let (mut own, resources) = split_resources(sent)?;
                        parameters::apply_accept_language(operation, headers, &mut own);
                        (scope_of(state, super::metadata::FHIR_VERSION, headers, resources)?, own)
                    }
                    None => {
                        if which == Which::Translate {
                            reverse_refusal(query.iter().map(|(name, _)| name.as_str()))?;
                        }
                        from_query(state, operation, headers, query)?
                    }
                };
                match which {
                    Which::Lookup => run_lookup(&scope, &invocation, &parameters),
                    Which::ValidateCode => run_validate_code(&scope, &invocation, &parameters),
                    Which::Subsumes => run_subsumes(&scope, &invocation, &parameters),
                    Which::Expand => run_expand(&scope, bound.as_ref(), &parameters, &Origin::Unplaced),
                    Which::ValueSetValidateCode => {
                        run_value_set_validate_code(&scope, bound.as_ref(), &parameters, &Origin::Unplaced)
                    }
                    Which::Translate => run_translate(&scope, bound.as_ref(), &parameters),
                }
            }

            $crate::version::operations::handlers::handlers!();
        }
    };
}

pub(crate) use operations;
