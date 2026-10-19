//! The dashboard: what this server serves on the FHIR version in view, what
//! was written lately, and the way into each operation.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::chart::{ActivityChart, activity_chart};
use ferrohealth_ui::view::empty::{empty_quiet, skeleton};
use ferrohealth_ui::view::layout::{divided_list, main_aside};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    Stat, card, link_card_glyph, page_header, page_meta, panel_body, stack, stat_card, stat_grid,
    titled_panel,
};
use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::failure::Failure;
use crate::components::icon;
use crate::components::shell::SelectedVersion;
use crate::fhir::activity::{Change, per_day, recent};
use crate::fhir::capability::CapabilityStatement;
use crate::fhir::error::FhirError;
use crate::fhir::named::NamedSearch;
use crate::fhir::terminology::{SystemCard, TerminologyCapabilities};
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP, FhirClient, VALUE_SET};
use crate::routes::{history_link, system_link, ui_link};

/// How many code systems the dashboard's table shows before it leads to the
/// whole listing.
const PREVIEW_ROWS: usize = 6;

/// How many writes the activity keeps, for its count and its chart.
const ACTIVITY_KEPT: usize = 500;

/// How many writes the activity lists.
const ACTIVITY_SHOWN: usize = 6;

/// The columns of the code system preview.
const PREVIEW_COLUMNS: [Column; 3] = [
    Column::text("system", "Code system"),
    Column::text("version", "Default version"),
    Column::text("content", "Content"),
];

/// The four operations: the screen that runs each, its icon, its name, and
/// what it is for.
const OPERATIONS: [(&str, Glyph, &str, &str); 4] = [
    (
        "lookup",
        icon::LOOKUP,
        "Look up a code",
        "Read a code's display, designations and properties in its system.",
    ),
    (
        "validate",
        icon::VALIDATE,
        "Check a code",
        "Ask whether a code is in a code system or a value set.",
    ),
    (
        "expand",
        icon::EXPAND,
        "Expand a value set",
        "List the codes a value set holds, a page at a time.",
    ),
    (
        "translate",
        icon::TRANSLATE,
        "Translate a code",
        "Follow a code through the concept maps this server holds.",
    ),
];

/// A read the dashboard makes.
type Fetched<T> = Resource<Result<T, FhirError>>;

/// Shows the dashboard.
///
/// Every read refetches when the version switcher moves, so each is read
/// under a transition boundary
/// (<https://github.com/leptos-rs/book/blob/main/src/async/12_transition.md>),
/// and each section draws its own failure, so one refused read never blanks
/// the screen.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn OverviewPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();

    let reads = Reads::new(&client, version);
    let header = header(&client, version, reads.statement);
    let stats = stats(reads);
    let systems = systems_panel(reads, version);
    let activity = activity_panel(reads.activity, version);
    let operations = operations_card(version);

    view! {
        <Title text="Dashboard" />
        {header}
        {stack(
            view! {
                {stats}
                {main_aside(systems, activity)}
                {operations}
            },
        )}
    }
}

/// The reads the dashboard makes, each on the version in view.
#[derive(Clone, Copy)]
struct Reads {
    /// The root's `CapabilityStatement`, for the line under the title.
    statement: Fetched<CapabilityStatement>,
    /// The root's `TerminologyCapabilities`: the code systems it serves.
    capabilities: Fetched<TerminologyCapabilities>,
    /// The names of the code systems the root publishes.
    system_names: Fetched<NamedSearch>,
    /// The value sets the root publishes.
    value_sets: Fetched<NamedSearch>,
    /// The concept maps the root publishes.
    concept_maps: Fetched<NamedSearch>,
    /// The newest writes of the three types.
    activity: Fetched<Vec<Change>>,
}

