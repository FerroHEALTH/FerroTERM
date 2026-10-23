// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `$translate` runner: one code, through the maps this root holds.
//!
//! The screen reads and never writes. Every parameter of a run lives in the
//! address, so a translation is a link you can share and the back button walks
//! the runs.

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
use crate::components::field::picked_field;
use crate::components::field::text_field;
use crate::components::field::version_words;
use crate::components::field::versioned_row;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::components::runs::History;
use crate::components::shell::SelectedVersion;
use crate::components::state::undeclared;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::CONCEPT_MAP;
use crate::fhir::FhirClient;
use crate::fhir::error::FhirError;
use crate::fhir::named::Choice;
use crate::fhir::translate::Coding;
use crate::fhir::translate::NamedValue;
use crate::fhir::translate::TranslateAnswer;
use crate::fhir::translate::TranslateRequest;
use crate::fhir::translate::TranslationMatch;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::offers::choices;
use crate::offers::published;
use crate::offers::versions;
use crate::pages::concept_map::relationship_tone;
use crate::routes::TRANSLATE_PATH;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::runs::Run;
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    chip, cluster, definitions, page_header, panel_body, stack, titled_panel,
};
use ferrohealth_ui::widget::help::help_switch;

/// The operation this screen runs.
const TRANSLATE: &str = "translate";

/// The address parameter carrying the concept map the runner translates with.
const MAP_PARAM: &str = "map";

/// The address parameter carrying that concept map's business version.
const MAP_VERSION_PARAM: &str = "mapVersion";

/// The address parameter carrying the code system the code belongs to.
const SYSTEM_PARAM: &str = "system";

/// The address parameter carrying that code system's version.
const SYSTEM_VERSION_PARAM: &str = "systemVersion";

/// The address parameter carrying the code to translate.
const CODE_PARAM: &str = "code";

/// The address parameter carrying the code system an answer is wanted in.
const TARGET_PARAM: &str = "target";

/// The columns of the match table.
fn match_columns() -> [Column; 3] {
    [
        Column::text("target", tr(Msg::Target)),
        Column::text("relation", tr(Msg::Relationship)),
        Column::text("about", tr(Msg::WhatCameWithIt)),
    ]
}

/// Runs `ConceptMap/$translate` and renders what the server answered.
///
/// Every parameter is read from the address rather than from a private signal,
/// so a run is shareable by link. The read is reactive: a submit is a
/// navigation onto this same route, and `leptos_router` 0.8.15 then updates
/// the query without re-running this body (`src/nested_router.rs`, the
/// same-route-id branch).
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn TranslatePage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let _help = ferrohealth_ui::widget::help::provide_help();

    let query = use_query_map();
    let run: Signal<TranslateRequest> =
        Memo::new(move |_| query.with(|map| read_run(&|name| map.get(name)))).into();

    let heading = view! {
        <Title text=tr(Msg::TranslateACode) />
        {page_header(tr(Msg::TranslateACode), Some(tr(Msg::OneCodeThroughConceptMapsServer)), ())}
    }
    .into_any();

    // A run this browser remembers is the address that made it, and the
    // address is what the form already puts every parameter in.
    let made = Memo::new(move |_| {
        run.with(|run| {
            run.runnable().then(|| Run {
                screen: tr(Msg::Translate).clone(),
                subject: run.code.clone(),
                address: address(run, version.get()),
            })
        })
    });
    let history = History::recording(made);

    let form = runner_section(&client, version, run);
    let answer = answer_section(&client, version, run);

    // The answer keeps its own column above the large breakpoint, so a
    // parameter that lengthens the form never pushes it down the page.
    view! {
        {heading}
        {stack(
            view! {
                {form}
                {answer}
                {history.view()}
            },
        )}
    }
}

