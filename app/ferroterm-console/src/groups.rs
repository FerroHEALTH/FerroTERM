// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages as lazy routes, one group for each part of the
//! navigation a reader reads together, and how a page is read and drawn.
//!
//! Under `cargo leptos build --split` each group loads its code from chunks
//! of its own (the Leptos book, "Code Splitting"), so the first page costs
//! the entry and its group, and every route of a group draws the page its
//! path names. [`ROUTES`] tells FerroUI which link loads which group ahead
//! of a visit (`ferrohealth_ui::widget::prefetch`).
//!
//! A page is read on the server, from one call of [`read_page`]: the server
//! reads the visit and runs the page's read of the terminology server,
//! recording each exchange (`crate::fhir::tape`). The page is drawn from that
//! recording on whichever half draws it, so the browser hydrates the page the
//! server drew and draws a page after a navigation from one round trip. No
//! specification governs the grouping or the read: our own design.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use std::future::Future;
use std::pin::Pin;

use ferrohealth_ui::widget::prefetch::Route;
use leptos::prelude::*;
use leptos_router::{LazyRoute, lazy_route};
use serde::{Deserialize, Serialize};

use crate::fhir::tape::{Recording, Tape};
use crate::pages::configuration::Shown;
use crate::request::Request;
use crate::visit::Visit;

/// A console page, as its path names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Page {
    /// `/`.
    Dashboard,
    /// `/find`.
    Find,
    /// `/systems`.
    CodeSystems,
    /// `/systems/:system`.
    CodeSystem,
    /// `/valuesets`.
    ValueSets,
    /// `/valuesets/:id`.
    ValueSet,
    /// `/conceptmaps`.
    ConceptMaps,
    /// `/conceptmaps/:id`.
    ConceptMap,
    /// `/browse`.
    Browse,
    /// `/history`.
    History,
    /// `/codesystem`.
    CodeSystemEditor,
    /// `/compose`.
    Composer,
    /// `/conceptmap`.
    ConceptMapEditor,
    /// `/lookup`.
    Lookup,
    /// `/validate`.
    Validate,
    /// `/expand`.
    Expand,
    /// `/translate`.
    Translate,
    /// `/server` and its tabs.
    Server,
    /// `/sync`.
    Sync,
    /// `/sync/:id` and its tabs.
    Run,
    /// `/configuration` and its tabs.
    Configuration,
    /// `/preferences` and its sections.
    Preferences,
    /// `/shortcuts`.
    Shortcuts,
    /// `/about`.
    About,
    /// `/auth/sign-in`.
    SignIn,
}

impl Page {
    /// The page the address path `path` names, if it names one.
    #[must_use]
    pub(crate) fn of(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        Some(match parts.as_slice() {
            [] => Self::Dashboard,
            ["find"] => Self::Find,
            ["systems"] => Self::CodeSystems,
            ["systems", _] => Self::CodeSystem,
            ["valuesets"] => Self::ValueSets,
            ["valuesets", _] => Self::ValueSet,
            ["conceptmaps"] => Self::ConceptMaps,
            ["conceptmaps", _] => Self::ConceptMap,
            ["browse"] => Self::Browse,
            ["history"] => Self::History,
            ["codesystem"] => Self::CodeSystemEditor,
            ["compose"] => Self::Composer,
            ["conceptmap"] => Self::ConceptMapEditor,
            ["lookup"] => Self::Lookup,
            ["validate"] => Self::Validate,
            ["expand"] => Self::Expand,
            ["translate"] => Self::Translate,
            ["server"] | ["server", _] => Self::Server,
            ["sync"] => Self::Sync,
            ["sync", _] | ["sync", _, _] => Self::Run,
            ["configuration"] | ["configuration", _] => Self::Configuration,
            ["preferences"] | ["preferences", _] => Self::Preferences,
            ["shortcuts"] => Self::Shortcuts,
            ["about"] => Self::About,
            ["auth", "sign-in"] => Self::SignIn,
            _ => return None,
        })
    }
}

