// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The problem types of the server, and the error bodies of the paths of the
//! FHIR listener that name no FHIR base.
//!
//! The server's own types are defined once, under `ferroterm_problem::BASE`,
//! the book page that documents each one (RFC 9457 §4), beside the access
//! refusals every service shares (`ferroterm_problem::Access`). The FHIR
//! bases keep the `OperationOutcome` FHIR prescribes (FHIR R4B `http.html`):
//! [`is_fhir`] tells them apart.

use std::sync::{Arc, LazyLock};

use ferrohealth_server::panic::PanicBody;
use ferrohealth_server::refusal::RefusalBody;
use ferrohealth_server::request_id::RequestIds;
use ferroterm_problem::Registry;
use http::{StatusCode, Uri};

/// A problem type of the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `500`: the served set could not be rebuilt, so the old one answers.
    ReloadFailed,
}

/// The registry of [`Kind`].
static REGISTRY: LazyLock<Registry<Kind>> = LazyLock::new(Registry::new);

impl ferroterm_problem::Kind for Kind {
    const ALL: &'static [Self] = &[Self::ReloadFailed];

    fn definition(self) -> (&'static str, &'static str, StatusCode) {
        match self {
            Self::ReloadFailed => (
                "reload-failed",
                "The served set could not be rebuilt; the previous one still answers",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        }
    }

    fn registry() -> &'static Registry<Self> {
        &REGISTRY
    }
}

/// Whether `path` lies under one of the FHIR bases, whose error bodies are
/// the `OperationOutcome` FHIR prescribes.
#[must_use]
pub fn is_fhir(path: &str) -> bool {
    crate::served::BASES.iter().any(|base| {
        path.strip_prefix(base.as_str())
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

    use ferroterm_problem::{BASE, Kind as _};

    use super::{Kind, is_fhir};

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
        let page = include_str!("../../../website/book/src/operate/problems.md")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
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
}
