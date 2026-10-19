//! The `$validate-code` and `$subsumes` runners.
//!
//! What the screen offers is what the served root declares. A `$validate-code`
//! form appears for a resource type the capability statement declares the
//! operation on, its instance-level field appears only where that root
//! declares the instance level, and the `$subsumes` form appears only for a
//! code system whose terminology capabilities declare subsumption. No code
//! system is named anywhere below.
//!
//! Every section is gated by a `Memo` and erased with `.into_any()` rather
//! than wrapped in a `<Show>`, because the component is generic over its
//! children and each use of it monomorphizes the whole subtree
//! (`.claude/rules/leptos-ui.md` §1, the bundle bar).

use leptos::ev::Event;
use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::components::failure::Failure;
use crate::components::field::Field;
use crate::components::field::canonical_words;
use crate::components::field::group;
use crate::components::field::language_example;
use crate::components::field::picked_field;
use crate::components::field::row;
use crate::components::field::select_field;
use crate::components::field::text_field;
use crate::components::field::text_field_showing;
use crate::components::field::version_words;
use crate::components::field::versioned_row;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::components::runs::History;
use crate::components::shell::SelectedVersion;
use crate::components::state::undeclared;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::FhirClient;
use crate::fhir::VALUE_SET;
use crate::fhir::capability::CapabilityStatement;
use crate::fhir::concept::CODE_PARAMETER;
use crate::fhir::concept::Hierarchy;
use crate::fhir::concept::chosen_version;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::DISPLAY_LANGUAGE_PARAMETER;
use crate::fhir::named::Choice;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::terminology::VersionRow;
use crate::fhir::translate::Coding;
use crate::fhir::validation::CODE_A_PARAMETER;
use crate::fhir::validation::CODE_B_PARAMETER;
use crate::fhir::validation::DISPLAY_PARAMETER;
use crate::fhir::validation::Offered;
use crate::fhir::validation::ParametersAnswer;
use crate::fhir::validation::SUBSUMES_OPERATION;
use crate::fhir::validation::SubsumesRequest;
use crate::fhir::validation::VALIDATE_CODE_OPERATION;
use crate::fhir::validation::VALUE_SET_VERSION_PARAMETER;
use crate::fhir::validation::ValidateOn;
use crate::fhir::validation::ValidateRequest;
use crate::fhir::validation::Validation;
use crate::fhir::validation::ValidationIssue;
use crate::fhir::validation::offered;
use crate::fhir::validation::subsumption_sentence;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::offers::choices;
use crate::offers::published;
use crate::offers::versions;
use crate::routes::SYSTEM_PARAM;
use crate::routes::SYSTEM_VERSION_PARAM;
use crate::routes::VALIDATE_PATH;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::routes::system_link;
use crate::runs::Run;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::refusal::{Issue, Severity, issue_list};
use ferrohealth_ui::view::surface::{
    card, chip, cluster, definitions, page_header, panel_body, stack,
};
use ferrohealth_ui::widget::help::help_switch;

/// The address parameter carrying which resource type the code is checked in.
const ON_PARAM: &str = "on";

/// The address parameter carrying the value set canonical.
const VALUE_SET_PARAM: &str = "valueSet";

/// The address parameter carrying the id of the resource a run addresses.
const ID_PARAM: &str = "id";

/// The address parameter carrying the id `$subsumes` addresses.
///
/// It is its own parameter because the two runners address different resource
/// types: one id names whatever `$validate-code` is running against, the other
/// always names a `CodeSystem`.
const SUBSUMES_ID_PARAM: &str = "subsumesId";

/// Runs `$validate-code` and `$subsumes`, and renders what the server answers.
///
/// Every parameter is read from the address rather than from a private signal,
/// so a case is shareable by link and the back button walks the run. The read
/// is a memo, because a resource refetches on a notification rather than on a
/// change.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ValidatePage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    // Both runners on this screen share the one help switch, so a reader who
    // asks for the parameters to be explained gets both forms explained.
    let _help = ferrohealth_ui::widget::help::provide_help();
    let query = use_query_map();
    let params: Signal<RunnerParams> =
        Memo::new(move |_| query.with(|map| RunnerParams::read(&|name| map.get(name)))).into();

    let terminology_client = client.clone();
    let capabilities = LocalResource::new(move || {
        let client = terminology_client.clone();
        let version = version.get();
        async move { client.terminology_capabilities(version).await }
    });
    let statement_client = client.clone();
    let statement = LocalResource::new(move || {
        let client = statement_client.clone();
        let version = version.get();
        async move { client.capability_statement(version).await }
    });
    let declared: Memo<Declared> = Memo::new(move |_| {
        let row = capabilities.with(|answered| {
            let document = answered.as_ref()?.as_ref().ok()?;
            params.with(|params| {
                let card = document.card(&params.system)?;
                chosen_version(&card, Some(&params.system_version))
                    .map(|row| (card.subsumption, card.url.clone(), row))
            })
        });
        let operations = statement.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(operations_of)
                .unwrap_or_default()
        });
        Declared {
            served: row.is_some(),
            subsumption: row.as_ref().and_then(|(subsumption, _, _)| *subsumption),
            hierarchy: row
                .as_ref()
                .map(|(_, _, row)| Hierarchy::of(row))
                .unwrap_or_default(),
            row: row.map(|(_, _, row)| row),
            operations,
        }
    });

    let heading = view! {
        <Title text=tr(Msg::CheckACode) />
        {page_header(
            tr(Msg::CheckACode),
            Some(
                tr(Msg::WhetherCodeValidCodeSystemValue),
            ),
            (),
        )}
    }
    .into_any();

    // A run this browser remembers is the address that made it, and the
    // address is what the form already puts every parameter in. Only the
    // validation is recorded: a subsumption is two codes of the system a
    // validation already named, so it rides in the same address.
    let made = Memo::new(move |_| {
        params.with(|params| {
            let offered = declared.with(|d| d.operations.validate(params.on));
            params.validation(offered).map(|_| Run {
                screen: tr(Msg::Validate).clone(),
                subject: params.code.clone(),
                address: params.address(version.get()),
            })
        })
    });
    let history = History::recording(made);

    let system_search = published(&client, version, CODE_SYSTEM);
    let value_set_search = published(&client, version, VALUE_SET);
    let picks = Picks {
        systems: choices(system_search),
        system_versions: versions(
            system_search,
            Memo::new(move |_| params.with(|params| params.system.clone())),
        ),
        value_sets: choices(value_set_search),
        value_set_versions: versions(
            value_set_search,
            Memo::new(move |_| params.with(|params| params.value_set.clone())),
        ),
    };

    let root = root_section(version, params, capabilities, statement, declared);
    let validate = validate_section(&client, version, params, declared, picks);
    let subsumes = subsumes_section(&client, version, params, declared);

    view! {
        {heading}
        {stack(
            view! {
                {validate}
                {root}
                {subsumes}
                {history.view()}
            },
        )}
    }
}

