// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value sets this root holds: the search and its pages. One value set
//! is read on its own screen, `pages::value_set`.
//!
//! The screen reads and never writes. The RESTful API defines create, update,
//! and delete beside read and search
//! (<https://hl7.org/fhir/R4B/http.html>), and this server answers all of
//! them; the viewer calls none, so nothing here offers one.

use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::NavigateOptions;
use leptos_router::components::Redirect;
use leptos_router::hooks::use_navigate;
use leptos_router::hooks::use_query_map;

use crate::components::failure::Failure;
use crate::components::reading::Reading;
use crate::components::shell::SelectedVersion;
use crate::components::state::empty;
use crate::fhir::FhirClient;
use crate::fhir::error::FhirError;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::searchset::SearchSet;
use crate::fhir::value_set::PublishedValueSet;
use crate::fhir::version::FhirVersion;
use crate::listing::Action;
use crate::listing::ListParams;
use crate::listing::Published;
use crate::listing::SortOrder;
use crate::listing::count_sentence;
use crate::listing::filter_form;
use crate::listing::pager_view;
use crate::listing::published_row;
use crate::listing::published_table;
use crate::listing::window;
use crate::locale::{Msg, message, tr};
use crate::routes::VALUE_SETS_PATH;
use crate::routes::expansion_link;
use crate::routes::resource_link;
use crate::settings::Settings;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::Listing;
use ferrohealth_ui::view::surface::{page_header, stack};

/// Lists the `ValueSet` resources this root holds, and reads one of them.
///
/// The filter, the page, and the resource being read all live in the address,
/// so every one of them is shareable and the back button walks them. Each is
/// read reactively: a click on a row is a navigation onto this same route, and
/// `leptos_router` 0.8.15 then updates the params without re-running this body
/// (`src/nested_router.rs`, the same-route-id branch).
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn ValueSetsPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let page_size = expect_context::<Settings>().page_size;
    let _help = ferrohealth_ui::widget::help::provide_help();

    let query = use_query_map();
    let params: Signal<ListParams> =
        Memo::new(move |_| query.with(|map| ListParams::read(&|name| map.get(name), page_size)))
            .into();

    let heading = view! {
        <Title text=tr(Msg::ValueSets) />
        {page_header(
            tr(Msg::ValueSets),
            Some(tr(Msg::ValuesetResourcesServerPublishesOpenOne)),
            (),
        )}
    }
    .into_any();

    let form = search_form(params, version);
    // The order lives in the address, so an ordered list is a link.
    let order = Memo::new(move |_| query.with(|map| SortOrder::read(&|name| map.get(name))));
    let list = list_section(&client, version, params, order);
    // NOTE: no specification governs this: our own design; an address from
    // before the detail screen named the resource as `?id=`, now its path.
    let moved = move || {
        params.with(|params| {
            (!params.id.is_empty()).then(|| {
                let target = resource_link(VALUE_SETS_PATH, &params.id, version.get());
                view! { <Redirect path=target /> }
            })
        })
    };

    view! {
        {moved}
        {heading}
        {stack(
            view! {
                {form}
                {list}
            },
        )}
    }
}

/// The search filter, as a form that navigates rather than reloading.
fn search_form(params: Signal<ListParams>, version: Signal<FhirVersion>) -> AnyView {
    let canonical: NodeRef<Input> = NodeRef::new();
    let resource_version: NodeRef<Input> = NodeRef::new();
    let navigate = use_navigate();
    let submit = move |event: SubmitEvent| {
        event.prevent_default();
        let typed = params.with(|params| {
            params.searching(SearchFilter {
                url: typed_value(canonical),
                version: typed_value(resource_version),
            })
        });
        // NOTE: the router resolves a navigation against its base, so an
        // address that already carries the base is passed unresolved
        // (`leptos_router` 0.8.15 `matching/resolve_path.rs`).
        navigate(
            &typed.address(VALUE_SETS_PATH, version.get()),
            NavigateOptions {
                resolve: false,
                ..NavigateOptions::default()
            },
        );
    };
    filter_form(
        &tr(Msg::UrlSearchParameterMatchedAgainstValueset),
        canonical,
        resource_version,
        params,
        Box::new(submit),
    )
}

