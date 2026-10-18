//! The served code systems as one table, one row per system.
//!
//! A deployment serves twenty systems as readily as five, so the listing is
//! an inventory a reader scans rather than a card each. It carries the five
//! facts a reader picks a system by; the grammar, the designation languages,
//! the filters and the published resource are on the system's own screen, one
//! click from the row.
//!
//! Nothing here names a system, assumes a hierarchy, or assumes a language.
//! Every cell is the capability statement rendered, so a system this server
//! has never served draws correctly with no change to this file.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::{
    DEFAULT_SIZE, Direction, Noun, PAGE_KEY, Paging, SIZE_KEY, Sort, Window,
};
use ferrohealth_ui::table::view::{
    ACTIONS, Body, Column as TableColumn, DataTable, Listing, data_table, link_pager,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::chip;
use ferrohealth_ui::widget::filter::keeps;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::components::shell::SelectedVersion;
use crate::fhir::concept::Hierarchy;
use crate::fhir::terminology::Artifact;
use crate::fhir::terminology::SystemCard;
use crate::fhir::terminology::VersionRow;
use crate::fhir::version::FhirVersion;
use crate::routes::BROWSE_PATH;
use crate::routes::SYSTEMS_PATH;
use crate::routes::VALIDATE_PATH;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::routes::system_link;
use crate::routes::system_tool_link;

/// The address parameter naming the column the table is sorted on.
const SORT_PARAM: &str = "sort";

/// The address parameter naming which way that column is sorted.
const DIRECTION_PARAM: &str = "dir";

/// The value that parameter carries for a descending sort.
const DESCENDING: &str = "desc";

/// A column the systems can be ordered by.
///
/// Only the system-level facts sort. A version is a row under its system, so
/// ordering the table by one would break the group it belongs to, and a
/// reader looking for a version is looking inside a system they already found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Column {
    /// The name the system is known by, or its canonical where the server
    /// published no name.
    System,
    /// The content mode the server declared for it.
    Content,
    /// Whether the server answers subsumption for it.
    Subsumes,
}

impl Column {
    /// The value the address carries for this column.
    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Content => "content",
            Self::Subsumes => "subsumes",
        }
    }

    /// The heading a reader reads on it.
    fn label(self) -> &'static str {
        match self {
            Self::System => "Code system",
            Self::Content => "Content",
            Self::Subsumes => "Subsumes",
        }
    }

    /// Reads a column out of the address, or `None` for the order the server
    /// declared.
    fn read(text: &str) -> Option<Self> {
        [Self::System, Self::Content, Self::Subsumes]
            .into_iter()
            .find(|column| column.key() == text)
    }

    /// What this column sorts on, as the text it compares.
    ///
    /// A fact the server did not state sorts after every one it did, in both
    /// directions, so absence never lands in the middle of the answers.
    fn of(self, card: &SystemCard, names: &BTreeMap<String, String>) -> (bool, String) {
        match self {
            // A reader ordering this column is looking for a name, so the sort
            // runs on the name where the server published one, case-folded so
            // that two spellings of one letter sit together.
            Self::System => (
                card.url.is_empty(),
                name_of(card, names).unwrap_or(&card.url).to_lowercase(),
            ),
            Self::Content => (
                card.content.is_none(),
                card.content.clone().unwrap_or_default(),
            ),
            Self::Subsumes => (
                card.subsumption.is_none(),
                match card.subsumption {
                    Some(true) => "yes".to_owned(),
                    Some(false) => "no".to_owned(),
                    None => String::new(),
                },
            ),
        }
    }
}

/// How the table is ordered right now.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Order {
    /// The column, or `None` for the order the server declared.
    column: Option<Column>,
    /// Whether that column runs the other way.
    descending: bool,
}

