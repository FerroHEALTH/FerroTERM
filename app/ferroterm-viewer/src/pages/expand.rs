//! The expansion runner: `ValueSet/$expand`, one page at a time.

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
use crate::components::field::check_field;
use crate::components::field::group;
use crate::components::field::language_example;
use crate::components::field::note;
use crate::components::field::number_field;
use crate::components::field::picked_field;
use crate::components::field::text_field;
use crate::components::field::text_field_showing;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::components::runs::History;
use crate::components::shell::SelectedVersion;
use crate::fhir::FhirClient;
use crate::fhir::VALUE_SET;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::ACTIVE_ONLY_PARAMETER;
use crate::fhir::expansion::COUNT_PARAMETER;
use crate::fhir::expansion::ConceptRow;
use crate::fhir::expansion::DISPLAY_LANGUAGE_PARAMETER;
use crate::fhir::expansion::DesignationRow;
use crate::fhir::expansion::ExpandRequest;
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::expansion::Expansion;
use crate::fhir::expansion::FILTER_PARAMETER;
use crate::fhir::expansion::INCLUDE_DESIGNATIONS_PARAMETER;
use crate::fhir::expansion::OFFSET_PARAMETER;
use crate::fhir::expansion::ParameterLine;
use crate::fhir::expansion::URL_PARAMETER;
use crate::fhir::expansion::Unclosed;
use crate::fhir::named::Choice;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, message, tr, tr_with};
use crate::offers::choices;
use crate::offers::published;
use crate::paging::MAX_COUNT;
use crate::paging::Page;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::runs::Run;
use crate::settings::Settings;
use crate::settings::parse_page_size;
use crate::styles;
use ferrohealth_ui::table::model::filled_columns;
use ferrohealth_ui::table::model::{Noun, OpenWindow, Paging, Window};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Listing, Step, data_table, open_pager, step_label,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::layout::bullet_list;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{card, page_header, panel_body, stack};
use ferrohealth_ui::widget::help::help_switch;

/// The runner's own path below the router base.
const RUNNER_PATH: &str = "expand";

/// The `IssueType` code a server refuses an oversized selection with.
///
/// "The operation was refused because the value set is too costly to expand"
/// (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
const TOO_COSTLY: &str = "too-costly";

/// The columns of the concept table.
fn concept_columns() -> [Column; 4] {
    [
        Column::text("code", tr(Msg::Code)),
        Column::text("display", tr(Msg::Display)),
        Column::text("system", tr(Msg::System)),
        Column::text("flags", tr(Msg::Flags)),
    ]
}

/// Runs `ValueSet/$expand` and walks the pages of its answer.
///
/// Every parameter is read from the address rather than from a private signal,
/// so a page is shareable and the back button walks the run. The read is under
/// a transition boundary, because it refetches on every parameter and every
/// page
/// (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>).
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ExpandPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let page_size = expect_context::<Settings>().page_size;

    // A submit is a navigation onto this same path, which `leptos_router`
    // answers by updating the query without re-running this body, so the
    // parameters are read reactively and never once at setup. Both reads are
    // memos: a resource refetches on a notification rather than on a change,
    // so an unrelated write to the stored page size would otherwise re-issue
    // an identical `$expand`.
    let query = use_query_map();
    let params: Signal<RunnerParams> =
        Memo::new(move |_| query.with(|map| RunnerParams::read(&|name| map.get(name), page_size)))
            .into();
    let request: Signal<Option<ExpandRequest>> =
        Memo::new(move |_| params.with(RunnerParams::request)).into();

    let heading = view! {
        <Title text=tr(Msg::ExpandAValueSet) />
        {page_header(
            tr(Msg::ExpandAValueSet),
            Some(
                tr(Msg::WhichCodesValueSetHoldsFrom),
            ),
            (),
        )}
    }
    .into_any();

    // A run this browser remembers is the address that made it, and the
    // address is what the form already puts every parameter in.
    let made = Memo::new(move |_| {
        params.with(|params| {
            params.request().map(|_| Run {
                screen: tr(Msg::Expand).clone(),
                subject: params.url.clone(),
                address: params.address(version.get()),
            })
        })
    });
    let history = History::recording(made);

    let form = form_section(
        params,
        version,
        choices(published(&client, version, VALUE_SET)),
    );
    let results = result_section(&client, version, params, request);

    // The answer keeps its own column above the large breakpoint, so a
    // parameter that lengthens the form never pushes it down the page. Below
    // that breakpoint the two stack, form first, which is the order the work
    // happens in.
    view! {
        {heading}
        {stack(
            view! {
                {form}
                {results}
                {history.view()}
            },
        )}
    }
}

/// The parameters the address carries, which are the ones the runner sends.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RunnerParams {
    /// The value set canonical, empty when the reader has typed none yet.
    url: String,
    /// The text filter.
    filter: String,
    /// The BCP 47 tag displays are asked for in.
    display_language: String,
    /// The page size, absent for a run that asks for no page at all.
    count: Option<u32>,
    /// A page size the address named and the viewer cannot ask for, kept as
    /// the reader wrote it so the screen can say it was not used.
    refused_count: Option<String>,
    /// Where the page starts.
    offset: u32,
    /// Whether inactive concepts are left out.
    active_only: bool,
    /// Whether every concept carries its designations.
    include_designations: bool,
}

/// One page of a result, and what the reader is told about it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pager {
    /// The page the address asked for.
    page: Page,
    /// `expansion.total`, absent when the server declared none.
    total: Option<u32>,
    /// How many concepts this page actually holds.
    rows: u32,
}

