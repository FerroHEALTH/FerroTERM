// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server draws, in Leptos islands mode.
//!
//! The document carries the reader's theme, density, motion and shortcut
//! hints, written by the server from their cookies so the page arrives drawn
//! in them, and the locale as `lang`; the two stylesheets are the
//! components' (`ferrohealth-ui`) and the FerroTERM theme, both of this
//! origin, so `style-src 'self'` holds. The browser hydrates the islands of
//! a page and nothing else (the Leptos book, "Guide: Islands").
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]
#![expect(
    clippy::must_use_candidate,
    reason = "the leptos component macro drops a #[must_use] from the function it writes"
)]

use ferrohealth_ui::i18n::provide_locale;
use ferrohealth_ui::island::ToastRegion;
use ferrohealth_ui::widget::toast::RegionWords;
use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::locale::Msg;
use crate::pages;
use crate::request::Request;
use crate::shell::{ConsoleShell, PRODUCT};

/// The whole HTML document the server draws around [`App`], in the theme,
/// density, motion and hints the request's cookies name.
#[must_use]
pub fn document(options: LeptosOptions) -> impl IntoView {
    let request = Request::read();
    let family = request.prefs.family.clone();
    let lang = request.locale.tag();
    let description =
        crate::locale::tr_in(crate::locale::catalog(), &request.locale, Msg::Description);
    view! {
        <!DOCTYPE html>
        <html
            lang=lang
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
                <HydrationScripts options islands=true />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the request every screen reads, the locale, the
/// title format, the routes and the toast region.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let request = Request::read();
    provide_locale(request.locale.clone());
    provide_context(request);
    view! {
        <Title formatter=|text: String| format!("{text} · {PRODUCT}") />
        <Router>
            <Routes fallback=|| crate::shell::framed(pages::errors::not_found_page)>
                <Route
                    path=path!("/auth/sign-in")
                    view=pages::sign_in::SignInPage
                    ssr=SsrMode::Async
                />
                <ParentRoute path=path!("") view=ConsoleShell ssr=SsrMode::Async>
                    <Route path=path!("") view=pages::dashboard::DashboardPage />
                    <Route path=path!("systems") view=pages::code_systems::CodeSystemsPage />
                    <Route path=path!("systems/:system") view=pages::code_system::CodeSystemPage />
                    <Route
                        path=path!("codesystem")
                        view=pages::code_system_editor::CodeSystemEditorPage
                    />
                    <Route path=path!("valuesets") view=pages::value_sets::ValueSetsPage />
                    <Route path=path!("valuesets/:id") view=pages::value_set::ValueSetPage />
                    <Route path=path!("compose") view=pages::composer::ComposerPage />
                    <Route path=path!("conceptmaps") view=pages::concept_maps::ConceptMapsPage />
                    <Route path=path!("conceptmaps/:id") view=pages::concept_map::ConceptMapPage />
                    <Route
                        path=path!("conceptmap")
                        view=pages::concept_map_editor::ConceptMapEditorPage
                    />
                    <Route path=path!("history") view=pages::history::HistoryPage />
                    <Route path=path!("browse") view=pages::browse::BrowsePage />
                    <Route path=path!("lookup") view=pages::lookup::LookupPage />
                    <Route path=path!("validate") view=pages::validate::ValidatePage />
                    <Route path=path!("expand") view=pages::expand::ExpandPage />
                    <Route path=path!("translate") view=pages::translate::TranslatePage />
                    <Route path=path!("find") view=pages::find::FindPage />
                    <Route path=path!("sync") view=pages::sync::SyncPage />
                    <Route path=path!("sync/:id") view=pages::sync::RunPage />
                    <Route path=path!("sync/:id/:tab") view=pages::sync::RunPage />
                    <Route path=path!("server") view=pages::server::ServerPage />
                    <Route path=path!("server/:tab") view=pages::server::ServerPage />
                    <Route
                        path=path!("configuration")
                        view=pages::configuration::ConfigurationPage
                    />
                    <Route
                        path=path!("configuration/:tab")
                        view=pages::configuration::ConfigurationPage
                    />
                    <Route path=path!("preferences") view=pages::preferences::PreferencesPage />
                    <Route
                        path=path!("preferences/:section")
                        view=pages::preferences::PreferencesPage
                    />
                    <Route path=path!("shortcuts") view=pages::shortcuts::ShortcutsPage />
                    <Route path=path!("about") view=pages::about::AboutPage />
                </ParentRoute>
            </Routes>
        </Router>
        <ToastRegion words=RegionWords::of_page() flash=Vec::new() />
    }
}