/// The parameters of a run, as the address carries them.
fn read_run(query: &dyn Fn(&str) -> Option<String>) -> TranslateRequest {
    let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
    TranslateRequest {
        concept_map: text(MAP_PARAM),
        concept_map_version: text(MAP_VERSION_PARAM),
        system: text(SYSTEM_PARAM),
        system_version: text(SYSTEM_VERSION_PARAM),
        code: text(CODE_PARAM),
        target_system: text(TARGET_PARAM),
    }
}

/// A run as the address parameters that reproduce it.
fn run_pairs(run: &TranslateRequest) -> Vec<(&'static str, String)> {
    vec![
        (MAP_PARAM, run.concept_map.clone()),
        (MAP_VERSION_PARAM, run.concept_map_version.clone()),
        (SYSTEM_PARAM, run.system.clone()),
        (SYSTEM_VERSION_PARAM, run.system_version.clone()),
        (CODE_PARAM, run.code.clone()),
        (TARGET_PARAM, run.target_system.clone()),
    ]
}

/// The address one run is, for a link or a navigation.
///
/// A parameter the reader left empty is left out, so the address says what the
/// run sent and nothing more.
pub(crate) fn address(run: &TranslateRequest, version: FhirVersion) -> String {
    let mut url = base_url()
        .segment(TRANSLATE_PATH)
        .query(VERSION_PARAM, version.segment());
    for (name, value) in run_pairs(run) {
        if !value.is_empty() {
            url = url.query(name, &value);
        }
    }
    url.render("")
}

/// The navigation options an address that already carries the base needs.
// NOTE: the router resolves a navigation against its base, so an address that
// already carries the base is passed unresolved (`leptos_router` 0.8.15
// `matching/resolve_path.rs`).
fn unresolved() -> NavigateOptions {
    NavigateOptions {
        resolve: false,
        ..NavigateOptions::default()
    }
}

/// What a control holds now, trimmed, or the empty string when it is gone.
fn typed_value(node: NodeRef<Input>) -> String {
    node.get()
        .map(|input| input.value())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The `$translate` runner, gated on what this root declares it can do.
///
/// The affordance appears only where the `CapabilityStatement` declares
/// `$translate` on `ConceptMap`, which is where a server states the operations
/// it answers per resource type
/// (<https://hl7.org/fhir/R4B/capabilitystatement.html>), so the screen never
/// offers a run the root then refuses. The answer below it is drawn whenever a
/// run is in the address, because a link a reader was sent still has to say
/// what the server answered.
fn runner_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    run: Signal<TranslateRequest>,
) -> AnyView {
    let map_search = published(client, version, CONCEPT_MAP);
    let system_search = published(client, version, CODE_SYSTEM);
    let picks = Picks {
        maps: choices(map_search),
        map_versions: versions(
            map_search,
            Memo::new(move |_| run.with(|run| run.concept_map.clone())),
        ),
        systems: choices(system_search),
        system_versions: versions(
            system_search,
            Memo::new(move |_| run.with(|run| run.system.clone())),
        ),
    };
    let read_client = client.clone();
    let statement = LocalResource::new(move || {
        let client = read_client.clone();
        let version = version.get();
        async move { client.capability_statement(version).await }
    });
    let declared = Signal::derive(move || {
        statement.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .is_some_and(|statement| statement.declares_operation(CONCEPT_MAP, TRANSLATE))
        })
    });
    view! {
        <section class="fui-card" aria-labelledby="translate-heading">
            <h2 id="translate-heading" class="fui-card-title">
                {tr(Msg::Parameters)}
            </h2>
            <Reading label=&{
                tr(Msg::ReadingWhatRootCanDo)
            }>
                {move || {
                    statement
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|result| match result {
                                    Ok(statement) => {
                                        if statement.declares_operation(CONCEPT_MAP, TRANSLATE) {
                                            ().into_any()
                                        } else {
                                            undeclared_view()
                                        }
                                    }
                                    Err(error) => failure_view(error),
                                })
                        })
                }}
            </Reading>
            <Show when=move || declared.get() fallback=|| ()>
                {runner_form(run, version, picks)}
            </Show>
        </section>
    }
    .into_any()
}

