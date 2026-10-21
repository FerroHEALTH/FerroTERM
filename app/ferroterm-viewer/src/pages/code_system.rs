// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One code system: what it says it is, what this server can do with it,
//! and the resource as the server sent it.
//!
//! Two documents describe a code system and they answer different questions.
//! The published `CodeSystem` resource says what the code system is;
//! `TerminologyCapabilities` says what this server can do with it. Each tab
//! names the document behind it, so a claim can be traced to the request
//! that produced it rather than blended into one table.

use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::{empty_quiet, empty_state};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::{
    Crumb, Header, chip, cluster, definitions, facts_row, grid, page_heading, page_meta, panel,
    stack, titled_panel,
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params;
use leptos_router::params::Params;

use crate::components::detail::{current_tab, detail_tabs, json_pane};
use crate::components::failure::Failure;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::components::shell::{Console, SelectedVersion};
use crate::components::state::undeclared;
use crate::fhir::code_system::{CodeSystemSearch, PublishedCodeSystem};
use crate::fhir::concept::{Hierarchy, chosen_version};
use crate::fhir::error::FhirError;
use crate::fhir::terminology::{SystemCard, TerminologyCapabilities, VersionRow};
use crate::fhir::version::FhirVersion;
use crate::fhir::{Document, FhirClient, resources_text};
use crate::locale::{Msg, tr, tr_with};
use crate::routes::{
    BROWSE_PATH, CODE_SYSTEM_EDITOR_PATH, EXPAND_PATH, VALIDATE_PATH, system_tool_link,
};

/// The tabs of the screen: the definition, the server's support, the JSON.
const TABS: &[(&str, Msg)] = &[
    ("overview", Msg::Definition),
    ("support", Msg::ServerSupport),
    ("json", Msg::Json),
];

/// The columns of a version's filter table.
fn filter_columns() -> [Column; 2] {
    [
        Column::text("property", tr(Msg::Property)),
        Column::text("operators", tr(Msg::Operators)),
    ]
}

/// The path parameter this screen is addressed by.
#[derive(Clone, Debug, Params, PartialEq)]
struct SystemParams {
    /// The code system canonical, which the router percent-decodes on read.
    url: Option<String>,
}

/// A read the screen makes.
type Fetched<T> = Resource<Result<T, FhirError>>;

/// Shows one code system, from the two documents that describe it.
///
/// The canonical is read reactively. A link from one system to another
/// matches this same `<Route>`, and `leptos_router` 0.8 then updates the
/// params signal without re-running this body, so a read taken untracked at
/// setup would go stale.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn CodeSystemPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let params = use_params::<SystemParams>();
    // NOTE: an address carrying no canonical is a link a reader typed, so it
    // reads as the empty system and every section states what it found.
    let system = Signal::derive(move || {
        params
            .read()
            .as_ref()
            .ok()
            .and_then(|params| params.url.clone())
            .unwrap_or_default()
    });
    let capabilities = {
        let client = client.clone();
        Resource::new(
            move || version.get(),
            move |version| {
                let client = client.clone();
                async move { client.terminology_capabilities(version).await }
            },
        )
    };
    let search = Resource::new(
        move || (version.get(), system.get()),
        move |(version, system)| {
            let client = client.clone();
            async move { client.code_system_search(version, &system).await }
        },
    );
    let tab = current_tab(TABS, "overview");

    let title = move || system.with(|system| title_of(system));
    let header = view! {
        <Transition fallback=move || {
            system.with(|system| heading(heading_of(system), system, None, ()))
        }>{move || header(system, version, capabilities, search)}</Transition>
    };
    let body = move || match tab.get() {
        "support" => support_tab(system, capabilities),
        "json" => json_tab(search),
        _ => overview_tab(search),
    };
    view! {
        <Title text=title />
        {stack(
            view! {
                {header}
                {detail_tabs(tr(Msg::ViewsCodeSystem), TABS, tab)}
                {body}
            },
        )}
    }
}

/// The document title for one canonical.
fn title_of(system: &str) -> String {
    if system.is_empty() {
        tr(Msg::CodeSystem2).clone()
    } else {
        system.to_owned()
    }
}

/// The heading for one canonical.
fn heading_of(system: &str) -> String {
    if system.is_empty() {
        tr(Msg::AddressNamesNoCodeSystem).clone()
    } else {
        system.to_owned()
    }
}

/// The page heading: the trail, the title, its subtitle and the actions.
fn heading(
    title: String,
    canonical: &str,
    subtitle: Option<String>,
    actions: impl IntoView,
) -> AnyView {
    let mono = title == canonical;
    page_heading(
        Header {
            title,
            subtitle,
            crumbs: vec![Crumb::new(tr(Msg::CodeSystems), "/systems")],
            mono,
        },
        actions,
    )
    .into_any()
}

