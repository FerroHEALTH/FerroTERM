// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome the resource listing screens share: the address, and the pager.
//!
//! `GET [base]/{type}` answers one `searchset` with every match in it: no
//! FHIR specification requires a server to page a search, and this one does
//! not (<https://hl7.org/fhir/R4B/http.html#search>). So the pages a reader
//! walks are the viewer's own, over the answer that arrived, and each screen
//! says so. The arithmetic and the address live here, outside every
//! component, so plain unit tests pin them.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::{Direction, Paging, Sort};
use ferrohealth_ui::table::view::{
    ACTIONS, Body, Column, DataTable, Listing, data_table, link_pager,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::status::pill;
use ferrohealth_ui::view::surface::{card, chip};
use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;

use crate::components::field::Field;
use crate::components::field::row;
use crate::components::field::text_field;
use crate::components::icon;
use crate::components::not_declared;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::paging::Page;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use ferrohealth_ui::widget::help::help_switch;

/// The address parameter carrying the canonical the search filters on.
const URL_PARAM: &str = "url";

/// The address parameter carrying the resource version the search filters on.
const RESOURCE_VERSION_PARAM: &str = "version";

/// The address parameter carrying the page the reader is on.
const PAGE_PARAM: &str = "page";

/// The address parameter carrying the id of the resource being read.
const ID_PARAM: &str = "id";

/// The address parameter carrying a page size the pager chose, which
/// outranks the reader's stored one.
const SIZE_PARAM: &str = "size";

/// What a listing screen's address says: the filter, the page, and the read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ListParams {
    /// The search parameters the listing sends.
    pub(crate) filter: SearchFilter,
    /// The id of the resource the detail pane reads, empty for none.
    pub(crate) id: String,
    /// The 1-based page the reader asked for.
    page: u32,
    /// How many rows a page holds, from the pager or the reader's stored
    /// preference.
    size: u32,
    /// Whether the address named the size, so a link keeps it.
    sized: bool,
}

impl ListParams {
    /// Reads the parameters out of the address.
    ///
    /// `query` looks one parameter up, which is what a `ParamsMap` does and
    /// what a test can do without a browser. A page that is not a whole
    /// number above zero is a link a reader typed, so it reads as the first
    /// page rather than refusing the whole screen.
    pub(crate) fn read(query: &dyn Fn(&str) -> Option<String>, size: u32) -> Self {
        let text = |name: &str| query(name).unwrap_or_default().trim().to_owned();
        // NOTE: no specification governs this: our own design; a size that is no
        // whole number from 1 to the bound is a typed link, read as not given.
        let asked = query(SIZE_PARAM).and_then(|typed| crate::settings::parse_page_size(&typed));
        Self {
            filter: SearchFilter {
                url: text(URL_PARAM),
                version: text(RESOURCE_VERSION_PARAM),
            },
            id: text(ID_PARAM),
            page: query(PAGE_PARAM)
                .and_then(|typed| typed.trim().parse::<u32>().ok())
                .filter(|page| *page > 0)
                .unwrap_or(1),
            size: asked.unwrap_or(size).max(1),
            sized: asked.is_some(),
        }
    }

    /// The 1-based page the address asked for.
    pub(crate) fn page(&self) -> u32 {
        self.page
    }

    /// How many rows a page holds.
    pub(crate) fn size(&self) -> u32 {
        self.size
    }

    /// These parameters with another filter, back at the first page.
    ///
    /// A changed filter selects a different answer, so the walk starts again
    /// and the resource being read is left behind: an id from the previous
    /// search need not be in the new one.
    pub(crate) fn searching(&self, filter: SearchFilter) -> Self {
        Self {
            filter,
            id: String::new(),
            page: 1,
            size: self.size,
            sized: self.sized,
        }
    }