/// The statement that this root does not declare the operation.
fn undeclared_view() -> AnyView {
    undeclared(tr(Msg::RootSCapabilityStatementDoesNot))
}

/// A control's own value, gated so an unrelated navigation cannot wipe an edit.
///
/// `prop:value` writes the DOM property on every notification with no equality
/// check (tachys 0.2.18, `html/property.rs`), so a control seeded from a derive
/// over the whole run loses what a reader is typing the moment any other
/// parameter moves. A `Memo` over the one field notifies only when that field
/// changes.
fn seeded(run: Signal<TranslateRequest>, read: fn(&TranslateRequest) -> String) -> Memo<String> {
    Memo::new(move |_| run.with(read))
}

/// What this root publishes, as the offers this screen's pickers make.
#[derive(Clone, Copy)]
struct Picks {
    /// The concept maps a translation can run over.
    maps: Memo<Vec<Choice>>,
    /// The versions of the concept map the form names.
    map_versions: Memo<Vec<Choice>>,
    /// The code systems a code can be read in, and translated into.
    systems: Memo<Vec<Choice>>,
    /// The versions of the code system the form names.
    system_versions: Memo<Vec<Choice>>,
}

/// The runner, as a form that navigates rather than reloading the page.
fn runner_form(
    run: Signal<TranslateRequest>,
    version: Signal<FhirVersion>,
    picks: Picks,
) -> AnyView {
    let map: NodeRef<Input> = NodeRef::new();
    let map_version: NodeRef<Input> = NodeRef::new();
    let system: NodeRef<Input> = NodeRef::new();
    let system_version: NodeRef<Input> = NodeRef::new();
    let code: NodeRef<Input> = NodeRef::new();
    let target: NodeRef<Input> = NodeRef::new();
    let navigate = use_navigate();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = TranslateRequest {
            concept_map: typed_value(map),
            concept_map_version: typed_value(map_version),
            system: typed_value(system),
            system_version: typed_value(system_version),
            code: typed_value(code),
            target_system: typed_value(target),
        };
        navigate(&address(&typed, version.get()), unresolved());
    };
    view! {
        <form class="fui-form" on:submit=submit>
            {code_group(run, system, system_version, code, picks)}
            {map_group(run, map, map_version, picks)}
            {target_group(run, target, picks)}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::TRANSLATE, "16")}
                    {tr(Msg::Translate)}
                </button>
                {help_switch()}
            </div>
        </form>
    }
    .into_any()
}

/// Which map the run reads, and which version of it.
fn map_group(
    run: Signal<TranslateRequest>,
    map: NodeRef<Input>,
    map_version: NodeRef<Input>,
    picks: Picks,
) -> AnyView {
    group(
        tr(Msg::TheMap),
        vec![versioned_row(vec![
            picked_field(
                Field {
                    id: "translate-map",
                    name: MAP_PARAM,
                    label: tr(Msg::ConceptMap),
                    hint: tr(Msg::SentUrlParameterListWhatRoot),
                },
                map,
                seeded(run, |run| run.concept_map.clone()),
                picks.maps,
                canonical_words(),
            ),
            picked_field(
                Field {
                    id: "translate-map-version",
                    name: MAP_VERSION_PARAM,
                    label: tr(Msg::ConceptMapVersion),
                    hint: tr(Msg::SentConceptmapversionListVersionsRootPublished),
                },
                map_version,
                seeded(run, |run| run.concept_map_version.clone()),
                picks.map_versions,
                version_words(),
            ),
        ])],
    )
}