/// The header once the reads answered: the name the resource carries, the
/// canonical under it, the facts across a row, and the screens it opens in.
fn header(
    system: Signal<String>,
    version: Signal<FhirVersion>,
    capabilities: Fetched<TerminologyCapabilities>,
    search: Fetched<Document<CodeSystemSearch>>,
) -> AnyView {
    let canonical = system.get();
    let published = search.with(|answered| {
        answered
            .as_ref()
            .and_then(|answer| answer.as_ref().ok())
            .and_then(|document| document.value.published().into_iter().next())
    });
    let card = capabilities.with(|answered| {
        answered
            .as_ref()
            .and_then(|answer| answer.as_ref().ok())
            .and_then(|document| document.card(&canonical))
    });
    let title = published
        .as_ref()
        .and_then(PublishedCodeSystem::title)
        .map_or_else(|| heading_of(&canonical), str::to_owned);
    let subtitle = published
        .as_ref()
        .and_then(PublishedCodeSystem::description)
        .map(str::to_owned);
    let actions = card
        .as_ref()
        .map_or_else(|| ().into_any(), |card| actions(card, version.get()));
    let facts = facts(&canonical, card.as_ref(), published.as_ref());
    view! {
        <div>{heading(title, &canonical, subtitle, actions)}</div>
        {panel(facts)}
    }
    .into_any()
}

/// The facts across the top of the screen.
fn facts(
    canonical: &str,
    card: Option<&SystemCard>,
    published: Option<&PublishedCodeSystem>,
) -> AnyView {
    let default = card
        .and_then(|card| chosen_version(card, None))
        .and_then(|row| row.code)
        .unwrap_or_else(|| String::from("—"));
    let status = published
        .and_then(PublishedCodeSystem::status)
        .map_or_else(|| String::from("—"), str::to_owned);
    let content = card
        .and_then(|card| card.content.clone())
        .unwrap_or_else(|| String::from("—"));
    let subsumes = match card.and_then(|card| card.subsumption) {
        Some(true) => pill(Tone::Ok, "yes").into_any(),
        Some(false) => pill(Tone::Neutral, "no").into_any(),
        None => view! { <span class="fui-muted">"—"</span> }.into_any(),
    };
    facts_row(vec![
        (
            tr(Msg::Canonical),
            view! { <span class="fui-mono fui-break">{canonical.to_owned()}</span> }.into_any(),
        ),
        (
            tr(Msg::DefaultVersion),
            view! { <span class="fui-mono">{default}</span> }.into_any(),
        ),
        (tr(Msg::Status), status.into_any()),
        (tr(Msg::Content), content.into_any()),
        (tr(Msg::Subsumption), subsumes),
    ])
    .into_any()
}

/// The screens that work over this system, carrying it and its version.
///
/// The concept browser appears only where that version declares the
/// direct-child operator, because a tree with no operator to walk it has
/// nothing to draw (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>).
fn actions(card: &SystemCard, version: FhirVersion) -> AnyView {
    if card.url.is_empty() {
        return ().into_any();
    }
    let served = chosen_version(card, None);
    let code = served.as_ref().and_then(|row| row.code.clone());
    let walkable = served.is_some_and(|row| Hierarchy::of(&row).walk().is_some());
    let link = |path: &str, variant: Variant, glyph, label: String| {
        let href = system_tool_link(path, &card.url, code.as_deref(), version);
        view! {
            <a href=href class=variant.class()>
                {ferrohealth_ui::icon::icon(glyph, "16")}
                {label}
            </a>
        }
        .into_any()
    };
    let browse = walkable.then(|| {
        link(
            BROWSE_PATH,
            Variant::Primary,
            icon::BROWSE,
            tr(Msg::BrowseConcepts),
        )
    });
    // NOTE: no specification governs this: our own design; a console without
    // a sign-in has no editor, so it offers none.
    let edit = expect_context::<Console>().sign_in.then(|| {
        link(
            CODE_SYSTEM_EDITOR_PATH,
            Variant::Secondary,
            icon::EDIT,
            tr(Msg::Edit),
        )
    });
    view! {
        <nav aria-label=tr(Msg::ScreensCodeSystem) class="fui-cluster">
            {browse}
            {link(VALIDATE_PATH, Variant::Secondary, icon::VALIDATE, tr(Msg::CheckACode))}
            {link(EXPAND_PATH, Variant::Secondary, icon::EXPAND, tr(Msg::Expand))}
            {edit}
        </nav>
    }
    .into_any()
}

