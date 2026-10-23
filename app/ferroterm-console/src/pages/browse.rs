// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept browser: search one code system, read a concept, walk its tree.
//!
//! What the screen offers is what the served version declares. The hierarchy
//! pane appears for a version whose capability statement declares a filter
//! operator that selects direct children, and a version that declares none
//! gets the flat searchable list with the reason stated. No code system is
//! named anywhere below.
//!
//! Every section is gated by a `Memo<bool>` and erased with `.into_any()`
//! rather than wrapped in a `<Show>`, because the component is generic over
//! its children and each use of it monomorphizes the whole subtree
//! (`.claude/rules/leptos-ui.md` §1, the bundle bar).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use ferrohealth_ui::table::model::filled_columns;
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::form::{INPUT, MEDIUM, SELECT, field, help_id};
use ferrohealth_ui::view::layout::{Pane, divided_list, three_panes};
use ferrohealth_ui::view::surface::{
    Crumb, Header, card, cluster, definitions, page_heading, stack,
};
use ferrohealth_ui::widget::tree::{Tree, TreeNode, tree_view_collapsing};
use leptos::ev::Event;
use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::components::failure::Failure;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::components::shell::SelectedVersion;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::FhirClient;
use crate::fhir::concept::CODE_PARAMETER;
use crate::fhir::concept::ChildOf;
use crate::fhir::concept::Concept;
use crate::fhir::concept::ConceptQuery;
use crate::fhir::concept::Hierarchy;
use crate::fhir::concept::LookupRequest;
use crate::fhir::concept::chosen_version;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::ConceptRow;
use crate::fhir::expansion::DISPLAY_LANGUAGE_PARAMETER;
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::expansion::FILTER_PARAMETER;
use crate::fhir::named::Choice;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::terminology::VersionRow;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::offers::choices;
use crate::offers::published;
use crate::paging::MAX_COUNT;
use crate::routes::BROWSE_PATH;
use crate::routes::SYSTEM_PARAM;
use crate::routes::SYSTEM_VERSION_PARAM;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::routes::system_link;
use crate::settings::Settings;
use crate::tree::TreeConcept;
use crate::tree::TreeRow;
use crate::tree::decode_open;
use crate::tree::encode_open;
use crate::tree::rows;
use ferrohealth_ui::view::empty::{Busy, busy};

/// The address parameter carrying the concept the tree hangs from.
const ROOT_PARAM: &str = "root";

/// The address parameter carrying the open nodes of the tree.
const OPEN_PARAM: &str = "open";

/// Browses one code system: search, one concept, and its hierarchy.
///
/// Every parameter is read from the address rather than from a private signal,
/// so a concept is shareable by link and the back button walks the reading.
/// The parameters are a memo, because a resource refetches on a notification
/// rather than on a change.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn BrowsePage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let page_size = expect_context::<Settings>().page_size;
    let query = use_query_map();
    let params: Signal<BrowseParams> =
        Memo::new(move |_| query.with(|map| BrowseParams::read(&|name| map.get(name)))).into();
    let count: Memo<u32> = Memo::new(move |_| page_size.clamp(1, MAX_COUNT));
    let named: Memo<bool> = Memo::new(move |_| params.with(|params| !params.system.is_empty()));

    let capability_client = client.clone();
    let capabilities = LocalResource::new(move || {
        let client = capability_client.clone();
        let version = version.get();
        async move { client.terminology_capabilities(version).await }
    });
    let declared: Memo<Declared> = Memo::new(move |_| {
        let row = capabilities.with(|answered| {
            let document = answered.as_ref()?.as_ref().ok()?;
            params.with(|params| {
                let card = document.card(&params.system)?;
                chosen_version(&card, Some(&params.system_version))
            })
        });
        Declared {
            hierarchy: row.as_ref().map(Hierarchy::of).unwrap_or_default(),
            languages: row
                .as_ref()
                .map(|row| row.languages.clone())
                .unwrap_or_default(),
            row,
        }
    });

    let heading = view! {
        <Title text=tr(Msg::BrowseConcepts) />
        <div>
            {page_heading(
                Header {
                    title: tr(Msg::BrowseConcepts),
                    subtitle: Some(tr(Msg::SearchCodeSystemReadConceptWalk)),
                    crumbs: vec![Crumb::new(tr(Msg::CodeSystems), "/systems")],
                    mono: false,
                },
                (),
            )}
        </div>
    }
    .into_any();
    let picker = system_picker(
        version,
        params,
        choices(published(&client, version, CODE_SYSTEM)),
    );

    let system = system_section(version, params, capabilities, declared, named);
    let search = search_section(&client, version, params, count, named);
    let concept = concept_section(&client, version, params, declared, named);
    let tree = tree_section(&client, version, params, declared, count);

    // Three panes side by side above the large breakpoint, each scrolling its
    // own content, and one stack below it. The order is the order of the work:
    // search for a concept, walk what is under it, read the one you landed on.
    let settings = card(
        tr(Msg::WhatIsBrowsed),
        view! {
            {picker}
            {system}
        },
    );
    view! {
        {stack(
            view! {
                {heading}
                {settings}
                {three_panes(
                    Pane::new(tr(Msg::Search), search),
                    Pane::new(tr(Msg::BelowThisConcept), tree),
                    Pane::new(tr(Msg::TheConcept), concept),
                )}
            },
        )}
    }
}

/// What the served version declares, as every section below reads it.
///
/// The three facts travel in one memo rather than three, because each memo
/// type instantiates its own reactive machinery in the bundle.
#[derive(Clone, Debug, Default, PartialEq)]
struct Declared {
    /// The version of the system this screen is reading.
    row: Option<VersionRow>,
    /// What it declares about walking its hierarchy.
    hierarchy: Hierarchy,
    /// The designation languages it holds.
    languages: Vec<String>,
}

/// The parameters the address carries, which are the ones the screen sends.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BrowseParams {
    /// The code system canonical, empty when the address names none.
    system: String,
    /// The code system version, empty for the server's own default.
    system_version: String,
    /// The text the search sends as `filter`.
    term: String,
    /// The BCP 47 tag the operations send as `displayLanguage`.
    language: String,
    /// The concept the detail pane reads.
    code: String,
    /// The concept the tree hangs from, which the detail pane's code seeds.
    root: String,
    /// The codes whose children are showing.
    open: BTreeSet<String>,
}