/// What this root publishes, as the offers this screen's pickers make.
#[derive(Clone, Copy)]
struct Picks {
    /// The code systems a code can be checked against.
    systems: Memo<Vec<Choice>>,
    /// The versions of the code system the form names.
    system_versions: Memo<Vec<Choice>>,
    /// The value sets a code can be checked against.
    value_sets: Memo<Vec<Choice>>,
    /// The versions of the value set the form names.
    value_set_versions: Memo<Vec<Choice>>,
}

/// What the served root declares, as every section below reads it.
///
/// The facts travel in one memo rather than five, because each memo type
/// instantiates its own reactive machinery in the bundle.
#[derive(Clone, Debug, Default, PartialEq)]
struct Declared {
    /// Whether this root's terminology capabilities name the system at all.
    served: bool,
    /// The version of the system the address selects.
    row: Option<VersionRow>,
    /// `codeSystem.subsumption`, whether the root answers `$subsumes` for it.
    subsumption: Option<bool>,
    /// What that version declares about walking its hierarchy.
    hierarchy: Hierarchy,
    /// What the root declares about the three operations this screen runs.
    operations: Operations,
}

/// What one root declares about the operations this screen offers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Operations {
    /// `CodeSystem/$validate-code`.
    code_system_validate: Offered,
    /// `ValueSet/$validate-code`.
    value_set_validate: Offered,
    /// `CodeSystem/$subsumes`.
    subsumes: Offered,
}

/// What `statement` declares about the operations this screen runs.
fn operations_of(statement: &CapabilityStatement) -> Operations {
    Operations {
        code_system_validate: offered(
            statement,
            ValidateOn::CodeSystem.resource_type(),
            VALIDATE_CODE_OPERATION,
        ),
        value_set_validate: offered(
            statement,
            ValidateOn::ValueSet.resource_type(),
            VALIDATE_CODE_OPERATION,
        ),
        subsumes: offered(
            statement,
            ValidateOn::CodeSystem.resource_type(),
            SUBSUMES_OPERATION,
        ),
    }
}

impl Operations {
    /// What the root declares about `$validate-code` on `on`.
    fn validate(self, on: ValidateOn) -> Offered {
        match on {
            ValidateOn::CodeSystem => self.code_system_validate,
            ValidateOn::ValueSet => self.value_set_validate,
        }
    }
}

/// The parameters the address carries, which are the ones the runners send.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RunnerParams {
    /// Which resource type `$validate-code` is invoked on.
    on: ValidateOn,
    /// The code system canonical, empty when the address names none.
    system: String,
    /// The code system version, empty for the server's own default.
    system_version: String,
    /// The value set canonical a value set run checks against.
    value_set: String,
    /// That value set's business version.
    value_set_version: String,
    /// The id of the resource a `$validate-code` run addresses.
    id: String,
    /// The id of the `CodeSystem` a `$subsumes` run addresses.
    subsumes_id: String,
    /// The code to check.
    code: String,
    /// The display the client asserts for it.
    display: String,
    /// The BCP 47 tag the operations send as `displayLanguage`.
    language: String,
    /// The first code of a subsumption test.
    code_a: String,
    /// The second code of a subsumption test.
    code_b: String,
}

impl RunnerParams {
    /// Reads the parameters out of the address.
    ///
    /// `query` looks one parameter up, which is what a `ParamsMap` does and
    /// what a test can do without a browser.
    fn read(query: &dyn Fn(&str) -> Option<String>) -> Self {
        let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
        Self {
            on: ValidateOn::read(&text(ON_PARAM)),
            system: text(SYSTEM_PARAM),
            system_version: text(SYSTEM_VERSION_PARAM),
            value_set: text(VALUE_SET_PARAM),
            value_set_version: text(VALUE_SET_VERSION_PARAMETER),
            id: text(ID_PARAM),
            subsumes_id: text(SUBSUMES_ID_PARAM),
            code: text(CODE_PARAMETER),
            display: text(DISPLAY_PARAMETER),
            language: text(DISPLAY_LANGUAGE_PARAMETER),
            code_a: text(CODE_A_PARAMETER),
            code_b: text(CODE_B_PARAMETER),
        }
    }