/// The value every control on the form is seeded with, one field at a time.
///
/// Each seed is a `Memo` over its own parameter, so it notifies only when that
/// parameter's own value changed.
// NOTE: `prop:value` writes the property on every notification with no
// equality check (`tachys` 0.2.18 `html/property.rs`), so a seed read from the
// whole parameter set discards an edit the reader has not submitted.
#[derive(Clone, Copy)]
struct Seeds {
    /// The value set canonical.
    url: Memo<String>,
    /// The text filter.
    filter: Memo<String>,
    /// The BCP 47 tag displays are asked for in.
    display_language: Memo<String>,
    /// The page size, as the control holds it.
    count: Memo<String>,
    /// Whether inactive concepts are left out.
    active_only: Memo<bool>,
    /// Whether every concept carries its designations.
    include_designations: Memo<bool>,
}

impl RunnerParams {
    /// Reads the parameters out of the address.
    ///
    /// `query` looks one parameter up, which is what a `ParamsMap` does and
    /// what a test can do without a browser. An address naming no `count`
    /// takes the reader's stored page size; an empty one asks for the whole
    /// selection in a single answer; one the viewer cannot use falls back to
    /// the stored size and is kept, so the screen says it was not used rather
    /// than turning a typing mistake into an unpaged request.
    fn read(query: &dyn Fn(&str) -> Option<String>, stored_count: u32) -> Self {
        let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
        let flag = |name: &str| query(name).is_some_and(|value| value.trim() == "true");
        let stored = stored_count.clamp(1, MAX_COUNT);
        let (count, refused_count) = match query(COUNT_PARAMETER) {
            None => (Some(stored), None),
            Some(typed) if typed.trim().is_empty() => (None, None),
            Some(typed) => match parse_page_size(&typed) {
                Some(size) => (Some(size), None),
                None => (Some(stored), Some(typed)),
            },
        };
        Self {
            url: text(URL_PARAMETER),
            filter: text(FILTER_PARAMETER),
            display_language: text(DISPLAY_LANGUAGE_PARAMETER),
            count,
            refused_count,
            offset: query(OFFSET_PARAMETER)
                .and_then(|typed| typed.trim().parse::<u32>().ok())
                .unwrap_or_default(),
            active_only: flag(ACTIVE_ONLY_PARAMETER),
            include_designations: flag(INCLUDE_DESIGNATIONS_PARAMETER),
        }
    }

    /// The request these parameters make, or `None` when they name no value set.
    fn request(&self) -> Option<ExpandRequest> {
        if self.url.is_empty() {
            return None;
        }
        Some(ExpandRequest {
            url: self.url.clone(),
            filter: (!self.filter.is_empty()).then(|| self.filter.clone()),
            count: self.count,
            offset: (self.offset > 0).then_some(self.offset),
            display_language: (!self.display_language.is_empty())
                .then(|| self.display_language.clone()),
            active_only: self.active_only.then_some(true),
            include_designations: self.include_designations.then_some(true),
        })
    }

    /// The page on screen, or `None` for an unpaged run.
    ///
    /// `answered_offset` is `expansion.offset`, the offset the server says it
    /// applied (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). It
    /// wins over the address, because the transition boundary keeps the
    /// previous rows while the next page loads and a summary that follows the
    /// address would describe rows that are not on screen yet.
    fn page(&self, answered_offset: Option<u32>) -> Option<Page> {
        let count = self.count?;
        Some(Page::at(answered_offset.unwrap_or(self.offset), count))
    }

    /// These parameters, moved onto `page`.
    fn on(&self, page: Page) -> Self {
        Self {
            count: Some(page.count()),
            refused_count: None,
            offset: page.offset(),
            ..self.clone()
        }
    }

    /// The viewer address these parameters are, for a link or a navigation.
    ///
    /// `count` is written even when it is empty, so a run the reader asked to
    /// go unpaged stays unpaged when the address is shared or revisited.
    // NOTE: every value stays in the query, which a click navigation carries
    // through untouched while it unescapes the path a second time
    // (`leptos_router` 0.8.15 `src/location/mod.rs`).
    fn address(&self, version: FhirVersion) -> String {
        let mut url = base_url()
            .segment(RUNNER_PATH)
            .query(VERSION_PARAM, version.segment());
        if self.url.is_empty() {
            return url.render("");
        }
        url = url.query(URL_PARAMETER, &self.url);
        if !self.filter.is_empty() {
            url = url.query(FILTER_PARAMETER, &self.filter);
        }
        url = url.query(
            COUNT_PARAMETER,
            &match (&self.refused_count, self.count) {
                (Some(refused), _) => refused.clone(),
                (None, Some(count)) => count.to_string(),
                (None, None) => String::new(),
            },
        );
        if self.offset > 0 {
            url = url.query(OFFSET_PARAMETER, &self.offset.to_string());
        }
        if !self.display_language.is_empty() {
            url = url.query(DISPLAY_LANGUAGE_PARAMETER, &self.display_language);
        }
        if self.active_only {
            url = url.query(ACTIVE_ONLY_PARAMETER, "true");
        }
        if self.include_designations {
            url = url.query(INCLUDE_DESIGNATIONS_PARAMETER, "true");
        }
        url.render("")
    }
}

impl Seeds {
    /// The seeds these parameters give, one memo per control.
    fn of(params: Signal<RunnerParams>) -> Self {
        Self {
            url: Memo::new(move |_| params.with(|params| params.url.clone())),
            filter: Memo::new(move |_| params.with(|params| params.filter.clone())),
            display_language: Memo::new(move |_| {
                params.with(|params| params.display_language.clone())
            }),
            count: Memo::new(move |_| {
                params.with(|params| {
                    params
                        .count
                        .map(|count| count.to_string())
                        .unwrap_or_default()
                })
            }),
            active_only: Memo::new(move |_| params.with(|params| params.active_only)),
            include_designations: Memo::new(move |_| {
                params.with(|params| params.include_designations)
            }),
        }
    }
}

impl Pager {
    /// The first page, or `None` when this page is already it.
    fn first(self) -> Option<Page> {
        (self.page.offset() > 0).then(|| Page::first(self.page.count()))
    }

    /// The previous page, or `None` when this page starts the result.
    fn previous(self) -> Option<Page> {
        self.page.previous()
    }