    /// These parameters, reading one resource by its id.
    #[cfg(test)]
    pub(crate) fn reading(&self, id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ..self.clone()
        }
    }

    /// These parameters at the paging the pager asks for.
    pub(crate) fn paged(&self, paging: Paging) -> Self {
        Self {
            page: paging.page.max(1),
            sized: self.sized || paging.size != self.size,
            size: paging.size.max(1),
            ..self.clone()
        }
    }

    /// The viewer address these parameters are, for a link or a navigation.
    ///
    /// Every value stays in the query, which a click navigation carries
    /// through untouched while it unescapes a path segment a second time
    /// (`leptos_router` 0.8.15 `src/location/mod.rs`).
    pub(crate) fn address(&self, path: &str, version: FhirVersion) -> String {
        self.address_with(path, version, &[])
    }

    /// The same address, with `extra` parameters the screen also carries.
    ///
    /// A screen that puts a second form in the same address round-trips its
    /// parameters here, so walking a page or opening a resource does not throw
    /// away a run the reader has going.
    pub(crate) fn address_with(
        &self,
        path: &str,
        version: FhirVersion,
        extra: &[(&str, &str)],
    ) -> String {
        let mut url = base_url()
            .segment(path)
            .query(VERSION_PARAM, version.segment());
        if !self.filter.url.is_empty() {
            url = url.query(URL_PARAM, &self.filter.url);
        }
        if !self.filter.version.is_empty() {
            url = url.query(RESOURCE_VERSION_PARAM, &self.filter.version);
        }
        if self.page > 1 {
            url = url.query(PAGE_PARAM, &self.page.to_string());
        }
        if self.sized {
            url = url.query(SIZE_PARAM, &self.size.to_string());
        }
        if !self.id.is_empty() {
            url = url.query(ID_PARAM, &self.id);
        }
        for (name, value) in extra {
            if !value.is_empty() {
                url = url.query(name, value);
            }
        }
        url.render("")
    }
}

/// What a search answered, as the sentence a live region reads.
///
/// `counted` is `Bundle.total`, "the total number of matches" a search found
/// (<https://hl7.org/fhir/R4B/http.html#search>), which can exceed the entries
/// this answer carries when the server paged the search itself. This viewer
/// walks its own pages over what arrived, so a larger total is stated as a
/// partial answer rather than printed beside a row count it disagrees with.
pub(crate) fn count_sentence(view: Window, noun: &str, counted: Option<u32>) -> String {
    let shown = view.summary(noun);
    let Some(total) = counted else {
        return tr_with(
            Msg::ThisRootCounted,
            &[
                ("not_declared", not_declared().into()),
                ("shown", (shown).clone().into()),
            ],
        );
    };
    let carried = u32::try_from(view.matched).unwrap_or(u32::MAX);
    if total > carried {
        return tr_with(
            Msg::RootCountedSentSoPartAnswer,
            &[
                ("shown", (shown).clone().into()),
                ("total", (total).to_string().into()),
                ("carried", (carried).to_string().into()),
            ],
        );
    }
    tr_with(
        Msg::ThisRootCounted2,
        &[
            ("shown", (shown).clone().into()),
            ("total", (total).to_string().into()),
        ],
    )
}

/// The rows one page draws, and what the reader is told about them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Window {
    /// The page actually drawn, which may be earlier than the one asked for.
    page: Page,
    /// The index of the first row drawn.
    start: usize,
    /// How many rows are drawn.
    rows: usize,
    /// How many rows the answer holds in all.
    matched: usize,
}

/// The window `asked` covers over `matched` rows of `size` each.
///
/// The page is clamped to the last one that holds a row, so what is drawn and
/// what is announced always describe each other. That matters under
/// `<Transition>`, where the address advances a paint before the rows do.
pub(crate) fn window(asked: u32, size: u32, matched: usize) -> Window {
    let size = size.max(1);
    let total = u32::try_from(matched).unwrap_or(u32::MAX);
    let wanted = Page::at(asked.saturating_sub(1).saturating_mul(size), size);
    let page = if wanted.offset() >= total {
        Page::at(0, size).last(total)
    } else {
        wanted
    };
    let start = usize::try_from(page.offset()).unwrap_or(usize::MAX);
    Window {
        page,
        start,
        rows: matched.saturating_sub(start).min(size_as_usize(size)),
        matched,
    }
}

/// A page size as an index count, saturating where the target is narrower.
fn size_as_usize(size: u32) -> usize {
    usize::try_from(size).unwrap_or(usize::MAX)
}