/// What the code system itself says it is, from the published resources.
fn overview_tab(search: Fetched<Document<CodeSystemSearch>>) -> AnyView {
    view! {
        <section aria-labelledby="system-published-heading">
            <h2 id="system-published-heading" class="fui-sr-only">
                {tr(Msg::WhatCodeSystemSays)}
            </h2>
            <Reading label=&{
                tr(Msg::ReadingThePublishedCodesystem)
            }>
                {move || {
                    search
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|answer| match answer {
                                    Ok(document) => {
                                        let meta = page_meta(
                                            partial(document.value.matched(), document.value.total()),
                                        );
                                        view! {
                                            {meta}
                                            {resources_view(&document.value.published())}
                                        }
                                            .into_any()
                                    }
                                    Err(error) => failure_view(error),
                                })
                        })
                }}
            </Reading>
        </section>
    }
    .into_any()
}

/// What this server declares it can do with the system, one panel per
/// served version.
fn support_tab(system: Signal<String>, capabilities: Fetched<TerminologyCapabilities>) -> AnyView {
    view! {
        <section aria-labelledby="system-capability-heading">
            <h2 id="system-capability-heading" class="fui-sr-only">
                {tr(Msg::WhatServerCanDo)}
            </h2>
            <Reading label=&{
                tr(Msg::ReadingTheTerminologyCapabilities)
            }>
                {move || {
                    capabilities
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|answer| match answer {
                                    Ok(document) => {
                                        match system.with(|system| document.card(system)) {
                                            Some(card) => support_view(&card),
                                            None => undeclared_view(),
                                        }
                                    }
                                    Err(error) => failure_view(error),
                                })
                        })
                }}
            </Reading>
        </section>
    }
    .into_any()
}

/// The published resources as the server sent them.
fn json_tab(search: Fetched<Document<CodeSystemSearch>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingThePublishedCodesystem)
        }>
            {move || {
                search
                    .with(|answered| {
                        answered
                            .as_ref()
                            .map(|answer| match answer {
                                Ok(document) => {
                                    json_pane(
                                        "system-json",
                                        tr(Msg::CodesystemServerSent),
                                        resources_text(&document.text),
                                    )
                                }
                                Err(error) => failure_view(error),
                            })
                    })
            }}
        </Reading>
    }
    .into_any()
}

/// The statement that this root's capabilities do not name the system.
fn undeclared_view() -> AnyView {
    undeclared(tr(Msg::RootSTerminologyCapabilitiesDoNot))
}

/// A refused read, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    view! { <Failure error=Signal::stored(error) /> }.into_any()
}

/// One panel per served version, or the statement that none is declared.
///
/// The panels are a plain `Vec`, which rebuilds every position when the read
/// settles again. A `<For>` would keep a retained key's old body while its
/// version code stayed the same (`leptos` 0.8, `for_loop.rs`).
fn support_view(card: &SystemCard) -> AnyView {
    if card.versions.is_empty() {
        return empty_quiet(
            icon::VERSION,
            tr(Msg::NoVersionDeclared),
            tr(Msg::ServerDeclaresNoVersionCodeSystem),
        )
        .into_any();
    }
    let panels: Vec<AnyView> = card.versions.iter().map(version_view).collect();
    view! { <div class="fui-stack">{panels}</div> }.into_any()
}

/// One served version: what it resolves, what it holds, what it filters on
/// and what it answers.
fn version_view(row: &VersionRow) -> AnyView {
    let code = row.code.clone().map_or_else(
        || tr_with(Msg::Version3, &[("not_declared", not_declared().into())]),
        |code| tr_with(Msg::Version4, &[("code", (code).clone().into())]),
    );
    let mark = row
        .is_default
        .then(|| badge(Tone::Accent, &tr(Msg::DefaultMark)));
    let compositional = match row.compositional {
        Some(true) => tr(Msg::StateRead),
        Some(false) => tr(Msg::NotRead),
        None => not_declared(),
    };
    let resolves = if row.is_default {
        tr(Msg::AnUnversionedRequest)
    } else {
        tr(Msg::RequestNamingVersion)
    };
    let rows = vec![
        (tr(Msg::Answers), resolves.into_any()),
        (tr(Msg::CompositionalGrammar), compositional.into_any()),
        (
            tr(Msg::DesignationLanguages),
            chips(&row.languages, tr(Msg::NoneDeclared)),
        ),
        (
            tr(Msg::LookupProperties),
            chips(&row.properties, tr(Msg::NoneDeclared)),
        ),
    ];
    let filters = filter_view(row);
    let body = view! { <div class="fui-panel-body fui-stack">{definitions(rows)} {filters}</div> };
    titled_panel(code, mark, body).into_any()
}

