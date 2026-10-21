// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! One value set: what its definition selects, the codes it expands to, and
//! the resource as the server sent it.

use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::{empty_quiet, empty_state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    Crumb, Header, chip, definitions, facts_row, page_heading, panel, stack, titled_panel,
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
use crate::fhir::error::FhirError;
use crate::fhir::expansion::{ExpandRequest, ExpandedValueSet};
use crate::fhir::value_set::{ClauseRow, PublishedValueSet};
use crate::fhir::version::FhirVersion;
use crate::fhir::{Document, FhirClient, VALUE_SET};
use crate::listing::status_pill;
use crate::locale::{Msg, tr, tr_with};
use crate::routes::{editing_link, expansion_link, history_link};

/// The tabs of the screen.
const TABS: &[(&str, Msg)] = &[
    ("overview", Msg::Definition),
    ("expansion", Msg::Expansion),
    ("json", Msg::Json),
];

/// How many codes the expansion tab shows before it leads to the runner.
const PREVIEW_CODES: u32 = 25;

/// The columns of the definition's clause table.
fn clause_columns() -> [Column; 3] {
    [
        Column::text("clause", tr(Msg::Clause)),
        Column::text("system", tr(Msg::CodeSystem2)),
        Column::text("selects", tr(Msg::Selects)),
    ]
}

/// The columns of the expansion preview.
fn code_columns() -> [Column; 3] {
    [
        Column::text("code", tr(Msg::Code)),
        Column::text("display", tr(Msg::Display)),
        Column::text("system", tr(Msg::System)),
    ]
}

/// The path parameter this screen is addressed by.
#[derive(Clone, Debug, Params, PartialEq)]
struct ValueSetParams {
    /// The logical id, which the router percent-decodes on read.
    id: Option<String>,
}

/// A read the screen makes.
type Fetched<T> = Resource<Result<T, FhirError>>;

/// Shows one value set, read by its id.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ValueSetPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let params = use_params::<ValueSetParams>();
    let id = Signal::derive(move || {
        params
            .read()
            .as_ref()
            .ok()
            .and_then(|params| params.id.clone())
            .unwrap_or_default()
    });
    let read = {
        let client = client.clone();
        Resource::new(
            move || (version.get(), id.get()),
            move |(version, id)| {
                let client = client.clone();
                async move { client.value_set_read(version, &id).await }
            },
        )
    };
    let tab = current_tab(TABS, "overview");
    let header = move || header(id, version, read);
    let body = move || match tab.get() {
        "expansion" => expansion_tab(&client, version, read),
        "json" => json_tab(read),
        _ => overview_tab(read),
    };
    view! {
        <Title text=move || tr_with(Msg::ValueSet3, &[("id", (id.get()).clone().into())]) />
        {stack(
            view! {
                <Transition fallback=move || { heading(id.get(), None, ()) }>{header}</Transition>
                {detail_tabs(tr(Msg::ViewsValueSet), TABS, tab)}
                {body}
            },
        )}
    }
}

/// The page heading: the trail, the title, its subtitle and the actions.
fn heading(title: String, subtitle: Option<String>, actions: impl IntoView) -> AnyView {
    view! {
        <div>
            {page_heading(
                Header {
                    title,
                    subtitle,
                    crumbs: vec![Crumb::new(tr(Msg::ValueSets), "/valuesets")],
                    mono: false,
                },
                actions,
            )}
        </div>
    }
    .into_any()
}