/// A page's read and draw, erased, so every page shares one `Suspend`.
pub(crate) type Drawing = Pin<Box<dyn Future<Output = AnyView> + Send>>;

/// What the server read for a page: the visit and the exchanges the page
/// was drawn from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Read {
    /// What the server read of the request.
    pub(crate) visit: Visit,
    /// The page's exchanges with the terminology server.
    pub(crate) recording: Recording,
    /// The console's running settings, for the Configuration page alone.
    pub(crate) configuration: Option<Shown>,
}

/// Reads the page at the address `path` and `query` on the server: the
/// visit, and the exchanges the page sends as it is drawn.
///
/// The page is drawn once to learn what it reads, and that drawing is
/// dropped; the half that shows the page draws it again from the recording.
///
/// # Errors
///
/// When the server function cannot be reached or answered.
#[server(endpoint = "page")]
pub(crate) async fn read_page(path: String, query: String) -> Result<Read, ServerFnError> {
    let visit = Visit::requested();
    let page = Page::of(&path);
    let configuration = (page == Some(Page::Configuration))
        .then(use_context::<crate::server::ConsoleState>)
        .flatten()
        .map(|state| Shown::of(state.settings()));
    let tape = Tape::recording();
    let request = Request::of(&visit, path, query);
    let request = Request {
        client: request.client.clone().with_tape(tape.clone()),
        ..request
    };
    if let Some(page) = page {
        ferrohealth_ui::i18n::provide_locale(request.locale.clone());
        provide_context(request.clone());
        if let Some(configuration) = configuration.clone() {
            provide_context(configuration);
        }
        drop(drawing(page)(request).await);
    }
    // NOTE: no specification governs this: our own design; the status a page sets is
    // the document's, set again as the page is drawn, never the read's answer.
    crate::pages::errors::answered_ok();
    Ok(Read {
        visit,
        recording: tape.recorded(),
        configuration,
    })
}

/// The read and draw of `page`, on the server, where every group is in reach.
#[cfg(not(target_arch = "wasm32"))]
fn drawing(page: Page) -> fn(Request) -> Drawing {
    match page {
        Page::Dashboard => |request| Box::pin(crate::pages::dashboard::page(request)),
        Page::Find => |request| Box::pin(crate::pages::find::page(request)),
        Page::CodeSystems => |request| Box::pin(crate::pages::code_systems::page(request)),
        Page::CodeSystem => |request| Box::pin(crate::pages::code_system::page(request)),
        Page::ValueSets => |request| Box::pin(crate::pages::value_sets::page(request)),
        Page::ValueSet => |request| Box::pin(crate::pages::value_set::page(request)),
        Page::ConceptMaps => |request| Box::pin(crate::pages::concept_maps::page(request)),
        Page::ConceptMap => |request| Box::pin(crate::pages::concept_map::page(request)),
        Page::Browse => |request| Box::pin(crate::pages::browse::page(request)),
        Page::History => |request| Box::pin(crate::pages::history::page(request)),
        Page::CodeSystemEditor => {
            |request| Box::pin(crate::pages::code_system_editor::page(request))
        }
        Page::Composer => |request| Box::pin(crate::pages::composer::page(request)),
        Page::ConceptMapEditor => {
            |request| Box::pin(crate::pages::concept_map_editor::page(request))
        }
        Page::Lookup => |request| Box::pin(crate::pages::lookup::page(request)),
        Page::Validate => |request| Box::pin(crate::pages::validate::page(request)),
        Page::Expand => |request| Box::pin(crate::pages::expand::page(request)),
        Page::Translate => |request| Box::pin(crate::pages::translate::page(request)),
        Page::Server => |request| Box::pin(crate::pages::server::page(request)),
        Page::Sync => |request| Box::pin(crate::pages::sync::page(request)),
        Page::Run => |request| Box::pin(crate::pages::sync::run_page(request)),
        Page::Configuration => |request| Box::pin(crate::pages::configuration::page(request)),
        Page::Preferences => |request| Box::pin(crate::pages::preferences::page(request)),
        Page::Shortcuts => |request| Box::pin(crate::pages::shortcuts::page(request)),
        Page::About => |request| Box::pin(crate::pages::about::page(request)),
        Page::SignIn => |request| Box::pin(crate::pages::sign_in::page(request)),
    }
}