    /// The viewer address these parameters are, for a link or a navigation.
    // NOTE: every value stays in the query, which a click navigation carries
    // through untouched while it unescapes the path a second time
    // (`leptos_router` 0.8.15 `src/location/mod.rs`).
    fn address(&self, version: FhirVersion) -> String {
        let mut url = base_url()
            .segment(VALIDATE_PATH)
            .query(VERSION_PARAM, version.segment())
            .query(ON_PARAM, self.on.segment());
        for (name, value) in [
            (SYSTEM_PARAM, &self.system),
            (SYSTEM_VERSION_PARAM, &self.system_version),
            (VALUE_SET_PARAM, &self.value_set),
            (VALUE_SET_VERSION_PARAMETER, &self.value_set_version),
            (ID_PARAM, &self.id),
            (SUBSUMES_ID_PARAM, &self.subsumes_id),
            (CODE_PARAMETER, &self.code),
            (DISPLAY_PARAMETER, &self.display),
            (DISPLAY_LANGUAGE_PARAMETER, &self.language),
            (CODE_A_PARAMETER, &self.code_a),
            (CODE_B_PARAMETER, &self.code_b),
        ] {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        url.render("")
    }

    /// The `$validate-code` run these parameters make, when they make one.
    ///
    /// The instance-level id reaches the request only where the root declares
    /// that level, so an address carrying a stale id cannot make the screen
    /// ask for a route the root never offered.
    fn validation(&self, offered: Offered) -> Option<ValidateRequest> {
        let request = ValidateRequest {
            on: self.on,
            url: match self.on {
                ValidateOn::CodeSystem => self.system.clone(),
                ValidateOn::ValueSet => self.value_set.clone(),
            },
            id: if offered.instance {
                self.id.clone()
            } else {
                String::new()
            },
            resource_version: match self.on {
                ValidateOn::CodeSystem => self.system_version.clone(),
                ValidateOn::ValueSet => self.value_set_version.clone(),
            },
            system: match self.on {
                ValidateOn::CodeSystem => String::new(),
                ValidateOn::ValueSet => self.system.clone(),
            },
            system_version: match self.on {
                ValidateOn::CodeSystem => String::new(),
                ValidateOn::ValueSet => self.system_version.clone(),
            },
            code: self.code.clone(),
            display: self.display.clone(),
            display_language: self.language.clone(),
        };
        request.runnable().then_some(request)
    }

    /// The `$subsumes` run these parameters make, when they make one.
    fn subsumption(&self, offered: Offered) -> Option<SubsumesRequest> {
        let request = SubsumesRequest {
            system: self.system.clone(),
            version: self.system_version.clone(),
            id: if offered.instance {
                self.subsumes_id.clone()
            } else {
                String::new()
            },
            code_a: self.code_a.clone(),
            code_b: self.code_b.clone(),
        };
        request.runnable().then_some(request)
    }
}

/// What this root declares about the system the screen is working over.
fn root_section(
    version: Signal<FhirVersion>,
    params: Signal<RunnerParams>,
    capabilities: LocalResource<Result<TerminologyCapabilities, FhirError>>,
    statement: LocalResource<Result<CapabilityStatement, FhirError>>,
    declared: Memo<Declared>,
) -> AnyView {
    let named = Memo::new(move |_| params.with(|params| !params.system.is_empty()));

    let body = move || {
        view! {
            <Reading label=&{tr(Msg::ReadingWhatRootDeclares)}>
                {move || {
                    let refusal = capabilities
                        .with(|answered| answered.as_ref()?.as_ref().err().cloned())
                        .or_else(|| {
                            statement.with(|answered| answered.as_ref()?.as_ref().err().cloned())
                        });
                    match refusal {
                        Some(error) => failure_view(&error),
                        None => declared.with(system_view),
                    }
                }}
            </Reading>
        }
        .into_any()
    };

    view! {
        <section class="fui-card" aria-labelledby="validate-root-heading">
            <h2 id="validate-root-heading" class="fui-card-title">
                {tr(Msg::CodeSystemScreenWorksOver)}
            </h2>
            {move || {
                params
                    .with(|params| {
                        if params.system.is_empty() {
                            view! {
                                <p class="fui-muted">
                                    {tr(Msg::NoCodeSystemNamedYetChoose)}
                                </p>
                            }
                                .into_any()
                        } else {
                            let target = system_link(&params.system, version.get());
                            view! {
                                <p class="fui-mono break-all">
                                    <a href=target>{params.system.clone()}</a>
                                </p>
                            }
                                .into_any()
                        }
                    })
            }}
            {move || named.get().then(body)}
        </section>
    }
    .into_any()
}

/// What the served version holds, and what it says it can answer about it.
fn system_view(declared: &Declared) -> AnyView {
    let Some(row) = declared.row.as_ref() else {
        return note(tr(Msg::RootSTerminologyCapabilitiesDeclareNo));
    };
    let code = row
        .code
        .clone()
        .unwrap_or_else(|| tr_with(Msg::Version8, &[("not_declared", not_declared().into())]));
    let subsumption = match declared.subsumption {
        Some(true) => tr(Msg::SystemDeclaresSubsumptionSoSubsumesPanel),
        Some(false) => tr(Msg::SystemDeclaresDoesNotAnswerSubsumption),
        None => tr(Msg::RootDeclaresNothingAboutSubsumptionSystem),
    };
    let hierarchy = match &declared.hierarchy {
        Hierarchy::Walkable { property } => tr_with(
            Msg::DeclaresOperatorFilterSoHasHierarchy,
            &[("property", (property).clone().into())],
        ),
        Hierarchy::WithoutChildren { operators } => tr_with(
            Msg::DeclaresHierarchyOperatorsNoneWhichSelects,
            &[("operators", (operators.join(", ")).clone().into())],
        ),
        Hierarchy::Absent => tr(Msg::DeclaresNoHierarchyFilterOperatorSo),
    };
    definitions(vec![
        (
            tr(Msg::Version),
            view! { <span class="fui-mono">{code}</span> }.into_any(),
        ),
        (tr(Msg::Subsumption), subsumption.into_any()),
        (tr(Msg::Hierarchy), hierarchy.into_any()),
    ])
    .into_any()
}

/// The `$validate-code` form, and the answer it gets back.
fn validate_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<RunnerParams>,
    declared: Memo<Declared>,
    picks: Picks,
) -> AnyView {
    let offered: Memo<Offered> = Memo::new(move |_| {
        params.with(|params| declared.with(|d| d.operations.validate(params.on)))
    });
    let request: Signal<Option<ValidateRequest>> =
        Memo::new(move |_| params.with(|params| params.validation(offered.get()))).into();

    let read_client = client.clone();
    let answer = LocalResource::new(move || {
        let client = read_client.clone();
        let version = version.get();
        let request = request.get();
        async move {
            match request {
                Some(request) => Some(client.validate_code(version, &request).await),
                None => None,
            }
        }
    });

    // The live region is in the document before the read settles, which is
    // what lets a screen reader announce the outcome when it arrives.
    let announcement = move || {
        answer.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .map(|answer| answer.validation().sentence())
                .unwrap_or_default()
        })
    };

    let body = move || {
        let form = validate_form(params, version, declared, offered, picks);
        let answered = view! {
            <section class="fui-card fui-panel" aria-labelledby="validate-answer-heading">
                <div class="fui-panel-head">
                    <h3 id="validate-answer-heading" class="fui-card-title">
                        {tr(Msg::TheValidation)}
                    </h3>
                    <p aria-live="polite" class="fui-sr-only">
                        <Transition fallback=|| ()>{announcement}</Transition>
                    </p>
                </div>
                <Reading label=&{tr(Msg::RunningTheValidation)}>
                    {move || {
                        answer
                            .with(|answered| {
                                answered
                                    .as_ref()
                                    .map(|answer| match answer {
                                        None => {
                                            invitation(
                                                tr(Msg::NameCodeResourceCheckAgainstThen),
                                            )
                                        }
                                        Some(Ok(value)) => validation_view(&value.validation()),
                                        Some(Err(error)) => failure_view(error),
                                    })
                            })
                    }}
                </Reading>
            </section>
        }
        .into_any();
        view! {
            {form}
            {answered}
        }
        .into_any()
    };

    view! {
        <section class="fui-stack" aria-labelledby="validate-heading">
            <h2 id="validate-heading" class="fui-sr-only">
                {tr(Msg::IsThisCodeValid)}
            </h2>
            {move || {
                if offered.get().declared {
                    body()
                } else {
                    note(
                        tr(Msg::RootSCapabilityStatementDoesNot3),
                    )
                }
            }}
        </section>
    }
    .into_any()
}