/// The header once the read answered: the name, the facts, the actions.
fn header(
    id: Signal<String>,
    version: Signal<FhirVersion>,
    read: Fetched<Document<PublishedValueSet>>,
) -> AnyView {
    let resource = read.with(|answered| {
        answered
            .as_ref()
            .and_then(|answer| answer.as_ref().ok())
            .map(|document| document.value.clone())
    });
    let Some(resource) = resource else {
        return heading(id.get(), None, ());
    };
    let title = resource
        .label()
        .or_else(|| resource.url())
        .map_or_else(|| id.get(), str::to_owned);
    let subtitle = resource.description().map(str::to_owned);
    let at = version.get();
    let canonical = resource.url().map(str::to_owned);
    let expand = canonical.clone().map(|canonical| {
        view! {
            <a href=expansion_link(&canonical, at) class=Variant::Primary.class()>
                {ferrohealth_ui::icon::icon(icon::EXPAND, "16")}
                {tr(Msg::Expand)}
            </a>
        }
    });
    let console = expect_context::<Console>();
    let edit = console
        .sign_in
        .then(|| {
            editing_link(
                VALUE_SET,
                canonical.as_deref().unwrap_or_default(),
                &id.get(),
                at,
            )
        })
        .flatten()
        .map(|href| {
            view! {
                <a href=href class=Variant::Secondary.class()>
                    {ferrohealth_ui::icon::icon(icon::EDIT, "16")}
                    {tr(Msg::Compose)}
                </a>
            }
        });
    let history = view! {
        <a href=history_link(VALUE_SET, &id.get(), at) class=Variant::Secondary.class()>
            {ferrohealth_ui::icon::icon(ferrohealth_ui::icon::Glyph::FILE_CLOCK, "16")}
            {tr(Msg::Versions)}
        </a>
    };
    let actions = view! { <div class="fui-cluster">{expand} {edit} {history}</div> };
    let facts = facts_row(vec![
        (
            tr(Msg::Canonical),
            view! { <span class="fui-mono fui-break">{canonical.unwrap_or_else(|| String::from("—"))}</span> }
                .into_any(),
        ),
        (
            tr(Msg::Version),
            view! { <span class="fui-mono">{resource.version().unwrap_or("—").to_owned()}</span> }
                .into_any(),
        ),
        (tr(Msg::Status), status_pill(resource.status())),
        (tr(Msg::Clauses), resource.clauses().len().to_string().into_any()),
    ]);
    view! {
        {heading(title, subtitle, actions)}
        {panel(facts)}
    }
    .into_any()
}

/// The facts the resource declares and the clauses its definition holds.
fn overview_tab(read: Fetched<Document<PublishedValueSet>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingTheValueSet)
        }>
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(document) => definition_view(&document.value),
                            Err(error) => failure_view(error),
                        })
                })
            }}
        </Reading>
    }
    .into_any()
}

/// The resource's facts and its definition.
fn definition_view(resource: &PublishedValueSet) -> AnyView {
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
    let facts = titled_panel(
        tr(Msg::WhatItDeclares),
        (),
        view! { <div class="fui-panel-body">{definitions(rows)}</div> },
    );
    let clauses = titled_panel(
        tr(Msg::WhatTheDefinitionSelects),
        (),
        clauses_view(&resource.clauses()),
    );
    view! {
        {clauses}
        {facts}
    }
    .into_any()
}