impl Window {
    /// This window as FerroUI's pager reads it.
    pub(crate) fn paging(self) -> ferrohealth_ui::table::model::Window {
        let total = u32::try_from(self.matched).unwrap_or(u32::MAX);
        ferrohealth_ui::table::model::Window::over(
            total,
            Paging {
                page: self.page.number(),
                size: self.page.count(),
            },
        )
    }

    /// The indexes of the rows this page draws.
    pub(crate) fn indexes(self) -> impl Iterator<Item = usize> {
        (self.start..).take(self.rows)
    }

    #[cfg(test)]
    /// Where this page sits in the walk.
    pub(crate) fn position(self) -> String {
        let total = u32::try_from(self.matched).unwrap_or(u32::MAX);
        format!(
            "Page {} of {}",
            self.page.number(),
            self.page.total_pages(total)
        )
    }

    /// Which rows of the answer this page holds, for a live region.
    ///
    /// `noun` is the plural a reader reads, in the middle of a sentence.
    pub(crate) fn summary(self, noun: &str) -> String {
        if self.rows == 0 {
            return tr_with(Msg::NoOnThisPage, &[("noun", (noun).to_string().into())]);
        }
        let first = self.start.saturating_add(1);
        let last = self.start.saturating_add(self.rows);
        tr_with(
            Msg::ShowingToOf,
            &[
                ("noun", (noun).to_string().into()),
                ("first", (first).to_string().into()),
                ("last", (last).to_string().into()),
                ("matched", (self.matched).to_string().into()),
            ],
        )
    }
}

/// The filter, as a form that navigates rather than reloading the page.
///
/// The router installs no `submit` listener, so the submit is handled by the
/// caller and turned into a navigation. The handler arrives boxed, so both
/// screens share one emitted body rather than one monomorphized copy each
/// (<https://doc.rust-lang.org/book/ch10-01-syntax.html>). Each control is seeded from the
/// address through `prop:value`, which follows a back navigation and leaves
/// what the reader is typing alone, and is read back at submit.
pub(crate) fn filter_form(
    canonical_hint: &str,
    canonical: NodeRef<Input>,
    resource_version: NodeRef<Input>,
    params: Signal<ListParams>,
    submit: Box<dyn FnMut(SubmitEvent)>,
) -> AnyView {
    let canonical_field = Field {
        id: "filter-url",
        name: "url",
        label: tr(Msg::Canonical),
        hint: canonical_hint.to_owned(),
    };
    let version_field = Field {
        id: "filter-version",
        name: "version",
        label: tr(Msg::Version),
        hint: tr(Msg::VersionSearchParameterLeftEmptyEvery),
    };
    let form = view! {
        <form class="fui-form" role="search" aria-label=tr(Msg::SearchThisRoot) on:submit=submit>
            {row(
                vec![
                    text_field(
                        canonical_field,
                        canonical,
                        Memo::new(move |_| params.with(|params| params.filter.url.clone())),
                    ),
                    text_field(
                        version_field,
                        resource_version,
                        Memo::new(move |_| params.with(|params| params.filter.version.clone())),
                    ),
                ],
            )}
            <div class="fui-cluster">
                <button type="submit" class=Variant::Primary.class()>
                    {ferrohealth_ui::icon::icon(icon::SEARCH, "16")}
                    {tr(Msg::Search)}
                </button>
                {help_switch()}
            </div>
        </form>
    };
    card(tr(Msg::Search), form).into_any()
}

/// The address parameter naming the column a list is ordered on.
const SORT_PARAM: &str = "sort";

/// The address parameter naming which way that column is ordered.
const DIRECTION_PARAM: &str = "dir";

/// The value that parameter carries for a descending order.
const DESCENDING: &str = "desc";

/// A column a publishing list can be ordered by.
///
/// The three the list draws as facts. The fourth column is what a row hands
/// its subject to, which is the same on every row and orders nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortColumn {
    /// The name a person recognises the resource by.
    Name,
    /// The business version.
    Version,
    /// The publication status.
    Status,
}