/// The `$validate-code` parameters, as a form that navigates.
///
/// The router installs no `submit` listener, so the submit is handled here and
/// turned into a navigation. Each control is seeded from a memo over its own
/// field, so an unrelated navigation cannot rewrite what a reader is typing.
fn validate_form(
    params: Signal<RunnerParams>,
    version: Signal<FhirVersion>,
    declared: Memo<Declared>,
    offered: Memo<Offered>,
    picks: Picks,
) -> AnyView {
    let system: NodeRef<Input> = NodeRef::new();
    let system_version: NodeRef<Input> = NodeRef::new();
    let value_set: NodeRef<Input> = NodeRef::new();
    let value_set_version: NodeRef<Input> = NodeRef::new();
    let id: NodeRef<Input> = NodeRef::new();
    let code: NodeRef<Input> = NodeRef::new();
    let display: NodeRef<Input> = NodeRef::new();
    let language: NodeRef<Input> = NodeRef::new();

    let navigate = StoredValue::new(use_navigate());
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = params.with(|params| RunnerParams {
            system: typed_or(system, &params.system),
            system_version: typed_or(system_version, &params.system_version),
            value_set: typed_or(value_set, &params.value_set),
            value_set_version: typed_or(value_set_version, &params.value_set_version),
            id: typed_or(id, &params.id),
            code: typed_or(code, &params.code),
            display: typed_or(display, &params.display),
            language: typed_or(language, &params.language),
            ..params.clone()
        });
        let target = typed.address(version.get());
        navigate.with_value(|navigate| go(navigate, &target));
    };

    let form = view! {
        <form class="fui-form" on:submit=submit>
            {target_group(
                params,
                version,
                declared,
                offered,
                picks,
                Nodes {
                    system,
                    system_version,
                    value_set,
                    value_set_version,
                    id,
                },
            )}
            {code_group(params, code, display, language)}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::VALIDATE, "16")}
                    {tr(Msg::RunTheValidation)}
                </button>
                {help_switch()}
            </div>
        </form>
    };
    card(tr(Msg::IsThisCodeValid), form).into_any()
}

/// The controls one group of the form draws, by the value each carries.
#[derive(Clone, Copy)]
struct Nodes {
    /// The code system canonical.
    system: NodeRef<Input>,
    /// The version of that code system.
    system_version: NodeRef<Input>,
    /// The value set canonical.
    value_set: NodeRef<Input>,
    /// The version of that value set.
    value_set_version: NodeRef<Input>,
    /// The id of a stored resource, for an instance-level run.
    id: NodeRef<Input>,
}

/// Which resource the code is checked against, and which version of it.
fn target_group(
    params: Signal<RunnerParams>,
    version: Signal<FhirVersion>,
    declared: Memo<Declared>,
    offered: Memo<Offered>,
    picks: Picks,
    nodes: Nodes,
) -> AnyView {
    let value_set_fields = move || {
        (params.with(|params| params.on) == ValidateOn::ValueSet).then(|| {
            versioned_row(vec![
                picked_field(
                    Field {
                        id: "validate-value-set",
                        name: "url",
                        label: tr(Msg::ValueSet),
                        hint: tr(Msg::UrlParameterValuesetValidateCodeList),
                    },
                    nodes.value_set,
                    seeded(params, |params| params.value_set.clone()),
                    picks.value_sets,
                    canonical_words(),
                ),
                picked_field(
                    Field {
                        id: "validate-value-set-version",
                        name: VALUE_SET_VERSION_PARAMETER,
                        label: tr(Msg::ValueSetVersion),
                        hint: tr(Msg::ValuesetversionParameterListVersionsRootPublished),
                    },
                    nodes.value_set_version,
                    seeded(params, |params| params.value_set_version.clone()),
                    picks.value_set_versions,
                    version_words(),
                ),
            ])
        })
    };
    let instance_field = move || {
        offered.get().instance.then(|| {
            text_field(
                Field {
                    id: "validate-id",
                    name: ID_PARAM,
                    label: tr(Msg::ResourceIdInstanceLevel),
                    hint: tr(Msg::RootDeclaresInstanceLevelValidateCode),
                },
                nodes.id,
                seeded(params, |params| params.id.clone()),
            )
        })
    };
    let optional = view! {
        {value_set_fields}
        {instance_field}
    }
    .into_any();

    group(
        tr(Msg::WhatCodeCheckedAgainst),
        vec![
            on_field(params, version, declared),
            system_fields(params, picks, nodes),
            optional,
        ],
    )
}

