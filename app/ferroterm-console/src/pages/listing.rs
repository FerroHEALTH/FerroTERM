// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the three listings share (T1): the find field and the Source
//! filter in the address, the sort by name, the page, the filter chips, and
//! the publication status in words.
//!
//! `GET [base]/{type}` answers one `searchset` with every match in it: no
//! FHIR specification requires a server to page a search, and this one does
//! not (<https://hl7.org/fhir/R4B/http.html#search>). So the console filters,
//! sorts and pages the answer it read on its server, and every one of those
//! is a query parameter the next link or form carries. No specification
//! governs the listing: our own design.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::filter::{FilterChoice, FilterMenu, MenuForm, filter_menu};
use ferrohealth_ui::table::model::{
    Direction, ORDER_KEY, PAGE_KEY, Paging, SIZE_KEY, SORT_KEY, Sort, Window,
};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::template::list::{ActiveFilters, FilterChip};
use leptos::prelude::*;

use crate::locale::{Msg, tr, tr_with};
use crate::pages::parts::Source;
use crate::request::Request;
use crate::routes::VERSION_PARAM;

/// The query parameter of the find field.
pub(crate) const QUERY_PARAM: &str = "q";

/// The query parameter of the Source filter.
pub(crate) const SOURCE_PARAM: &str = "source";

/// The column every listing sorts by.
pub(crate) const NAME_COLUMN: &str = "name";

/// What the address asks of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListState {
    /// The words the find field holds, trimmed.
    pub(crate) query: String,
    /// The sources the filter keeps; every source when it names none.
    pub(crate) sources: Vec<Source>,
    /// The order the names are listed in.
    pub(crate) direction: Direction,
    /// The page and its size.
    pub(crate) paging: Paging,
}

impl ListState {
    /// What `request`'s address asks of the listing, the page size the
    /// reader's preference when the address names none.
    #[must_use]
    pub(crate) fn read(request: &Request) -> Self {
        let size = request
            .param(SIZE_KEY)
            .unwrap_or_else(|| request.prefs.page_size.to_string());
        let sort = Sort::read(
            request.param(SORT_KEY).as_deref(),
            request.param(ORDER_KEY).as_deref(),
            &[NAME_COLUMN],
        );
        Self {
            query: request.text(QUERY_PARAM),
            sources: crate::request::params_in(&request.query, SOURCE_PARAM)
                .iter()
                .filter_map(|key| Source::parse(key))
                .collect(),
            direction: sort.map_or(Direction::Ascending, |sort| sort.direction),
            paging: Paging::read(request.param(PAGE_KEY).as_deref(), Some(&size)),
        }
    }

    /// The sort in force, which the name column's header carries.
    #[must_use]
    pub(crate) fn sort(&self) -> Sort {
        Sort {
            column: String::from(NAME_COLUMN),
            direction: self.direction,
        }
    }

    /// Whether `row`, of the name `label`, the canonical `url` and the source
    /// `source`, is one the filter keeps.
    #[must_use]
    pub(crate) fn keeps(&self, label: &str, url: &str, source: Source) -> bool {
        let words = self.query.to_lowercase();
        let found = words.is_empty()
            || label.to_lowercase().contains(&words)
            || url.to_lowercase().contains(&words);
        found && (self.sources.is_empty() || self.sources.contains(&source))
    }

    /// `rows` kept by the filter, named by `name`, its canonical by `url` and
    /// its source by `source`, sorted by name in the order in force.
    #[must_use]
    pub(crate) fn kept<T>(
        &self,
        rows: Vec<T>,
        name: impl Fn(&T) -> String,
        url: impl Fn(&T) -> String,
        source: impl Fn(&T) -> Source,
    ) -> Vec<T> {
        let mut kept: Vec<T> = rows
            .into_iter()
            .filter(|row| self.keeps(&name(row), &url(row), source(row)))
            .collect();
        kept.sort_by_key(|row| name(row).to_lowercase());
        if self.direction == Direction::Descending {
            kept.reverse();
        }
        kept
    }