impl BrowseParams {
    /// Reads the parameters out of the address.
    ///
    /// `query` looks one parameter up, which is what a `ParamsMap` does and
    /// what a test can do without a browser.
    fn read(query: &dyn Fn(&str) -> Option<String>) -> Self {
        let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
        let code = text(CODE_PARAMETER);
        let named = text(ROOT_PARAM);
        Self {
            system: text(SYSTEM_PARAM),
            system_version: text(SYSTEM_VERSION_PARAM),
            term: text(FILTER_PARAMETER),
            language: text(DISPLAY_LANGUAGE_PARAMETER),
            root: if named.is_empty() {
                code.clone()
            } else {
                named
            },
            code,
            open: decode_open(&text(OPEN_PARAM)),
        }
    }

    /// The viewer address these parameters are, for a link or a navigation.
    // NOTE: every value stays in the query, which a click navigation carries
    // through untouched while it unescapes the path a second time
    // (`leptos_router` 0.8.15 `src/location/mod.rs`).
    fn address(&self, version: FhirVersion) -> String {
        let mut url = base_url()
            .segment(BROWSE_PATH)
            .query(VERSION_PARAM, version.segment());
        if self.system.is_empty() {
            return url.render("");
        }
        url = url.query(SYSTEM_PARAM, &self.system);
        for (name, value) in [
            (SYSTEM_VERSION_PARAM, &self.system_version),
            (FILTER_PARAMETER, &self.term),
            (DISPLAY_LANGUAGE_PARAMETER, &self.language),
            (CODE_PARAMETER, &self.code),
        ] {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        if !self.root.is_empty() && self.root != self.code {
            url = url.query(ROOT_PARAM, &self.root);
        }
        if !self.open.is_empty() {
            url = url.query(OPEN_PARAM, &encode_open(&self.open));
        }
        url.render("")
    }

    /// These parameters, searching for `term`.
    ///
    /// The selection is kept: a new search does not un-read the concept the
    /// reader is looking at.
    fn searching(&self, term: &str) -> Self {
        Self {
            term: term.trim().to_owned(),
            ..self.clone()
        }
    }

    /// Parameters over `system`, with everything read of the one before it
    /// left behind.
    ///
    /// A concept, a search term and an opened tree all belong to the system
    /// they were read in, so moving to another system starts it clean rather
    /// than carrying a code that system does not have.
    fn browsing(system: &str) -> Self {
        Self {
            system: system.trim().to_owned(),
            ..Self::default()
        }
    }

    /// These parameters, asking the operations for displays in `language`.
    fn in_language(&self, language: &str) -> Self {
        Self {
            language: language.trim().to_owned(),
            ..self.clone()
        }
    }

    /// These parameters, reading `code` and hanging the tree from it.
    fn rooted_at(&self, code: &str) -> Self {
        Self {
            code: code.to_owned(),
            root: code.to_owned(),
            open: BTreeSet::new(),
            ..self.clone()
        }
    }

    /// These parameters, reading `code` without moving the tree.
    fn selecting(&self, code: &str) -> Self {
        Self {
            code: code.to_owned(),
            ..self.clone()
        }
    }

    /// These parameters, with the children of `code` showing or hidden.
    fn toggling(&self, code: &str, open: bool) -> Self {
        let mut showing = self.open.clone();
        if open {
            showing.insert(code.to_owned());
        } else {
            showing.remove(code);
        }
        Self {
            open: showing,
            ..self.clone()
        }
    }

    /// The version to pin the operations to, when the address names one.
    fn pinned(&self) -> Option<String> {
        (!self.system_version.is_empty()).then(|| self.system_version.clone())
    }

    /// The display language to ask for, when the address names one.
    fn wanted_language(&self) -> Option<String> {
        (!self.language.is_empty()).then(|| self.language.clone())
    }

    /// The search this address asks for, or `None` when it names no system.
    fn search(&self, count: u32) -> Option<ConceptQuery> {
        (!self.system.is_empty()).then(|| ConceptQuery {
            system: self.system.clone(),
            system_version: self.pinned(),
            filter: (!self.term.is_empty()).then(|| self.term.clone()),
            child_of: None,
            display_language: self.wanted_language(),
            count,
        })
    }

    /// The concept read this address asks for, or `None` when it names none.
    fn lookup(&self) -> Option<LookupRequest> {
        (!self.system.is_empty() && !self.code.is_empty()).then(|| LookupRequest {
            system: self.system.clone(),
            system_version: self.pinned(),
            code: self.code.clone(),
            display_language: self.wanted_language(),
        })
    }
}

/// The walk one drawing of the tree performs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ChildWalk {
    /// The canonical of the system being walked.
    system: String,
    /// The code system version, when the address names one.
    system_version: Option<String>,
    /// The BCP 47 tag displays are asked for in.
    language: Option<String>,
    /// How many children one level may hold.
    count: u32,
    /// The filter property the version declares the child operator on.
    property: String,
    /// The concept the tree hangs from.
    root: String,
    /// The codes whose children are showing.
    open: BTreeSet<String>,
}

impl ChildWalk {
    /// The selection that reads the direct children of `code`.
    fn query(&self, code: &str) -> ConceptQuery {
        ConceptQuery {
            system: self.system.clone(),
            system_version: self.system_version.clone(),
            filter: None,
            child_of: Some(ChildOf {
                property: self.property.clone(),
                code: code.to_owned(),
            }),
            display_language: self.language.clone(),
            count: self.count,
        }
    }
}

/// What this server declares it can do with the system being browsed.
fn system_section(
    version: Signal<FhirVersion>,
    params: Signal<BrowseParams>,
    capabilities: LocalResource<Result<TerminologyCapabilities, FhirError>>,
    declared: Memo<Declared>,
    named: Memo<bool>,
) -> AnyView {
    let body = move || {
        view! {
            <section class="fui-stack" aria-labelledby="browse-system-heading">
                <h2 id="browse-system-heading" class="fui-sr-only">
                    {tr(Msg::CodeSystemBeingBrowsed)}
                </h2>
                <p class="fui-mono break-all">
                    {move || {
                        params
                            .with(|params| {
                                let target = system_link(&params.system, version.get());
                                view! { <a href=target>{params.system.clone()}</a> }
                            })
                    }}
                </p>
                <Reading label=&{
                    tr(Msg::ReadingTheTerminologyCapabilities)
                }>
                    {move || {
                        capabilities
                            .with(|answered| {
                                answered
                                    .as_ref()
                                    .map(|result| match result {
                                        Ok(_) => {
                                            declared
                                                .with(|declared| match &declared.row {
                                                    Some(row) => version_view(row, &declared.hierarchy),
                                                    None => note(tr(Msg::RootSTerminologyCapabilitiesDeclareNo)),
                                                })
                                        }
                                        Err(error) => failure_view(error),
                                    })
                            })
                    }}
                </Reading>
                {language_view(params, declared, version)}
            </section>
        }
        .into_any()
    };
    view! { {move || named.get().then(body)} }.into_any()
}