/// The code system both runners work over, and the version of it.
fn system_fields(params: Signal<RunnerParams>, picks: Picks, nodes: Nodes) -> AnyView {
    versioned_row(vec![
        picked_field(
            Field {
                id: "validate-system",
                name: "system",
                label: tr(Msg::CodeSystem2),
                hint: tr(Msg::CodeSystemCodeBelongsListWhat),
            },
            nodes.system,
            seeded(params, |params| params.system.clone()),
            picks.systems,
            canonical_words(),
        ),
        picked_field(
            Field {
                id: "validate-system-version",
                name: "version",
                label: tr(Msg::CodeSystemVersion),
                hint: tr(Msg::ListVersionsRootPublishedCodeSystem2),
            },
            nodes.system_version,
            seeded(params, |params| params.system_version.clone()),
            picks.system_versions,
            version_words(),
        ),
    ])
}

/// The code being checked, the display asserted for it, and the language.
fn code_group(
    params: Signal<RunnerParams>,
    code: NodeRef<Input>,
    display: NodeRef<Input>,
    language: NodeRef<Input>,
) -> AnyView {
    group(
        tr(Msg::TheCode),
        vec![
            row(vec![
                text_field(
                    Field {
                        id: "validate-code",
                        name: CODE_PARAMETER,
                        label: tr(Msg::Code),
                        hint: tr(Msg::CodeCheckNothingSentUntilFilled),
                    },
                    code,
                    seeded(params, |params| params.code.clone()),
                ),
                text_field(
                    Field {
                        id: "validate-display",
                        name: DISPLAY_PARAMETER,
                        label: tr(Msg::Display),
                        hint: "The display you assert for the code. A wrong one comes back with the display the system prefers.".to_owned(),
                    },
                    display,
                    seeded(params, |params| params.display.clone()),
                ),
            ]),
            text_field_showing(
                Field {
                    id: "validate-display-language",
                    name: DISPLAY_LANGUAGE_PARAMETER,
                    label: tr(Msg::DisplayLanguage),
                    hint: "A BCP 47 tag the display is asserted in. Left empty, the server picks its own display.".to_owned(),
                },
                language,
                seeded(params, |params| params.language.clone()),
                Some(language_example()),
            ),
        ],
    )
}

/// The resource type the code is checked against, as the root declares them.
///
/// Changing it is a navigation rather than a form value, because it decides
/// which operation the screen runs and which fields belong on the form.
fn on_field(
    params: Signal<RunnerParams>,
    version: Signal<FhirVersion>,
    declared: Memo<Declared>,
) -> AnyView {
    let navigate = StoredValue::new(use_navigate());
    let change = move |event: Event| {
        let chosen = ValidateOn::read(&event_target_value(&event));
        let target = params.with(|params| {
            RunnerParams {
                on: chosen,
                ..params.clone()
            }
            .address(version.get())
        });
        navigate.with_value(|navigate| go(navigate, &target));
    };
    let options = move || {
        let operations = declared.with(|declared| declared.operations);
        [
            (
                ValidateOn::CodeSystem,
                tr(Msg::CodeSystemCodesystemValidateCode),
            ),
            (ValidateOn::ValueSet, tr(Msg::ValueSetValuesetValidateCode)),
        ]
        .into_iter()
        .filter(|(on, _)| operations.validate(*on).declared)
        .map(|(on, label)| {
            let selected = move || params.with(|params| params.on == on);
            view! {
                <option value=on.segment() selected=selected>
                    {label}
                </option>
            }
            .into_any()
        })
        .collect::<Vec<AnyView>>()
    };
    select_field(
        Field {
            id: "validate-on",
            name: ON_PARAM,
            label: tr(Msg::ValidateAgainst),
            hint: tr(Msg::OnlyResourceTypesRootSCapability),
        },
        Memo::new(move |_| params.with(|params| params.on.segment().to_owned())),
        options,
        change,
    )
}

/// The `$subsumes` form, and the answer it gets back.
fn subsumes_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<RunnerParams>,
    declared: Memo<Declared>,
) -> AnyView {
    let offered: Memo<Offered> = Memo::new(move |_| declared.with(|d| d.operations.subsumes));
    // The panel runs only for a system the root says it answers subsumption
    // for, which is `codeSystem.subsumption` of the terminology capabilities.
    let runs: Memo<bool> =
        Memo::new(move |_| declared.with(|declared| declared.subsumption == Some(true)));
    let request: Signal<Option<SubsumesRequest>> =
        Memo::new(move |_| params.with(|params| params.subsumption(offered.get()))).into();

    let read_client = client.clone();
    let answer = Resource::new(
        move || (version.get(), if runs.get() { request.get() } else { None }),
        move |(version, request)| {
            let client = read_client.clone();
            async move {
                match request {
                    Some(request) => Some(client.subsumes(version, &request).await),
                    None => None,
                }
            }
        },
    );
    let announcement = move || {
        let (code_a, code_b) = params.with(|params| (params.code_a.clone(), params.code_b.clone()));
        answer.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .map(|answer| outcome_sentence(answer, &code_a, &code_b))
                .unwrap_or_default()
        })
    };

    let panel = move || {
        let form = subsumes_form(params, version, offered);
        let answered = view! {
            <section class="fui-card fui-panel" aria-labelledby="subsumes-answer-heading">
                <div class="fui-panel-head">
                    <h3 id="subsumes-answer-heading" class="fui-card-title">
                        {tr(Msg::TheRelation)}
                    </h3>
                    <p aria-live="polite" class="fui-sr-only">
                        <Transition fallback=|| ()>{announcement}</Transition>
                    </p>
                </div>
                <Reading label=&{tr(Msg::RunningTheSubsumptionTest)}>
                    {move || {
                        let (code_a, code_b) = params
                            .with(|params| (params.code_a.clone(), params.code_b.clone()));
                        answer
                            .with(|answered| {
                                answered
                                    .as_ref()
                                    .map(|answer| match answer {
                                        None => {
                                            invitation(tr(Msg::NameTwoCodesSystemRunThem))
                                        }
                                        Some(Ok(value)) => outcome_view(value, &code_a, &code_b),
                                        Some(Err(error)) => failure_view(error),
                                    })
                            })
                    }}
                </Reading>
            </section>
        }
        .into_any();
        view! {
            {form}
            {answered}
        }
        .into_any()
    };

    view! {
        <section class="fui-stack" aria-labelledby="subsumes-heading">
            <h2 id="subsumes-heading" class="fui-sr-only">
                {tr(Msg::HowDoTwoCodesRelate)}
            </h2>
            {move || {
                if !offered.get().declared {
                    note(
                        tr(Msg::RootSCapabilityStatementDoesNot2),
                    )
                } else if runs.get() {
                    panel()
                } else {
                    note(
                        tr(Msg::RunnerOffersSubsumesOnlyCodeSystem),
                    )
                }
            }}
        </section>
    }
    .into_any()
}

