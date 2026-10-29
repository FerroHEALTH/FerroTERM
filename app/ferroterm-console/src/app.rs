// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server draws and the browser hydrates whole.
//!
//! The server draws the first page and the browser takes the whole tree
//! over, then draws every page after it, each group of pages a lazy route
//! (`crate::groups`), so no link or form reloads the document (FerroUI
//! `docs/consoles.md`, "How a console builds"). The document carries the
//! reader's theme, density, motion and shortcut hints, written by the server
//! from their cookies so the page arrives drawn in them, and the locale as
//! `lang`; the two stylesheets are the components' (`ferrohealth-ui`) and the
//! FerroTERM theme, both of this origin, so `style-src 'self'` holds.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]

use ferrohealth_ui::i18n::provide_locale;
use ferrohealth_ui::island::{FormNavigator, PageAnnouncer, ToastRegion};
use ferrohealth_ui::widget::toast::RegionWords;
use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::{Lazy, SsrMode, path};

use crate::groups::{LConsole, LEditors, LOperations, LOverview, LTerminology};
use crate::shell::PRODUCT;
use crate::visit::{FollowVisit, Visits};

/// The whole HTML document the server draws around [`App`], in the theme,
/// density, motion and hints the request's cookies name.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn document(options: LeptosOptions) -> impl IntoView {
    let visit = crate::visit::Visit::requested();
    let family = crate::prefs::Prefs::read(&visit.cookies, &visit.prefix).family;
    let locale = visit.locale();
    let description = crate::locale::tr_in(
        crate::locale::catalog(),
        &locale,
        crate::locale::Msg::Description,
    );
    view! {
        <!DOCTYPE html>
        <html
            lang=locale.tag()
            data-theme=family.theme.key()
            data-density=family.density.key()
            data-motion=family.motion.key()
            data-hints=family.hints.key()
        >
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="color-scheme" content="light dark" />
                <meta name="description" content=description />
                <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
                <link rel="stylesheet" href=crate::server::COMPONENTS_CSS />
                <link rel="stylesheet" href=crate::server::THEME_CSS />
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the visit both halves draw from, the locale, the
/// title format, the routes inside the shell, the toast region and the page
/// announcer.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: what the server read reaches the
    // hydrating page serialized, so both halves draw the same page.
    let visits = RwSignal::new(
        (*SharedValue::new(|| crate::visit::Visit {
            posted: None,
            ..crate::visit::server_visit()
        }))
        .clone(),
    );
    provide_context(Visits(visits));
    provide_locale(visits.with_untracked(crate::visit::Visit::locale));
    ferrohealth_ui::widget::prefetch::routes(crate::groups::ROUTES);
    view! {
        <Title formatter=|text: String| format!("{text} · {PRODUCT}") />
        <Router>
            <FollowVisit />
            <FormNavigator />
            <Layout />
        </Router>
        <ToastRegion words=RegionWords::of_page() flash=Vec::new() />
        <PageAnnouncer />
    }
}

/// The routes, inside the shell for every page but the sign-in, which
/// stands on its own.
#[component]
fn Layout() -> impl IntoView {
    let location = use_location();
    let framed = Memo::new(move |_| location.pathname.get() != crate::routes::SIGN_IN_PAGE);
    // NOTE: Leptos 0.8 `tachys` renders a view closure once per pass of the server's
    // render, so the server draws the layout once, outside a closure.
    #[cfg(not(target_arch = "wasm32"))]
    let view = untrack(|| layout(framed.get()));
    #[cfg(target_arch = "wasm32")]
    let view = (move || layout(framed.get())).into_any();
    view
}

/// The routes inside the shell when `framed`, else on their own.
fn layout(framed: bool) -> AnyView {
    if framed {
        crate::shell::console_shell(routes())
    } else {
        routes()
    }
}

/// A route the console's server answers itself, which the router reaches
/// when a link names it: the browser loads it from the server.
///
/// The FHIR version select's links lead to `/fhir-version`, which keeps the
/// choice in a cookie and redirects back; the server lists the route with
/// none of its pages (`crate::server::router`), so it only draws here.
fn server_route() -> impl IntoView {
    // NOTE: no specification governs this: our own design; a browser that refuses
    // the load keeps the page it was on, and says so in its console.
    #[cfg(target_arch = "wasm32")]
    {
        let location = use_location();
        let search = location.search.get_untracked();
        let search = search.trim_start_matches('?');
        let href = if search.is_empty() {
            location.pathname.get_untracked()
        } else {
            format!("{}?{search}", location.pathname.get_untracked())
        };
        if window().location().set_href(&href).is_err() {
            leptos::logging::warn!("the browser refused to load the route from the server");
        }
    }
}

/// The lazy route of the group [`LOverview`].
fn overview() -> Lazy<LOverview> {
    Lazy::new()
}

/// The lazy route of the group [`LTerminology`].
fn terminology() -> Lazy<LTerminology> {
    Lazy::new()
}

/// The lazy route of the group [`LEditors`].
fn editors() -> Lazy<LEditors> {
    Lazy::new()
}

/// The lazy route of the group [`LOperations`].
fn operations() -> Lazy<LOperations> {
    Lazy::new()
}

/// The lazy route of the group [`LConsole`].
fn console() -> Lazy<LConsole> {
    Lazy::new()
}

/// Every route, each drawn by the lazy group that holds its page.
fn routes() -> AnyView {
    // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the server reads renders
    // `Async`, whole in one sweep, so no content waits on a script.
    view! {
        <Routes fallback=crate::pages::errors::not_found_page>
            <Route path=path!("/") view=overview() ssr=SsrMode::Async />
            <Route path=path!("/find") view=overview() ssr=SsrMode::Async />
            <Route path=path!("/systems") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/systems/:system") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/valuesets") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/valuesets/:id") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/conceptmaps") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/conceptmaps/:id") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/browse") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/history") view=terminology() ssr=SsrMode::Async />
            <Route path=path!("/codesystem") view=editors() ssr=SsrMode::Async />
            <Route path=path!("/compose") view=editors() ssr=SsrMode::Async />
            <Route path=path!("/conceptmap") view=editors() ssr=SsrMode::Async />
            <Route path=path!("/lookup") view=operations() ssr=SsrMode::Async />
            <Route path=path!("/validate") view=operations() ssr=SsrMode::Async />
            <Route path=path!("/expand") view=operations() ssr=SsrMode::Async />
            <Route path=path!("/translate") view=operations() ssr=SsrMode::Async />
            <Route path=path!("/server") view=console() ssr=SsrMode::Async />
            <Route path=path!("/server/:tab") view=console() ssr=SsrMode::Async />
            <Route path=path!("/sync") view=console() ssr=SsrMode::Async />
            <Route path=path!("/sync/:id") view=console() ssr=SsrMode::Async />
            <Route path=path!("/sync/:id/:tab") view=console() ssr=SsrMode::Async />
            <Route path=path!("/configuration") view=console() ssr=SsrMode::Async />
            <Route path=path!("/configuration/:tab") view=console() ssr=SsrMode::Async />
            <Route path=path!("/preferences") view=console() ssr=SsrMode::Async />
            <Route path=path!("/preferences/:section") view=console() ssr=SsrMode::Async />
            <Route path=path!("/shortcuts") view=console() ssr=SsrMode::Async />
            <Route path=path!("/about") view=console() ssr=SsrMode::Async />
            <Route path=path!("/auth/sign-in") view=console() ssr=SsrMode::Async />
            <Route path=path!("/fhir-version") view=server_route />
        </Routes>
    }
    .into_any()
}