/// The code being translated, and the system it is read in.
fn code_group(
    run: Signal<TranslateRequest>,
    system: NodeRef<Input>,
    system_version: NodeRef<Input>,
    code: NodeRef<Input>,
    picks: Picks,
) -> AnyView {
    group(
        tr(Msg::TheCode),
        vec![
            versioned_row(vec![
                picked_field(
                    Field {
                        id: "translate-system",
                        name: SYSTEM_PARAM,
                        label: tr(Msg::CodeSystem2),
                        hint: tr(Msg::SystemCodeBelongsListWhatRoot),
                    },
                    system,
                    seeded(run, |run| run.system.clone()),
                    picks.systems,
                    canonical_words(),
                ),
                picked_field(
                    Field {
                        id: "translate-system-version",
                        name: SYSTEM_VERSION_PARAM,
                        label: tr(Msg::CodeSystemVersion),
                        hint: tr(Msg::ListVersionsRootPublishedCodeSystem),
                    },
                    system_version,
                    seeded(run, |run| run.system_version.clone()),
                    picks.system_versions,
                    version_words(),
                ),
            ]),
            text_field(
                Field {
                    id: "translate-code",
                    name: CODE_PARAM,
                    label: tr(Msg::Code),
                    hint: tr(Msg::CodeTranslateSentExactlyYouType),
                },
                code,
                seeded(run, |run| run.code.clone()),
            ),
        ],
    )
}

/// Where the answer is allowed to land.
fn target_group(run: Signal<TranslateRequest>, target: NodeRef<Input>, picks: Picks) -> AnyView {
    group(
        tr(Msg::TheTarget),
        vec![picked_field(
            Field {
                id: "translate-target",
                name: TARGET_PARAM,
                label: tr(Msg::TargetCodeSystem),
                hint: tr(Msg::NarrowsAnswerMatchesSystemListWhat),
            },
            target,
            seeded(run, |run| run.target_system.clone()),
            picks.systems,
            canonical_words(),
        )],
    )
}

/// The answer of one run: the matches, or the refusal that came instead.
///
/// The resource is created here, once, and the block below reads it. Nothing
/// is drawn until the address carries a code and its system, so a screen that
/// has not been asked to translate anything says nothing about a translation.
fn answer_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    run: Signal<TranslateRequest>,
) -> AnyView {
    let read_client = client.clone();
    let translated = Resource::new(
        move || (version.get(), run.get()),
        move |(version, request)| {
            let client = read_client.clone();
            async move {
                if request.runnable() {
                    Some(client.translate(version, &request).await)
                } else {
                    None
                }
            }
        },
    );

    // The live region is mounted the moment the address names a run, which is
    // before the read settles, so a screen reader hears the count when it
    // arrives rather than reading a sentence that was already there.
    let announcement = Memo::new(move |_| {
        translated.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .map(match_sentence)
                .unwrap_or_default()
        })
    });

    view! {
        <Show
            when=move || run.with(TranslateRequest::runnable)
            fallback=|| {
                titled_panel(
                    tr(Msg::TheTranslation),
                    (),
                    panel_body(
                        empty_quiet(
                            icon::TRANSLATE,
                            tr(Msg::NothingTranslatedYet),
                            tr(Msg::NameCodeSystemBelongsThenTranslate),
                        ),
                    ),
                )
            }
        >
            {answer_block(translated, announcement)}
        </Show>
    }
    .into_any()
}

/// The heading, the live region, and the answer.
fn answer_block(
    translated: Resource<Option<Result<TranslateAnswer, FhirError>>>,
    announcement: Memo<String>,
) -> AnyView {
    view! {
        <section class="fui-card fui-panel" aria-labelledby="translate-answer-heading">
            <div class="fui-panel-head">
                <h2 id="translate-answer-heading" class="fui-card-title">
                    {tr(Msg::TheTranslation)}
                </h2>
                <p aria-live="polite" class="fui-muted fui-small">
                    {move || announcement.get()}
                </p>
            </div>
            <Reading label=&{
                tr(Msg::RunningTheTranslation)
            }>
                {move || {
                    translated
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|answer| match answer {
                                    None => ().into_any(),
                                    Some(Ok(answer)) => answer_view(answer),
                                    Some(Err(error)) => failure_view(error),
                                })
                        })
                }}
            </Reading>
        </section>
    }
    .into_any()
}