/// What the served version holds, and whether its hierarchy can be walked.
fn version_view(row: &VersionRow, hierarchy: &Hierarchy) -> AnyView {
    let code = row
        .code
        .clone()
        .unwrap_or_else(|| tr_with(Msg::Version2, &[("not_declared", not_declared().into())]));
    let walk = match hierarchy {
        Hierarchy::Walkable { property } => tr_with(
            Msg::VersionDeclaresOperatorFilterSoTree,
            &[("property", (property).clone().into())],
        ),
        Hierarchy::WithoutChildren { operators } => tr_with(
            Msg::VersionDeclaresHierarchyOperatorsNoneThem,
            &[("operators", (operators.join(", ")).clone().into())],
        ),
        Hierarchy::Absent => tr(Msg::VersionDeclaresNoHierarchyFilterOperator),
    };
    definitions(vec![
        (
            tr(Msg::Version),
            view! { <span class="fui-mono">{code}</span> }.into_any(),
        ),
        (tr(Msg::Hierarchy), walk.into_any()),
    ])
    .into_any()
}

/// The display language, offered as the version declares it.
fn language_view(
    params: Signal<BrowseParams>,
    declared: Memo<Declared>,
    version: Signal<FhirVersion>,
) -> AnyView {
    let navigate = StoredValue::new(use_navigate());
    let change = move |event: Event| {
        let chosen = event_target_value(&event);
        let target = params.with(|params| params.in_language(&chosen).address(version.get()));
        navigate.with_value(|navigate| go(navigate, &target));
    };
    let offered: Memo<Vec<String>> = Memo::new(move |_| {
        let mut offered = declared.with(|declared| declared.languages.clone());
        params.with(|params| {
            if !params.language.is_empty() && !offered.contains(&params.language) {
                offered.push(params.language.clone());
            }
        });
        offered
    });
    let options = move || {
        offered
            .get()
            .into_iter()
            .map(|tag| {
                let value = tag.clone();
                view! { <option value=value>{tag}</option> }.into_any()
            })
            .collect::<Vec<AnyView>>()
    };
    let picker = move || {
        let select = view! {
            <select
                id="browse-language"
                name=DISPLAY_LANGUAGE_PARAMETER
                class=format!("{SELECT} {MEDIUM}")
                aria-describedby=help_id("browse-language")
                prop:value=move || params.with(|params| params.language.clone())
                on:change=change
            >
                <option value="">{tr(Msg::ServerSOwnChoice)}</option>
                {options}
            </select>
        };
        field(
            "browse-language",
            tr(Msg::DisplayLanguage),
            Some(tr(Msg::LanguagesVersionDeclaresChoiceSentDisplaylanguage)),
            select,
        )
        .into_any()
    };
    view! {
        {move || {
            if offered.with(Vec::is_empty) {
                note(tr(Msg::VersionDeclaresNoDesignationLanguageSo))
            } else {
                picker()
            }
        }}
    }
    .into_any()
}

/// The search over the system, and the concepts it answers.
fn search_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<BrowseParams>,
    count: Memo<u32>,
    named: Memo<bool>,
) -> AnyView {
    let query: Memo<Option<ConceptQuery>> =
        Memo::new(move |_| params.with(|params| params.search(count.get())));
    let read_client = client.clone();
    let matches = Resource::new(
        move || (version.get(), query.get()),
        move |(version, query)| {
            let client = read_client.clone();
            async move {
                match query {
                    Some(query) => Some(client.expand_inline(version, &query).await),
                    None => None,
                }
            }
        },
    );

    // The live region is in the document before the read settles, which is
    // what lets a screen reader announce the count when it arrives. It is
    // derived from the answer, never from the address, because `<Transition>`
    // keeps the previous list on screen while the next one loads.
    let announcement = move || {
        matches.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .and_then(|result| result.as_ref().ok())
                .map(|value| found_sentence(&concepts_of(value), count.get()))
                .unwrap_or_default()
        })
    };

    // The field is seeded from a memo over the term alone, because
    // `prop:value` writes the property on every notification with no equality
    // check (`tachys` 0.2.18 `html/property.rs`), so a memo over the whole
    // address would discard what the reader is typing when any other parameter
    // moves. A memo does not notify while its own value is unchanged.
    let term: Memo<String> = Memo::new(move |_| params.with(|params| params.term.clone()));
    let field: NodeRef<Input> = NodeRef::new();
    let navigate = StoredValue::new(use_navigate());
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = field.get().map(|input| input.value()).unwrap_or_default();
        let target = params.with(|params| params.searching(&typed).address(version.get()));
        navigate.with_value(|navigate| go(navigate, &target));
    };

    let body = move || {
        view! {
            <section class="fui-stack" aria-labelledby="browse-search-heading">
                <h2 id="browse-search-heading" class="fui-card-title">
                    {tr(Msg::Search)}
                </h2>
                <form role="search" on:submit=submit>
                    {ferrohealth_ui::view::form::field(
                        "browse-filter",
                        tr(Msg::Filter),
                        Some(tr(Msg::SentFilterParameterExpandOverValue)),
                        view! {
                            <div class="fui-search">
                                <input
                                    id="browse-filter"
                                    name=FILTER_PARAMETER
                                    type="search"
                                    class=INPUT
                                    aria-describedby=help_id("browse-filter")
                                    node_ref=field
                                    prop:value=move || term.get()
                                />
                                <button type="submit" class=Variant::Primary.class()>
                                    {ferrohealth_ui::icon::icon(icon::SEARCH, "16")}
                                    {tr(Msg::Search)}
                                </button>
                            </div>
                        },
                    )}
                </form>
                <p aria-live="polite" class="fui-muted fui-small">
                    <Transition fallback=|| ()>{announcement}</Transition>
                </p>
                <Reading label=&{
                    tr(Msg::SearchingTheCodeSystem)
                }>
                    {move || {
                        let params = params.get();
                        let version = version.get();
                        matches
                            .with(|answered| {
                                answered
                                    .as_ref()
                                    .map(|answer| match answer {
                                        None => ().into_any(),
                                        Some(Ok(value)) => {
                                            matches_view(&concepts_of(value), &params, version)
                                        }
                                        Some(Err(error)) => failure_view(error),
                                    })
                            })
                    }}
                </Reading>
            </section>
        }
        .into_any()
    };
    view! { {move || named.get().then(body)} }.into_any()
}