/// What the definition selects, one clause a row.
fn clauses_view(clauses: &[ClauseRow]) -> AnyView {
    let body = if clauses.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::VALUE_SETS,
                tr(Msg::NoComposeElement),
                tr(Msg::ContentWhateverServerHoldsNotSelection),
            )
            .into_any(),
        )
    } else {
        Body::Rows(clauses.iter().map(clause_row).collect_view().into_any())
    };
    data_table(DataTable {
        caption: &tr(Msg::WhatTheDefinitionSelects),
        columns: &clause_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// One clause, with the word that says whether it draws codes in or out.
fn clause_row(clause: &ClauseRow) -> AnyView {
    let kind = if clause.included {
        badge(Tone::Ok, "include").into_any()
    } else {
        badge(Tone::Bad, "exclude").into_any()
    };
    let system = match (clause.system.as_deref(), clause.version.as_deref()) {
        (Some(system), Some(version)) => format!("{system}|{version}"),
        (Some(system), None) => system.to_owned(),
        (None, _) => String::from("—"),
    };
    view! {
        <tr>
            <td>{kind}</td>
            <td data-label=tr(Msg::CodeSystem2) class="fui-mono fui-break">
                {system}
            </td>
            <td data-label=tr(Msg::Selects)>{selection_view(clause)}</td>
        </tr>
    }
    .into_any()
}

/// What one clause selects, as the lines the publisher wrote.
fn selection_view(clause: &ClauseRow) -> AnyView {
    let mut lines: Vec<AnyView> = Vec::new();
    if clause.codes > 0 {
        lines.push(
            match clause.codes {
                1 => tr(Msg::N1CodeNamedOneOne),
                codes => tr_with(
                    Msg::CodesNamedOneOne,
                    &[("codes", (codes).to_string().into())],
                ),
            }
            .into_any(),
        );
    }
    lines.extend(
        clause
            .filters
            .iter()
            .map(|filter| chip(filter.clone()).into_any()),
    );
    lines.extend(clause.value_sets.iter().map(|url| {
        view! {
            {tr(Msg::EveryCodeIn)}
            <span class="fui-mono fui-break">{url.clone()}</span>
        }
        .into_any()
    }));
    if lines.is_empty() {
        return tr(Msg::TheWholeCodeSystem).into_any();
    }
    view! { <div class="fui-stack">{lines}</div> }.into_any()
}

/// The first codes the value set expands to, from this root's `$expand`.
fn expansion_tab(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    read: Fetched<Document<PublishedValueSet>>,
) -> AnyView {
    let client = client.clone();
    // NOTE: Leptos book, `async/10_resources`: a resource that needs another's
    // answer awaits it, so the server renders the expansion and not its absence.
    let expansion = Resource::new(
        move || version.get(),
        move |version| {
            let client = client.clone();
            async move {
                let canonical = read
                    .await
                    .ok()
                    .and_then(|document| document.value.url().map(str::to_owned));
                match canonical {
                    Some(url) => Some((
                        url.clone(),
                        client
                            .expand(
                                version,
                                &ExpandRequest {
                                    url,
                                    count: Some(PREVIEW_CODES),
                                    ..ExpandRequest::default()
                                },
                            )
                            .await,
                    )),
                    None => None,
                }
            }
        },
    );
    let drawn = move || {
        let at = version.get();
        expansion.with(|answered| {
            answered.as_ref().map(|answer| match answer {
                None => panel(empty_state(
                    icon::EXPAND,
                    tr(Msg::NoCanonical),
                    tr(Msg::ResourceDeclaresNoCanonicalSoThere),
                    (),
                ))
                .into_any(),
                Some((canonical, Ok(expanded))) => preview(expanded, Some(canonical.clone()), at),
                Some((_, Err(error))) => failure_view(error),
            })
        })
    };
    view! { <Reading label=&{ tr(Msg::ExpandingTheValueSet) }>{drawn}</Reading> }.into_any()
}

/// The preview table of an expansion, with the way into the runner.
fn preview(
    expanded: &ExpandedValueSet,
    canonical: Option<String>,
    version: FhirVersion,
) -> AnyView {
    let Some(expansion) = expanded.expansion() else {
        return panel(empty_state(
            icon::EXPAND,
            tr(Msg::NoExpansion),
            tr(Msg::ServerAnsweredWithoutExpansion),
            (),
        ))
        .into_any();
    };
    let listed = expansion.listed();
    let total = expansion.total.map_or_else(
        || tr_with(Msg::CodesShown, &[("listed", (listed).to_string().into())]),
        |total| {
            tr_with(
                Msg::OfCodes,
                &[
                    ("listed", (listed).to_string().into()),
                    ("total", (total).to_string().into()),
                ],
            )
        },
    );
    let body = if expansion.concepts.is_empty() {
        Body::Empty(
            empty_quiet(icon::EXPAND, tr(Msg::NoCode), tr(Msg::ExpansionListsNoCode)).into_any(),
        )
    } else {
        Body::Rows(
            expansion
                .concepts
                .iter()
                .map(|concept| {
                    let display = concept.display.clone().unwrap_or_default();
                    view! {
                        <tr>
                            <td class="fui-mono">{concept.code.clone()}</td>
                            <td data-label=tr(Msg::Display)>{display}</td>
                            <td data-label=tr(Msg::System) class="fui-mono fui-small fui-muted">
                                {concept.system.clone()}
                            </td>
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    let all = canonical.map(|canonical| {
        view! {
            <a
                href=expansion_link(&canonical, version)
                class="fui-button fui-button-ghost fui-button-small"
            >
                {tr(Msg::OpenInTheRunner)}
            </a>
        }
    });
    let table = data_table(DataTable {
        caption: &tr(Msg::CodesValueSetExpands),
        columns: &code_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body,
    });
    let aside = view! {
        <span class="fui-cluster">
            <span class="fui-small fui-muted">{total}</span>
            {all}
        </span>
    };
    view! { {titled_panel(tr(Msg::TheExpansion), aside, table)} }.into_any()
}

/// The resource as the server sent it.
fn json_tab(read: Fetched<Document<PublishedValueSet>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingTheValueSet)
        }>
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(document) => {
                                json_pane(
                                    "valueset-json",
                                    tr(Msg::ValuesetServerSent),
                                    document.text.clone(),
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

/// A refused read, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    view! { <Failure error=Signal::stored(error) /> }.into_any()
}