/// The result, the message, and one block per reported match.
fn answer_view(answer: &TranslateAnswer) -> AnyView {
    let (result, tone) = match answer.result() {
        Some(true) => (tr(Msg::ServerTranslatedCode).clone(), Tone::Ok),
        Some(false) => (tr(Msg::ServerDidNotTranslateCode).clone(), Tone::Warn),
        None => (
            tr_with(Msg::Result, &[("not_declared", not_declared().into())]),
            Tone::Neutral,
        ),
    };
    let message = answer.message().map(|message| view! { <p>{message}</p> });
    let verdict = notice(
        tone,
        Announce::Status,
        None,
        view! {
            <p>
                <strong>{result}</strong>
            </p>
            {message}
        },
    );
    let used = used_view(&answer.used_maps());
    let matches = answer.matches();
    let table = if matches.is_empty() {
        panel_body(empty_quiet(
            icon::TRANSLATE,
            tr(Msg::NoMatch),
            tr(Msg::ServerReportedNoMatchCode),
        ))
        .into_any()
    } else {
        data_table(DataTable {
            caption: &tr(Msg::MatchesServerReported),
            columns: &match_columns(),
            sort: None,
            sort_href: &|_| String::new(),
            body: Body::Rows(matches.iter().map(match_row).collect_view().into_any()),
        })
        .into_any()
    };
    view! {
        <div class="fui-panel-body">{verdict}</div>
        {table}
        {used}
    }
    .into_any()
}

/// One match: how it relates, what it points at, and what came with it.
///
/// The relation is labelled by the element that carried it: R4 and R4B answer
/// `equivalence` and R5 and the R6 ballot `relationship`, over two different
/// sets of codes.
fn match_row(found: &TranslationMatch) -> AnyView {
    let target = found.target.as_ref().map_or_else(
        || {
            let words = if found.no_map {
                tr(Msg::CodeMapsNothingTarget).clone()
            } else {
                tr_with(Msg::Target2, &[("not_declared", not_declared().into())])
            };
            view! { <span class="fui-muted">{words}</span> }.into_any()
        },
        coding_cell,
    );
    let relations: Vec<AnyView> = found
        .relations()
        .into_iter()
        .map(|relation| {
            view! {
                <span title=tr(
                    relation.label,
                )>{badge(relationship_tone(Some(&relation.code)), &relation.code)}</span>
            }
            .into_any()
        })
        .collect();
    let relations = if relations.is_empty() {
        view! { <span class="fui-muted">{tr(Msg::NoneStated)}</span> }.into_any()
    } else {
        cluster(relations).into_any()
    };
    let facts: Vec<(String, AnyView)> = [
        (
            tr(Msg::FromConcept),
            found.source_concept.as_ref().map(Coding::rendered),
        ),
        (tr(Msg::FromMap), found.origin_map.clone()),
        (tr(Msg::SourceComment), found.source_comment.clone()),
        (tr(Msg::TargetComment), found.target_comment.clone()),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|value| (label, value)))
    .map(|(label, value)| {
        (
            label,
            view! { <span class="wrap-break-word">{value}</span> }.into_any(),
        )
    })
    .collect();
    let facts = (!facts.is_empty()).then(|| definitions(facts));
    view! {
        <tr>
            <td>{target}</td>
            <td data-label=tr(Msg::Relationship)>{relations}</td>
            <td data-label=tr(Msg::WhatCameWithIt)>
                <div>
                    {facts} {values_view(tr(Msg::AlsoProduces), &found.products)}
                    {values_view(tr(Msg::DependsOn), &found.depends_on)}
                    {values_view(tr(Msg::Properties), &found.properties)}
                </div>
            </td>
        </tr>
    }
    .into_any()
}

