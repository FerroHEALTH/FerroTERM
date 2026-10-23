// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard: what the server serves on the FHIR version in view, what
//! was written lately, and the first code systems.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::label;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    Stat, card_action, definitions, disclosure, grid_start, stack, stat_card_marked, stat_grid,
};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::template::list::{ListCard, list_card, list_empty};
use leptos::prelude::*;

use crate::fhir::activity::{Change, recent};
use crate::fhir::error::FhirError;
use crate::fhir::named::{Listed, NamedSearch};
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP, FhirClient, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::parts::{Source, failure, heading, loaded, relative, versions_word};
use crate::request::{Request, request};
use crate::routes::{
    CONCEPT_MAPS_PATH, SYSTEMS_PATH, VALUE_SETS_PATH, history_link, system_link, ui_link, ui_path,
};

/// How many code systems the dashboard's table shows before it leads to the
/// whole listing.
const PREVIEW_ROWS: usize = 6;

/// How many writes the activity reads, for its count.
const ACTIVITY_KEPT: usize = 500;

/// How many writes the activity lists.
const ACTIVITY_SHOWN: usize = 7;

/// Everything the dashboard reads, each read on its own so one refusal
/// never blanks the page.
struct Reads {
    /// The root's `TerminologyCapabilities`: the code systems it serves.
    capabilities: Result<TerminologyCapabilities, FhirError>,
    /// The code systems the root publishes as resources.
    systems: Result<NamedSearch, FhirError>,
    /// The value sets the root publishes.
    value_sets: Result<NamedSearch, FhirError>,
    /// The concept maps the root publishes.
    concept_maps: Result<NamedSearch, FhirError>,
    /// The newest writes of the three types.
    activity: Result<Vec<Change>, FhirError>,
}

impl Reads {
    /// Reads everything on `version`.
    async fn of(client: &FhirClient, version: FhirVersion) -> Self {
        Self {
            capabilities: client.terminology_capabilities(version).await,
            systems: client.published_names(version, CODE_SYSTEM).await,
            value_sets: client.published_names(version, VALUE_SET).await,
            concept_maps: client.published_names(version, CONCEPT_MAP).await,
            activity: activity(client, version).await,
        }
    }
}

/// The newest writes of the three types on `version`, merged newest first.
///
/// # Errors
///
/// Returns the first refused read: an activity missing one type would list
/// the others as if that type had none.
async fn activity(client: &FhirClient, version: FhirVersion) -> Result<Vec<Change>, FhirError> {
    let systems = client.type_history(version, CODE_SYSTEM).await?;
    let sets = client.type_history(version, VALUE_SET).await?;
    let maps = client.type_history(version, CONCEPT_MAP).await?;
    Ok(recent(
        &[
            (CODE_SYSTEM, &systems),
            (VALUE_SET, &sets),
            (CONCEPT_MAP, &maps),
        ],
        ACTIVITY_KEPT,
    ))
}

/// The dashboard.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn DashboardPage() -> impl IntoView {
    let request = request();
    loaded(async move {
        let reads = Reads::of(&request.client, request.version).await;
        dashboard(&request, &reads)
    })
}

/// The host part of the server's address, as the subtitle names it.
pub(crate) fn host(root: &str) -> String {
    url::Url::parse(root)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| root.to_owned())
}

/// A count as the page writes it, grouped in the page's language.
fn grouped(count: usize) -> String {
    ferrohealth_ui::i18n::number::grouped(u64::try_from(count).unwrap_or(u64::MAX))
}