/// The `$subsumes` parameters, as a form that navigates.
///
/// The system and its version come from the form above, because both runners
/// work over the one code system the address names.
fn subsumes_form(
    params: Signal<RunnerParams>,
    version: Signal<FhirVersion>,
    offered: Memo<Offered>,
) -> AnyView {
    let code_a: NodeRef<Input> = NodeRef::new();
    let code_b: NodeRef<Input> = NodeRef::new();
    let id: NodeRef<Input> = NodeRef::new();

    let navigate = StoredValue::new(use_navigate());
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = params.with(|params| RunnerParams {
            code_a: typed_or(code_a, &params.code_a),
            code_b: typed_or(code_b, &params.code_b),
            subsumes_id: typed_or(id, &params.subsumes_id),
            ..params.clone()
        });
        let target = typed.address(version.get());
        navigate.with_value(|navigate| go(navigate, &target));
    };
    let instance_field = move || {
        offered.get().instance.then(|| {
            text_field(
                Field {
                    id: "subsumes-id",
                    name: SUBSUMES_ID_PARAM,
                    label: tr(Msg::CodesystemIdInstanceLevel),
                    hint: tr(Msg::RootDeclaresInstanceLevelSubsumesSo),
                },
                id,
                seeded(params, |params| params.subsumes_id.clone()),
            )
        })
    };

    let codes = group(
        tr(Msg::TheTwoCodes),
        vec![
            row(vec![
                text_field(
                    Field {
                        id: "subsumes-code-a",
                        name: CODE_A_PARAMETER,
                        label: tr(Msg::CodeA),
                        hint: tr(Msg::FirstCodeAnswerSaysHowRelates),
                    },
                    code_a,
                    seeded(params, |params| params.code_a.clone()),
                ),
                text_field(
                    Field {
                        id: "subsumes-code-b",
                        name: CODE_B_PARAMETER,
                        label: tr(Msg::CodeB),
                        hint: tr(Msg::SecondCodeBothCodesReadCode),
                    },
                    code_b,
                    seeded(params, |params| params.code_b.clone()),
                ),
            ]),
            view! { {instance_field} }.into_any(),
        ],
    );

    let form = view! {
        <form class="fui-form" on:submit=submit>
            {codes}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::BROWSE, "16")}
                    {tr(Msg::RunTheSubsumptionTest)}
                </button>
                {help_switch()}
            </div>
        </form>
    };
    card(tr(Msg::HowDoTwoCodesRelate), form).into_any()
}

/// The answer of one `$validate-code` run, in full.
fn validation_view(read: &Validation) -> AnyView {
    let (verdict, tone) = match read.result {
        Some(true) => (tr(Msg::ValidResultTrue), Tone::Ok),
        Some(false) => (tr(Msg::NotValidResultFalse), Tone::Bad),
        None => (tr(Msg::ServerDeclaredNoResult), Tone::Neutral),
    };
    let message = read
        .message
        .clone()
        .map(|message| view! { <p>{message}</p> });
    let verdict = notice(
        tone,
        Announce::Status,
        None,
        view! {
            <p>
                <strong>{verdict}</strong>
            </p>
            {message}
        },
    );
    let facts: Vec<(String, AnyView)> = [
        (tr(Msg::Code), read.code.clone()),
        (tr(Msg::CodeSystemSpells), read.normalized_code.clone()),
        (tr(Msg::System), read.system.clone()),
        (tr(Msg::VersionValidatedIn), read.version.clone()),
        (tr(Msg::Display), read.display.clone()),
        (tr(Msg::Status), read.status.clone()),
        (
            tr(Msg::Inactive2),
            read.inactive
                .map(|flag| String::from(if flag { "yes" } else { "no" })),
        ),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        value.map(|value| {
            (
                label,
                view! { <span class="break-all">{value}</span> }.into_any(),
            )
        })
    })
    .collect();
    let inactive = (read.inactive == Some(true)).then(|| {
        notice(
            Tone::Warn,
            Announce::Status,
            None,
            tr(Msg::ConceptInactiveCodeSystemServerServed),
        )
    });
    view! {
        <div class="fui-panel-body fui-stack">
            {verdict} {inactive} {definitions(facts)} {concept_view(read)}
            {unknown_systems_view(&read.unknown_systems)} {issues_view(&read.issues)}
        </div>
    }
    .into_any()
}

/// The `codeableConcept` the answer echoed back, when it echoed one.
fn concept_view(read: &Validation) -> AnyView {
    let Some(concept) = read.concept.as_ref() else {
        return ().into_any();
    };
    let text = concept
        .text
        .clone()
        .unwrap_or_else(|| not_declared().clone());
    let codings: Vec<AnyView> = concept
        .coding
        .iter()
        .map(|coding| {
            let version = coding
                .version
                .clone()
                .map(|version| tr_with(Msg::Version9, &[("version", (version).clone().into())]))
                .unwrap_or_default();
            view! { <span class="fui-mono break-all">{Coding::rendered(coding)} {version}</span> }
                .into_any()
        })
        .collect();
    view! {
        <div>
            <p class="fui-label">{tr(Msg::ConceptServerEchoed)}</p>
            <p>{tr(Msg::Text)} {text}</p>
            {bullet_list(codings)}
        </div>
    }
    .into_any()
}

/// The code systems the answer says this server does not hold.
fn unknown_systems_view(systems: &[String]) -> AnyView {
    if systems.is_empty() {
        return ().into_any();
    }
    let listed: Vec<AnyView> = systems
        .iter()
        .map(|system| chip(system.clone()).into_any())
        .collect();
    view! {
        <div>
            <p class="fui-label">{tr(Msg::CodeSystemsServerDoesNotHold)}</p>
            {cluster(listed)}
        </div>
    }
    .into_any()
}