impl SortColumn {
    /// The value the address carries for this column.
    fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Version => "version",
            Self::Status => "status",
        }
    }

    /// Reads a column out of the address, or `None` for the server's order.
    fn read(text: &str) -> Option<Self> {
        [Self::Name, Self::Version, Self::Status]
            .into_iter()
            .find(|column| column.key() == text)
    }

    /// What this column orders on: whether the fact is absent, and its text.
    ///
    /// A fact the resource did not state sorts after every one it did, in both
    /// directions, so absence never lands in the middle of the answers.
    fn of(self, published: &Published<'_>) -> (bool, String) {
        let stated = match self {
            Self::Name => published.title,
            Self::Version => published.version,
            Self::Status => published.status,
        };
        (stated.is_none(), stated.unwrap_or_default().to_lowercase())
    }
}

/// How a publishing list is ordered right now.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SortOrder {
    /// The column, or `None` for the order the server answered in.
    column: Option<SortColumn>,
    /// Whether that column runs the other way.
    descending: bool,
}

impl SortOrder {
    /// Reads the order out of the address.
    pub(crate) fn read(query: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            column: query(SORT_PARAM)
                .as_deref()
                .map(str::trim)
                .and_then(SortColumn::read),
            descending: query(DIRECTION_PARAM).as_deref().map(str::trim) == Some(DESCENDING),
        }
    }

    #[cfg(test)]
    /// The order a click on `column` asks for.
    fn toggled(self, column: SortColumn) -> Self {
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

    /// The order FerroUI's `sort` asks for.
    fn of_sort(sort: &Sort) -> Self {
        Self {
            column: SortColumn::read(&sort.column),
            descending: sort.direction == Direction::Descending,
        }
    }

    /// What a screen reader announces about `column` on this order.
    #[cfg(test)]
    fn announced(self, column: SortColumn) -> &'static str {
        Sort::aria(self.sort().as_ref(), column.key())
    }

    /// The address parameters this order is, for a link.
    fn pairs(self) -> Vec<(&'static str, &'static str)> {
        let Some(column) = self.column else {
            return Vec::new();
        };
        let mut carried = vec![(SORT_PARAM, column.key())];
        if self.descending {
            carried.push((DIRECTION_PARAM, DESCENDING));
        }
        carried
    }

    /// This order, ordering `published`.
    ///
    /// A stable sort, so resources that compare equal keep the order the
    /// server answered in rather than one this function invented.
    pub(crate) fn ordering(self, published: &mut [(Published<'_>, usize)]) {
        let Some(column) = self.column else {
            return;
        };
        published.sort_by(|(left, _), (right, _)| {
            let (left_absent, left_key) = column.of(left);
            let (right_absent, right_key) = column.of(right);
            left_absent.cmp(&right_absent).then_with(|| {
                let ordering = left_key.cmp(&right_key);
                if self.descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            })
        });
    }
}

/// The table a publishing list draws: the name, the version and the status
/// a reader can order it by, and what a row hands its resource to.
///
/// FerroUI draws each sortable header as a link carrying `aria-sort`, so the
/// order is shareable and the keyboard needs no handler of ours
/// (<https://www.w3.org/WAI/ARIA/apg/patterns/table/>).
pub(crate) fn published_table(
    caption: &str,
    noun: String,
    rows: Vec<AnyView>,
    order: SortOrder,
    params: &ListParams,
    path: &'static str,
    version: FhirVersion,
) -> AnyView {
    let columns = [
        Column::text(SortColumn::Name.key(), noun).sortable(),
        Column::text(SortColumn::Version.key(), tr(Msg::Version)).sortable(),
        Column::text(SortColumn::Status.key(), tr(Msg::Status)).sortable(),
        Column::text("open", tr(Msg::Actions)),
    ];
    let sort = order.sort();
    let href =
        |asked: &Sort| params.address_with(path, version, &SortOrder::of_sort(asked).pairs());
    data_table(DataTable {
        caption,
        columns: &columns,
        sort: sort.as_ref(),
        sort_href: &href,
        body: if rows.is_empty() {
            Body::Empty(
                empty_quiet(
                    Glyph::SEARCH_X,
                    tr(Msg::NoMatch),
                    tr(Msg::RootHoldsNoResourceMatchingSearch),
                )
                .into_any(),
            )
        } else {
            Body::Rows(rows.collect_view().into_any())
        },
    })
    .into_any()
}

/// One row of a publishing list: what it is, and what can be done with it.
///
/// The title leads and the canonical sits beside it, quieter. A resource is
/// something a person recognises by name; the canonical is what a request
/// carries, and a monospace URL at heading weight made every row look the
/// same. `ValueSet` and `ConceptMap` both declare `title`, `url`, `version`
/// and `status` (<https://hl7.org/fhir/R5/valueset.html>), so one row shape
/// draws both lists.
pub(crate) fn published_row(
    published: &Published<'_>,
    open: Option<String>,
    run: Option<Action>,
) -> AnyView {
    let title = published.title.map_or_else(not_declared, str::to_owned);
    let canonical = published.canonical.map_or_else(not_declared, str::to_owned);
    let heading = match open {
        Some(href) => view! { <a href=href>{title}</a> }.into_any(),
        None => view! {
            {title}
            <span class="fui-sr-only">{tr(Msg::WhichCarriesNoIdRead)}</span>
        }
        .into_any(),
    };
    let action = run.map(|action| {
        view! { <a href=action.href>{action.label}</a> }
    });
    let version = match published.version {
        Some(version) => chip(version.to_owned()).into_any(),
        None => view! { <span class="fui-muted">"—"</span> }.into_any(),
    };
    view! {
        <tr>
            <th scope="row">
                {heading}
                <span class="fui-truncate font-normal fui-small fui-muted fui-mono">
                    {canonical}
                </span>
            </th>
            <td data-label=tr(Msg::Version)>{version}</td>
            <td data-label=tr(Msg::Status)>{status_pill(published.status)}</td>
            <td data-label=tr(Msg::Actions) class=ACTIONS>
                {action}
            </td>
        </tr>
    }
    .into_any()
}

/// A publication status as a pill in the tone it reads in: active is in
/// force, draft is not yet, retired no longer
/// (<https://hl7.org/fhir/R4B/valueset-publication-status.html>).
pub(crate) fn status_pill(status: Option<&str>) -> AnyView {
    match status {
        Some(status) => {
            let tone = match status {
                "active" => Tone::Ok,
                "draft" => Tone::Warn,
                _ => Tone::Neutral,
            };
            pill(tone, status).into_any()
        }
        None => view! { <span class="fui-muted">"—"</span> }.into_any(),
    }
}

/// The four facts a publishing list draws about one resource.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Published<'a> {
    /// The name a person recognises it by.
    pub(crate) title: Option<&'a str>,
    /// The canonical a request carries.
    pub(crate) canonical: Option<&'a str>,
    /// The business version.
    pub(crate) version: Option<&'a str>,
    /// The publication status.
    pub(crate) status: Option<&'a str>,
}