/// How many concepts the answer carried, as a sentence.
fn found_sentence(concepts: &[ConceptRow], count: u32) -> String {
    let found = concepts.len();
    let whole = u32::try_from(found).unwrap_or(u32::MAX) >= count;
    match (found, whole) {
        (0, _) => tr(Msg::NoConceptMatched),
        (1, _) => tr(Msg::N1ConceptMatched),
        (found, false) => tr_with(
            Msg::ConceptsMatched,
            &[("found", (found).to_string().into())],
        ),
        (found, true) => tr_with(
            Msg::ConceptsWhichWholePageScreenAsked,
            &[("found", (found).to_string().into())],
        ),
    }
}

/// The concepts a search answered, as a list that reads each one.
fn matches_view(concepts: &[ConceptRow], params: &BrowseParams, version: FhirVersion) -> AnyView {
    if concepts.is_empty() {
        return note(tr(Msg::NoConceptMatchedFilterCodeSystem));
    }
    let items: Vec<AnyView> = concepts
        .iter()
        .map(|concept| {
            let target = params.rooted_at(&concept.code).address(version);
            let display = concept.display.clone().unwrap_or_else(|| {
                tr_with(Msg::Display2, &[("not_declared", not_declared().into())])
            });
            let code = concept.code.clone();
            let inactive = concept.inactive.then(|| String::from(" (inactive)"));
            view! {
                <a href=target>{display}</a>
                <span class="block fui-small fui-muted fui-mono break-all">{code} {inactive}</span>
            }
            .into_any()
        })
        .collect();
    divided_list(Some(tr(Msg::ConceptsTheSearchMatched).into()), items)
}

/// The concept the address names, as `$lookup` answers it.
fn concept_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<BrowseParams>,
    declared: Memo<Declared>,
    named: Memo<bool>,
) -> AnyView {
    let request: Memo<Option<LookupRequest>> =
        Memo::new(move |_| params.with(BrowseParams::lookup));
    let read_client = client.clone();
    let looked_up = Resource::new(
        move || (version.get(), request.get()),
        move |(version, request)| {
            let client = read_client.clone();
            async move {
                // The request travels back with its answer, because a resource
                // keeps its previous value while it refetches and `<Transition>`
                // keeps the previous view mounted, so a closure reading both the
                // address and the answer would pair the new code with the concept
                // that was read before it
                // (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>).
                match request {
                    Some(request) => {
                        let answer = client.lookup(version, &request).await;
                        Some((request, answer))
                    }
                    None => None,
                }
            }
        },
    );
    let asked: Memo<bool> = Memo::new(move |_| request.with(Option::is_some));

    let answered = move || {
        view! {
            <Reading label=&{
                tr(Msg::ReadingTheConcept)
            }>
                {move || {
                    let version = version.get();
                    let walked = declared.with(|declared| declared.hierarchy.walk().is_some());
                    looked_up
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|answer| match answer {
                                    None => {
                                        busy(tr(Msg::ReadingTheConcept), Busy::Section).into_any()
                                    }
                                    Some((asked, Ok(value))) => {
                                        params
                                            .with(|params| {
                                                concept_view(
                                                    &value.concept(),
                                                    &asked.code,
                                                    params,
                                                    version,
                                                    walked,
                                                )
                                            })
                                    }
                                    Some((_, Err(error))) => failure_view(error),
                                })
                        })
                }}
            </Reading>
        }
        .into_any()
    };
    let body = move || {
        view! {
            <section class="fui-stack" aria-labelledby="browse-concept-heading">
                <h2 id="browse-concept-heading" class="fui-card-title">
                    {tr(Msg::TheConcept)}
                </h2>
                {move || {
                    if asked.get() {
                        answered()
                    } else {
                        note(tr(Msg::ChooseConceptFromSearchResultsRead))
                    }
                }}
            </section>
        }
        .into_any()
    };
    view! { {move || named.get().then(body)} }.into_any()
}

/// One concept: its display, where it sits, and everything declared of it.
///
/// `code` is the code the answer was asked for rather than the one the address
/// carries now, so the code on screen always names the concept beside it.
/// `walked` says whether the tree below draws this concept's children. Where
/// it does not, and the server still answers the `child` property, the links
/// are drawn here, because a hierarchy the server declares is worth following
/// even when no filter operator can select a level of it.
fn concept_view(
    concept: &Concept,
    code: &str,
    params: &BrowseParams,
    version: FhirVersion,
    walked: bool,
) -> AnyView {
    let display = concept
        .display
        .clone()
        .unwrap_or_else(|| tr_with(Msg::Display3, &[("not_declared", not_declared().into())]));
    let code = code.to_owned();
    let held = concept.version.clone().map_or_else(
        || {
            tr_with(
                Msg::ReadFromVersionServerDidNot,
                &[("not_declared", not_declared().into())],
            )
        },
        |version| {
            tr_with(
                Msg::ReadFromVersion,
                &[("version", (version).clone().into())],
            )
        },
    );
    let parents = links_view(
        tr(Msg::Parents),
        tr(Msg::ServerDeclaresNoParentConcept),
        &concept.parents(),
        params,
        version,
    );
    let children = (!walked).then(|| {
        links_view(
            tr(Msg::Children),
            tr(Msg::ServerDeclaresNoChildConcept),
            &concept.children(),
            params,
            version,
        )
    });
    let designations = table_view(
        &tr(Msg::Designations),
        [tr(Msg::Term), tr(Msg::Language), tr(Msg::Use)],
        concept
            .designations
            .iter()
            .map(|designation| {
                [
                    designation.value.clone(),
                    designation.language.clone().unwrap_or_default(),
                    designation.usage.clone().unwrap_or_default(),
                ]
            })
            .collect(),
        tr(Msg::ServerHoldsNoDesignationConcept),
    );
    let properties = table_view(
        &tr(Msg::Properties),
        [tr(Msg::Property), tr(Msg::Value), tr(Msg::Meaning)],
        concept
            .properties
            .iter()
            .map(|property| {
                [
                    property.code.clone(),
                    property.value.clone().unwrap_or_default(),
                    property.description.clone().unwrap_or_default(),
                ]
            })
            .collect(),
        tr(Msg::ServerAnswersNoPropertyConcept),
    );
    view! {
        <div>
            <h3 class="text-title font-semibold">{display}</h3>
            <p class="fui-mono fui-small fui-muted break-all">{code}</p>
            <p class="fui-small fui-muted">{held}</p>
        </div>
        {parents}
        {children}
        {designations}
        {properties}
    }
    .into_any()
}