    /// The window of `total` rows the page shows.
    #[must_use]
    pub(crate) fn window(&self, total: usize) -> Window {
        Window::over(u32::try_from(total).unwrap_or(u32::MAX), self.paging)
    }

    /// The fields the address carries other than `except`, for a form or a
    /// link that changes one of them and keeps the rest.
    #[must_use]
    pub(crate) fn fields(&self, request: &Request, except: &[&str]) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        let mut push = |name: &str, value: String| {
            if !except.contains(&name) && !value.is_empty() {
                fields.push((name.to_owned(), value));
            }
        };
        if request.version_named {
            push(VERSION_PARAM, request.version.segment().to_owned());
        }
        push(QUERY_PARAM, self.query.clone());
        for source in &self.sources {
            push(SOURCE_PARAM, source.key().to_owned());
        }
        if self.direction == Direction::Descending {
            push(SORT_KEY, String::from(NAME_COLUMN));
            push(ORDER_KEY, String::from("desc"));
        }
        push(SIZE_KEY, self.paging.size.to_string());
        fields
    }

    /// The listing's address at `path` with `fields`.
    #[must_use]
    pub(crate) fn href(path: &str, fields: &[(String, String)]) -> String {
        if fields.is_empty() {
            return path.to_owned();
        }
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        format!("{path}?{query}")
    }

    /// The filter chips of the listing at `path`, each a link that removes
    /// it, and Clear filters; `shown` how many rows the filters keep.
    #[must_use]
    pub(crate) fn chips(&self, request: &Request, path: &str, shown: usize) -> ActiveFilters {
        let mut chips = Vec::new();
        if !self.query.is_empty() {
            chips.push(FilterChip {
                label: tr_with(
                    Msg::ListingFindChip,
                    &[("words", self.query.clone().into())],
                ),
                remove: Self::href(path, &self.fields(request, &[QUERY_PARAM, PAGE_KEY])),
            });
        }
        for source in &self.sources {
            let mut kept = self.fields(request, &[SOURCE_PARAM, PAGE_KEY]);
            kept.extend(
                self.sources
                    .iter()
                    .filter(|other| *other != source)
                    .map(|other| (String::from(SOURCE_PARAM), other.key().to_owned())),
            );
            chips.push(FilterChip {
                label: tr_with(Msg::ListingSourceChip, &[("source", source.word().into())]),
                remove: Self::href(path, &kept),
            });
        }
        ActiveFilters {
            chips,
            clear: Self::href(
                path,
                &self.fields(request, &[QUERY_PARAM, SOURCE_PARAM, PAGE_KEY]),
            ),
            shown: Some(u64::try_from(shown).unwrap_or(u64::MAX)),
        }
    }

    /// The Source filter (T1's filter menu): a checkbox per source among
    /// `counts` that holds a row, each with its count, sent with the find
    /// field's form.
    #[must_use]
    pub(crate) fn source_filter(&self, counts: &[(Source, usize)]) -> AnyView {
        let choices = counts
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(source, count)| {
                FilterChoice::new(
                    source.key(),
                    source.word(),
                    u64::try_from(*count).unwrap_or(u64::MAX),
                    self.sources.is_empty() || self.sources.contains(source),
                )
            })
            .collect();
        filter_menu(FilterMenu {
            id: "source-filter",
            name: SOURCE_PARAM,
            glyph: Glyph::LIST_FILTER,
            label: tr(Msg::ListingSource).into(),
            title: tr(Msg::ListingSourceShow),
            choices,
            note: None,
            form: MenuForm::Enclosing,
        })
    }
}