/// The resources the search answered, a page at a time.
fn list_section(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    params: Signal<ListParams>,
    order: Memo<SortOrder>,
) -> AnyView {
    let read_client = client.clone();
    let filter = Memo::new(move |_| params.with(|params| params.filter.clone()));
    let found = Resource::new(
        move || (version.get(), filter.get()),
        move |(version, filter)| {
            let client = read_client.clone();
            async move { client.value_set_search(version, &filter).await }
        },
    );

    // The live region is in the document before the read settles, which is
    // what lets a screen reader announce the count when it arrives. Every
    // number in it comes from the answer rather than from the address, because
    // the transition boundary keeps the previous rows on screen while the next
    // read runs and a sentence built from the address would describe rows that
    // are not there yet.
    let announcement = move || {
        let params = params.get();
        found.with(|answered| {
            answered
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(|found| {
                    let view = window(params.page(), params.size(), found.matched());
                    count_sentence(view, &tr(Msg::ValueSets2), found.total())
                })
                .unwrap_or_default()
        })
    };

    view! {
        <section class="fui-card fui-panel" aria-labelledby="valuesets-heading">
            <div class="fui-panel-head">
                <h2 id="valuesets-heading" class="fui-card-title">
                    {tr(Msg::WhatThisRootHolds)}
                </h2>
                <p aria-live="polite" class="fui-muted fui-small">
                    <Transition fallback=|| ()>{announcement}</Transition>
                </p>
            </div>
            <Reading label=&{
                tr(Msg::ReadingTheValueSets)
            }>
                {move || {
                    let params = params.get();
                    let version = version.get();
                    found
                        .with(|answered| {
                            answered
                                .as_ref()
                                .map(|result| match result {
                                    Ok(found) => list_view(order.get(), found, &params, version),
                                    Err(error) => failure_view(error),
                                })
                        })
                }}
            </Reading>
        </section>
    }
    .into_any()
}

/// The rows of one page, with the controls that walk the rest.
fn list_view(
    order: SortOrder,
    found: &SearchSet<PublishedValueSet>,
    params: &ListParams,
    version: FhirVersion,
) -> AnyView {
    let resources = found.found();
    if resources.is_empty() {
        return empty(tr(Msg::RootHoldsNoValuesetResourceMatching));
    }
    // The whole answer is ordered before it is paged, so a walk through the
    // pages walks the order the reader asked for rather than the server's.
    let mut ordered: Vec<(Published<'_>, usize)> = resources
        .iter()
        .enumerate()
        .map(|(index, resource)| (facts(resource), index))
        .collect();
    order.ordering(&mut ordered);
    let view = window(params.page(), params.size(), resources.len());
    let rows: Vec<AnyView> = view
        .indexes()
        .filter_map(|index| ordered.get(index))
        .filter_map(|(_, index)| resources.get(*index))
        .map(|resource| row_view(resource, version))
        .collect();
    let pager = pager_view(
        Listing::new(tr(Msg::ValueSetPages)).noun(Noun::new(message(Msg::NounValueSets))),
        view,
        params,
        VALUE_SETS_PATH,
        version,
        &[],
    );
    view! {
        {published_table(
            &tr(Msg::ValueSetsRootHolds),
            tr(Msg::ValueSet),
            rows,
            order,
            params,
            VALUE_SETS_PATH,
            version,
        )}
        {pager}
    }
    .into_any()
}

/// The four facts a row draws about one published value set.
fn facts(resource: &PublishedValueSet) -> Published<'_> {
    Published {
        title: resource.label(),
        canonical: resource.url(),
        version: resource.version(),
        status: resource.status(),
    }
}

/// One published value set, as a row that opens it and one that expands it.
fn row_view(resource: &PublishedValueSet, version: FhirVersion) -> AnyView {
    published_row(
        &facts(resource),
        resource
            .id()
            .map(|id| resource_link(VALUE_SETS_PATH, id, version)),
        // The runner takes the canonical as its `url` parameter, so a row
        // hands its subject on rather than making a reader retype one.
        resource.url().map(|canonical| Action {
            label: tr(Msg::Expand),
            href: expansion_link(canonical, version),
        }),
    )
}

/// A refused read, in the server's own words.
fn failure_view(error: &FhirError) -> AnyView {
    let error = error.clone();
    view! {
        <div class="mt-default">
            <Failure error=Signal::stored(error) />
        </div>
    }
    .into_any()
}

/// What a control holds now, trimmed, or the empty string when it is gone.
fn typed_value(node: NodeRef<Input>) -> String {
    node.get()
        .map(|input| input.value())
        .unwrap_or_default()
        .trim()
        .to_owned()
}
