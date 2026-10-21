// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The paging arithmetic every paged FHIR result shares.
//!
//! `ValueSet/$expand` pages with `offset` and `count` and reports
//! `expansion.total` (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>),
//! so a page is those two numbers and everything a reader sees is derived from
//! them. The counters are `u32` rather than `usize` because WebAssembly is
//! 32-bit and these numbers travel in URLs.

/// The largest `count` the viewer asks a server for in one request.
///
/// No specification caps `count`: this is our own design, a bound on how much
/// a single page can ask the browser to render.
pub(crate) const MAX_COUNT: u32 = 1_000;

/// One window over a paged result, in the terms `$expand` uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Page {
    offset: u32,
    count: u32,
}

impl Page {
    /// The first page of a result rendered `count` rows at a time.
    ///
    /// `count` is clamped into `1..=MAX_COUNT`, so a stored or typed value can
    /// never ask for a zero-row page that would never advance.
    pub(crate) fn first(count: u32) -> Self {
        Self::at(0, count)
    }

    /// The page starting at `offset`, rendered `count` rows at a time.
    ///
    /// Both numbers arrive from an address a reader can type, so `count` is
    /// clamped the same way here as anywhere else. `offset` is taken as it
    /// was given: an offset past the end of a result is a page the server
    /// answers empty, and correcting it under the reader would hide that.
    pub(crate) fn at(offset: u32, count: u32) -> Self {
        Self {
            offset,
            count: count.clamp(1, MAX_COUNT),
        }
    }

    /// The `offset` parameter this page sends.
    pub(crate) fn offset(self) -> u32 {
        self.offset
    }

    /// The `count` parameter this page sends.
    pub(crate) fn count(self) -> u32 {
        self.count
    }

    /// The 1-based number a reader is shown.
    #[expect(
        clippy::integer_division,
        reason = "the page number is the whole number of pages skipped; `first` clamps count to at least 1"
    )]
    pub(crate) fn number(self) -> u32 {
        self.offset / self.count + 1
    }

    /// How many pages `total` rows fill at this page's size.
    ///
    /// A result with no rows is one empty page, so the count is never zero and
    /// "page 1 of 0" cannot be rendered.
    pub(crate) fn total_pages(self, total: u32) -> u32 {
        total.div_ceil(self.count).max(1)
    }

    /// The next page, or `None` when this page ends the result.
    pub(crate) fn next(self, total: u32) -> Option<Self> {
        let offset = self.offset.checked_add(self.count)?;
        (offset < total).then_some(Self {
            offset,
            count: self.count,
        })
    }

    /// The previous page, or `None` when this page starts the result.
    pub(crate) fn previous(self) -> Option<Self> {
        let offset = self.offset.checked_sub(self.count)?;
        Some(Self {
            offset,
            count: self.count,
        })
    }

    /// The page holding the last row of `total`, at this page's size.
    ///
    /// An empty result has one page and it is this one, so the walk to the end
    /// always lands somewhere the server answers.
    #[expect(
        clippy::integer_division,
        reason = "the last page starts at the whole number of pages before it; the count is at least 1"
    )]
    pub(crate) fn last(self, total: u32) -> Self {
        Self {
            offset: total.saturating_sub(1) / self.count * self.count,
            count: self.count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_size_of_zero_is_clamped_so_paging_always_advances() {
        let page = Page::first(0);
        assert_eq!(page.count(), 1, "a zero-row page would never reach the end");
        assert_eq!(page.offset(), 0);
    }

    #[test]
    fn a_page_size_above_the_bound_is_clamped() {
        assert_eq!(
            Page::first(50_000).count(),
            MAX_COUNT,
            "one request never asks the browser to render an unbounded page"
        );
    }

    #[test]
    fn walking_forward_advances_the_offset_by_the_page_size() {
        let first = Page::first(20);
        let second = first
            .next(45)
            .expect("45 rows do not fit in one page of 20");
        assert_eq!((second.offset(), second.number()), (20, 2));
        let third = second.next(45).expect("45 rows need a third page of 20");
        assert_eq!((third.offset(), third.number()), (40, 3));
    }

    #[test]
    fn the_last_page_has_no_next() {
        let last = Page::first(20)
            .next(30)
            .expect("30 rows need two pages of 20");
        assert_eq!(last.next(30), None, "offset 40 is past a total of 30");
    }

    #[test]
    fn a_total_that_exactly_fills_the_page_has_no_next() {
        assert_eq!(
            Page::first(20).next(20),
            None,
            "offset 20 is not less than a total of 20"
        );
    }

    #[test]
    fn walking_back_from_the_first_page_is_refused() {
        assert_eq!(
            Page::first(20).previous(),
            None,
            "there is nothing before offset 0"
        );
    }

    #[test]
    fn walking_back_returns_the_page_that_was_left() {
        let second = Page::first(20)
            .next(45)
            .expect("45 rows need a second page");
        assert_eq!(second.previous(), Some(Page::first(20)));
    }

    #[test]
    fn a_partial_last_page_still_counts_as_a_page() {
        assert_eq!(
            Page::first(20).total_pages(41),
            3,
            "41 rows of 20 fill two full pages and one row"
        );
    }

    #[test]
    fn a_typed_offset_is_kept_and_a_typed_page_size_is_clamped() {
        let page = Page::at(35, 0);
        assert_eq!(
            (page.offset(), page.count()),
            (35, 1),
            "the address is read as it was typed, and only the size is bounded"
        );
        assert_eq!(Page::at(35, 50_000).count(), MAX_COUNT);
    }

    #[test]
    fn the_last_page_holds_the_last_row() {
        let page = Page::first(20);
        assert_eq!(page.last(41).offset(), 40, "row 41 sits on the third page");
        assert_eq!(
            page.last(40).offset(),
            20,
            "a total that exactly fills two pages ends on the second"
        );
    }

    #[test]
    fn the_last_page_of_an_empty_result_is_the_first_page() {
        assert_eq!(
            Page::first(20).last(0),
            Page::first(20),
            "the walk to the end lands where the server answers"
        );
    }

    #[test]
    fn an_empty_result_is_one_page_rather_than_none() {
        assert_eq!(
            Page::first(20).total_pages(0),
            1,
            "`page 1 of 0` is not something to render"
        );
    }
}
