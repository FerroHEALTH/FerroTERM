// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The problem types of the sync service's admin listener, beside the access
//! refusals every service shares (`ferroterm_problem::Access`).
//!
//! Each is defined once, under `ferroterm_problem::BASE`, the book page that
//! documents it (RFC 9457 §4).

use std::sync::LazyLock;

use axum::http::StatusCode;
use ferroterm_problem::Registry;

/// A problem type of the sync service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `404`: the service holds no such run.
    RunNotFound,
    /// `500`: the run records could not be read.
    RecordsUnreadable,
}

/// The registry of [`Kind`].
static REGISTRY: LazyLock<Registry<Kind>> = LazyLock::new(Registry::new);

impl ferroterm_problem::Kind for Kind {
    const ALL: &[Self] = &[Self::RunNotFound, Self::RecordsUnreadable];

    fn definition(self) -> (&'static str, &'static str, StatusCode) {
        match self {
            Self::RunNotFound => (
                "run-not-found",
                "The sync service holds no such run",
                StatusCode::NOT_FOUND,
            ),
            Self::RecordsUnreadable => (
                "records-unreadable",
                "The sync service could not read its run records",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        }
    }

    fn registry() -> &'static Registry<Self> {
        &REGISTRY
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferroterm_problem::{BASE, Kind as _};

    use super::Kind;

    #[test]
    fn every_kind_is_defined_once_under_the_book_page() {
        let mut slugs = BTreeSet::new();
        for &kind in Kind::ALL {
            let defined = kind.problem_type();
            let (slug, title, status) = kind.definition();
            assert_eq!(format!("{BASE}{slug}"), defined.uri());
            assert_eq!(Some(title), defined.title());
            assert_eq!(status, defined.status());
            assert!(slugs.insert(slug), "{slug} is defined twice");
        }
        assert_eq!(Kind::ALL.len(), Kind::registry().types().iter().count());
    }

    #[test]
    fn every_type_is_documented_on_the_book_page_with_its_title_and_status() {
        let page = include_str!("../../../website/book/src/operate/problems.md");
        for &kind in Kind::ALL {
            let (slug, title, status) = kind.definition();
            let row = format!("| [`{slug}`](#{slug}) | `{}` |", status.as_u16());
            assert!(page.contains(&row), "the table lists {slug}: {row}");
            assert!(
                page.contains(&format!("## `{slug}`")),
                "a section for {slug}"
            );
            assert!(page.contains(title), "the page names the title of {slug}");
        }
    }
}