/// What a row hands its subject to.
#[derive(Clone, Debug)]
pub(crate) struct Action {
    /// What the runner does, as a reader reads it.
    pub(crate) label: String,
    /// The address that opens it with the subject already filled in.
    pub(crate) href: String,
}

/// The page controls, FerroUI's pager over links: each step and each size is
/// the address of another page, so a page is shareable and the browser walks
/// the list with its back button. A step that leads nowhere says
/// "(unavailable)" in visible text (<https://www.w3.org/TR/WCAG22/#use-of-color>).
pub(crate) fn pager_view(
    listing: Listing,
    view: Window,
    params: &ListParams,
    path: &'static str,
    version: FhirVersion,
    extra: &[(&str, &str)],
) -> AnyView {
    let href = |paging: Paging| params.paged(paging).address_with(path, version, extra);
    view! { <div class="fui-panel-body">{link_pager(listing, view.paging(), &href)}</div> }
        .into_any()
}

#[cfg(test)]
mod tests {
    use crate::routes::UI_BASE;
    /// Three resources, one of which states no version.
    fn published() -> Vec<Published<'static>> {
        vec![
            Published {
                title: Some("Beta"),
                canonical: Some("urn:b"),
                version: Some("2"),
                status: Some("draft"),
            },
            Published {
                title: Some("alpha"),
                canonical: Some("urn:a"),
                version: None,
                status: Some("active"),
            },
            Published {
                title: Some("Gamma"),
                canonical: Some("urn:g"),
                version: Some("1"),
                status: Some("retired"),
            },
        ]
    }

    /// The titles of an ordered answer, in the order it holds them.
    fn titles(order: SortOrder) -> Vec<&'static str> {
        let held = published();
        let mut ordered: Vec<(Published<'static>, usize)> = held.iter().copied().zip(0..).collect();
        order.ordering(&mut ordered);
        ordered
            .into_iter()
            .map(|(published, _)| published.title.unwrap_or_default())
            .collect()
    }

    /// An order naming one column, read the way the address carries it.
    fn asked(column: &str, direction: Option<&str>) -> SortOrder {
        let column = column.to_owned();
        let direction = direction.map(str::to_owned);
        SortOrder::read(&|name| match name {
            "sort" => Some(column.clone()),
            "dir" => direction.clone(),
            _ => None,
        })
    }

    #[test]
    fn an_address_naming_no_column_keeps_the_order_the_server_answered_in() {
        let order = SortOrder::read(&|_| None);
        assert_eq!(order, SortOrder::default());
        assert_eq!(
            titles(order),
            ["Beta", "alpha", "Gamma"],
            "the server's own order is an answer, not an absence of one"
        );
    }

    #[test]
    fn a_name_orders_without_regard_to_its_capital() {
        assert_eq!(
            titles(asked("name", None)),
            ["alpha", "Beta", "Gamma"],
            "a reader looking for `alpha` does not care that it is lower case"
        );
    }

    #[test]
    fn the_second_click_turns_a_column_around() {
        let up = SortOrder::default().toggled(SortColumn::Status);
        let down = up.toggled(SortColumn::Status);
        assert!(down.descending);
        assert_eq!(titles(up), ["alpha", "Beta", "Gamma"]);
        assert_eq!(titles(down), ["Gamma", "Beta", "alpha"]);
    }

    #[test]
    fn moving_to_another_column_starts_it_upward() {
        let down = SortOrder::default()
            .toggled(SortColumn::Name)
            .toggled(SortColumn::Name);
        let moved = down.toggled(SortColumn::Status);
        assert_eq!(moved.column, Some(SortColumn::Status));
        assert!(
            !moved.descending,
            "a column a reader has not ordered yet starts upward"
        );
    }

    #[test]
    fn a_fact_the_resource_did_not_state_sorts_after_every_one_it_did() {
        for direction in [None, Some("desc")] {
            assert_eq!(
                titles(asked("version", direction)).last().copied(),
                Some("alpha"),
                "the resource with no version sorts last in both directions"
            );
        }
    }

    #[test]
    fn an_order_the_address_does_not_name_is_the_server_s_own() {
        assert_eq!(
            asked("colour", None),
            SortOrder::default(),
            "a link a reader typed cannot name a column this list does not draw"
        );
    }

    #[test]
    fn a_screen_reader_is_told_which_column_is_ordered_and_which_way() {
        let order = SortOrder::default().toggled(SortColumn::Version);
        assert_eq!(order.announced(SortColumn::Version), "ascending");
        assert_eq!(order.announced(SortColumn::Name), "none");
        assert_eq!(
            order
                .toggled(SortColumn::Version)
                .announced(SortColumn::Version),
            "descending"
        );
    }

    use super::*;

    /// A stand-in for the address, which reads the same way a `ParamsMap` does.
    fn map<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + use<'a> {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn an_address_that_names_nothing_reads_as_the_first_unfiltered_page() {
        let params = ListParams::read(&map(&[]), 25);
        assert_eq!(params.page(), 1);
        assert_eq!(params.size(), 25);
        assert_eq!(
            params.filter,
            SearchFilter::default(),
            "nothing was asked for, so nothing narrows the search"
        );
        assert_eq!(params.id, "");
    }

    #[test]
    fn a_page_that_is_not_a_whole_number_reads_as_the_first_one() {
        assert_eq!(ListParams::read(&map(&[("page", "x")]), 25).page(), 1);
        assert_eq!(
            ListParams::read(&map(&[("page", "0")]), 25).page(),
            1,
            "a page before the first one is a link a reader typed"
        );
    }

    #[test]
    fn the_address_carries_every_parameter_the_screen_reads_back() {
        let params = ListParams::read(
            &map(&[
                ("url", "https://terminology.example/ValueSet/x?a=b"),
                ("version", "2031"),
                ("page", "3"),
                ("id", "vs-1"),
            ]),
            25,
        );
        assert_eq!(
            params.address("valuesets", FhirVersion::R4B),
            format!(
                "{UI_BASE}/valuesets\
                 ?fhir=r4b&url=https%3A%2F%2Fterminology.example%2FValueSet%2Fx%3Fa%3Db\
                 &version=2031&page=3&id=vs-1"
            ),
            "a canonical carrying its own query string cannot truncate the address"
        );
    }

    #[test]
    fn the_first_page_is_left_out_of_the_address() {
        let params = ListParams::read(&map(&[]), 25);
        assert_eq!(
            params.address("conceptmaps", FhirVersion::R5),
            format!("{UI_BASE}/conceptmaps?fhir=r5"),
            "an address says what was asked for, and nothing was"
        );
    }

    #[test]
    fn searching_again_starts_the_walk_over_and_closes_the_open_resource() {
        let params = ListParams::read(&map(&[("page", "4"), ("id", "vs-1")]), 25);
        let searched = params.searching(SearchFilter {
            url: "https://x.example/v".to_owned(),
            version: String::new(),
        });
        assert_eq!(searched.page(), 1);
        assert!(
            searched.id.is_empty(),
            "an id from the previous search need not be in the new one"
        );
    }

    #[test]
    fn opening_a_resource_keeps_the_page_the_reader_is_on() {
        let params = ListParams::read(&map(&[("page", "2")]), 25);
        let reading = params.reading("vs-9");
        assert_eq!(reading.page(), 2);
        assert_eq!(reading.id, "vs-9");
    }

    #[test]
    fn a_screen_that_carries_a_second_form_round_trips_its_parameters() {
        let params = ListParams::read(&map(&[("page", "2")]), 25);
        assert_eq!(
            params.address_with(
                "conceptmaps",
                FhirVersion::R4B,
                &[
                    ("code", "x"),
                    ("target", ""),
                    ("system", "https://x.example/s")
                ]
            ),
            format!(
                "{UI_BASE}/conceptmaps?fhir=r4b&page=2&code=x&system=https%3A%2F%2Fx.example%2Fs"
            ),
            "a parameter the reader left empty is left out rather than sent empty"
        );
    }

    #[test]
    fn the_sentence_states_what_is_drawn_and_what_the_root_counted() {
        assert_eq!(
            count_sentence(window(1, 25, 3), "value sets", Some(3)),
            "Showing value sets 1 to 3 of 3. This root counted 3."
        );
        assert_eq!(
            count_sentence(window(1, 25, 0), "concept maps", None),
            "No concept maps on this page. This root counted not declared."
        );
    }

    #[test]
    fn a_search_the_server_paged_itself_is_stated_as_part_of_the_answer() {
        assert_eq!(
            count_sentence(window(1, 25, 25), "value sets", Some(500)),
            "Showing value sets 1 to 25 of 25. This root counted 500 and sent 25, \
             so this is part of the answer.",
            "a total larger than the entries carried means the server paged the search"
        );
    }

    #[test]
    fn a_window_draws_the_rows_of_the_page_it_is_on() {
        let view = window(2, 10, 25);
        assert_eq!(
            view.indexes().collect::<Vec<usize>>(),
            (10..20).collect::<Vec<usize>>()
        );
        assert_eq!(view.position(), "Page 2 of 3");
        assert_eq!(
            view.summary("value sets"),
            "Showing value sets 11 to 20 of 25."
        );
    }

    #[test]
    fn a_short_last_page_draws_only_the_rows_it_has() {
        let view = window(3, 10, 25);
        assert_eq!(view.indexes().count(), 5);
        assert_eq!(
            view.summary("value sets"),
            "Showing value sets 21 to 25 of 25."
        );
    }

    #[test]
    fn a_page_past_the_end_is_clamped_so_the_rows_and_the_count_agree() {
        let view = window(9, 10, 25);
        assert_eq!(
            view.position(),
            "Page 3 of 3",
            "the reader is told about the page that is drawn, never one that is not"
        );
        assert_eq!(view.indexes().count(), 5);
    }

    #[test]
    fn an_empty_answer_is_one_page_rather_than_none() {
        let view = window(1, 10, 0);
        assert_eq!(view.position(), "Page 1 of 1");
        assert_eq!(view.indexes().count(), 0);
        assert_eq!(
            view.summary("Concept maps"),
            "No Concept maps on this page."
        );
    }
}