/// One listing page as the three listings draw it.
#[derive(Debug)]
pub(crate) struct Frame {
    /// The page's title, its card's title and its table's caption.
    pub(crate) title: String,
    /// The one sentence under the title.
    pub(crate) subtitle: String,
    /// The title row's action group: New, where the reader may create.
    pub(crate) actions: ferrohealth_ui::view::action::ActionGroup,
    /// The listing's own path.
    pub(crate) path: String,
    /// How many rows the root serves, unfiltered.
    pub(crate) total: usize,
    /// How many the filters keep.
    pub(crate) kept: usize,
    /// How many rows each source counts.
    pub(crate) counts: Vec<(Source, usize)>,
    /// The find field's label and placeholder.
    pub(crate) find: String,
    /// The table, or the line of an empty listing.
    pub(crate) body: AnyView,
    /// The pager's name and the noun of its rows.
    pub(crate) pages: (String, ferrohealth_ui::table::model::Noun),
}

impl ListState {
    /// The listing page `frame`, as the address filters, sorts and pages it.
    #[must_use]
    pub(crate) fn frame(&self, request: &Request, frame: Frame) -> AnyView {
        use ferrohealth_ui::table::filter::{Find, find_form};
        use ferrohealth_ui::table::model::{SIZES, StateSource};
        use ferrohealth_ui::table::pager::{Listing, link_pager};
        use ferrohealth_ui::view::surface::Header;
        use ferrohealth_ui::view::template::list::{ListCard, ListPage, list_page};
        let Frame {
            title,
            subtitle,
            actions,
            path,
            total,
            kept,
            counts,
            find,
            body,
            pages,
        } = frame;
        let window = self.window(kept);
        let pager = (window.pages > 1).then(|| {
            let fields = self.fields(request, &[PAGE_KEY, SIZE_KEY]);
            let (label, noun) = pages;
            link_pager(
                Listing {
                    label: label.into(),
                    noun,
                    sizes: &SIZES,
                    stated: None,
                },
                window,
                &|paging| {
                    let mut each = fields.clone();
                    each.push((String::from(PAGE_KEY), paging.page.to_string()));
                    each.push((String::from(SIZE_KEY), paging.size.to_string()));
                    Self::href(&path, &each)
                },
            )
        });
        let fields = self.fields(request, &[SOURCE_PARAM]);
        let toolbar = find_form(
            Find {
                id: "listing-find",
                name: QUERY_PARAM,
                label: &find,
                value: &self.query,
                source: StateSource::Url,
                action: None,
                fields: &fields,
            },
            self.source_filter(&counts),
        );
        let page = list_page(ListPage {
            header: Header {
                title: title.clone().into(),
                subtitle: Some(subtitle.into()),
                subtitle_view: None,
                crumbs: Vec::new(),
                mono: false,
                badge: None,
            },
            actions,
            total: Some(u64::try_from(total).unwrap_or(u64::MAX)),
            summary: None,
            toolbar: Some(toolbar),
            filters: self.chips(request, &path, kept),
            card: ListCard {
                title: title.clone(),
                count: Some(HeadingCount::All(u64::try_from(kept).unwrap_or(u64::MAX))),
                order: None,
                tabs: None,
                actions: None,
                body,
                pager,
            },
        });
        view! {
            <leptos_meta::Title text=title />
            {page}
        }
        .into_any()
    }

    /// The sorting of the listing's table, which carries the rest of the
    /// address in its form; `sort` and `fields` are the caller's, which the
    /// sorting borrows.
    #[must_use]
    pub(crate) fn sorting<'a>(
        sort: &'a Sort,
        fields: &'a [(String, String)],
    ) -> ferrohealth_ui::table::view::Sorting<'a> {
        ferrohealth_ui::table::view::Sorting {
            sort: Some(sort),
            source: ferrohealth_ui::table::model::StateSource::Url,
            action: None,
            fields,
            stated: None,
        }
    }

    /// The line of a listing the filter emptied, with the link that clears it.
    #[must_use]
    pub(crate) fn emptied(&self, request: &Request, path: &str, line: String) -> AnyView {
        let clear = Self::href(
            path,
            &self.fields(request, &[QUERY_PARAM, SOURCE_PARAM, PAGE_KEY]),
        );
        ferrohealth_ui::view::template::list::list_empty(line, Some((tr(Msg::ListingClear), clear)))
    }
}