/// The itemised issues of the answer, in FerroUI's issue list.
///
/// The coding of `issue.details` is each issue's detail codes, because the
/// terminology ecosystem binds it as a `SHALL` and leaves `issue.code`
/// unbound (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
fn issues_view(issues: &[ValidationIssue]) -> AnyView {
    if issues.is_empty() {
        return ().into_any();
    }
    let listed: Vec<Issue> = issues
        .iter()
        .map(|issue| {
            let severity = match issue.severity.as_str() {
                "fatal" => Severity::Fatal,
                "warning" => Severity::Warning,
                "information" => Severity::Information,
                _ => Severity::Error,
            };
            Issue::new(severity, issue.issue_code.clone())
                .text(issue.text.clone())
                .place(issue.expressions.join(", "))
                .details(
                    issue
                        .classifications
                        .iter()
                        .map(|coding| coding.code.clone())
                        .collect(),
                )
        })
        .collect();
    view! {
        <div>
            <p class="fui-label">{tr(Msg::IssuesServerItemised)}</p>
            {issue_list(&listed)}
        </div>
    }
    .into_any()
}

/// The `$subsumes` answer, as the code and the sentence it stands for.
fn outcome_view(answer: &ParametersAnswer, code_a: &str, code_b: &str) -> AnyView {
    let Some(outcome) = answer.subsumption() else {
        return note(tr(Msg::ServerAnsweredNoOutcomeParameterSo));
    };
    let sentence = subsumption_sentence(&outcome, code_a, code_b);
    panel_body(notice(
        Tone::Info,
        Announce::Status,
        None,
        view! {
            <p>
                <strong class="fui-mono">{outcome}</strong>
            </p>
            <p>{sentence}</p>
        },
    ))
    .into_any()
}

/// The sentence a `$subsumes` answer is announced as.
fn outcome_sentence(answer: &ParametersAnswer, code_a: &str, code_b: &str) -> String {
    answer.subsumption().map_or_else(
        || tr(Msg::ServerAnsweredNoOutcome),
        |outcome| {
            format!(
                "{outcome}. {}",
                subsumption_sentence(&outcome, code_a, code_b)
            )
        },
    )
}

/// What a panel says before it has been given enough to run.
fn invitation(text: String) -> AnyView {
    panel_body(empty_quiet(icon::VALIDATE, tr(Msg::NothingRunYet), text)).into_any()
}

/// A sentence stating what the server did not declare.
fn note(text: String) -> AnyView {
    undeclared(text)
}

/// A refusal, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    panel_body(view! { <Failure error=Signal::stored(error) /> }).into_any()
}

/// A control's own value, gated so an unrelated navigation cannot wipe an edit.
///
/// `prop:value` writes the DOM property on every notification with no equality
/// check (tachys 0.2.18, `html/property.rs`), so a control seeded from a derive
/// over the whole parameter set loses what a reader is typing the moment any
/// other parameter moves. A `Memo` over the one field notifies only when that
/// field changes.
fn seeded(params: Signal<RunnerParams>, read: fn(&RunnerParams) -> String) -> Memo<String> {
    Memo::new(move |_| params.with(read))
}

/// What a control holds now, or `kept` when the control is not on the form.
///
/// A field the current form does not render keeps the value the address
/// carries, so switching between the two resource types does not discard what
/// the other form was told.
fn typed_or(node: NodeRef<Input>, kept: &str) -> String {
    node.get()
        .map_or_else(|| kept.to_owned(), |input| input.value().trim().to_owned())
}

