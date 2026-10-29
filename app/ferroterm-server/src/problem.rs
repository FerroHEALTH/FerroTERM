// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RFC 9457 problem details of every error outside the FHIR bases.
//!
//! RFC 9457 (<https://www.rfc-editor.org/rfc/rfc9457>) covers the admin
//! listener and the paths of the FHIR listener that name no FHIR base.
//!
//! The server's problem types are defined once, in [`types`], under
//! [`BASE`], the book page that documents each one (RFC 9457 §4). A refusal
//! whose status says all there is to say is `about:blank` ([`blank`]). A
//! detail is a fixed sentence and quotes nothing from the request (§5). The
//! FHIR bases keep the `OperationOutcome` FHIR prescribes (FHIR R4B
//! `http.html`): [`is_fhir`] tells them apart.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use axum::response::{IntoResponse as _, Response};
use ferrohealth_server::panic::PanicBody;
use ferrohealth_server::problem::{Problem, ProblemType, ProblemTypes};
use ferrohealth_server::refusal::RefusalBody;
use ferrohealth_server::request_id::RequestIds;
use http::{HeaderValue, StatusCode, Uri, header};

use crate::FHIR_BASES;

/// The base URI of every FerroTERM problem type: the book page that
/// documents them, one heading per slug.
pub const BASE: &str = "https://ferroterm.eu/docs/operate/problems.html#";

/// A problem type of the server, each defined once in [`types`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `401`: the request carries no valid credential.
    Unauthenticated,
    /// `403`: the credential does not permit the request.
    NotPermitted,
    /// `503`: the credential could not be checked.
    AuthorizationUnavailable,
    /// `500`: the served set could not be rebuilt, so the old one answers.
    ReloadFailed,
}

impl Kind {
    /// Every kind, in the order the book lists them.
    pub const ALL: [Self; 4] = [
        Self::Unauthenticated,
        Self::NotPermitted,
        Self::AuthorizationUnavailable,
        Self::ReloadFailed,
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
            Self::ReloadFailed => (
                "reload-failed",
                "The served set could not be rebuilt; the previous one still answers",
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

/// The server's problem types, in slug order.
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

/// Whether `path` lies under one of the FHIR bases, whose error bodies are
/// the `OperationOutcome` FHIR prescribes.
#[must_use]
pub fn is_fhir(path: &str) -> bool {
    FHIR_BASES.iter().any(|base| {
        path.strip_prefix(base)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The body of a request whose handler panicked on the FHIR listener: the
/// `OperationOutcome` under a FHIR base, a problem elsewhere.
#[must_use]
pub fn panic_body() -> PanicBody {
    let (fhir, problem) = (
        crate::outcome::panic_body(),
        ferrohealth_server::problem::panic_body(),
    );
    Arc::new(move |uri: &Uri, ids: RequestIds<'_>| {
        if is_fhir(uri.path()) {
            fhir(uri, ids)
        } else {
            problem(uri, ids)
        }
    })
}

/// The body of a refusal the stack writes itself on the FHIR listener: the
/// `OperationOutcome` under a FHIR base, an `about:blank` problem elsewhere.
#[must_use]
pub fn refusal_body() -> RefusalBody {
    let (fhir, problem) = (
        crate::outcome::refusal_body(),
        ferrohealth_server::problem::refusal_body(),
    );
    Arc::new(
        move |uri: &Uri, ids: RequestIds<'_>, status: StatusCode, reason: &str| {
            if is_fhir(uri.path()) {
                fhir(uri, ids, status, reason)
            } else {
                problem(uri, ids, status, reason)
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use http::StatusCode;

    use super::{BASE, Kind, blank, is_fhir, types};

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
        let page = include_str!("../../../website/book/src/operate/problems.md")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
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
    fn the_fhir_bases_keep_their_outcomes() {
        for path in [
            "/r4",
            "/r4/metadata",
            "/r4b/CodeSystem",
            "/r5",
            "/r6/ValueSet/x",
        ] {
            assert!(is_fhir(path), "{path}");
        }
        for path in [
            "/", "/metrics", "/health", "/r4x", "/reload", "/r", "/r44/x",
        ] {
            assert!(!is_fhir(path), "{path}");
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
