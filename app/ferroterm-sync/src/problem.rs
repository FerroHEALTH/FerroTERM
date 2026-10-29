// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RFC 9457 problem details of every error of the admin listener.
//!
//! RFC 9457 (<https://www.rfc-editor.org/rfc/rfc9457>) is the shape of every
//! error body the sync service writes. Its problem types are defined once,
//! in [`types`], under [`BASE`], the book page that documents each one
//! (RFC 9457 §4), the page the server's types share. A refusal whose status
//! says all there is to say is `about:blank` ([`blank`]). A detail is a
//! fixed sentence and quotes nothing from the request (§5).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use ferrohealth_server::problem::{Problem, ProblemType, ProblemTypes};

/// The base URI of every FerroTERM problem type: the book page that
/// documents them, one heading per slug.
pub const BASE: &str = "https://ferroterm.eu/docs/operate/problems.html#";

/// A problem type of the sync service, each defined once in [`types`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `401`: the request carries no valid credential.
    Unauthenticated,
    /// `403`: the credential does not permit the request.
    NotPermitted,
    /// `503`: the credential could not be checked.
    AuthorizationUnavailable,
    /// `404`: the service holds no such run.
    RunNotFound,
    /// `500`: the run records could not be read.
    RecordsUnreadable,
}

impl Kind {
    /// Every kind, in the order the book lists them.
    pub const ALL: [Self; 5] = [
        Self::Unauthenticated,
        Self::NotPermitted,
        Self::AuthorizationUnavailable,
        Self::RunNotFound,
        Self::RecordsUnreadable,
    ];

    /// The slug, the fragment of the type under [`BASE`], its title and its
    /// status.
    #[must_use]
    pub const fn definition(self) -> (&'static str, &'static str, StatusCode) {
        match self {
            Self::Unauthenticated => (
                "unauthenticated",
                "The request carries no valid credential",
                StatusCode::UNAUTHORIZED,
            ),
            Self::NotPermitted => (
                "not-permitted",
                "The credential does not permit this request",
                StatusCode::FORBIDDEN,
            ),
            Self::AuthorizationUnavailable => (
                "authorization-unavailable",
                "The credential could not be checked",
                StatusCode::SERVICE_UNAVAILABLE,
            ),
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

    /// The problem type of this kind, as [`types`] defines it.
    ///
    /// # Panics
    ///
    /// Never: the registry defines every kind, which a unit test holds.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "registry() defines every Kind::ALL entry, which every_kind_is_defined_once checks"
    )]
    pub fn problem_type(self) -> &'static ProblemType {
        let (types, by_kind) = &*REGISTRY;
        by_kind
            .get(&self)
            .and_then(|slug| types.get(slug))
            .expect("every kind should be defined in the registry")
    }

    /// A problem of this kind with `detail`.
    #[must_use]
    pub fn problem(self, detail: impl Into<String>) -> Problem {
        self.problem_type().problem().with_detail(detail)
    }
}

/// The registry and the slug of each kind.
static REGISTRY: LazyLock<(ProblemTypes, BTreeMap<Kind, &'static str>)> = LazyLock::new(registry);

/// Builds the registry of every [`Kind`].
#[expect(
    clippy::expect_used,
    reason = "BASE and every definition are constants that every_kind_is_defined_once builds"
)]
fn registry() -> (ProblemTypes, BTreeMap<Kind, &'static str>) {
    let mut types = ProblemTypes::new(BASE).expect("the problem base should be a legal base");
    let mut by_kind = BTreeMap::new();
    for kind in Kind::ALL {
        let (slug, title, status) = kind.definition();
        types
            .define(slug, title, status)
            .expect("every problem type should be defined once with a legal slug");
        by_kind.insert(kind, slug);
    }
    (types, by_kind)
}

/// The sync service's problem types, in slug order.
#[must_use]
pub fn types() -> &'static ProblemTypes {
    &REGISTRY.0
}

/// Returns `problem` as its response, never kept by a cache, with the
/// `WWW-Authenticate` value of `challenge` when one is given.
#[must_use]
pub fn respond(problem: Problem, challenge: Option<&HeaderValue>) -> Response {
    let mut response = problem.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(value) = challenge {
        headers.insert(header::WWW_AUTHENTICATE, value.clone());
    }
    response
}

/// Returns the response of an `about:blank` problem of `status`, whose
/// status says what there is to say, with `detail` (RFC 9457 §4.2.1).
#[must_use]
pub fn blank(status: StatusCode, detail: &'static str) -> Response {
    respond(Problem::new(status).with_detail(detail), None)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use axum::http::StatusCode;

    use super::{BASE, Kind, blank, types};

    #[test]
    fn every_kind_is_defined_once_under_the_book_page() {
        let mut slugs = BTreeSet::new();
        for kind in Kind::ALL {
            let defined = kind.problem_type();
            let (slug, title, status) = kind.definition();
            assert_eq!(format!("{BASE}{slug}"), defined.uri());
            assert_eq!(Some(title), defined.title());
            assert_eq!(status, defined.status());
            assert!(slugs.insert(slug), "{slug} is defined twice");
        }
        assert_eq!(Kind::ALL.len(), types().iter().count());
    }

    #[test]
    fn every_type_is_documented_on_the_book_page_with_its_title_and_status() {
        let page = include_str!("../../../website/book/src/operate/problems.md");
        for kind in Kind::ALL {
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

    #[test]
    fn a_problem_answers_its_status_as_problem_json_no_cache_keeps() {
        let response = blank(StatusCode::NOT_FOUND, "no route");
        assert_eq!(StatusCode::NOT_FOUND, response.status());
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        assert_eq!(
            Some("application/problem+json".to_owned()),
            header(axum::http::header::CONTENT_TYPE)
        );
        assert_eq!(
            Some("no-store".to_owned()),
            header(axum::http::header::CACHE_CONTROL)
        );
    }
}
