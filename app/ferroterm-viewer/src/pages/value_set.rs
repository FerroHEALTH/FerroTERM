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

use crate::components::NOT_DECLARED;
use crate::components::detail::{current_tab, detail_tabs, json_pane};
use crate::components::failure::Failure;
use crate::components::icon;
use crate::components::reading::Reading;
use crate::components::shell::{Console, SelectedVersion};
use crate::fhir::error::FhirError;
use crate::fhir::expansion::{ExpandRequest, ExpandedValueSet};
use crate::fhir::value_set::{ClauseRow, PublishedValueSet};
use crate::fhir::version::FhirVersion;
use crate::fhir::{Document, FhirClient, VALUE_SET};
use crate::listing::status_pill;
use crate::routes::{editing_link, expansion_link, history_link};

/// The tabs of the screen.
const TABS: &[(&str, &str)] = &[
    ("overview", "Definition"),
    ("expansion", "Expansion"),
    ("json", "JSON"),
];

/// How many codes the expansion tab shows before it leads to the runner.
const PREVIEW_CODES: u32 = 25;

/// The columns of the definition's clause table.
const CLAUSE_COLUMNS: [Column; 3] = [
    Column::text("clause", "Clause"),
    Column::text("system", "Code system"),
    Column::text("selects", "Selects"),
];

/// The columns of the expansion preview.
const CODE_COLUMNS: [Column; 3] = [
    Column::text("code", "Code"),
    Column::text("display", "Display"),
    Column::text("system", "System"),
];

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
        <Title text=move || format!("Value set {}", id.get()) />
        {stack(
            view! {
                <Transition fallback=move || { heading(id.get(), None, ()) }>{header}</Transition>
                {detail_tabs("Views of this value set", TABS, tab)}
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
                    crumbs: vec![Crumb::new("Value sets", "/valuesets")],
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
                "Expand"
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
                    "Compose"
                </a>
            }
        });
    let history = view! {
        <a href=history_link(VALUE_SET, &id.get(), at) class=Variant::Secondary.class()>
            {ferrohealth_ui::icon::icon(ferrohealth_ui::icon::Glyph::FILE_CLOCK, "16")}
            "Versions"
        </a>
    };
    let actions = view! { <div class="fui-cluster">{expand} {edit} {history}</div> };
    let facts = facts_row(vec![
        (
            "Canonical",
            view! { <span class="fui-mono break-all">{canonical.unwrap_or_else(|| String::from("—"))}</span> }
                .into_any(),
        ),
        (
            "Version",
            view! { <span class="fui-mono">{resource.version().unwrap_or("—").to_owned()}</span> }
                .into_any(),
        ),
        ("Status", status_pill(resource.status())),
        ("Clauses", resource.clauses().len().to_string().into_any()),
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
        <Reading label="Reading the value set">
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
                None => view! { <span class="fui-muted">{NOT_DECLARED}</span> }.into_any(),
            };
            (fact.label, value)
        })
        .collect();
    let facts = titled_panel(
        "What it declares",
        (),
        view! { <div class="fui-panel-body">{definitions(rows)}</div> },
    );
    let clauses = titled_panel(
        "What the definition selects",
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
                "No compose element",
                "Its content is whatever the server holds for it, not a selection written here.",
            )
            .into_any(),
        )
    } else {
        Body::Rows(clauses.iter().map(clause_row).collect_view().into_any())
    };
    data_table(DataTable {
        caption: "What the definition selects",
        columns: &CLAUSE_COLUMNS,
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
            <td data-label="Code system" class="fui-mono break-all">
                {system}
            </td>
            <td data-label="Selects">{selection_view(clause)}</td>
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
                1 => String::from("1 code named one by one"),
                codes => format!("{codes} codes named one by one"),
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
            "every code in "
            <span class="fui-mono break-all">{url.clone()}</span>
        }
        .into_any()
    }));
    if lines.is_empty() {
        return "the whole code system".into_any();
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
                    "No canonical",
                    "This resource declares no canonical, so there is nothing to expand.",
                    (),
                ))
                .into_any(),
                Some((canonical, Ok(expanded))) => preview(expanded, Some(canonical.clone()), at),
                Some((_, Err(error))) => failure_view(error),
            })
        })
    };
    view! { <Reading label="Expanding the value set">{drawn}</Reading> }.into_any()
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
            "No expansion",
            "The server answered without an expansion.",
            (),
        ))
        .into_any();
    };
    let listed = expansion.listed();
    let total = expansion.total.map_or_else(
        || format!("{listed} codes shown"),
        |total| format!("{listed} of {total} codes"),
    );
    let body = if expansion.concepts.is_empty() {
        Body::Empty(empty_quiet(icon::EXPAND, "No code", "The expansion lists no code.").into_any())
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
                            <td data-label="Display">{display}</td>
                            <td
                                data-label="System"
                                class="fui-mono fui-small fui-muted sm:whitespace-nowrap"
                            >
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
                "Open in the runner"
            </a>
        }
    });
    let table = data_table(DataTable {
        caption: "The codes the value set expands to",
        columns: &CODE_COLUMNS,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    });
    let aside = view! {
        <span class="inline-flex items-center gap-default">
            <span class="fui-small fui-muted">{total}</span>
            {all}
        </span>
    };
    view! { {titled_panel("The expansion", aside, table)} }.into_any()
}

/// The resource as the server sent it.
fn json_tab(read: Fetched<Document<PublishedValueSet>>) -> AnyView {
    view! {
        <Reading label="Reading the value set">
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(document) => {
                                json_pane(
                                    "valueset-json",
                                    "The ValueSet, as the server sent it",
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
