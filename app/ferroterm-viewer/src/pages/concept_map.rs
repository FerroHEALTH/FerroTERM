// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! One concept map: what it declares, every mapping it writes, and the
//! resource as the server sent it.

use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    Crumb, Header, definitions, facts_row, page_heading, panel, stack, titled_panel,
};
use ferrohealth_ui::widget::filter::{keeps, table_filter};
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
use crate::fhir::concept_map::{GroupRow, MappingRow, PublishedConceptMap};
use crate::fhir::error::FhirError;
use crate::fhir::translate::TranslateRequest;
use crate::fhir::version::FhirVersion;
use crate::fhir::{CONCEPT_MAP, Document, FhirClient};
use crate::listing::status_pill;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::translate;
use crate::routes::{editing_link, history_link};

/// The tabs of the screen.
const TABS: &[(&str, Msg)] = &[
    ("overview", Msg::Definition),
    ("mappings", Msg::Mappings),
    ("json", Msg::Json),
];

/// The columns of the group table.
fn group_columns() -> [Column; 3] {
    [
        Column::text("source", tr(Msg::From)),
        Column::text("target", tr(Msg::To)),
        Column::text("codes", tr(Msg::CodesMapped)).numeric(),
    ]
}

/// The columns of the mapping table.
fn mapping_columns() -> [Column; 3] {
    [
        Column::text("source", tr(Msg::SourceCode)),
        Column::text("relationship", tr(Msg::Relationship)),
        Column::text("target", tr(Msg::TargetCode)),
    ]
}

/// The path parameter this screen is addressed by.
#[derive(Clone, Debug, Params, PartialEq)]
struct ConceptMapParams {
    /// The logical id, which the router percent-decodes on read.
    id: Option<String>,
}

/// A read the screen makes.
type Fetched<T> = Resource<Result<T, FhirError>>;