    /// The next page, or `None` when there is nothing after this one.
    ///
    /// A server that declares no `total` says nothing about what follows, so a
    /// full page is the only evidence that more may exist and a short one ends
    /// the walk.
    fn next(self) -> Option<Page> {
        match self.total {
            Some(total) => self.page.next(total),
            None => (self.rows >= self.page.count()).then(|| {
                Page::at(
                    self.page.offset().saturating_add(self.page.count()),
                    self.page.count(),
                )
            }),
        }
    }

    /// The last page, or `None` when the end is not known or already here.
    fn last(self) -> Option<Page> {
        let last = self.page.last(self.total?);
        (last.offset() > self.page.offset()).then_some(last)
    }

    /// Which rows of the whole selection this page holds.
    fn summary(self) -> String {
        if self.rows == 0 {
            return match self.total {
                Some(total) => tr_with(
                    Msg::NoConceptsPageSelection,
                    &[("total", (total).to_string().into())],
                ),
                None => tr(Msg::NoConceptsPage).clone(),
            };
        }
        let first = self.page.offset().saturating_add(1);
        let last = self.page.offset().saturating_add(self.rows);
        match self.total {
            Some(total) => tr_with(
                Msg::ConceptsToOf,
                &[
                    ("first", (first).to_string().into()),
                    ("last", (last).to_string().into()),
                    ("total", (total).to_string().into()),
                ],
            ),
            None => tr_with(
                Msg::ConceptsServerDeclaredNoTotal,
                &[
                    ("first", (first).to_string().into()),
                    ("last", (last).to_string().into()),
                ],
            ),
        }
    }

    /// Where this page sits in the walk.
    fn position(self) -> String {
        match self.total {
            Some(total) => tr_with(
                Msg::PageOf2,
                &[
                    ("page", (self.page.number()).to_string().into()),
                    ("pages", (self.page.total_pages(total)).to_string().into()),
                ],
            ),
            None => tr_with(
                Msg::Page,
                &[("page", (self.page.number()).to_string().into())],
            ),
        }
    }
}

/// The parameters, as a form that navigates rather than reloading the page.
///
/// The router installs no `submit` listener, so the submit is handled here and
/// turned into a navigation. Each control is seeded from a memo over its own
/// parameter, so a back navigation refills it while a navigation that moves
/// another parameter leaves it as the reader left it, and it is read back from
/// the document when they submit.
fn form_section(
    params: Signal<RunnerParams>,
    version: Signal<FhirVersion>,
    value_sets: Memo<Vec<Choice>>,
) -> AnyView {
    let seeds = Seeds::of(params);
    let canonical: NodeRef<Input> = NodeRef::new();
    let filter: NodeRef<Input> = NodeRef::new();
    let language: NodeRef<Input> = NodeRef::new();
    let count: NodeRef<Input> = NodeRef::new();
    let active_only: NodeRef<Input> = NodeRef::new();
    let designations: NodeRef<Input> = NodeRef::new();

    let navigate = use_navigate();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let (page_size, refused_count) = typed_page_size(count);
        let typed = RunnerParams {
            url: typed_value(canonical),
            filter: typed_value(filter),
            display_language: typed_value(language),
            count: page_size,
            refused_count,
            // A changed parameter selects a different result, so the walk
            // starts again at its first page rather than at an offset into
            // something else.
            offset: 0,
            active_only: typed_flag(active_only),
            include_designations: typed_flag(designations),
        };
        // NOTE: the router resolves a navigation against its base, so an
        // address that already carries the base is passed unresolved
        // (`leptos_router` 0.8.15 `matching/resolve_path.rs`).
        navigate(
            &typed.address(version.get()),
            NavigateOptions {
                resolve: false,
                ..NavigateOptions::default()
            },
        );
    };

    let _help = ferrohealth_ui::widget::help::provide_help();

    let form = view! {
        <form class="fui-form" on:submit=submit>
            {value_set_group(canonical, seeds, value_sets)}
            {selection_group(filter, language, active_only, seeds)}
            {answer_group(count, designations, seeds, params)}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::EXPAND, "16")}
                    {tr(Msg::RunTheExpansion)}
                </button>
                {help_switch()}
            </div>
        </form>
    };
    card(tr(Msg::Parameters), form).into_any()
}

/// Which value set the run expands.
fn value_set_group(
    canonical: NodeRef<Input>,
    seeds: Seeds,
    value_sets: Memo<Vec<Choice>>,
) -> AnyView {
    let picker = view! {
        <div>
            {picked_field(
                Field {
                    id: "expand-url",
                    name: "url",
                    label: tr(Msg::ValueSet),
                    hint: tr(Msg::UrlParameterExpandListWhatRoot),
                },
                canonical,
                seeds.url,
                value_sets,
                canonical_words(),
            )}
        </div>
    }
    .into_any();
    group(tr(Msg::TheValueSet), vec![picker])
}

/// What the run selects out of it.
fn selection_group(
    filter: NodeRef<Input>,
    language: NodeRef<Input>,
    active_only: NodeRef<Input>,
    seeds: Seeds,
) -> AnyView {
    let controls = view! {
        <div class="fui-columns">
            {text_field(
                Field {
                    id: "expand-filter",
                    name: "filter",
                    label: tr(Msg::Filter),
                    hint: tr(Msg::TextServerMatchesAgainstDesignationsHolds),
                },
                filter,
                seeds.filter,
            )}
            {text_field_showing(
                Field {
                    id: "expand-display-language",
                    name: "displayLanguage",
                    label: tr(Msg::DisplayLanguage),
                    hint: tr(Msg::Bcp47TagLeftEmptyServer),
                },
                language,
                seeds.display_language,
                Some(language_example()),
            )}
        </div>
        <div>
            {check_field(
                Field {
                    id: "expand-active-only",
                    name: "activeOnly",
                    label: tr(Msg::ActiveConceptsOnly),
                    hint: tr(Msg::SendsActiveonlyTrueWhichAsksServer),
                },
                active_only,
                seeds.active_only,
            )}
        </div>
    }
    .into_any();
    group(tr(Msg::Selection), vec![controls])
}

