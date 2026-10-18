//! What a picker offers: the resources of one type this root publishes.
//!
//! A canonical is what every operation takes and what almost no reader knows
//! by heart, so a runner offers what the root holds rather than waiting for
//! one to be typed. The same read answers the version beside it, because a
//! version a reader can pick is one the root actually published.
//!
//! The read is the narrowed search in `fhir::named`, so offering twenty
//! resources and their versions costs four kilobytes.

use leptos::prelude::*;

use crate::fhir::FhirClient;
use crate::fhir::named::Choice;
use crate::fhir::named::NamedSearch;
use crate::fhir::version::FhirVersion;

/// The resources of `resource_type` this root publishes.
///
/// The read follows the version switcher, because a root serves what that
/// version's search answers. A refused read publishes nothing, which leaves
/// the reader the text field beside the picker rather than an error over a
/// form that still works.
pub(crate) fn published(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    resource_type: &'static str,
) -> Memo<NamedSearch> {
    let client = client.clone();
    // NOTE: Leptos book, `async/10_resources`: a read a memo outside every
    // suspense boundary consumes is the browser's own, so hydration never sees it.
    let read = LocalResource::new(move || {
        let client = client.clone();
        let version = version.get();
        async move { client.published_names(version, resource_type).await }
    });
    Memo::new(move |_| {
        read.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .cloned()
                .unwrap_or_default()
        })
    })
}

/// Those resources, as the offers a picker over a canonical makes.
pub(crate) fn choices(published: Memo<NamedSearch>) -> Memo<Vec<Choice>> {
    Memo::new(move |_| published.with(NamedSearch::choices))
}

/// The versions of `canonical`, as the offers a picker over a version makes.
///
/// The offers follow the canonical beside them, so picking a code system fills
/// the versions of that code system and nothing else.
pub(crate) fn versions(published: Memo<NamedSearch>, canonical: Memo<String>) -> Memo<Vec<Choice>> {
    Memo::new(move |_| canonical.with(|canonical| published.with(|held| held.versions(canonical))))
}
