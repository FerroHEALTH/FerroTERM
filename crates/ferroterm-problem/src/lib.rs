// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RFC 9457 problem document of every FerroTERM service outside the FHIR
//! bases.
//!
//! RFC 9457 (<https://www.rfc-editor.org/rfc/rfc9457>) is the shape of every
//! such error body. A service defines its problem types once, as a [`Kind`],
//! under [`BASE`], the book page that documents each one (RFC 9457 §4); the
//! access refusals both services write are [`Access`]. A refusal whose status
//! says all there is to say is `about:blank` ([`blank`]). A detail is a fixed
//! sentence and quotes nothing from the request (§5).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

use axum::response::{IntoResponse as _, Response};
use ferrohealth_server::problem::{Problem, ProblemType, ProblemTypes};
use http::{HeaderValue, StatusCode, header};

/// The base URI of every FerroTERM problem type: the book page that
/// documents them, one heading per slug.
pub const BASE: &str = "https://ferroterm.eu/docs/operate/problems.html#";

/// The problem types of one service, each defined once under [`BASE`].
pub trait Kind: Copy + Ord + fmt::Debug + Send + Sync + 'static {
    /// Every kind, in the order the book lists them.
    const ALL: &[Self];

    /// The slug, the fragment of the type under [`BASE`], its title and its
    /// status.
    fn definition(self) -> (&'static str, &'static str, StatusCode);

    /// The registry the service built its kinds into, once.
    fn registry() -> &'static Registry<Self>;

    /// The problem type of this kind, as the registry defines it.
    fn problem_type(self) -> &'static ProblemType {
        Self::registry().problem_type(self)
    }

    /// A problem of this kind with `detail`.
    fn problem(self, detail: impl Into<String>) -> Problem {
        Problem::of(self.problem_type()).with_detail(detail)
    }
}

/// The problem types of every kind of one service, and the slug of each kind.
#[derive(Debug)]
pub struct Registry<K> {
    types: ProblemTypes,
    by_kind: BTreeMap<K, &'static str>,
}

impl<K: Kind> Registry<K> {
    /// Defines every kind of `K` under [`BASE`].
    ///
    /// # Panics
    ///
    /// Never: [`BASE`] and every definition are constants, which a unit test
    /// of each service builds.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "BASE and every definition are constants that each service's every_kind_is_defined_once builds"
    )]
    pub fn new() -> Self {
        let mut types = ProblemTypes::new(BASE).expect("the problem base should be a legal base");
        let mut by_kind = BTreeMap::new();
        for &kind in K::ALL {
            let (slug, title, status) = kind.definition();
            types
                .define(slug, title, status)
                .expect("every problem type should be defined once with a legal slug");
            by_kind.insert(kind, slug);
        }
        Self { types, by_kind }
    }

    /// The problem type of `kind`.
    ///
    /// # Panics
    ///
    /// Never: [`Registry::new`] defines every kind, which a unit test holds.
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "Registry::new defines every K::ALL entry, which every_kind_is_defined_once checks"
    )]
    pub fn problem_type(&self, kind: K) -> &ProblemType {
        self.by_kind
            .get(&kind)
            .and_then(|slug| self.types.get(slug))
            .expect("every kind should be defined in the registry")
    }

    /// The problem types, in slug order.
    #[must_use]
    pub fn types(&self) -> &ProblemTypes {
        &self.types
    }
}

impl<K: Kind> Default for Registry<K> {
    fn default() -> Self {
        Self::new()
    }
}

/// The access refusals of an admin listener, which both services write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    /// `401`: the request carries no valid credential.
    Unauthenticated,
    /// `403`: the credential does not permit the request.
    NotPermitted,
    /// `503`: the credential could not be checked.
    AuthorizationUnavailable,
}

/// The registry of [`Access`].
static ACCESS: LazyLock<Registry<Access>> = LazyLock::new(Registry::new);

impl Kind for Access {
    const ALL: &[Self] = &[
        Self::Unauthenticated,
        Self::NotPermitted,
        Self::AuthorizationUnavailable,
    ];

    fn definition(self) -> (&'static str, &'static str, StatusCode) {
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
        }
    }

    fn registry() -> &'static Registry<Self> {
        &ACCESS
    }
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

/// Returns the response to a request an admin listener refused with `status`.
///
/// The body is the [`Access`] problem of that status with `detail`, or an
/// `about:blank` one for any other status, with the `WWW-Authenticate` value
/// of `challenge` when one is given.
#[must_use]
pub fn refused(status: StatusCode, detail: &str, challenge: Option<&HeaderValue>) -> Response {
    let problem = match status {
        StatusCode::UNAUTHORIZED => Access::Unauthenticated.problem(detail),
        StatusCode::FORBIDDEN => Access::NotPermitted.problem(detail),
        StatusCode::SERVICE_UNAVAILABLE => Access::AuthorizationUnavailable.problem(detail),
        status => Problem::new(status).with_detail(detail),
    };
    respond(problem, challenge)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use http::StatusCode;

    use super::{Access, BASE, Kind, blank};

    #[test]
    fn every_kind_is_defined_once_under_the_book_page() {
        let mut slugs = BTreeSet::new();
        for &kind in Access::ALL {
            let defined = kind.problem_type();
            let (slug, title, status) = kind.definition();
            assert_eq!(format!("{BASE}{slug}"), defined.uri());
            assert_eq!(Some(title), defined.title());
            assert_eq!(status, defined.status());
            assert!(slugs.insert(slug), "{slug} is defined twice");
        }
        assert_eq!(Access::ALL.len(), Access::registry().types().iter().count());
    }

    #[test]
    fn every_type_is_documented_on_the_book_page_with_its_title_and_status() {
        let page = include_str!("../../../website/book/src/operate/problems.md")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for &kind in Access::ALL {
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
            header(http::header::CONTENT_TYPE)
        );
        assert_eq!(
            Some("no-store".to_owned()),
            header(http::header::CACHE_CONTROL)
        );
    }
}