/// A set of related codes, as links that move the tree onto each one.
fn links_view(
    label: String,
    absent: String,
    codes: &[String],
    params: &BrowseParams,
    version: FhirVersion,
) -> AnyView {
    if codes.is_empty() {
        return note(absent);
    }
    let links: Vec<AnyView> = codes
        .iter()
        .map(|related| {
            let target = params.rooted_at(related).address(version);
            let code = related.clone();
            view! {
                <a href=target class="fui-button fui-button-secondary fui-button-small fui-mono">
                    {code}
                </a>
            }
            .into_any()
        })
        .collect();
    let named = tr_with(Msg::OfThisConcept, &[("label", label.clone().into())]);
    view! {
        <nav aria-label=named class="fui-stack">
            <p class="fui-label">{label}</p>
            {cluster(links)}
        </nav>
    }
    .into_any()
}

/// One table of three columns, whose first column heads each row.
///
/// A cell the server left empty states its absence rather than reading as a
/// gap the screen failed to fill.
fn table_view(
    caption: &str,
    headers: [String; 3],
    rows: Vec<[String; 3]>,
    absent: String,
) -> AnyView {
    let [first, second, third] = headers;
    // A last column no row fills says nothing, so it is not drawn.
    let columns = filled_columns(
        &[
            Column::text("first", first),
            Column::text("second", second.clone()),
            Column::text("third", third.clone()),
        ],
        &rows,
        &["first", "second"],
        |row, _| row.get(2).is_some_and(|cell| !cell.is_empty()),
    );
    let thirds = columns.len() == 3;
    let body = if rows.is_empty() {
        Body::Empty(empty_quiet(icon::BROWSE, tr(Msg::NothingDeclared), absent).into_any())
    } else {
        Body::Rows(
            rows.into_iter()
                .map(|cells| {
                    let [head, middle, tail] = cells.map(|cell| {
                        if cell.is_empty() {
                            not_declared().clone()
                        } else {
                            cell
                        }
                    });
                    view! {
                        <tr>
                            <th scope="row">{head}</th>
                            <td data-label=second.clone()>{middle}</td>
                            {thirds
                                .then(|| {
                                    view! {
                                        <td data-label=third.clone() class="fui-muted">
                                            {tail}
                                        </td>
                                    }
                                })}
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption,
        columns: &columns,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// The walk the address asks for, or `None` when this version declares none.
fn child_walk(
    params: Signal<BrowseParams>,
    declared: Memo<Declared>,
    count: Memo<u32>,
) -> Memo<Option<ChildWalk>> {
    Memo::new(move |_| {
        let property = declared.with(|declared| declared.hierarchy.walk().map(str::to_owned))?;
        params.with(|params| {
            (!params.system.is_empty() && !params.root.is_empty()).then(|| ChildWalk {
                system: params.system.clone(),
                system_version: params.pinned(),
                language: params.wanted_language(),
                count: count.get(),
                property,
                root: params.root.clone(),
                open: params.open.clone(),
            })
        })
    })
}

/// One level of the tree, and whether the server listed all of it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
struct Level {
    /// The children this level drew.
    children: Vec<TreeConcept>,
    /// Whether the answer held every child the server counted.
    whole: bool,
}

/// One walk and the children it read, by the code they hang below.
///
/// The walk travels back with its answers, because a resource keeps its
/// previous value while it refetches and `<Transition>` keeps the previous
/// view mounted, so rows built from the address over the last answer would
/// claim the new anchor has no children before its level has been read
/// (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>).
type Levels = Resource<Option<(ChildWalk, Vec<(String, Result<Level, FhirError>)>)>>;

/// Issues the reads one drawing of the tree needs.
///
/// The walk reads the anchor's children first and then follows the open nodes
/// it finds, so a code left open in the address that no longer hangs below the
/// anchor costs no request.
fn child_levels(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    walk: Memo<Option<ChildWalk>>,
) -> Levels {
    let read_client = client.clone();
    Resource::new(
        move || (version.get(), walk.get()),
        move |(version, walk)| {
            let client = read_client.clone();
            async move {
                let walk = walk?;
                let mut answers: Vec<(String, Result<Level, FhirError>)> = Vec::new();
                let mut frontier = vec![walk.root.clone()];
                let mut asked: BTreeSet<String> = BTreeSet::new();
                while let Some(code) = frontier.pop() {
                    if !asked.insert(code.clone()) {
                        continue;
                    }
                    let answer = client
                        .expand_inline(version, &walk.query(&code))
                        .await
                        .map(|value| children_of(&value));
                    if let Ok(level) = &answer {
                        frontier.extend(
                            level
                                .children
                                .iter()
                                .filter(|child| walk.open.contains(&child.code))
                                .map(|child| child.code.clone()),
                        );
                    }
                    answers.push((code, answer));
                }
                Some((walk, answers))
            }
        },
    )
}

/// The hierarchy below the concept, one declared level at a time.
fn tree_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<BrowseParams>,
    declared: Memo<Declared>,
    count: Memo<u32>,
) -> AnyView {
    let walk = child_walk(params, declared, count);
    let levels = child_levels(client, version, walk);
    let walkable: Memo<bool> = Memo::new(move |_| walk.with(Option::is_some));

    // The rows come from the walk the answer belongs to, never from the
    // address, so a level that has not been read yet is never drawn as a
    // concept the server said has no children. `None` is that not-yet case:
    // the section is only rendered while a walk exists, so an answer holding
    // no walk belongs to an address the reader has already left.
    let drawn = Signal::derive(move || {
        levels.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .map(|(walk, answers)| rows(&walk.root, &walk.open, &known_children(answers)))
        })
    });
    let refusals = Signal::derive(move || {
        levels.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .map(|(_, answers)| {
                    answers
                        .iter()
                        .filter_map(|(_, answer)| answer.as_ref().err().cloned())
                        .collect::<Vec<FhirError>>()
                })
                .unwrap_or_default()
        })
    });
    // A level the server counted higher than it listed is a partial answer,
    // and the tree says so on the node it hangs below rather than drawing a
    // short list as if it were the whole one.
    let partial = move || {
        levels.with(|answered| {
            answered
                .as_ref()
                .and_then(Option::as_ref)
                .map(|(_, answers)| partial_levels(answers))
                .unwrap_or_default()
        })
    };

    let navigate = StoredValue::new(use_navigate());
    // A node opened or closed is a navigation, so the open set is the address
    // and a reader can send the walk they made.
    let toggle = move |key: String, open: bool| {
        let Some(code) = drawn
            .get_untracked()
            .unwrap_or_default()
            .into_iter()
            .find(|row| row.key == key)
            .map(|row| row.concept.code)
        else {
            return;
        };
        let target = params.with_untracked(|params| {
            params
                .toggling(&code, open)
                .address(version.get_untracked())
        });
        navigate.with_value(|navigate| go(navigate, &target));
    };
    let on_expand = Callback::new(move |key: String| toggle(key, true));
    let on_collapse = Callback::new(move |key: String| toggle(key, false));
    // NOTE: Leptos book, `async/11_suspense`: the tree is built inside the
    // boundary from the rows the walk answered, so no read escapes it.
    let draw = move |rows: Vec<TreeRow>| {
        let version = version.get();
        let partial = partial();
        let (nodes, selected) = params.with(|params| {
            (
                nested(&rows, None, params, version, &partial),
                selected_row(&rows, &params.code),
            )
        });
        let open: BTreeSet<String> = rows
            .iter()
            .filter(|row| row.open)
            .map(|row| row.key.clone())
            .collect();
        tree_view_collapsing(
            Tree {
                id: "browse-tree",
                label: tr(Msg::ConceptsBelowOne).into(),
                nodes: Signal::stored(nodes),
                expanded: RwSignal::new(open),
                selected: Signal::stored((!selected.is_empty()).then_some(selected)),
                on_expand: Some(on_expand),
                actions: None,
            },
            on_collapse,
        )
    };
    let draw = StoredValue::new(draw);
    let body = move || tree_pane(drawn, refusals, draw);
    view! { {move || walkable.get().then(body)} }.into_any()
}

/// The tree pane: the heading, the tree once its first level answered, and
/// the levels the server refused.
fn tree_pane(
    drawn: Signal<Option<Vec<TreeRow>>>,
    refusals: Signal<Vec<FhirError>>,
    draw: StoredValue<impl Fn(Vec<TreeRow>) -> AnyView + Send + Sync + 'static>,
) -> AnyView {
    view! {
        <section class="fui-stack" aria-labelledby="browse-tree-heading">
            <h2 id="browse-tree-heading" class="fui-card-title">
                {tr(Msg::BelowThisConcept)}
            </h2>
            <p class="fui-help">{tr(Msg::ArrowKeysWalkEnterReadsConcept)}</p>
            <Reading label=&{
                tr(Msg::ReadingTheChildren)
            }>
                {move || {
                    let Some(rows) = drawn.get() else {
                        return busy(tr(Msg::ReadingTheChildren), Busy::Section).into_any();
                    };
                    if rows.is_empty() {
                        return view! {
                            <div id="browse-tree-empty">
                                {empty_quiet(
                                    icon::BROWSE,
                                    tr(Msg::NothingBelow),
                                    tr(Msg::ServerAnswersNoChildConcept),
                                )}
                            </div>
                        }
                            .into_any();
                    }
                    draw.with_value(|draw| draw(rows))
                }} {move || refusals.get().iter().map(failure_view).collect::<Vec<AnyView>>()}
            </Reading>
        </section>
    }
    .into_any()
}

/// The rows below `parent` (the top when `None`) as FerroUI's nested items,
/// each a link that reads its concept; a level the server listed short says
/// so in the item's words.
fn nested(
    rows: &[TreeRow],
    parent: Option<&str>,
    params: &BrowseParams,
    version: FhirVersion,
    partial: &BTreeSet<String>,
) -> Vec<TreeNode> {
    rows.iter()
        .filter(|row| row.parent.as_deref() == parent)
        .map(|row| {
            let display = row.concept.display.clone().unwrap_or_else(|| {
                tr_with(Msg::Display4, &[("not_declared", not_declared().into())])
            });
            let label = if row.open && partial.contains(&row.concept.code) {
                tr_with(
                    Msg::MoreChildrenThanPageSizeAsked,
                    &[("display", (display).clone().into())],
                )
            } else {
                display
            };
            let node = TreeNode::new(row.key.clone(), label)
                .code(row.concept.code.clone())
                .href(params.selecting(&row.concept.code).address(version));
            let children = nested(rows, Some(&row.key), params, version, partial);
            if !children.is_empty() {
                node.children(children)
            } else if row.expandable {
                node.expandable()
            } else {
                node
            }
        })
        .collect()
}

/// The one row the tree announces as selected: the first drawing the code
/// the address reads, because the address carries the concept rather than
/// the path that reached it, and a tree that sets no `aria-multiselectable`
/// announces one (<https://www.w3.org/TR/wai-aria-1.2/#aria-selected>).
fn selected_row(rows: &[TreeRow], code: &str) -> String {
    if code.is_empty() {
        return String::new();
    }
    rows.iter()
        .find(|row| row.concept.code == code)
        .map(|row| row.key.clone())
        .unwrap_or_default()
}

/// Navigates to an address that already carries the router's base.
// NOTE: the router resolves a navigation against its base, so an address that
// already carries it is passed unresolved (`leptos_router` 0.8.15
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

/// The code system this screen browses, picked from what the root publishes.
///
/// The browser works over one code system, so an address naming none draws
/// nothing. Picking one is a navigation onto this same screen, which is what
/// keeps a browsed system a link a reader can send.
fn system_picker(
    version: Signal<FhirVersion>,
    params: Signal<BrowseParams>,
    systems: Memo<Vec<Choice>>,
) -> AnyView {
    let navigate = StoredValue::new(use_navigate());
    let choose = move |event: Event| {
        let canonical = event_target_value(&event);
        if canonical.is_empty() {
            return;
        }
        let target = BrowseParams::browsing(&canonical).address(version.get());
        navigate.with_value(|navigate| go(navigate, &target));
    };
    let offered = move || {
        systems.with(|systems| {
            systems
                .iter()
                .map(|choice| {
                    let canonical = choice.canonical.clone();
                    let label = choice.label.clone();
                    // The options arrive after the select is drawn, so each
                    // says itself whether it is the one the address names.
                    let named = canonical.clone();
                    let selected = move || params.with(|params| params.system == named);
                    view! {
                        <option value=canonical.clone() title=canonical selected=selected>
                            {label}
                        </option>
                    }
                    .into_any()
                })
                .collect::<Vec<AnyView>>()
        })
    };
    let select = view! {
        <select
            id="browse-system"
            class=SELECT
            prop:value=move || params.with(|params| params.system.clone())
            on:change=choose
        >
            <option value="">{tr(Msg::ChooseACodeSystem)}</option>
            {offered}
        </select>
    };
    field("browse-system", tr(Msg::CodeSystem2), None, select).into_any()
}

/// A sentence stating what the server did not declare.
fn note(text: String) -> AnyView {
    crate::components::state::invitation(text)
}

/// The concepts an expansion listed, at every depth it answered at.
///
/// The request asks for a flat answer, so the depths are normally all zero.
/// A server that nests anyway still had every concept it listed match the
/// selection, and dropping the nested ones would hide a match from the reader.
fn concepts_of(value: &ExpandedValueSet) -> Vec<ConceptRow> {
    value
        .expansion()
        .map(|expansion| expansion.concepts)
        .unwrap_or_default()
}

/// The children an expansion listed, as the tree draws them.
fn children_of(value: &ExpandedValueSet) -> Level {
    let listed: Vec<TreeConcept> = concepts_of(value)
        .into_iter()
        .map(|concept| TreeConcept {
            code: concept.code,
            display: concept.display,
        })
        .collect();
    // `expansion.total` is "the total number of concepts in the expansion"
    // (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.total>),
    // so a total above what arrived is the server saying this level holds more
    // than the page asked for. A server that sends no total has said nothing,
    // and the level is drawn as it arrived.
    let held = u32::try_from(listed.len()).unwrap_or(u32::MAX);
    let whole = value
        .expansion()
        .and_then(|expansion| expansion.total)
        .is_none_or(|total| total <= held);
    Level {
        children: listed,
        whole,
    }
}

/// The children that were read, by the code they hang below.
fn known_children(
    answers: &[(String, Result<Level, FhirError>)],
) -> BTreeMap<String, Vec<TreeConcept>> {
    answers
        .iter()
        .filter_map(|(code, answer)| {
            answer
                .as_ref()
                .ok()
                .map(|level| (code.clone(), level.children.clone()))
        })
        .collect()
}

/// The codes whose children the server said it did not list in full.
fn partial_levels(answers: &[(String, Result<Level, FhirError>)]) -> BTreeSet<String> {
    answers
        .iter()
        .filter(|(_, answer)| answer.as_ref().is_ok_and(|level| !level.whole))
        .map(|(code, _)| code.clone())
        .collect()
}

/// A refused read, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    view! {
        <div class="mt-default">
            <Failure error=Signal::stored(error) />
        </div>
    }
    .into_any()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::UI_BASE;

    fn read(pairs: &[(&str, &str)]) -> BrowseParams {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        BrowseParams::read(&|name| {
            owned
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
    }

    #[test]
    fn an_address_that_names_nothing_reads_as_no_system() {
        let params = read(&[]);
        assert_eq!(params, BrowseParams::default());
        assert_eq!(
            params.search(20),
            None,
            "with no system there is nothing to search"
        );
        assert_eq!(params.lookup(), None);
    }

    #[test]
    fn the_tree_hangs_from_the_concept_when_the_address_names_no_anchor() {
        let params = read(&[("system", "https://terminology.example/x"), ("code", "a")]);
        assert_eq!(
            params.root, "a",
            "a link that names only a concept still draws its tree"
        );
    }

    #[test]
    fn the_address_carries_every_piece_of_state_the_screen_holds() {
        let params = read(&[
            ("system", "https://terminology.example/x?edition=2031"),
            ("version", "2.0"),
            ("filter", "fever"),
            ("displayLanguage", "nl-NL"),
            ("code", "a"),
            ("root", "b"),
            ("open", "b,c"),
        ]);
        assert_eq!(
            params.address(FhirVersion::R4B),
            format!(
                "{UI_BASE}/browse\
                 ?fhir=r4b&system=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031\
                 &version=2.0&filter=fever&displayLanguage=nl-NL&code=a&root=b&open=b%2Cc"
            ),
            "a reader who copies the address gets what they were reading"
        );
        assert_eq!(
            read(&[
                ("system", "https://terminology.example/x?edition=2031"),
                ("version", "2.0"),
                ("filter", "fever"),
                ("displayLanguage", "nl-NL"),
                ("code", "a"),
                ("root", "b"),
                ("open", "b,c"),
            ]),
            params,
            "the address round trips"
        );
    }

    #[test]
    fn a_code_that_carries_the_list_separator_survives_the_address() {
        let params = read(&[("system", "https://terminology.example/x"), ("code", "a")])
            .toggling("m,s2", true)
            .toggling("kg", true);
        let address = params.address(FhirVersion::R4B);
        let written = address
            .split("open=")
            .nth(1)
            .expect("the address carries the open nodes");
        // The router decodes a query value once before a screen reads it, so
        // reading the address back models that one pass.
        let decoded = crate::url::decode_query_component(written);
        assert_eq!(
            read(&[
                ("system", "https://terminology.example/x"),
                ("code", "a"),
                ("open", &decoded),
            ])
            .open,
            params.open,
            "a code carrying a comma cannot split the list it travels in: {address}"
        );
    }

    #[test]
    fn an_anchor_that_is_the_concept_is_left_out_of_the_address() {
        let params = read(&[("system", "https://terminology.example/x"), ("code", "a")]);
        let address = params.address(FhirVersion::R5);
        assert!(
            !address.contains("root="),
            "the concept seeds the anchor, so naming it twice says nothing: {address}"
        );
    }

    #[test]
    fn choosing_a_concept_moves_the_tree_and_closes_what_was_open() {
        let params = read(&[
            ("system", "https://terminology.example/x"),
            ("code", "a"),
            ("open", "a,b"),
        ]);
        let chosen = params.rooted_at("c");
        assert_eq!((chosen.code.as_str(), chosen.root.as_str()), ("c", "c"));
        assert!(
            chosen.open.is_empty(),
            "the tree hangs from the new concept, so what was open below the old one is gone"
        );
    }

    #[test]
    fn choosing_inside_the_tree_leaves_the_tree_where_it_is() {
        let params = read(&[
            ("system", "https://terminology.example/x"),
            ("code", "a"),
            ("open", "a,b"),
        ]);
        let chosen = params.selecting("b");
        assert_eq!((chosen.code.as_str(), chosen.root.as_str()), ("b", "a"));
        assert_eq!(
            chosen.open, params.open,
            "reading a concept in the tree does not collapse the tree under the reader"
        );
    }

    #[test]
    fn opening_and_closing_a_node_moves_only_that_node() {
        let params = read(&[("system", "https://terminology.example/x"), ("code", "a")]);
        let opened = params.toggling("b", true);
        assert!(opened.open.contains("b"));
        assert_eq!(
            opened.toggling("b", false).open,
            params.open,
            "closing what was opened leaves the address as it was"
        );
    }

    #[test]
    fn a_search_sends_the_filter_and_the_pinned_version() {
        let params = read(&[
            ("system", "https://terminology.example/x"),
            ("version", "2.0"),
            ("filter", " fever "),
            ("displayLanguage", "nl"),
        ]);
        let query = params.search(25).expect("the address names a system");
        assert_eq!(
            (
                query.system.as_str(),
                query.system_version.as_deref(),
                query.filter.as_deref(),
                query.display_language.as_deref(),
                query.count
            ),
            (
                "https://terminology.example/x",
                Some("2.0"),
                Some("fever"),
                Some("nl"),
                25
            ),
            "surrounding space is trimmed off the term the reader typed"
        );
        assert_eq!(
            query.child_of, None,
            "a search selects the whole system, not one level of it"
        );
    }

    #[test]
    fn a_search_with_no_filter_still_asks_for_the_first_concepts() {
        let params = read(&[("system", "https://terminology.example/x")]);
        let query = params.search(10).expect("the address names a system");
        assert_eq!(
            query.filter, None,
            "an empty filter is left out, so the server's own default applies"
        );
    }

    #[test]
    fn a_concept_read_names_the_system_the_code_belongs_to() {
        let params = read(&[
            ("system", "https://terminology.example/x"),
            ("code", "a"),
            ("displayLanguage", "de"),
        ]);
        let request = params.lookup().expect("the address names a concept");
        assert_eq!(
            (
                request.system.as_str(),
                request.code.as_str(),
                request.display_language.as_deref()
            ),
            ("https://terminology.example/x", "a", Some("de"))
        );
    }

    #[test]
    fn a_level_of_the_tree_asks_for_the_children_of_one_code() {
        let walk = ChildWalk {
            system: String::from("https://terminology.example/x"),
            system_version: Some(String::from("2.0")),
            language: None,
            count: 50,
            property: String::from("concept"),
            root: String::from("a"),
            open: BTreeSet::new(),
        };
        let query = walk.query("b");
        assert_eq!(
            query.child_of,
            Some(ChildOf {
                property: String::from("concept"),
                code: String::from("b")
            }),
            "the walk names the property the version declared the operator on"
        );
        assert_eq!(query.filter, None, "a level is selected, not searched");
    }

    #[test]
    fn the_count_sentence_says_what_the_answer_held() {
        let concept = |code: &str| ConceptRow {
            code: code.to_owned(),
            ..ConceptRow::default()
        };
        assert_eq!(found_sentence(&[], 20), "No concept matched.");
        assert_eq!(found_sentence(&[concept("a")], 20), "1 concept matched.");
        assert_eq!(
            found_sentence(&[concept("a"), concept("b")], 20),
            "2 concepts matched."
        );
        assert!(
            found_sentence(&[concept("a"), concept("b")], 2).contains("Narrow the filter"),
            "a full page may be hiding matches, and the reader is told so"
        );
    }

    #[test]
    fn a_nested_match_stays_in_the_list_the_reader_sees() {
        let answer: ExpandedValueSet = serde_json::from_str(
            r#"{"resourceType":"ValueSet","expansion":{"contains":[
                {"code":"a","contains":[{"code":"b"}]}]}}"#,
        )
        .expect("the fixture is valid JSON");
        assert_eq!(
            concepts_of(&answer)
                .into_iter()
                .map(|concept| concept.code)
                .collect::<Vec<String>>(),
            vec![String::from("a"), String::from("b")],
            "every code a server listed matched the selection, however it nested them"
        );
    }

    #[test]
    fn a_level_the_server_counted_higher_than_it_listed_is_marked_partial() {
        let level = |json: &str| {
            let answer: ExpandedValueSet =
                serde_json::from_str(json).expect("the fixture is valid JSON");
            children_of(&answer)
        };
        assert!(
            !level(
                r#"{"resourceType":"ValueSet","expansion":{"total":5,"contains":[{"code":"a"}]}}"#
            )
            .whole,
            "a total above what arrived is the server saying the level holds more"
        );
        assert!(
            level(
                r#"{"resourceType":"ValueSet","expansion":{"total":1,"contains":[{"code":"a"}]}}"#
            )
            .whole,
            "a total the answer met is a whole level"
        );
        assert!(
            level(r#"{"resourceType":"ValueSet","expansion":{"contains":[{"code":"a"}]}}"#).whole,
            "a server that sent no total has said nothing, so the level is drawn as it arrived"
        );
    }

    #[test]
    fn one_concept_drawn_twice_is_announced_as_selected_once() {
        let row = |key: &str, code: &str| TreeRow {
            key: String::from(key),
            concept: TreeConcept {
                code: String::from(code),
                display: None,
            },
            ..TreeRow::default()
        };
        let rows = vec![row("a,c", "c"), row("b,c", "c"), row("b,d", "d")];
        assert_eq!(
            selected_row(&rows, "c"),
            "a,c",
            "the first row drawing the code is the one the tree announces"
        );
        assert_eq!(
            selected_row(&rows, ""),
            "",
            "an address that reads no concept selects no row"
        );
        assert_eq!(
            selected_row(&rows, "gone"),
            "",
            "a concept no row draws selects no row"
        );
    }
}
