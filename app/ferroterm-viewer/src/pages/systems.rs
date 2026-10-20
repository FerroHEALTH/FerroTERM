// SPDX-License-Identifier: BUSL-1.1
//! The code systems the root in view serves, as one listing.

use ferrohealth_ui::view::surface::page_header;
use ferrohealth_ui::widget::filter::table_filter;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::failure::Failure;
use crate::components::reading::Reading;
use crate::components::shell::SelectedVersion;
use crate::components::system_table::SystemTable;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::FhirClient;
use crate::fhir::named::NamedSearch;
use crate::locale::{Msg, tr};

/// Shows the listing of the code systems the root serves.
///
/// The rows are the root's `TerminologyCapabilities`, named from its
/// published `CodeSystem` resources, so a system this console has never seen
/// draws with no change here.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn SystemsPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let capabilities = {
        let client = client.clone();
        Resource::new(
            move || version.get(),
            move |version| {
                let client = client.clone();
                async move { client.terminology_capabilities(version).await }
            },
        )
    };
    // NOTE: <https://hl7.org/fhir/R5/search.html#elements>: the capability
    // statement names no code system, so the published resources lend the name.
    let published = Resource::new(
        move || version.get(),
        move |version| {
            let client = client.clone();
            async move { client.published_names(version, CODE_SYSTEM).await }
        },
    );
    let words = RwSignal::new(String::new());
    let count = RwSignal::new(String::new());
    let listing = move || {
        let names = published.with(|answered| {
            answered
                .as_ref()
                .and_then(|answer| answer.as_ref().ok())
                .map(NamedSearch::names)
                .unwrap_or_default()
        });
        capabilities.with(|answered| {
            answered.as_ref().map(|answer| match answer {
                Ok(capabilities) => {
                    let cards = capabilities.cards();
                    view! { <SystemTable cards names words count /> }.into_any()
                }
                Err(error) => {
                    let error = error.clone();
                    view! { <Failure error=Signal::stored(error) /> }.into_any()
                }
            })
        })
    };
    view! {
        <Title text=tr(Msg::CodeSystems) />
        // NOTE: no specification governs this: our own design; the header is
        // the title alone, so twenty systems fit one desktop screen in compact.
        {page_header(tr(Msg::CodeSystems), None, ())}
        <section aria-labelledby="systems-heading" class="fui-stack">
            <h2 id="systems-heading" class="fui-sr-only">
                {tr(Msg::CodeSystemsRootServes)}
            </h2>
            {table_filter("systems-filter", tr(Msg::FilterNameCanonical), words, count.into())}
            <Reading label=tr(
                Msg::ReadingTheTerminologyCapabilities,
            )>{move || listing().map(IntoAny::into_any)}</Reading>
        </section>
    }
}