/// How the answer is paged and written.
fn answer_group(
    count: NodeRef<Input>,
    designations: NodeRef<Input>,
    seeds: Seeds,
    params: Signal<RunnerParams>,
) -> AnyView {
    // A page size the address carried and the runner could not use is not
    // help, so it is a note beside the control rather than its hint.
    let refused = Memo::new(move |_| {
        params.with(|params| match (&params.refused_count, params.count) {
            (Some(refused), Some(used)) => tr_with(
                Msg::NotUsedAddressAskedPageSize,
                &[
                    ("refused", (refused).clone().into()),
                    ("used", (used).to_string().into()),
                ],
            ),
            _ => String::new(),
        })
    });
    let controls = view! {
        <div class="fui-columns">
            {number_field(
                Field {
                    id: "expand-count",
                    name: "count",
                    label: tr(Msg::PageSize),
                    hint: tr(Msg::CountParameterLeftEmptyRunAsks),
                },
                count,
                seeds.count,
                MAX_COUNT,
            )}
            {check_field(
                Field {
                    id: "expand-include-designations",
                    name: "includeDesignations",
                    label: tr(Msg::IncludeDesignations),
                    hint: tr(Msg::SendsIncludedesignationsTrueWhichAsksEvery),
                },
                designations,
                seeds.include_designations,
            )}
        </div>
    }
    .into_any();
    let refusal = view! {
        <Show when=move || !refused.get().is_empty() fallback=|| ()>
            {note(refused)}
        </Show>
    }
    .into_any();
    group(tr(Msg::Answer), vec![controls, refusal])
}

/// The answer: the page, what it leaves out, and the request that fetched it.
fn result_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<RunnerParams>,
    request: Signal<Option<ExpandRequest>>,
) -> AnyView {
    let read_client = client.clone();
    let expansion = Resource::new(
        move || (version.get(), request.get()),
        move |(version, request)| {
            let client = read_client.clone();
            async move {
                match request {
                    Some(request) => Some(client.expand(version, &request).await),
                    None => None,
                }
            }
        },
    );

    // The live region is in the document before the read settles, which is
    // what lets a screen reader announce the count when it arrives.
    let announcement = move || {
        let params = params.get();
        expansion.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .and_then(ExpandedValueSet::expansion)
                .map(|expansion| count_sentence(&expansion, &params))
                .unwrap_or_default()
        })
    };

    view! {
        <section class="fui-card fui-panel" aria-labelledby="expansion-heading">
            <div class="fui-panel-head">
                <h2 id="expansion-heading" class="fui-card-title">
                    {tr(Msg::TheExpansion)}
                </h2>
                <p aria-live="polite" class="fui-muted fui-small">
                    <Transition fallback=|| ()>{announcement}</Transition>
                </p>
            </div>
            <Show when=move || request.with(Option::is_none) fallback=|| ()>
                {invitation()}
            </Show>
            <Show when=move || request.with(Option::is_some) fallback=|| ()>
                <Reading label=&{tr(Msg::RunningTheExpansion)}>
                    {move || {
                        let params = params.get();
                        let version = version.get();
                        expansion
                            .with(|answered| {
                                answered
                                    .as_ref()
                                    .map(|answer| match answer {
                                        None => ().into_any(),
                                        Some(Ok(value)) => expansion_view(value, &params, version),
                                        Some(Err(error)) => refusal_view(error),
                                    })
                            })
                    }}
                </Reading>
            </Show>
        </section>
    }
    .into_any()
}

/// What the screen says before a canonical has been typed.
fn invitation() -> AnyView {
    panel_body(empty_quiet(
        icon::EXPAND,
        tr(Msg::NoExpansionYet),
        tr(Msg::NameValueSetAboveRunCanonical),
    ))
    .into_any()
}

/// The page, the mark saying what it leaves out, and the echoed parameters.
fn expansion_view(
    value: &ExpandedValueSet,
    params: &RunnerParams,
    version: FhirVersion,
) -> AnyView {
    let Some(expansion) = value.expansion() else {
        return panel_body(empty_quiet(
            icon::EXPAND,
            tr(Msg::NoExpansion),
            tr(Msg::ServerAnsweredValuesetCarryingNoExpansion),
        ))
        .into_any();
    };
    let unclosed = expansion
        .unclosed
        .as_ref()
        .map_or_else(|| ().into_any(), unclosed_view);
    let table = concepts_table(&expansion.concepts);
    let pager = page_of(&expansion, params).map_or_else(
        || ().into_any(),
        |page| {
            pager_view(
                Pager {
                    page,
                    total: expansion.total,
                    rows: expansion.listed(),
                },
                params,
                version,
            )
        },
    );
    let echoed = parameters_view(&expansion.parameters);
    view! {
        {unclosed}
        {table}
        <div class="fui-panel-body fui-stack">{pager} {echoed}</div>
    }
    .into_any()
}

/// The statement that the value set admits codes this list does not carry.
fn unclosed_view(unclosed: &Unclosed) -> AnyView {
    let reasons: Vec<AnyView> = unclosed
        .reasons
        .iter()
        .map(|reason| view! { <li>{reason.clone()}</li> }.into_any())
        .collect();
    let stated = if reasons.is_empty() {
        view! { <p>{tr(Msg::ServerStatedNoReason)}</p> }.into_any()
    } else {
        bullet_list(reasons)
    };
    panel_body(notice(
        Tone::Warn,
        Announce::Status,
        None,
        view! {
            <p>
                <strong>{tr(Msg::UnclosedExpansion)}</strong>
                {tr(Msg::ValueSetAdmitsCodesExpansionDoes)}
            </p>
            {stated}
        },
    ))
    .into_any()
}