impl Reads {
    /// Starts every read on `version`.
    fn new(client: &FhirClient, version: Signal<FhirVersion>) -> Self {
        let statement = {
            let client = client.clone();
            Resource::new(
                move || version.get(),
                move |version| {
                    let client = client.clone();
                    async move { client.capability_statement(version).await }
                },
            )
        };
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
        let named = |resource_type: &'static str| {
            let client = client.clone();
            Resource::new(
                move || version.get(),
                move |version| {
                    let client = client.clone();
                    async move { client.published_names(version, resource_type).await }
                },
            )
        };
        let activity = {
            let client = client.clone();
            Resource::new(
                move || version.get(),
                move |version| {
                    let client = client.clone();
                    async move { activity(&client, version).await }
                },
            )
        };
        Self {
            statement,
            capabilities,
            system_names: named(CODE_SYSTEM),
            value_sets: named(VALUE_SET),
            concept_maps: named(CONCEPT_MAP),
            activity,
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

/// The page header: the title, the version in view, and the root it read.
fn header(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    statement: Fetched<CapabilityStatement>,
) -> AnyView {
    let client = client.clone();
    let meta = move || {
        let base = client.version_base(version.get());
        let summary = statement.with(|answered| {
            answered
                .as_ref()
                .and_then(|answer| answer.as_ref().ok())
                .and_then(CapabilityStatement::summary)
        });
        page_meta(Some(match summary {
            Some(summary) => format!("{base} · {summary}"),
            None => base,
        }))
    };
    view! {
        {move || {
            page_header(
                "Dashboard",
                Some(
                    format!(
                        "The code systems, value sets and concept maps this server serves on FHIR {}.",
                        version.get().label(),
                    ),
                ),
                (),
            )
        }}
        <Transition fallback=|| ()>{meta}</Transition>
    }
    .into_any()
}

/// The count a stat card shows for a read: the figure once it answered, a
/// dash when it was refused, which the section below says in words.
fn figure<T>(read: Fetched<T>, count: impl Fn(&T) -> usize) -> String
where
    T: Clone + Send + Sync + 'static,
{
    read.with(|answered| {
        answered
            .as_ref()
            .and_then(|answer| answer.as_ref().ok())
            .map_or_else(|| String::from("—"), |answer| count(answer).to_string())
    })
}

/// The four stat cards, each a link to the screen it counts.
fn stats(reads: Reads) -> AnyView {
    let cards = move || {
        let systems = figure(reads.capabilities, |capabilities| {
            capabilities.cards().len()
        });
        let sets = figure(reads.value_sets, |named| named.choices().len());
        let maps = figure(reads.concept_maps, |named| named.choices().len());
        let writes = figure(reads.activity, Vec::len);
        stat_grid(view! {
            {stat_card(Stat {
                glyph: icon::CODE_SYSTEMS,
                value: systems,
                label: "Code systems".into(),
                href: Some(String::from("/systems")),
            })}
            {stat_card(Stat {
                glyph: icon::VALUE_SETS,
                value: sets,
                label: "Value sets".into(),
                href: Some(String::from("/valuesets")),
            })}
            {stat_card(Stat {
                glyph: icon::CONCEPT_MAPS,
                value: maps,
                label: "Concept maps".into(),
                href: Some(String::from("/conceptmaps")),
            })}
            {stat_card(Stat {
                glyph: Glyph::ACTIVITY,
                value: writes,
                label: "Recent writes".into(),
                href: None,
            })}
        })
    };
    view! {
        <Transition fallback=|| skeleton(2, "Reading what this server serves")>{cards}</Transition>
    }
    .into_any()
}

/// The first code systems the root serves, as a table leading to the
/// listing.
fn systems_panel(reads: Reads, version: Signal<FhirVersion>) -> AnyView {
    let all = move || ui_link("systems", version.get());
    let action = view! {
        <a href=all class="fui-button fui-button-secondary fui-button-small">
            "All code systems"
            {ferrohealth_ui::icon::icon(Glyph::CHEVRON_RIGHT, "14")}
        </a>
    };
    let table = move || {
        let names = reads.system_names.with(|answered| {
            answered
                .as_ref()
                .and_then(|answer| answer.as_ref().ok())
                .map(NamedSearch::names)
                .unwrap_or_default()
        });
        reads.capabilities.with(|answered| {
            answered.as_ref().map(|answer| match answer {
                Ok(capabilities) => preview(&capabilities.cards(), &names, version.get()),
                Err(error) => failure(error),
            })
        })
    };
    let loading = || preview_table(Body::Loading(4));
    let body = view! { <Transition fallback=loading>{table}</Transition> };
    titled_panel("Code systems", action, body).into_any()
}

/// The preview table holding `body`.
fn preview_table(body: Body) -> AnyView {
    data_table(DataTable {
        caption: "Code systems",
        columns: &PREVIEW_COLUMNS,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}

/// The preview table of `cards`, named from `names`.
fn preview(
    cards: &[SystemCard],
    names: &BTreeMap<String, String>,
    version: FhirVersion,
) -> AnyView {
    if cards.is_empty() {
        return preview_table(Body::Empty(
            empty_quiet(
                icon::CODE_SYSTEMS,
                "No code system",
                "This root serves none. A deployment loads one with the offline build.",
            )
            .into_any(),
        ));
    }
    preview_table(Body::Rows(
        cards
            .iter()
            .take(PREVIEW_ROWS)
            .map(|card| preview_row(card, names, version))
            .collect_view()
            .into_any(),
    ))
}

/// One code system of the preview: its name over its canonical, its default
/// version and its content mode.
fn preview_row(
    card: &SystemCard,
    names: &BTreeMap<String, String>,
    version: FhirVersion,
) -> AnyView {
    let title = names
        .get(&card.url)
        .cloned()
        .unwrap_or_else(|| card.url.clone());
    let default = card
        .versions
        .iter()
        .find(|row| row.is_default)
        .or_else(|| card.versions.first())
        .and_then(|row| row.code.clone())
        .unwrap_or_else(|| String::from("—"));
    let content = card.content.clone().unwrap_or_else(|| String::from("—"));
    let href = system_link(&card.url, version);
    let canonical = card.url.clone();
    view! {
        <tr>
            <td>
                <a href=href>{title}</a>
                <span class="fui-truncate fui-small fui-muted fui-mono">{canonical}</span>
            </td>
            <td data-label="Default version" class="fui-mono">
                {default}
            </td>
            <td data-label="Content">{content}</td>
        </tr>
    }
    .into_any()
}

/// The newest writes on this root, with the days they fell on.
fn activity_panel(activity: Fetched<Vec<Change>>, version: Signal<FhirVersion>) -> AnyView {
    let changes = move || {
        activity.with(|answered| {
            answered.as_ref().map(|answer| match answer {
                Ok(changes) => changes_view(changes, version.get()),
                Err(error) => failure(error),
            })
        })
    };
    let loading = || panel_body(skeleton(4, "Reading what was written lately"));
    let body = view! { <Transition fallback=loading>{changes}</Transition> };
    titled_panel("Recent changes", (), body).into_any()
}

/// The writes `changes` as a chart of the days they fell on and a list, or
/// the quiet statement that nothing was written.
fn changes_view(changes: &[Change], version: FhirVersion) -> AnyView {
    if changes.is_empty() {
        return panel_body(empty_quiet(
            Glyph::ACTIVITY,
            "Nothing written yet",
            "Every resource on this root was loaded by the offline build.",
        ))
        .into_any();
    }
    let chart = per_day(changes)
        .filter(|days| days.points.len() > 1)
        .map(|days| {
            activity_chart(&ActivityChart {
                name: "writes-per-day",
                label: String::from("Writes per day"),
                summary: format!(
                    "{} writes from {} to {}",
                    changes.len(),
                    days.first,
                    days.last
                ),
                points: days.points,
                ends: (days.first, days.last),
            })
        });
    let items = changes
        .iter()
        .take(ACTIVITY_SHOWN)
        .map(|change| change_item(change, version))
        .collect();
    panel_body(view! {
        {chart}
        {divided_list(Some("Recent changes"), items)}
    })
    .into_any()
}

/// One write: what it was made on, the interaction, and when.
fn change_item(change: &Change, version: FhirVersion) -> AnyView {
    let (word, tone) = match change.method.as_deref() {
        Some("POST") => ("created", Tone::Ok),
        Some("DELETE") => ("deleted", Tone::Bad),
        Some(_) => ("updated", Tone::Info),
        None => ("written", Tone::Neutral),
    };
    let named = if change.named.is_empty() {
        format!("{}/{}", change.resource_type, change.id)
    } else {
        change.named.clone()
    };
    let href = history_link(&change.resource_type, &change.id, version);
    let when = change
        .when
        .as_deref()
        .map(to_the_minute)
        .unwrap_or_default();
    let kind = change.resource_type.clone();
    let title = named.clone();
    view! {
        <div class="flex items-start justify-between gap-default">
            <div class="min-w-0">
                <a href=href class="block truncate" title=title>
                    {named}
                </a>
                <span class="block fui-small fui-muted">{kind} " · " {when}</span>
            </div>
            <span class="shrink-0">{badge(tone, word)}</span>
        </div>
    }
    .into_any()
}

/// A FHIR instant to the minute, the precision a reader scans a list by:
/// `2026-10-18T18:31:52.42Z` reads `2026-10-18 18:31 UTC`. Anything shorter
/// than a minute, or not an instant, is shown as the server sent it.
fn to_the_minute(instant: &str) -> String {
    let minute: String = instant.chars().take(16).collect();
    if minute.chars().count() < 16 || !instant.contains('T') {
        return instant.to_owned();
    }
    let zone = if instant.ends_with('Z') { " UTC" } else { "" };
    format!("{}{zone}", minute.replace('T', " "))
}

/// The way into each operation.
fn operations_card(version: Signal<FhirVersion>) -> AnyView {
    // Four cards, two to a row or four, so none is left alone on a row
    // (FerroUI's grid fits as many as the width takes).
    let cards = move || {
        let links = OPERATIONS
            .into_iter()
            .map(|(path, glyph, title, line)| {
                link_card_glyph(
                    ui_link(path, version.get()),
                    glyph,
                    title,
                    view! { <p class="fui-muted">{line}</p> },
                )
            })
            .collect_view();
        view! { <div class="grid gap-loose sm:grid-cols-2 xl:grid-cols-4">{links}</div> }
    };
    card("Run an operation", cards).into_any()
}

/// A refused read, in the server's own words.
fn failure(error: &FhirError) -> AnyView {
    let error = error.clone();
    panel_body(view! { <Failure error=Signal::stored(error) /> }).into_any()
}

#[cfg(test)]
mod tests {
    use super::to_the_minute;

    #[test]
    fn an_instant_is_shown_to_the_minute() {
        assert_eq!(
            to_the_minute("2026-10-18T18:31:52.427842Z"),
            "2026-10-18 18:31 UTC"
        );
        assert_eq!(
            to_the_minute("2026-10-18T18:31:52+02:00"),
            "2026-10-18 18:31"
        );
        assert_eq!(to_the_minute("2026-10-18"), "2026-10-18");
    }
}