impl Order {
    /// Reads the order out of the address.
    fn read(query: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            column: query(SORT_PARAM)
                .as_deref()
                .map(str::trim)
                .and_then(Column::read),
            descending: query(DIRECTION_PARAM).as_deref().map(str::trim) == Some(DESCENDING),
        }
    }

    #[cfg(test)]
    /// The order a click on `column` asks for.
    ///
    /// A first click sorts the column upward, and a second turns it around, so
    /// the two ends of a column are one click apart.
    fn toggled(self, column: Column) -> Self {
        Self {
            column: Some(column),
            descending: self.column == Some(column) && !self.descending,
        }
    }

    /// This order as FerroUI's table reads it, which draws `aria-sort`.
    fn sort(self) -> Option<Sort> {
        self.column.map(|column| Sort {
            column: column.key().to_owned(),
            direction: if self.descending {
                Direction::Descending
            } else {
                Direction::Ascending
            },
        })
    }

    /// What a screen reader announces about `column` on this order.
    #[cfg(test)]
    fn announced(self, column: Column) -> &'static str {
        Sort::aria(self.sort().as_ref(), column.key())
    }

    /// The order FerroUI's `sort` asks for.
    fn of_sort(sort: &Sort) -> Self {
        Self {
            column: Column::read(&sort.column),
            descending: sort.direction == Direction::Descending,
        }
    }

    /// The address this order is, on the listing of `version` at `paging`.
    fn address(self, version: FhirVersion, paging: Paging) -> String {
        let mut url = base_url().segment(SYSTEMS_PATH);
        url = url.query(VERSION_PARAM, version.segment());
        if let Some(column) = self.column {
            url = url.query(SORT_PARAM, column.key());
            if self.descending {
                url = url.query(DIRECTION_PARAM, DESCENDING);
            }
        }
        if paging.page > 1 {
            url = url.query(PAGE_KEY, &paging.page.to_string());
        }
        if paging.size != DEFAULT_SIZE {
            url = url.query(SIZE_KEY, &paging.size.to_string());
        }
        url.render("")
    }
}

/// The name the server published for `card`, where it published one.
fn name_of<'a>(card: &SystemCard, names: &'a BTreeMap<String, String>) -> Option<&'a str> {
    names.get(&card.url).map(String::as_str)
}

/// The systems in the order the address asks for.
fn ordered(
    mut cards: Vec<SystemCard>,
    order: Order,
    names: &BTreeMap<String, String>,
) -> Vec<SystemCard> {
    let Some(column) = order.column else {
        return cards;
    };
    // A stable sort, so systems that compare equal keep the order the server
    // declared them in rather than an order this function invented. Absence is
    // compared first and never reversed, which is what keeps a fact the server
    // did not state at the foot of the table in both directions.
    cards.sort_by(|left, right| {
        let (left_absent, left_key) = column.of(left, names);
        let (right_absent, right_key) = column.of(right, names);
        left_absent.cmp(&right_absent).then_with(|| {
            let ordering = left_key.cmp(&right_key);
            if order.descending {
                ordering.reverse()
            } else {
                ordering
            }
        })
    });
    cards
}

/// The noun the pager names the rows by.
const NOUN: Noun = Noun {
    one: "code system",
    many: "code systems",
};