/// Shows one concept map, read by its id.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ConceptMapPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let params = use_params::<ConceptMapParams>();
    let id = Signal::derive(move || {
        params
            .read()
            .as_ref()
            .ok()
            .and_then(|params| params.id.clone())
            .unwrap_or_default()
    });
    let read = Resource::new(
        move || (version.get(), id.get()),
        move |(version, id)| {
            let client = client.clone();
            async move { client.concept_map_read(version, &id).await }
        },
    );
    let tab = current_tab(TABS, "overview");
    let header = move || header(id, version, read);
    let body = move || match tab.get() {
        "mappings" => mappings_tab(read),
        "json" => json_tab(read),
        _ => overview_tab(read),
    };
    view! {
        <Title text=move || tr_with(Msg::ConceptMap3, &[("id", (id.get()).clone().into())]) />
        {stack(
            view! {
                <Transition fallback=move || heading(id.get(), None, ())>{header}</Transition>
                {detail_tabs(tr(Msg::ViewsConceptMap), TABS, tab)}
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
                    crumbs: vec![Crumb::new(tr(Msg::ConceptMaps), "/conceptmaps")],
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
    read: Fetched<Document<PublishedConceptMap>>,
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
    // NOTE: the runner is handed the map and the one source system it names,
    // so a reader never retypes a canonical this screen is showing them.
    let run = canonical.as_ref().map(|canonical| {
        let filled = TranslateRequest {
            concept_map: canonical.clone(),
            concept_map_version: resource.version().unwrap_or_default().to_owned(),
            system: resource.only_source_system().unwrap_or_default(),
            ..TranslateRequest::default()
        };
        view! {
            <a href=translate::address(&filled, at) class=Variant::Primary.class()>
                {ferrohealth_ui::icon::icon(icon::TRANSLATE, "16")}
                {tr(Msg::TranslateACode)}
            </a>
        }
    });
    let edit = expect_context::<Console>()
        .sign_in
        .then(|| {
            editing_link(
                CONCEPT_MAP,
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
                    {tr(Msg::Edit)}
                </a>
            }
        });
    let history = view! {
        <a href=history_link(CONCEPT_MAP, &id.get(), at) class=Variant::Secondary.class()>
            {ferrohealth_ui::icon::icon(ferrohealth_ui::icon::Glyph::FILE_CLOCK, "16")}
            {tr(Msg::Versions)}
        </a>
    };
    let actions = view! { <div class="fui-cluster">{run} {edit} {history}</div> };
    let scope = |scope: Option<&str>| {
        view! { <span class="fui-mono break-all">{scope.unwrap_or("—").to_owned()}</span> }
            .into_any()
    };
    let facts = facts_row(vec![
        (tr(Msg::Canonical), scope(resource.url())),
        (tr(Msg::Version), scope(resource.version())),
        (tr(Msg::Status), status_pill(resource.status())),
        (
            tr(Msg::Mappings),
            resource.mappings().len().to_string().into_any(),
        ),
    ]);
    view! {
        {heading(title, subtitle, actions)}
        {panel(facts)}
    }
    .into_any()
}

/// The facts the map declares and the groups it is made of.
fn overview_tab(read: Fetched<Document<PublishedConceptMap>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingTheConceptMap)
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

/// The map's groups and its facts.
fn definition_view(resource: &PublishedConceptMap) -> AnyView {
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
    view! {
        {titled_panel(tr(Msg::WhatTheMapMaps), (), groups_view(&resource.groups()))}
        {titled_panel(
            tr(Msg::WhatItDeclares),
            (),
            view! { <div class="fui-panel-body">{definitions(rows)}</div> },
        )}
    }
    .into_any()
}

/// One group a row: the system pair and how many codes it maps.
fn groups_view(groups: &[GroupRow]) -> AnyView {
    let body = if groups.is_empty() {
        Body::Empty(
            empty_quiet(
                icon::CONCEPT_MAPS,
                tr(Msg::NoGroup),
                tr(Msg::WhatMapsWhateverServerHoldsNot),
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            groups
                .iter()
                .map(|group| {
                    view! {
                        <tr>
                            <td class="fui-mono break-all">
                                {group.source.clone().unwrap_or_else(|| String::from("—"))}
                            </td>
                            <td data-label=tr(Msg::To) class="fui-mono break-all">
                                {group.target.clone().unwrap_or_else(|| String::from("—"))}
                            </td>
                            <td data-label=tr(Msg::CodesMapped) class="fui-num">
                                {group.elements.to_string()}
                            </td>
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: &tr(Msg::WhatTheMapMaps),
        columns: &group_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// Every mapping the map writes, with a filter over them.
fn mappings_tab(read: Fetched<Document<PublishedConceptMap>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingTheConceptMap)
        }>
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(document) => mappings_view(document.value.mappings()),
                            Err(error) => failure_view(error),
                        })
                })
            }}
        </Reading>
    }
    .into_any()
}

/// The mappings `rows`, with the filter that narrows them.
///
/// The rows arrive as a value read inside the boundary, so the filter's
/// signals are the page's own and never read the resource
/// (Leptos book, `async/11_suspense`).
fn mappings_view(rows: Vec<MappingRow>) -> AnyView {
    let words = RwSignal::new(String::new());
    let held = StoredValue::new(rows);
    let kept = Memo::new(move |_| {
        let words = words.get();
        held.with_value(|rows| {
            rows.iter()
                .filter(|row| {
                    keeps(
                        &words,
                        &[
                            row.source_code.as_str(),
                            row.source_display.as_deref().unwrap_or_default(),
                            row.target_code.as_deref().unwrap_or_default(),
                            row.target_display.as_deref().unwrap_or_default(),
                        ],
                    )
                })
                .cloned()
                .collect::<Vec<MappingRow>>()
        })
    });
    let count = Signal::derive(move || {
        let all = held.with_value(Vec::len);
        let shown = kept.with(Vec::len);
        if all == shown {
            tr_with(Msg::Mappings2, &[("all", (all).to_string().into())])
        } else {
            tr_with(
                Msg::OfMappings,
                &[
                    ("shown", (shown).to_string().into()),
                    ("all", (all).to_string().into()),
                ],
            )
        }
    });
    let table = move || {
        let body = kept.with(|kept| {
            if kept.is_empty() {
                Body::Empty(
                    empty_quiet(
                        icon::CONCEPT_MAPS,
                        tr(Msg::NoMapping),
                        tr(Msg::NoMappingMatches),
                    )
                    .into_any(),
                )
            } else {
                Body::Rows(kept.iter().map(mapping_row).collect_view().into_any())
            }
        });
        data_table(DataTable {
            caption: &tr(Msg::EveryMappingMapWrites),
            columns: &mapping_columns(),
            sort: None,
            sort_href: &|_| String::new(),
            body,
        })
    };
    view! {
        {table_filter("mappings-filter", tr(Msg::FilterCodeDisplay), words, count)}
        {table}
    }
    .into_any()
}

/// The tone a relationship code is drawn in, the same on every screen: a
/// code that maps to the same meaning, one that maps to none, and the rest.
pub(crate) fn relationship_tone(code: Option<&str>) -> Tone {
    match code {
        Some("equivalent" | "equal") => Tone::Ok,
        Some("not-related-to" | "unmatched" | "disjoint") => Tone::Bad,
        Some(_) => Tone::Info,
        None => Tone::Neutral,
    }
}

/// One mapping: the source code, how it relates, and the target code.
fn mapping_row(row: &MappingRow) -> AnyView {
    let tone = relationship_tone(row.relationship.as_deref());
    let relationship = row
        .relationship
        .clone()
        .unwrap_or_else(|| String::from("—"));
    let code = |code: Option<String>, display: Option<String>, system: Option<String>| {
        view! {
            <span class="fui-mono">{code.unwrap_or_else(|| tr(Msg::NoTarget))}</span>
            <span class="block">{display.unwrap_or_default()}</span>
            <span class="block font-normal fui-small fui-muted fui-mono sm:whitespace-nowrap">
                {system.unwrap_or_default()}
            </span>
        }
    };
    view! {
        <tr>
            <td>
                {code(
                    Some(row.source_code.clone()),
                    row.source_display.clone(),
                    row.source_system.clone(),
                )}
            </td>
            <td data-label=tr(Msg::Relationship)>{badge(tone, &relationship)}</td>
            <td data-label=tr(Msg::TargetCode)>
                <div>
                    {code(
                        row.target_code.clone(),
                        row.target_display.clone(),
                        row.target_system.clone(),
                    )}
                </div>
            </td>
        </tr>
    }
    .into_any()
}

/// The resource as the server sent it.
fn json_tab(read: Fetched<Document<PublishedConceptMap>>) -> AnyView {
    view! {
        <Reading label=&{
            tr(Msg::ReadingTheConceptMap)
        }>
            {move || {
                read.with(|answered| {
                    answered
                        .as_ref()
                        .map(|answer| match answer {
                            Ok(document) => {
                                json_pane(
                                    "conceptmap-json",
                                    tr(Msg::ConceptmapServerSent),
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