/// The page at the address in view, read on the server and drawn by `draw`
/// from what that read recorded.
fn drawn(draw: fn(Request) -> Drawing) -> AnyView {
    let location = leptos_router::hooks::use_location();
    let path = location.pathname.get_untracked();
    let query = location
        .search
        .get_untracked()
        .trim_start_matches('?')
        .to_owned();
    let read = OnceResource::new(read_page(path.clone(), query.clone()));
    view! {
        <Suspense>
            {Suspend::new(async move {
                match read.await {
                    Ok(read) => {
                        let request = Request::of(&read.visit, path, query);
                        let request = Request {
                            client: request
                                .client
                                .clone()
                                .with_tape(Tape::replaying(read.recording)),
                            ..request
                        };
                        ferrohealth_ui::i18n::provide_locale(request.locale.clone());
                        provide_context(request.clone());
                        if let Some(configuration) = read.configuration {
                            provide_context(configuration);
                        }
                        draw(request).await
                    }
                    Err(error) => crate::pages::errors::unread_page(&error.to_string()),
                }
            })}
        </Suspense>
    }
    .into_any()
}

/// Declares the lazy route `$name` of a group of pages, drawing each page
/// `$page` by its read and draw `$draw`.
///
/// Each group matches its own pages alone, so its chunks hold their code
/// and no other group's, and draws its page again as the address changes.
macro_rules! group {
    ($(#[$doc:meta])* $name:ident { $($page:ident => $draw:path),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub(crate) struct $name;

        impl $name {
            /// The pages the group draws.
            #[cfg(test)]
            pub(crate) const PAGES: &'static [Page] = &[$(Page::$page),+];

            /// The page at the address path `path`.
            fn page(path: &str) -> AnyView {
                match Page::of(path) {
                    $(Some(Page::$page) => drawn(|request| Box::pin($draw(request))),)+
                    _ => crate::pages::errors::not_found_page(),
                }
            }
        }

        #[lazy_route]
        impl LazyRoute for $name {
            fn data() -> Self {
                Self
            }

            fn view(_this: Self) -> AnyView {
                let location = leptos_router::hooks::use_location();
                // NOTE: Leptos 0.8 `tachys` renders a view closure once per pass of the
                // server's render, so the server draws the page once, outside a closure.
                #[cfg(not(target_arch = "wasm32"))]
                let view = untrack(|| $name::page(&location.pathname.get_untracked()));
                // NOTE: leptos_router, `Route`: a navigation that stays on a route keeps
                // its view, so the group draws the page its address names as it changes.
                #[cfg(target_arch = "wasm32")]
                let view = (move || {
                    location.search.track();
                    $name::page(&location.pathname.get())
                })
                .into_any();
                view
            }
        }
    };
}

group!(
    /// The dashboard and the find page.
    LOverview {
        Dashboard => crate::pages::dashboard::page,
        Find => crate::pages::find::page,
    }
);

group!(
    /// The resources: the code systems, value sets and concept maps, the
    /// concept browser and a resource's history.
    LTerminology {
        CodeSystems => crate::pages::code_systems::page,
        CodeSystem => crate::pages::code_system::page,
        ValueSets => crate::pages::value_sets::page,
        ValueSet => crate::pages::value_set::page,
        ConceptMaps => crate::pages::concept_maps::page,
        ConceptMap => crate::pages::concept_map::page,
        Browse => crate::pages::browse::page,
        History => crate::pages::history::page,
    }
);

group!(
    /// The three editors, which draw the authoring forms.
    LEditors {
        CodeSystemEditor => crate::pages::code_system_editor::page,
        Composer => crate::pages::composer::page,
        ConceptMapEditor => crate::pages::concept_map_editor::page,
    }
);

group!(
    /// The operation runners.
    LOperations {
        Lookup => crate::pages::lookup::page,
        Validate => crate::pages::validate::page,
        Expand => crate::pages::expand::page,
        Translate => crate::pages::translate::page,
    }
);

group!(
    /// The console's own pages: the server, synchronisation, the
    /// configuration, Your preferences, the shortcuts, About and the
    /// sign-in.
    LConsole {
        Server => crate::pages::server::page,
        Sync => crate::pages::sync::page,
        Run => crate::pages::sync::run_page,
        Configuration => crate::pages::configuration::page,
        Preferences => crate::pages::preferences::page,
        Shortcuts => crate::pages::shortcuts::page,
        About => crate::pages::about::page,
        SignIn => crate::pages::sign_in::page,
    }
);

/// The route at `path`, loaded by the group `$group`.
macro_rules! route {
    ($path:literal, $group:ident, $idle:literal) => {
        Route {
            path: $path,
            preload: || Box::pin($group::preload()),
            idle: $idle,
        }
    };
}

/// Every route by its path and the group that draws it.
///
/// The groups of the main navigation load once the first page is idle,
/// each once, through the first of its routes; every other route loads on
/// a hover or a focus.
pub(crate) static ROUTES: &[Route] = &[
    route!("/", LOverview, true),
    route!("/find", LOverview, false),
    route!("/systems", LTerminology, true),
    route!("/systems/:system", LTerminology, false),
    route!("/valuesets", LTerminology, false),
    route!("/valuesets/:id", LTerminology, false),
    route!("/conceptmaps", LTerminology, false),
    route!("/conceptmaps/:id", LTerminology, false),
    route!("/browse", LTerminology, false),
    route!("/history", LTerminology, false),
    route!("/codesystem", LEditors, false),
    route!("/compose", LEditors, false),
    route!("/conceptmap", LEditors, false),
    route!("/lookup", LOperations, true),
    route!("/validate", LOperations, false),
    route!("/expand", LOperations, false),
    route!("/translate", LOperations, false),
    route!("/server", LConsole, true),
    route!("/server/:tab", LConsole, false),
    route!("/sync", LConsole, false),
    route!("/sync/:id", LConsole, false),
    route!("/sync/:id/:tab", LConsole, false),
    route!("/configuration", LConsole, false),
    route!("/configuration/:tab", LConsole, false),
    route!("/preferences", LConsole, false),
    route!("/preferences/:section", LConsole, false),
    route!("/shortcuts", LConsole, false),
    route!("/about", LConsole, false),
    route!("/auth/sign-in", LConsole, false),
];

#[cfg(test)]
mod tests {
    use super::{LConsole, LEditors, LOperations, LOverview, LTerminology, Page, ROUTES};

    #[test]
    fn every_route_names_a_page_in_exactly_one_group() {
        let groups = [
            LOverview::PAGES,
            LTerminology::PAGES,
            LEditors::PAGES,
            LOperations::PAGES,
            LConsole::PAGES,
        ];
        for route in ROUTES {
            let path = route
                .path
                .replace(":system", "a")
                .replace(":id", "b")
                .replace(":tab", "c")
                .replace(":section", "d");
            let page = Page::of(&path).unwrap_or_else(|| panic!("{} names a page", route.path));
            let holding = groups.iter().filter(|group| group.contains(&page)).count();
            assert_eq!(1, holding, "{} sits in {holding} groups", route.path);
        }
        assert_eq!(Some(Page::Run), Page::of("/sync/abc/log"));
        assert_eq!(Some(Page::CodeSystemEditor), Page::of("/codesystem"));
        assert_eq!(None, Page::of("/no/such/page"));
        assert_eq!(None, Page::of("/systems/a/b"));
    }
}