/// The served systems: a filter over them, one row per system, and the pager.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn SystemTable(
    /// The systems to draw, as the capability statement declared them.
    cards: Vec<SystemCard>,
    /// The name each canonical was published under, where the search that
    /// read them answered. A system missing from it leads with its canonical.
    names: BTreeMap<String, String>,
    /// The filter's words, which the page's header holds the box for.
    words: RwSignal<String>,
    /// The sentence of how many systems the filter keeps, which the box says.
    count: RwSignal<String>,
) -> impl IntoView {
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let query = use_query_map();
    let order = Memo::new(move |_| query.with(|map| Order::read(&|name| map.get(name))));
    let paging = Memo::new(move |_| query.with(Paging::from_query));
    let declared = StoredValue::new(cards);
    let published = StoredValue::new(names);
    // The rows the filter keeps, in the order the address asks for. A sort
    // is a navigation onto this same route, which `leptos_router` 0.8
    // answers by updating the query without re-running this body.
    let kept = Memo::new(move |_| {
        let order = order.get();
        let words = words.get();
        published.with_value(|names| {
            declared.with_value(|cards| {
                ordered(cards.clone(), order, names)
                    .into_iter()
                    .filter(|card| {
                        let name = name_of(card, names).unwrap_or_default();
                        keeps(&words, &[name, card.url.as_str()])
                    })
                    .collect::<Vec<SystemCard>>()
            })
        })
    });
    Effect::new(move |_| {
        let held = declared.with_value(Vec::len);
        let shown = kept.with(Vec::len);
        count.set(if shown == held {
            format!("{held} {}", if held == 1 { NOUN.one } else { NOUN.many })
        } else {
            format!("{shown} of {held} {}", NOUN.many)
        });
    });
    let table = move || {
        let order = order.get();
        let at = version.get();
        let rows = kept.get();
        let total = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let window = Window::over(total, paging.get());
        let body = if rows.is_empty() {
            Body::Empty(
                empty_quiet(
                    Glyph::SEARCH_X,
                    "No match",
                    "No code system this root serves matches the filter.",
                )
                .into_any(),
            )
        } else {
            Body::Rows(published.with_value(|names| {
                window
                    .slice(&rows)
                    .iter()
                    .map(|card| system_row(card, version, name_of(card, names)))
                    .collect_view()
                    .into_any()
            }))
        };
        let columns = [
            TableColumn::text(Column::System.key(), Column::System.label()).sortable(),
            TableColumn::text("version", "Versions"),
            TableColumn::text(Column::Content.key(), Column::Content.label()).sortable(),
            TableColumn::text(Column::Subsumes.key(), Column::Subsumes.label()).sortable(),
            TableColumn::text("artifact", "Read from"),
            TableColumn::text("open", "Actions"),
        ];
        let sort = order.sort();
        let sort_href = |asked: &Sort| Order::of_sort(asked).address(at, Paging::default());
        let page_href = |asked: Paging| order.address(at, asked);
        view! {
            {data_table(DataTable {
                caption: "The code systems this root serves",
                columns: &columns,
                sort: sort.as_ref(),
                sort_href: &sort_href,
                body,
            })}
            {link_pager(Listing::new("Pages of code systems").noun(NOUN), window, &page_href)}
        }
        .into_any()
    };
    view! { {table} }
}

/// One system: its name over its canonical, its versions, what the server
/// declared of it, and the screens it opens in.
fn system_row(card: &SystemCard, version: Signal<FhirVersion>, name: Option<&str>) -> AnyView {
    let artifact = card
        .versions
        .iter()
        .find_map(|served| served.artifact.clone());
    view! {
        <tr>
            <td>{system_cell(card, version, name)}</td>
            <td data-label="Versions" class="sm:whitespace-nowrap">
                {versions_cell(&card.versions)}
            </td>
            <td data-label="Content" class="sm:whitespace-nowrap">
                {word_cell(card.content.clone())}
            </td>
            <td data-label="Subsumes">{yes_no_cell(card.subsumption)}</td>
            <td data-label="Read from">{artifact_cell(artifact.as_ref())}</td>
            <td data-label="Actions" class=ACTIONS>
                {tools_cell(card, version)}
            </td>
        </tr>
    }
    .into_any()
}

/// The screens this system can be opened in.
///
/// A row hands its subject on rather than making a reader retype a canonical.
/// The browser is offered only where a version declares the direct-child
/// operator, because that is the one operator a tree walks
/// (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>).
fn tools_cell(card: &SystemCard, version: Signal<FhirVersion>) -> AnyView {
    if card.url.is_empty() {
        return absent("a screen needs the canonical this system was declared without");
    }
    let served = card
        .versions
        .iter()
        .find(|served| served.is_default)
        .or_else(|| card.versions.first());
    let code = served.and_then(|served| served.code.clone());
    let walkable = served.is_some_and(|served| Hierarchy::of(served).walk().is_some());
    let link = |path: &'static str, label: &'static str| {
        let system = card.url.clone();
        let code = code.clone();
        let href = move || system_tool_link(path, &system, code.as_deref(), version.get());
        view! { <a href=href>{label}</a> }.into_any()
    };
    let browse = walkable.then(|| link(BROWSE_PATH, "Browse"));
    // NOTE: no specification governs this: our own design; the screens a row
    // opens are links of the text's height, so a row stays one line tall.
    view! { <span class="inline-flex gap-default">{browse} {link(VALIDATE_PATH, "Validate")}</span> }
        .into_any()
}