/// A coding as a cell: the code, its display under it, and its system.
fn coding_cell(coding: &Coding) -> AnyView {
    let version = coding
        .version
        .as_ref()
        .map(|version| format!(" | {version}"))
        .unwrap_or_default();
    view! {
        <span class="fui-mono">{coding.code.clone().unwrap_or_default()}</span>
        <span class="block">{coding.display.clone().unwrap_or_default()}</span>
        <span class="block font-normal fui-small fui-muted fui-mono sm:whitespace-nowrap">
            {coding.system.clone().unwrap_or_default()} {version}
        </span>
    }
    .into_any()
}

/// One list of named values a match carried, or nothing when it carried none.
fn values_view(label: String, values: &[NamedValue]) -> AnyView {
    if values.is_empty() {
        return ().into_any();
    }
    let items: Vec<AnyView> = values
        .iter()
        .map(|value| {
            let rendered = value.value.clone().map_or_else(
                || tr(Msg::ValueShapeConsoleDrawsNoText).into_any(),
                crate::components::stated,
            );
            view! {
                <span class="break-all">
                    <span class="fui-mono">{value.name.clone()}</span>
                    ": "
                    {rendered}
                </span>
            }
            .into_any()
        })
        .collect();
    view! {
        <p class="fui-label">{label}</p>
        <div class="fui-small">{bullet_list(items)}</div>
    }
    .into_any()
}

/// The maps the server says it translated through.
fn used_view(used: &[String]) -> AnyView {
    if used.is_empty() {
        return ().into_any();
    }
    let items: Vec<AnyView> = used
        .iter()
        .map(|canonical| chip(canonical.clone()).into_any())
        .collect();
    panel_body(view! {
        <p class="fui-label">{tr(Msg::MapsServerUsed)}</p>
        {cluster(items)}
    })
    .into_any()
}

/// A refused read, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    panel_body(view! { <Failure error=Signal::stored(error) /> }).into_any()
}

/// How many matches a translation reported, as a sentence.
fn match_sentence(answer: &TranslateAnswer) -> String {
    match answer.matches().len() {
        0 => tr(Msg::NoMatches).clone(),
        1 => tr(Msg::N1Match).clone(),
        matches => tr_with(Msg::Matches, &[("matches", (matches).to_string().into())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::UI_BASE;

    /// A stand-in for the address, which reads the same way a `ParamsMap` does.
    fn query<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + use<'a> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn a_run_reads_out_of_the_address_and_back_into_it() {
        let run = read_run(&query(&[
            ("map", "https://terminology.example/ConceptMap/m"),
            ("system", "https://terminology.example/a"),
            ("code", "x"),
        ]));
        assert!(run.runnable());
        assert_eq!(
            address(&run, FhirVersion::R4B),
            format!(
                "{UI_BASE}/translate\
                 ?fhir=r4b&map=https%3A%2F%2Fterminology.example%2FConceptMap%2Fm\
                 &system=https%3A%2F%2Fterminology.example%2Fa&code=x"
            ),
            "a parameter the reader left empty is left out of the address"
        );
    }

    #[test]
    fn a_run_naming_no_code_is_not_sent() {
        let run = read_run(&query(&[("system", "https://x.example/a")]));
        assert!(
            !run.runnable(),
            "the operation requires a code with its system"
        );
    }

    #[test]
    fn the_match_count_is_the_number_of_matches_the_server_reported() {
        let answer: TranslateAnswer = serde_json::from_str(
            r#"{"parameter":[{"name":"result","valueBoolean":true},
                {"name":"match","part":[{"name":"concept","valueCoding":{"code":"a"}}]},
                {"name":"match","part":[{"name":"concept","valueCoding":{"code":"b"}}]}]}"#,
        )
        .expect("the fixture is valid JSON");
        assert_eq!(match_sentence(&answer), "2 matches.");
    }
}