/// The dashboard drawn from `reads`.
fn dashboard(request: &Request, reads: &Reads) -> AnyView {
    if let Err(error) = &reads.capabilities
        && error.status().is_none()
    {
        return unreachable(request, error);
    }
    let version = request.version;
    let answered_at = jiff::Zoned::now().strftime("%H:%M").to_string();
    let subtitle = tr_with(
        Msg::DashboardSubtitle,
        &[
            ("host", host(&request.root).into()),
            ("version", version.label().into()),
            ("time", answered_at.into()),
        ],
    );
    let header = heading(
        tr(Msg::ShellDashboard),
        Some(subtitle),
        Vec::new(),
        ActionGroup::none(),
    );
    let cards = reads
        .capabilities
        .as_ref()
        .map(TerminologyCapabilities::cards)
        .unwrap_or_default();
    let listed = |read: &Result<NamedSearch, FhirError>| -> Vec<Listed> {
        read.as_ref().map(NamedSearch::listed).unwrap_or_default()
    };
    let systems = listed(&reads.systems);
    let sets = listed(&reads.value_sets);
    let maps = listed(&reads.concept_maps);
    if reads.capabilities.is_ok() && cards.is_empty() && sets.is_empty() && maps.is_empty() {
        let instead = FhirVersion::default();
        let line = tr_with(
            Msg::DashboardServesNothing,
            &[("version", version.label().into())],
        );
        let instead_words = tr_with(
            Msg::DashboardReadInstead,
            &[("version", instead.label().into())],
        );
        return view! {
            {header}
            {list_empty(line, Some((instead_words, ui_link("", instead))))}
        }
        .into_any();
    }
    let stored_systems = cards
        .iter()
        .filter(|card| Source::of_system(card, &systems) == Source::Stored)
        .count();
    let stored = |resources: &[Listed]| {
        resources
            .iter()
            .filter(|resource| Source::of_resource(resource) == Source::Stored)
            .count()
    };
    let stats = stat_grid(view! {
        {counted_stat(
            Glyph::BOOK_OPEN,
            cards.len(),
            Msg::ShellCodeSystems,
            stored_systems,
            &ui_path(SYSTEMS_PATH),
        )}
        {counted_stat(
            Glyph::LAYERS,
            sets.len(),
            Msg::ShellValueSets,
            stored(&sets),
            &ui_path(VALUE_SETS_PATH),
        )}
        {counted_stat(
            Glyph::ARROW_LEFT_RIGHT,
            maps.len(),
            Msg::ShellConceptMaps,
            stored(&maps),
            &ui_path(CONCEPT_MAPS_PATH),
        )}
        {writes_stat(&reads.activity)}
    })
    .into_any();
    let recent = recent_card(&reads.activity, version);
    let preview = systems_card(request, &reads.capabilities, &systems);
    view! {
        {header}
        {stack(
            view! {
                {stats}
                {grid_start(
                    view! {
                        {recent}
                        {preview}
                    },
                )}
            },
        )}
    }
    .into_any()
}

/// A stat card of `count` items, marked with how many of them are `stored`.
fn counted_stat(glyph: Glyph, count: usize, label: Msg, stored: usize, href: &str) -> AnyView {
    let mark = tr_with(Msg::DashboardStored, &[("count", stored.into())]);
    stat_card_marked(
        Stat {
            glyph,
            value: grouped(count),
            label: tr(label).into(),
            href: Some(href.to_owned()),
        },
        badge(Tone::Neutral, mark),
    )
    .into_any()
}

/// The stat card of the recent writes, marked with the day the oldest of
/// them fell on.
fn writes_stat(activity: &Result<Vec<Change>, FhirError>) -> AnyView {
    let (value, mark) = match activity {
        Ok(changes) => {
            let since = changes.iter().filter_map(Change::day).min().map(|day| {
                let line = tr_with(Msg::DashboardSince, &[("day", day.to_owned().into())]);
                view! { <span class="fui-small fui-muted">{line}</span> }.into_any()
            });
            (grouped(changes.len()), since)
        }
        Err(_refused) => (String::from("\u{2013}"), None),
    };
    stat_card_marked(
        Stat {
            glyph: Glyph::ROTATE_CCW_CLOCK,
            value,
            label: tr(Msg::DashboardRecentWrites).into(),
            href: Some(String::from("#recent-changes")),
        },
        mark,
    )
    .into_any()
}

/// The words of a resource type as the recent changes name it.
pub(crate) fn type_word(resource_type: &str) -> String {
    match resource_type {
        CODE_SYSTEM => tr(Msg::TypeCodeSystem),
        VALUE_SET => tr(Msg::TypeValueSet),
        CONCEPT_MAP => tr(Msg::TypeConceptMap),
        other => other.to_owned(),
    }
}

/// One write as a row: what it was made on, the interaction, and when.
fn change_row(change: &Change, columns: &[Column; 3], version: FhirVersion) -> AnyView {
    let now = jiff::Timestamp::now();
    let named = if change.named.is_empty() {
        format!("{}/{}", change.resource_type, change.id)
    } else {
        change.named.clone()
    };
    let sub = match &change.version_id {
        Some(id) => tr_with(
            Msg::DashboardTypeVersion,
            &[
                ("type", type_word(&change.resource_type).into()),
                ("version", id.clone().into()),
            ],
        ),
        None => type_word(&change.resource_type),
    };
    let interaction = match change.method.as_deref() {
        Some("POST") => tr(Msg::DashboardCreated),
        Some("DELETE") => tr(Msg::DashboardDeleted),
        _ => tr(Msg::DashboardUpdated),
    };
    let href = history_link(&change.resource_type, &change.id, version);
    let when = change.when.as_deref().map(|raw| relative(raw, now));
    let [_, change_column, when_column] = columns;
    view! {
        <tr>
            {title_cell(Some(&href), named, Some(sub.into_any()))}
            {cell(change_column, interaction)} {cell(when_column, when)}
        </tr>
    }
    .into_any()
}