/// The concepts of this page, as a table.
///
/// The rows are a whole-page replacement with no per-row state, so they are a
/// plain `Vec`, which rebuilds every position when the page changes. A `<For>`
/// key it retained would be moved rather than re-rendered, and a concept whose
/// code repeats on the next page would keep the row it had.
fn concepts_table(rows: &[ConceptRow]) -> AnyView {
    // No page flags every concept, so the column is drawn only when one does.
    let columns = filled_columns(
        &concept_columns(),
        rows,
        &["code", "display", "system"],
        |row, _| !flags(row).is_empty(),
    );
    let flagged = columns.iter().any(|column| column.key == "flags");
    let body = if rows.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::EXPAND,
                tr(Msg::NoConceptPage),
                tr(Msg::PagePastEndSelectionEmptySo),
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            rows.iter()
                .map(|row| concept_row(row, flagged))
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: &tr(Msg::ConceptsPage),
        columns: &columns,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// One concept, with its designations under the display it was listed by,
/// and its flags when the table draws that column.
fn concept_row(row: &ConceptRow, flagged: bool) -> AnyView {
    let nesting = (row.depth > 0).then(|| {
        tr_with(
            Msg::NestedAtLevel,
            &[("row", (row.depth.saturating_add(1)).to_string().into())],
        )
    });
    let system = match (row.system.as_str(), &row.version) {
        ("", _) => not_declared().clone(),
        (system, Some(version)) => format!("{system}|{version}"),
        (system, None) => system.to_owned(),
    };
    let flags = flags(row);
    let flags = (!flags.is_empty()).then(|| badge(Tone::Neutral, &flags));
    view! {
        <tr>
            <td class=format!("fui-mono sm:whitespace-nowrap {}", styles::depth(row.depth))>
                <span class="fui-sr-only">{nesting}</span>
                {row.code.clone()}
            </td>
            <td data-label=tr(Msg::Display)>
                <div>
                    {row.display.clone().unwrap_or_else(|| not_declared().clone())}
                    {designation_list(&row.designations)}
                </div>
            </td>
            <td data-label=tr(Msg::System) class="fui-mono fui-small fui-muted sm:whitespace-nowrap">
                {system}
            </td>
            {flagged.then(|| view! { <td data-label=tr(Msg::Flags)>{flags}</td> })}
        </tr>
    }
    .into_any()
}

/// The designations a concept was listed with.
fn designation_list(designations: &[DesignationRow]) -> AnyView {
    if designations.is_empty() {
        return ().into_any();
    }
    let lines: Vec<AnyView> = designations
        .iter()
        .map(|designation| designation_line(designation).into_any())
        .collect();
    view! { <div class="fui-small fui-muted">{bullet_list(lines)}</div> }.into_any()
}

/// One designation, as the term and what the server said about it.
fn designation_line(designation: &DesignationRow) -> String {
    let about: Vec<String> = designation
        .language
        .iter()
        .chain(designation.usage.iter())
        .cloned()
        .collect();
    if about.is_empty() {
        designation.value.clone()
    } else {
        format!("{} ({})", designation.value, about.join(", "))
    }
}

/// The flags the server set on a concept, as words rather than as a colour.
fn flags(row: &ConceptRow) -> String {
    let mut set: Vec<&str> = Vec::new();
    if row.inactive {
        set.push("inactive");
    }
    if row.abstract_concept {
        set.push("abstract");
    }
    set.join(", ")
}

/// The page controls, which are the address of another page.
///
/// Each control is a link, so a page is shareable, the browser walks the run
/// with its back button, and the router turns the click into a navigation of
/// its own.
fn pager_view(pager: Pager, params: &RunnerParams, version: FhirVersion) -> AnyView {
    let size = pager.page.count();
    let paging = Paging {
        page: pager.page.number(),
        size,
    };
    // NOTE: no specification governs this: our own design; the steps are this
    // runner's own walk over offsets, so each leads where `Pager` says.
    let step = |each: Step, _: Option<u32>| -> AnyView {
        let target = match each {
            Step::First => pager.first(),
            Step::Previous => pager.previous(),
            Step::Next => pager.next(),
            Step::Last => pager.last(),
        };
        match target {
            Some(page) => view! {
                <a
                    href=params.on(page).address(version)
                    class="fui-button fui-button-secondary fui-button-small"
                >
                    {step_label(each, true)}
                </a>
            }
            .into_any(),
            None => view! {
                <span class="fui-button fui-button-secondary fui-button-small" aria-disabled="true">
                    {step_label(each, false)}
                </span>
            }
            .into_any(),
        }
    };
    let no_sizes = |_: u32, _: bool| ().into_any();
    let listing = Listing::new(tr(Msg::ExpansionPages))
        .noun(Noun::new(message(Msg::NounConcepts)))
        .sizes(&[]);
    match pager.total {
        Some(total) => ferrohealth_ui::table::view::pager(
            listing,
            Window::over(total, paging),
            &step,
            &no_sizes,
        ),
        None => open_pager(
            listing,
            OpenWindow::of(paging, pager.rows),
            &step,
            &no_sizes,
        ),
    }
}

/// The parameters the server says it applied, echoed back verbatim.
fn parameters_view(lines: &[ParameterLine]) -> AnyView {
    if lines.is_empty() {
        return view! { <p class="fui-small fui-muted">{tr(Msg::ServerEchoedNoParametersExpansion)}</p> }
            .into_any();
    }
    let rows: Vec<AnyView> = lines
        .iter()
        .map(|line| {
            view! {
                <dt class="fui-mono">{line.name.clone()}</dt>
                <dd class="break-all">{crate::components::stated(line.value.clone())}</dd>
            }
            .into_any()
        })
        .collect();
    view! {
        <details class="fui-disclosure">
            <summary>{tr(Msg::ParametersServerSaysApplied)}</summary>
            <dl class="fui-facts">{rows}</dl>
        </details>
    }
    .into_any()
}

/// A refusal, in the server's own words, with what to do about a costly one.
fn refusal_view(error: &FhirError) -> AnyView {
    let costly = error
        .outcome()
        .is_some_and(|outcome| outcome.carries_code(TOO_COSTLY));
    let advice = if costly {
        view! {
            <p class="mt-default text-body">
                {tr(Msg::ServerRefusedExpandSelectionLargeOne)}
            </p>
        }
        .into_any()
    } else {
        ().into_any()
    };
    let error = error.clone();
    view! {
        <div class="mt-default">
            <Failure error=Signal::stored(error) />
            {advice}
        </div>
    }
    .into_any()
}

/// The page this answer is a page of, or `None` when it is not one.
///
/// A nested answer carries no page window, so nothing on the screen claims
/// one: no summary counts towards it and no control walks it.
// NOTE: paging "only applies to flat expansions"
// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, the `count`
// parameter), so the offset the address asked for describes no answer here.
fn page_of(expansion: &Expansion, params: &RunnerParams) -> Option<Page> {
    if expansion.hierarchical() {
        return None;
    }
    params.page(expansion.offset)
}

/// How much of the selection this answer holds, as a sentence.
///
/// Every number counts the rows the table renders, so the sentence, the table,
/// and the live region describe one set.
fn count_sentence(expansion: &Expansion, params: &RunnerParams) -> String {
    let rows = expansion.listed();
    if let Some(page) = page_of(expansion, params) {
        let pager = Pager {
            page,
            total: expansion.total,
            rows,
        };
        return format!("{} {}.", pager.summary(), pager.position());
    }
    let counted = match expansion.total {
        Some(total) if total != rows => tr_with(
            Msg::ConceptsSelection,
            &[
                ("rows", (rows).to_string().into()),
                ("total", (total).to_string().into()),
            ],
        ),
        Some(_) | None => tr_with(Msg::Concepts2, &[("rows", (rows).to_string().into())]),
    };
    if expansion.hierarchical() && params.count.is_some() {
        return tr_with(
            Msg::AnswerNestsExpandPagesFlatExpansions,
            &[("counted", (counted).clone().into())],
        );
    }
    counted
}

/// The page size a control holds now, and the text it holds instead.
///
/// A typed size the viewer cannot ask for travels into the address as it was
/// typed, so the run is refused there and the screen says which value was not
/// used. Dropping it here would turn a typing mistake into an unpaged request.
fn typed_page_size(node: NodeRef<Input>) -> (Option<u32>, Option<String>) {
    let typed = typed_value(node);
    if typed.is_empty() {
        return (None, None);
    }
    match parse_page_size(&typed) {
        Some(size) => (Some(size), None),
        None => (None, Some(typed)),
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

/// Whether a checkbox is ticked now.
fn typed_flag(node: NodeRef<Input>) -> bool {
    node.get().is_some_and(|input| input.checked())
}

#[cfg(test)]
mod tests {
    use crate::routes::UI_BASE;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering;

    use super::*;

    /// The stored page size a reader who has changed nothing carries.
    const STORED: u32 = 50;

    /// A stand-in for the address, which reads the same way a `ParamsMap` does.
    fn map<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + use<'a> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn pager(offset: u32, count: u32, total: Option<u32>, rows: u32) -> Pager {
        Pager {
            page: Page::at(offset, count),
            total,
            rows,
        }
    }

    #[test]
    fn an_address_with_no_count_takes_the_stored_page_size() {
        let params = RunnerParams::read(&map(&[("url", "https://terminology.example/vs")]), STORED);
        assert_eq!(params.count, Some(STORED));
        assert_eq!(params.offset, 0, "a run starts at the first page");
    }

    #[test]
    fn an_empty_count_asks_for_no_page_at_all() {
        let params = RunnerParams::read(
            &map(&[("url", "https://terminology.example/vs"), ("count", "")]),
            STORED,
        );
        assert_eq!(
            params.count, None,
            "the reader asked for the whole selection, and the server answers or refuses"
        );
        assert_eq!(
            params.page(None),
            None,
            "an unpaged run has no page to walk"
        );
    }

    #[test]
    fn a_count_the_viewer_cannot_use_is_kept_and_said_rather_than_obeyed() {
        for typed in ["0", "1001", "twenty", "-5"] {
            let params = RunnerParams::read(&map(&[("count", typed)]), STORED);
            assert_eq!(
                (params.count, params.refused_count.as_deref()),
                (Some(STORED), Some(typed)),
                "`{typed}` names no page size the viewer asks for, so the run keeps paging and the screen says what was not used"
            );
        }
    }

    #[test]
    fn a_page_the_reader_walks_to_leaves_no_refusal_behind() {
        let params = RunnerParams::read(&map(&[("url", "x"), ("count", "twenty")]), STORED);
        let walked = params.on(Page::at(50, 25));
        assert_eq!(
            (walked.count, walked.refused_count),
            (Some(25), None),
            "the address the control points at carries a page size the viewer asked for"
        );
    }

    #[test]
    fn the_page_on_screen_is_the_one_the_server_answered() {
        let params = RunnerParams::read(
            &map(&[("url", "x"), ("count", "20"), ("offset", "40")]),
            STORED,
        );
        assert_eq!(
            params.page(Some(20)).map(Page::offset),
            Some(20),
            "the rows on screen are the answered page, which the address has already moved past"
        );
        assert_eq!(
            params.page(None).map(Page::offset),
            Some(40),
            "a server that declares no offset leaves the address as the only evidence"
        );
    }

    #[test]
    fn every_parameter_is_read_from_the_address() {
        let params = RunnerParams::read(
            &map(&[
                ("url", "https://terminology.example/vs"),
                ("filter", " fever "),
                ("count", "20"),
                ("offset", "40"),
                ("displayLanguage", "nl-NL"),
                ("activeOnly", "true"),
                ("includeDesignations", "true"),
            ]),
            STORED,
        );
        assert_eq!(
            params,
            RunnerParams {
                url: "https://terminology.example/vs".to_owned(),
                filter: "fever".to_owned(),
                display_language: "nl-NL".to_owned(),
                count: Some(20),
                refused_count: None,
                offset: 40,
                active_only: true,
                include_designations: true,
            },
            "the address is the whole state of the run"
        );
    }

    #[test]
    fn a_flag_is_set_only_by_the_value_that_names_it() {
        for typed in ["false", "1", "yes", ""] {
            assert!(
                !RunnerParams::read(&map(&[("activeOnly", typed)]), STORED).active_only,
                "`{typed}` does not turn a flag on"
            );
        }
    }

    #[test]
    fn an_unreadable_offset_starts_at_the_beginning() {
        assert_eq!(
            RunnerParams::read(&map(&[("offset", "halfway")]), STORED).offset,
            0,
            "an address a reader typed cannot wedge the walk"
        );
    }

    #[test]
    fn an_address_naming_no_value_set_makes_no_request() {
        assert_eq!(
            RunnerParams::read(&map(&[("filter", "fever")]), STORED).request(),
            None,
            "the screen invites a canonical rather than asking the server for nothing"
        );
    }

    #[test]
    fn only_the_parameters_the_reader_set_reach_the_request() {
        let params = RunnerParams::read(
            &map(&[("url", "https://terminology.example/vs"), ("count", "20")]),
            STORED,
        );
        assert_eq!(
            params.request(),
            Some(ExpandRequest {
                url: "https://terminology.example/vs".to_owned(),
                count: Some(20),
                ..ExpandRequest::default()
            }),
            "an unticked box sends nothing, so the server's own default applies"
        );
    }

    #[test]
    fn an_address_round_trips_through_the_reader() {
        let params = RunnerParams {
            url: "https://terminology.example/vs?all=true".to_owned(),
            filter: "a&b".to_owned(),
            display_language: "cy".to_owned(),
            count: Some(25),
            refused_count: None,
            offset: 50,
            active_only: true,
            include_designations: true,
        };
        let address = params.address(FhirVersion::R5);
        assert_eq!(
            address,
            format!(
                "{UI_BASE}/expand\
                 ?fhir=r5&url=https%3A%2F%2Fterminology.example%2Fvs%3Fall%3Dtrue\
                 &filter=a%26b&count=25&offset=50&displayLanguage=cy&activeOnly=true\
                 &includeDesignations=true"
            ),
            "every value the reader typed is encoded into the parameter it belongs to"
        );
        // The router percent-decodes a query on read, so the parameters come
        // back as they were typed.
        assert_eq!(
            RunnerParams::read(
                &map(&[
                    ("url", "https://terminology.example/vs?all=true"),
                    ("filter", "a&b"),
                    ("count", "25"),
                    ("offset", "50"),
                    ("displayLanguage", "cy"),
                    ("activeOnly", "true"),
                    ("includeDesignations", "true"),
                ]),
                STORED,
            ),
            params,
            "the address the runner writes is the address it reads"
        );
    }

    #[test]
    fn an_unpaged_run_stays_unpaged_when_its_address_is_shared() {
        let params = RunnerParams {
            url: "https://terminology.example/vs".to_owned(),
            count: None,
            ..RunnerParams::default()
        };
        assert_eq!(
            params.address(FhirVersion::R4B),
            format!("{UI_BASE}/expand?fhir=r4b&url=https%3A%2F%2Fterminology.example%2Fvs&count="),
            "the empty count is written, or a revisit would page a run that asked not to be"
        );
        assert_eq!(
            RunnerParams::read(&map(&[("url", "x"), ("count", "")]), STORED).count,
            None
        );
    }

    #[test]
    fn a_refused_page_size_reaches_the_address_and_is_reported_from_there() {
        let typed = RunnerParams {
            url: "https://terminology.example/vs".to_owned(),
            count: None,
            refused_count: Some("twenty".to_owned()),
            ..RunnerParams::default()
        };
        assert_eq!(
            typed.address(FhirVersion::R4B),
            format!(
                "{UI_BASE}/expand?fhir=r4b&url=https%3A%2F%2Fterminology.example%2Fvs&count=twenty"
            ),
            "the value the reader typed travels rather than being dropped on the way"
        );
        let read = RunnerParams::read(
            &map(&[
                ("url", "https://terminology.example/vs"),
                ("count", "twenty"),
            ]),
            STORED,
        );
        assert_eq!(
            (read.count, read.refused_count.as_deref()),
            (Some(STORED), Some("twenty")),
            "the run keeps paging, and the screen has what it needs to say why"
        );
    }

    #[test]
    fn an_address_with_no_value_set_carries_only_the_version() {
        assert_eq!(
            RunnerParams::default().address(FhirVersion::R4),
            format!("{UI_BASE}/expand?fhir=r4"),
            "an empty runner is a link worth sharing and nothing more"
        );
    }

    #[test]
    fn walking_a_page_keeps_every_other_parameter() {
        let params = RunnerParams {
            url: "https://terminology.example/vs".to_owned(),
            filter: "fever".to_owned(),
            count: Some(20),
            offset: 0,
            active_only: true,
            ..RunnerParams::default()
        };
        let walked = params.on(Page::at(20, 20));
        assert_eq!(walked.offset, 20);
        assert_eq!(
            walked.filter, "fever",
            "the page moved and nothing else did"
        );
        assert!(walked.active_only);
    }

    #[test]
    fn a_navigation_that_moves_one_parameter_leaves_the_other_controls_alone() {
        let owner = Owner::new();
        owner.set();
        let address = RwSignal::new(RunnerParams {
            url: "https://terminology.example/vs".to_owned(),
            count: Some(20),
            ..RunnerParams::default()
        });
        let seeds = Seeds::of(address.into());
        // Counts how often the seed notifies whatever writes it into the
        // control, which is once per changed value and never on a notification
        // carrying the same one.
        let writes = Arc::new(AtomicU32::new(0));
        let written = {
            let writes = Arc::clone(&writes);
            Memo::new(move |_| {
                writes.fetch_add(1, Ordering::Relaxed);
                seeds.url.get()
            })
        };

        written.get();
        assert_eq!(
            writes.load(Ordering::Relaxed),
            1,
            "the control is seeded from the address it opened on"
        );

        address.update(|params| params.offset = 20);
        written.get();
        assert_eq!(
            writes.load(Ordering::Relaxed),
            1,
            "walking to the next page moves the offset alone, so an unsubmitted edit in the canonical control survives it"
        );

        address.update(|params| params.url = "https://terminology.example/other".to_owned());
        written.get();
        assert_eq!(
            writes.load(Ordering::Relaxed),
            2,
            "a back navigation onto another canonical does refill the control"
        );
    }

    #[test]
    fn the_walk_stops_at_both_ends_of_a_counted_result() {
        let first = pager(0, 20, Some(45), 20);
        assert_eq!(first.first(), None, "this page is the first one");
        assert_eq!(first.previous(), None);
        assert_eq!(first.next().map(Page::offset), Some(20));
        assert_eq!(first.last().map(Page::offset), Some(40));

        let last = pager(40, 20, Some(45), 5);
        assert_eq!(last.next(), None, "45 rows end on the third page of 20");
        assert_eq!(last.last(), None, "this page is the last one");
        assert_eq!(last.previous().map(Page::offset), Some(20));
        assert_eq!(last.first().map(Page::offset), Some(0));
    }

    #[test]
    fn a_server_that_declares_no_total_walks_while_the_pages_are_full() {
        assert_eq!(
            pager(0, 20, None, 20).next().map(Page::offset),
            Some(20),
            "a full page may be followed by another"
        );
        assert_eq!(
            pager(20, 20, None, 7).next(),
            None,
            "a short page is the end of what there is"
        );
        assert_eq!(
            pager(20, 20, None, 20).last(),
            None,
            "the end is unknown, so no control claims to jump to it"
        );
    }

    #[test]
    fn the_summary_names_the_rows_of_the_whole_selection() {
        assert_eq!(
            pager(20, 20, Some(45), 20).summary(),
            "Concepts 21 to 40 of 45."
        );
        assert_eq!(
            pager(20, 20, None, 20).summary(),
            "Concepts 21 to 40. The server declared no total.",
            "an absent total is stated rather than invented"
        );
        assert_eq!(
            pager(60, 20, Some(45), 0).summary(),
            "No concepts on this page, of 45 in the selection.",
            "a page past the end says so"
        );
    }

    #[test]
    fn the_position_is_a_word_rather_than_a_highlight() {
        assert_eq!(pager(20, 20, Some(45), 20).position(), "Page 2 of 3");
        assert_eq!(
            pager(20, 20, None, 20).position(),
            "Page 2",
            "without a total there is no last page to count towards"
        );
    }

    #[test]
    fn the_announced_sentence_covers_a_paged_and_an_unpaged_run() {
        let expansion = Expansion {
            total: Some(45),
            concepts: vec![ConceptRow::default(); 20],
            ..Expansion::default()
        };
        let paged = RunnerParams {
            count: Some(20),
            offset: 20,
            ..RunnerParams::default()
        };
        assert_eq!(
            count_sentence(&expansion, &paged),
            "Concepts 21 to 40 of 45. Page 2 of 3."
        );
        assert_eq!(
            count_sentence(&expansion, &RunnerParams::default()),
            "20 concepts, of 45 in the selection.",
            "an unpaged answer that is short of the total says both numbers"
        );
    }

    #[test]
    fn a_nested_answer_counts_the_rows_the_table_renders_and_claims_no_page() {
        let nested = |depth: u32| ConceptRow {
            depth,
            ..ConceptRow::default()
        };
        let expansion = Expansion {
            total: Some(5),
            concepts: vec![nested(0), nested(1), nested(2), nested(2), nested(1)],
            ..Expansion::default()
        };
        let params = RunnerParams {
            count: Some(50),
            ..RunnerParams::default()
        };
        assert_eq!(
            page_of(&expansion, &params),
            None,
            "the server ignores paging for a hierarchy, so no control walks one"
        );
        assert_eq!(
            count_sentence(&expansion, &params),
            "5 concepts. The answer nests, and $expand pages flat expansions only, so this run was not paged.",
            "the sentence counts the five rows the table draws, and says why the page size did nothing"
        );
    }

    #[test]
    fn a_flat_answer_is_still_described_as_the_page_it_is() {
        let expansion = Expansion {
            total: Some(45),
            offset: Some(20),
            concepts: vec![ConceptRow::default(); 20],
            ..Expansion::default()
        };
        let params = RunnerParams {
            count: Some(20),
            offset: 20,
            ..RunnerParams::default()
        };
        assert_eq!(page_of(&expansion, &params), Some(Page::at(20, 20)));
        assert_eq!(
            count_sentence(&expansion, &params),
            "Concepts 21 to 40 of 45. Page 2 of 3."
        );
    }

    #[test]
    fn a_designation_states_what_the_server_said_about_it() {
        assert_eq!(
            designation_line(&DesignationRow {
                language: Some("nl".to_owned()),
                usage: Some("Preferred".to_owned()),
                value: "Koorts".to_owned(),
            }),
            "Koorts (nl, Preferred)"
        );
        assert_eq!(
            designation_line(&DesignationRow {
                value: "Fever".to_owned(),
                ..DesignationRow::default()
            }),
            "Fever",
            "a bare term is rendered bare rather than with empty brackets"
        );
    }

    #[test]
    fn the_flags_are_words() {
        assert_eq!(
            flags(&ConceptRow {
                inactive: true,
                abstract_concept: true,
                ..ConceptRow::default()
            }),
            "inactive, abstract",
            "no meaning on this screen is carried by colour alone"
        );
        assert_eq!(flags(&ConceptRow::default()), "");
    }
}