/// `values` as chips, or the words `absent` when there are none.
fn chips(values: &[String], absent: String) -> AnyView {
    if values.is_empty() {
        return view! { <span class="fui-muted">{absent}</span> }.into_any();
    }
    cluster(
        values
            .iter()
            .map(|value| chip(value.clone()))
            .collect_view(),
    )
    .into_any()
}

/// The filters `$expand` accepts for one version, with their operators.
fn filter_view(row: &VersionRow) -> AnyView {
    let body = if row.filters.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::EXPAND,
                tr(Msg::NoFilter),
                tr(Msg::VersionDeclaresNoExpandFilter),
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            row.filters
                .iter()
                .map(|filter| {
                    let code = if filter.code.is_empty() {
                        tr_with(Msg::Filter2, &[("not_declared", not_declared().into())])
                    } else {
                        filter.code.clone()
                    };
                    let operators = chips(&filter.operators, tr(Msg::NoOperatorDeclared));
                    view! {
                        <tr>
                            <td class="fui-mono">{code}</td>
                            <td data-label=tr(Msg::Operators)>{operators}</td>
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: &tr(Msg::FiltersExpandAccepts),
        columns: &filter_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// How many published resources answered, as a sentence.
///
/// The count the server declared is shown beside the number of resources the
/// answer actually carried, because a `Bundle.total` larger than the entries
/// means the search was paged.
fn match_sentence(drawn: usize, total: Option<u32>) -> String {
    let counted = total.map_or_else(
        || {
            tr_with(
                Msg::ThisRootCounted3,
                &[("not_declared", not_declared().into())],
            )
        },
        |total| {
            tr_with(
                Msg::ThisRootCounted4,
                &[("total", (total).to_string().into())],
            )
        },
    );
    if drawn == 1 {
        tr_with(
            Msg::N1PublishedResourceAnd,
            &[("counted", (counted).clone().into())],
        )
    } else {
        tr_with(
            Msg::PublishedResourcesAnd,
            &[
                ("drawn", (drawn).to_string().into()),
                ("counted", (counted).clone().into()),
            ],
        )
    }
}

/// The sentence for a search the server counted more matches for than it
/// sent, which means it was paged; nothing when the two agree.
fn partial(drawn: usize, total: Option<u32>) -> Option<String> {
    let agrees = total.is_some_and(|total| usize::try_from(total).ok() == Some(drawn));
    (!agrees).then(|| match_sentence(drawn, total))
}

/// One panel per published resource, or the statement that there is none.
fn resources_view(published: &[PublishedCodeSystem]) -> AnyView {
    if published.is_empty() {
        return panel(empty_state(
            icon::CODE_SYSTEMS,
            tr(Msg::NotPublishedResource),
            tr(Msg::RootPublishesNoCodesystemResourceCanonical),
            (),
        ))
        .into_any();
    }
    let panels: Vec<AnyView> = published.iter().map(resource_view).collect();
    grid(panels).into_any()
}

/// One published resource, as a definition grid of its declared facts.
fn resource_view(resource: &PublishedCodeSystem) -> AnyView {
    let heading = resource.version().map_or_else(
        || tr_with(Msg::Version5, &[("not_declared", not_declared().into())]),
        |version| tr_with(Msg::Version6, &[("version", (version).to_string().into())]),
    );
    let rows = resource
        .facts()
        .into_iter()
        .map(|fact| {
            let value = match fact.value {
                Some(value) => crate::components::stated(value),
                None => view! { <span class="fui-muted">{not_declared()}</span> }.into_any(),
            };
            (tr(fact.label), value)
        })
        .collect();
    titled_panel(
        heading,
        (),
        view! { <div class="fui-panel-body">{definitions(rows)}</div> },
    )
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_that_names_no_system_still_titles_the_page() {
        assert_eq!(title_of(""), "Code system");
        assert_eq!(
            heading_of(""),
            "This address names no code system",
            "the screen states what it found rather than rendering an empty heading"
        );
    }

    #[test]
    fn a_named_system_titles_the_page_with_its_canonical() {
        assert_eq!(
            title_of("https://terminology.example/x"),
            "https://terminology.example/x"
        );
        assert_eq!(
            heading_of("https://terminology.example/x"),
            "https://terminology.example/x"
        );
    }

    #[test]
    fn the_match_sentence_states_the_drawn_count_and_the_declared_one() {
        assert_eq!(
            match_sentence(1, Some(1)),
            "1 published resource, and this root counted 1"
        );
        assert_eq!(
            match_sentence(0, Some(0)),
            "0 published resources, and this root counted 0"
        );
    }

    #[test]
    fn a_search_that_counted_nothing_says_so_rather_than_inventing_a_number() {
        assert_eq!(
            match_sentence(1, None),
            "1 published resource, and this root counted not declared"
        );
    }
}