/// Navigates to an address that already carries the router base.
// NOTE: the router resolves a navigation against its base, so an address that
// already carries the base is passed unresolved (`leptos_router` 0.8.15
// `matching/resolve_path.rs`).
fn go(navigate: &dyn Fn(&str, NavigateOptions), target: &str) {
    navigate(
        target,
        NavigateOptions {
            resolve: false,
            ..NavigateOptions::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::UI_BASE;

    fn read(pairs: &[(&str, &str)]) -> RunnerParams {
        RunnerParams::read(&|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    fn both_levels() -> Offered {
        Offered {
            declared: true,
            instance: true,
        }
    }

    fn type_level() -> Offered {
        Offered {
            declared: true,
            instance: false,
        }
    }

    #[test]
    fn every_parameter_is_read_from_the_address() {
        assert_eq!(
            read(&[
                ("on", "valueset"),
                ("system", " https://terminology.example/animals "),
                ("version", "2.0"),
                ("valueSet", "https://terminology.example/vs"),
                ("valueSetVersion", "1.2"),
                ("id", "animals"),
                ("subsumesId", "cs-1"),
                ("code", "cat"),
                ("display", "Dog"),
                ("displayLanguage", "nl-NL"),
                ("codeA", "cat"),
                ("codeB", "kitten"),
            ]),
            RunnerParams {
                on: ValidateOn::ValueSet,
                system: "https://terminology.example/animals".to_owned(),
                system_version: "2.0".to_owned(),
                value_set: "https://terminology.example/vs".to_owned(),
                value_set_version: "1.2".to_owned(),
                id: "animals".to_owned(),
                subsumes_id: "cs-1".to_owned(),
                code: "cat".to_owned(),
                display: "Dog".to_owned(),
                language: "nl-NL".to_owned(),
                code_a: "cat".to_owned(),
                code_b: "kitten".to_owned(),
            },
            "the address is the whole state of both runners"
        );
    }

    #[test]
    fn an_address_round_trips_through_the_reader() {
        let params = read(&[
            ("on", "valueset"),
            ("system", "https://terminology.example/x?a=b"),
            ("valueSet", "https://terminology.example/vs"),
            ("code", "c&d"),
            ("codeA", "cat"),
            ("codeB", "kitten"),
        ]);
        assert_eq!(
            params.address(FhirVersion::R5),
            format!(
                "{UI_BASE}/validate?fhir=r5&on=valueset\
                 &system=https%3A%2F%2Fterminology.example%2Fx%3Fa%3Db\
                 &valueSet=https%3A%2F%2Fterminology.example%2Fvs&code=c%26d&codeA=cat&codeB=kitten"
            ),
            "every value the reader typed is encoded into the parameter it belongs to"
        );
        // The router percent-decodes a query on read, so the parameters come
        // back as they were typed.
        assert_eq!(
            read(&[
                ("on", "valueset"),
                ("system", "https://terminology.example/x?a=b"),
                ("valueSet", "https://terminology.example/vs"),
                ("code", "c&d"),
                ("codeA", "cat"),
                ("codeB", "kitten"),
            ]),
            params,
            "the address the runner writes is the address it reads"
        );
    }

    #[test]
    fn an_empty_runner_is_a_link_worth_sharing_and_nothing_more() {
        assert_eq!(
            RunnerParams::default().address(FhirVersion::R4B),
            format!("{UI_BASE}/validate?fhir=r4b&on=codesystem")
        );
    }

    #[test]
    fn a_code_system_run_sends_the_system_as_the_url_it_checks_against() {
        let params = read(&[
            ("system", "https://terminology.example/animals"),
            ("version", "2.0"),
            ("code", "cat"),
        ]);
        assert_eq!(
            params.validation(type_level()),
            Some(ValidateRequest {
                on: ValidateOn::CodeSystem,
                url: "https://terminology.example/animals".to_owned(),
                resource_version: "2.0".to_owned(),
                code: "cat".to_owned(),
                ..ValidateRequest::default()
            }),
            "the code system is the resource the code is checked against"
        );
    }

    #[test]
    fn a_value_set_run_names_the_value_set_and_the_code_system_apart() {
        let params = read(&[
            ("on", "valueset"),
            ("system", "https://terminology.example/animals"),
            ("version", "2.0"),
            ("valueSet", "https://terminology.example/vs"),
            ("valueSetVersion", "1.2"),
            ("code", "cat"),
        ]);
        assert_eq!(
            params.validation(type_level()),
            Some(ValidateRequest {
                on: ValidateOn::ValueSet,
                url: "https://terminology.example/vs".to_owned(),
                resource_version: "1.2".to_owned(),
                system: "https://terminology.example/animals".to_owned(),
                system_version: "2.0".to_owned(),
                code: "cat".to_owned(),
                ..ValidateRequest::default()
            }),
            "the value set is what the code is checked against, and the system says where the code is from"
        );
    }

    #[test]
    fn an_instance_id_reaches_the_request_only_where_the_root_declares_that_level() {
        let params = read(&[("id", "animals"), ("code", "cat")]);
        assert_eq!(
            params.validation(both_levels()).map(|run| run.id),
            Some("animals".to_owned()),
            "the root declares the instance level, so the id addresses the resource"
        );
        assert_eq!(
            params.validation(type_level()),
            None,
            "a root that declares no instance level leaves the run naming nothing to check against"
        );
    }

    #[test]
    fn a_half_named_case_asks_the_server_for_nothing() {
        assert_eq!(
            read(&[("code", "cat")]).validation(type_level()),
            None,
            "a code with no system names nothing to check it against"
        );
        assert_eq!(
            read(&[("system", "https://terminology.example/animals")]).validation(type_level()),
            None,
            "a system with no code asks nothing"
        );
        assert_eq!(
            read(&[
                ("on", "valueset"),
                ("valueSet", "https://terminology.example/vs"),
                ("code", "cat"),
            ])
            .validation(type_level()),
            None,
            "a value set run needs the code system the code is from"
        );
    }

    #[test]
    fn a_subsumption_run_takes_the_system_the_screen_is_working_over() {
        let params = read(&[
            ("system", "https://terminology.example/animals"),
            ("version", "2.0"),
            ("codeA", "cat"),
            ("codeB", "kitten"),
        ]);
        assert_eq!(
            params.subsumption(type_level()),
            Some(SubsumesRequest {
                system: "https://terminology.example/animals".to_owned(),
                version: "2.0".to_owned(),
                code_a: "cat".to_owned(),
                code_b: "kitten".to_owned(),
                ..SubsumesRequest::default()
            }),
            "both runners work over the one code system the address names"
        );
        assert_eq!(
            read(&[
                ("system", "https://terminology.example/animals"),
                ("codeA", "cat")
            ])
            .subsumption(type_level()),
            None,
            "one code is not a comparison"
        );
    }

    #[test]
    fn a_subsumption_instance_id_is_its_own_parameter() {
        let params = read(&[
            ("subsumesId", "cs-1"),
            ("codeA", "cat"),
            ("codeB", "kitten"),
        ]);
        assert_eq!(
            params.subsumption(both_levels()).map(|run| run.id),
            Some("cs-1".to_owned()),
            "the id a subsumption run addresses always names a CodeSystem, so it is not the other runner's id"
        );
        assert_eq!(
            params.subsumption(type_level()),
            None,
            "without the instance level there is nothing left naming a system"
        );
    }

    #[test]
    fn switching_the_resource_type_keeps_what_the_other_form_was_told() {
        let params = read(&[
            ("system", "https://terminology.example/animals"),
            ("valueSet", "https://terminology.example/vs"),
            ("code", "cat"),
        ]);
        let switched = RunnerParams {
            on: ValidateOn::ValueSet,
            ..params
        };
        assert!(
            switched.address(FhirVersion::R4B).contains("valueSet="),
            "the value set a reader typed survives the switch"
        );
        assert!(
            switched.address(FhirVersion::R4B).contains("system="),
            "so does the code system"
        );
    }

    #[test]
    fn the_root_offers_only_the_operations_the_recorded_statement_declares() {
        let statement: CapabilityStatement =
            serde_json::from_str(include_str!("../../fixtures/capability-statement-r4b.json"))
                .expect("the recorded statement is valid JSON");
        let operations = operations_of(&statement);
        assert!(operations.validate(ValidateOn::CodeSystem).declared);
        assert!(operations.validate(ValidateOn::ValueSet).declared);
        assert!(operations.subsumes.declared);
        assert!(
            operations.subsumes.instance,
            "this root declares the instance level of $subsumes"
        );
        assert_eq!(
            operations_of(&CapabilityStatement::default()),
            Operations::default(),
            "a statement the viewer could not read offers no affordance at all"
        );
    }
}