/// The card of the recent writes, each row opening its resource's history.
fn recent_card(activity: &Result<Vec<Change>, FhirError>, version: FhirVersion) -> AnyView {
    let columns = [
        Column::text("resource", tr(Msg::DashboardResource)).role(Role::Title),
        Column::text("change", tr(Msg::DashboardChange)).role(Role::Meta),
        Column::text("when", tr(Msg::DashboardWhen)).role(Role::Meta),
    ];
    let caption = tr(Msg::DashboardRecentChanges);
    let (body, count) = match activity {
        Ok(changes) if changes.is_empty() => (
            list_empty(tr(Msg::DashboardNothingWritten), None),
            Some(shown_of(0, 0)),
        ),
        Ok(changes) => {
            let rows = changes
                .iter()
                .take(ACTIVITY_SHOWN)
                .map(|change| change_row(change, &columns, version))
                .collect_view()
                .into_any();
            let table = data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            })
            .into_any();
            (
                table,
                Some(shown_of(changes.len().min(ACTIVITY_SHOWN), changes.len())),
            )
        }
        Err(error) => (failure(error), None),
    };
    view! {
        <div id="recent-changes">
            {list_card(ListCard {
                title: caption.clone(),
                count,
                order: None,
                tabs: None,
                actions: None,
                body,
                pager: None,
            })}
        </div>
    }
    .into_any()
}

/// A card's count: every item when it shows all `total`, else the `shown`
/// first of them.
fn shown_of(shown: usize, total: usize) -> HeadingCount {
    let number = |count: usize| u64::try_from(count).unwrap_or(u64::MAX);
    if shown < total {
        HeadingCount::Shown {
            shown: number(shown),
            total: number(total),
        }
    } else {
        HeadingCount::All(number(total))
    }
}

/// The card of the first code systems, leading to the whole listing.
fn systems_card(
    request: &Request,
    capabilities: &Result<TerminologyCapabilities, FhirError>,
    listed: &[Listed],
) -> AnyView {
    let columns = [
        Column::text("system", tr(Msg::DashboardCodeSystem)).role(Role::Title),
        Column::text("source", tr(Msg::DashboardSource)).role(Role::Meta),
        Column::text("versions", tr(Msg::DashboardVersions)).role(Role::Meta),
    ];
    let caption = tr(Msg::ShellCodeSystems);
    let (body, count) = match capabilities {
        Ok(capabilities) => {
            let cards = capabilities.cards();
            let names: BTreeMap<&str, &str> = listed
                .iter()
                .map(|resource| (resource.url.as_str(), resource.label.as_str()))
                .collect();
            let [_, source_column, versions_column] = &columns;
            let rows = cards
                .iter()
                .take(PREVIEW_ROWS)
                .map(|card| {
                    let name = names
                        .get(card.url.as_str())
                        .map_or_else(|| card.url.clone(), |name| (*name).to_owned());
                    let versions: Vec<String> = card
                        .versions
                        .iter()
                        .filter_map(|row| row.code.clone())
                        .collect();
                    let href = system_link(&card.url, request.version);
                    view! {
                        <tr>
                            {title_cell(Some(&href), name, None)}
                            {cell(source_column, Source::of_system(card, listed).tag())}
                            {cell(versions_column, versions_word(&versions))}
                        </tr>
                    }
                })
                .collect_view()
                .into_any();
            let table = data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            })
            .into_any();
            (
                table,
                Some(shown_of(cards.len().min(PREVIEW_ROWS), cards.len())),
            )
        }
        Err(error) => (failure(error), None),
    };
    list_card(ListCard {
        title: caption,
        count,
        order: None,
        tabs: None,
        actions: Some(card_action(
            ui_link(SYSTEMS_PATH, request.version),
            tr(Msg::DashboardAllCodeSystems),
        )),
        body,
        pager: None,
    })
}

/// The dashboard of a server that did not answer: the refusal in plain
/// words, Try again, and the detail behind a disclosure.
fn unreachable(request: &Request, error: &FhirError) -> AnyView {
    let header = heading(
        tr(Msg::ShellDashboard),
        Some(tr_with(
            Msg::DashboardSubtitleUnanswered,
            &[
                ("host", host(&request.root).into()),
                ("version", request.version.label().into()),
            ],
        )),
        Vec::new(),
        ActionGroup::none(),
    );
    let detail = definitions(vec![
        (tr(Msg::DashboardFailure), error.to_string().into_any()),
        (
            tr(Msg::DashboardAddress),
            view! { <code>{error.url().to_owned()}</code> }.into_any(),
        ),
    ]);
    view! {
        {header}
        {stack(
            view! {
                {failure(error)}
                <p>
                    <a href=request.back() class="fui-button fui-button-primary">
                        {label(Some(Glyph::REFRESH_CW), tr(Msg::DashboardTryAgain))}
                    </a>
                </p>
                <p class="fui-small fui-muted">{tr(Msg::DashboardKeepsNoCopy)}</p>
                {disclosure(tr(Msg::DashboardTechnicalDetail), detail)}
            },
        )}
    }
    .into_any()
}