/// How many of `sources` each source counts, in a fixed order.
#[must_use]
pub(crate) fn counted(sources: impl Iterator<Item = Source>) -> Vec<(Source, usize)> {
    let mut counts: [(Source, usize); 3] =
        [(Source::Stored, 0), (Source::Index, 0), (Source::Loaded, 0)];
    for source in sources {
        if let Some((_, count)) = counts.iter_mut().find(|(each, _)| *each == source) {
            *count = count.saturating_add(1);
        }
    }
    counts.to_vec()
}

/// A publication status as the F12 state: Active, Draft, Retired, or the
/// code as the resource wrote it.
#[must_use]
pub(crate) fn status_state(status: Option<&str>) -> AnyView {
    let (mark, words) = match status {
        Some("active") => (Mark::Ok, tr(Msg::StatusActive)),
        Some("draft") => (Mark::Neutral, tr(Msg::StatusDraft)),
        Some("retired") => (Mark::Off, tr(Msg::StatusRetired)),
        Some(other) => (Mark::Neutral, other.to_owned()),
        None => (Mark::Neutral, tr(Msg::StatusUnknown)),
    };
    state(mark, words, None)
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::table::model::{Direction, Paging};

    use super::{ListState, counted};
    use crate::pages::parts::Source;

    fn state(query: &str, sources: &[Source], direction: Direction) -> ListState {
        ListState {
            query: query.to_owned(),
            sources: sources.to_vec(),
            direction,
            paging: Paging::default(),
        }
    }

    #[test]
    fn the_find_field_matches_the_name_or_the_canonical_in_any_case() {
        let listing = state("RESP", &[], Direction::Ascending);
        assert!(listing.keeps("Respiratory findings", "http://x/a", Source::Stored));
        assert!(listing.keeps("A", "http://x/respiratory", Source::Loaded));
        assert!(!listing.keeps("Skin", "http://x/skin", Source::Stored));
    }

    #[test]
    fn the_source_filter_keeps_the_sources_it_names() {
        let listing = state("", &[Source::Index, Source::Loaded], Direction::Ascending);
        assert!(listing.keeps("A", "u", Source::Index));
        assert!(listing.keeps("A", "u", Source::Loaded));
        assert!(!listing.keeps("A", "u", Source::Stored));
        assert!(state("", &[], Direction::Ascending).keeps("A", "u", Source::Stored));
    }

    #[test]
    fn the_source_filter_offers_a_checkbox_per_source_that_holds_a_row() {
        use leptos::prelude::{Owner, RenderHtml};
        let counts = [(Source::Stored, 2), (Source::Index, 0), (Source::Loaded, 3)];
        let html = Owner::new().with(|| {
            state("", &[Source::Loaded], Direction::Ascending)
                .source_filter(&counts)
                .to_html()
        });
        assert_eq!(2, html.matches("type=\"checkbox\"").count(), "{html}");
        assert!(html.contains("value=\"loaded\" checked"), "{html}");
        assert!(!html.contains("value=\"stored\" checked"), "{html}");
        assert!(!html.contains("value=\"index\""), "{html}");
    }

    #[test]
    fn the_rows_sort_by_name_in_the_order_in_force() {
        let rows = vec!["b", "C", "a"];
        let kept = state("", &[], Direction::Descending).kept(
            rows,
            |row| (*row).to_owned(),
            |_| String::new(),
            |_| Source::Stored,
        );
        assert_eq!(vec!["C", "b", "a"], kept);
    }

    #[test]
    fn the_sources_are_counted_in_a_fixed_order() {
        let counts = counted([Source::Loaded, Source::Stored, Source::Loaded].into_iter());
        assert_eq!(
            vec![(Source::Stored, 1), (Source::Index, 0), (Source::Loaded, 2)],
            counts
        );
    }
}