/// The system, as the link onto its screen: the name a reader knows it by,
/// the canonical under it.
fn system_cell(card: &SystemCard, version: Signal<FhirVersion>, name: Option<&str>) -> AnyView {
    if card.url.is_empty() {
        return view! { <span class="fui-muted">"Declared without a canonical URI"</span> }
            .into_any();
    }
    let target = card.url.clone();
    let href = move || system_link(&target, version.get());
    let canonical = card.url.clone();
    let title = canonical.clone();
    match name {
        Some(name) => {
            let name = name.to_owned();
            // NOTE: no specification governs this: our own design; the name and
            // the canonical share one line, so twenty systems fit one screen.
            view! {
                <span class="flex flex-col sm:flex-row sm:items-baseline sm:gap-default system-cell">
                    <a href=href class="shrink-0">
                        {name}
                    </a>
                    <span class="fui-truncate font-normal fui-small fui-muted fui-mono" title=title>
                        {canonical}
                    </span>
                </span>
            }
            .into_any()
        }
        None => view! {
            <a href=href class="fui-mono">
                {canonical}
            </a>
        }
        .into_any(),
    }
}

/// The served versions as chips, the default one marked.
fn versions_cell(versions: &[VersionRow]) -> AnyView {
    let chips: Vec<AnyView> = versions
        .iter()
        .filter_map(|served| {
            let code = served.code.clone().filter(|code| !code.is_empty())?;
            let mark = served
                .is_default
                .then(|| badge(Tone::Accent, "default").into_any());
            Some(view! { <span class="inline-flex items-center gap-tight">{chip(code)} {mark}</span> }.into_any())
        })
        .collect();
    if chips.is_empty() {
        return absent("this server declared no version for the code system");
    }
    view! { <div class="inline-flex flex-col gap-tight">{chips}</div> }.into_any()
}

/// A word the server stated, or the mark for one it did not.
fn word_cell(word: Option<String>) -> AnyView {
    match word.filter(|word| !word.is_empty()) {
        Some(word) => view! { <span>{word}</span> }.into_any(),
        None => absent("this FHIR version has no such element, or the server declared none"),
    }
}

/// A flag the server stated, or the mark for one it did not.
fn yes_no_cell(flag: Option<bool>) -> AnyView {
    match flag {
        Some(true) => view! { <span>"yes"</span> }.into_any(),
        Some(false) => view! { <span class="fui-muted">"no"</span> }.into_any(),
        None => absent("the server declared neither yes nor no"),
    }
}

/// The artifact a version was read from.
///
/// A system the deployment built no index for, such as a registry the server
/// carries or a `CodeSystem` posted through the API, was read from none. Only
/// the artifact's own name is on the wire, so no path is shown.
fn artifact_cell(artifact: Option<&Artifact>) -> AnyView {
    let Some(artifact) = artifact else {
        return absent("this version was not read from a built artifact");
    };
    let name = artifact.name.clone();
    let release = artifact.release.clone();
    let release_line =
        release.map(|release| view! { <span class="block fui-small fui-muted">{release}</span> });
    match name {
        Some(name) => view! {
            <span class="block fui-mono">{name}</span>
            {release_line}
        }
        .into_any(),
        None => absent("the server named no artifact for this version"),
    }
}

/// The mark for a fact the server did not state, with the sentence behind it.
///
/// A cell that spells absence out in prose makes "not" the densest word on the
/// screen; the reader who wants the sentence hovers or focuses the mark.
fn absent(why: &'static str) -> AnyView {
    view! {
        <span class="fui-muted" title=why>
            "—"
        </span>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One card carrying only the facts the order compares.
    fn card(url: &str, content: Option<&str>, subsumption: Option<bool>) -> SystemCard {
        SystemCard {
            url: url.to_owned(),
            content: content.map(str::to_owned),
            subsumption,
            versions: Vec::new(),
        }
    }

    /// The three cards every case below orders.
    fn served() -> Vec<SystemCard> {
        vec![
            card("urn:b", Some("complete"), Some(false)),
            card("urn:a", None, Some(true)),
            card("urn:c", Some("not-present"), None),
        ]
    }

    /// The canonicals of an ordered answer, in the order it holds them.
    fn urls(cards: &[SystemCard]) -> Vec<&str> {
        cards.iter().map(|card| card.url.as_str()).collect()
    }

    /// The three cards ordered, with no name published for any of them.
    fn unnamed(order: Order) -> Vec<SystemCard> {
        ordered(served(), order, &BTreeMap::new())
    }

    #[test]
    fn an_address_naming_no_column_keeps_the_order_the_server_declared() {
        let order = Order::read(&|_| None);
        assert_eq!(order, Order::default());
        assert_eq!(
            urls(&unnamed(order)),
            ["urn:b", "urn:a", "urn:c"],
            "the server's own order is an answer, not an absence of one"
        );
    }

    #[test]
    fn a_column_orders_the_systems_and_turns_around_on_the_second_click() {
        let up = Order::default().toggled(Column::System);
        assert_eq!(urls(&unnamed(up)), ["urn:a", "urn:b", "urn:c"]);
        let down = up.toggled(Column::System);
        assert!(down.descending, "a second click on one column turns it");
        assert_eq!(urls(&unnamed(down)), ["urn:c", "urn:b", "urn:a"]);
    }

    #[test]
    fn moving_to_another_column_starts_it_upward() {
        let down = Order::default()
            .toggled(Column::System)
            .toggled(Column::System);
        let moved = down.toggled(Column::Content);
        assert_eq!(moved.column, Some(Column::Content));
        assert!(
            !moved.descending,
            "a column a reader has not sorted yet starts upward"
        );
    }

    #[test]
    fn a_fact_the_server_did_not_state_sorts_after_every_one_it_did() {
        let up = Order::default().toggled(Column::Content);
        assert_eq!(
            urls(&unnamed(up)).last().copied(),
            Some("urn:a"),
            "the system with no content mode sorts last going up"
        );
        let down = up.toggled(Column::Content);
        assert_eq!(
            urls(&unnamed(down)).last().copied(),
            Some("urn:a"),
            "and last going down, so absence never lands among the answers"
        );
    }

    #[test]
    fn the_order_round_trips_through_the_address() {
        for column in [Column::System, Column::Content, Column::Subsumes] {
            for descending in [false, true] {
                let order = Order {
                    column: Some(column),
                    descending,
                };
                let address = order.address(FhirVersion::default(), Paging::default());
                let read = Order::read(&|name| {
                    let query = address
                        .split_once('?')
                        .expect("the address carries a query")
                        .1;
                    query.split('&').find_map(|pair| {
                        let (key, value) = pair.split_once('=')?;
                        (key == name).then(|| value.to_owned())
                    })
                });
                assert_eq!(read, order, "{} must survive the link", column.key());
            }
        }
    }

    #[test]
    fn a_screen_reader_is_told_which_column_is_sorted_and_which_way() {
        let order = Order::default().toggled(Column::Subsumes);
        assert_eq!(order.announced(Column::Subsumes), "ascending");
        assert_eq!(order.announced(Column::System), "none");
        assert_eq!(
            order.toggled(Column::Subsumes).announced(Column::Subsumes),
            "descending"
        );
    }

    #[test]
    fn the_column_orders_by_the_name_a_system_was_published_under() {
        let names = BTreeMap::from([
            ("urn:a".to_owned(), "Zinc".to_owned()),
            ("urn:c".to_owned(), "Almanac".to_owned()),
        ]);
        let up = Order::default().toggled(Column::System);
        assert_eq!(
            urls(&ordered(served(), up, &names)),
            ["urn:c", "urn:b", "urn:a"],
            "a reader ordering this column is looking for the name, and a system published without one is ordered by the canonical that names it"
        );
    }
}
