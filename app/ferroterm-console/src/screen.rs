// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages by the paths that name them, and the screen that
//! draws each.
//!
//! A page is one async function from its [`Request`] to its content, which
//! reads what it shows from the terminology server and draws it; the handler
//! puts it in the document (`crate::app`). A page that writes takes a plain
//! form post to its own address, drawn again with the posted fields. No
//! specification governs this: our own design.

use std::future::Future;
use std::pin::Pin;

use ferrohealth_ui::html::Html;

use crate::request::Request;

/// A console page, as its path names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Whether the page is drawn inside the shell; the sign-in page stands
    /// on its own.
    #[must_use]
    pub(crate) const fn framed(self) -> bool {
        !matches!(self, Self::SignIn)
    }
}

/// A page's read and draw, erased, so every page shares one type.
pub(crate) type Drawing = Pin<Box<dyn Future<Output = Html> + Send>>;

/// The read and draw of `page` for `request`.
pub(crate) fn draw(page: Page, request: Request) -> Drawing {
    match page {
        Page::Dashboard => Box::pin(crate::pages::dashboard::page(request)),
        Page::Find => Box::pin(crate::pages::find::page(request)),
        Page::CodeSystems => Box::pin(crate::pages::code_systems::page(request)),
        Page::CodeSystem => Box::pin(crate::pages::code_system::page(request)),
        Page::ValueSets => Box::pin(crate::pages::value_sets::page(request)),
        Page::ValueSet => Box::pin(crate::pages::value_set::page(request)),
        Page::ConceptMaps => Box::pin(crate::pages::concept_maps::page(request)),
        Page::ConceptMap => Box::pin(crate::pages::concept_map::page(request)),
        Page::Browse => Box::pin(crate::pages::browse::page(request)),
        Page::History => Box::pin(crate::pages::history::page(request)),
        Page::CodeSystemEditor => Box::pin(crate::pages::code_system_editor::page(request)),
        Page::Composer => Box::pin(crate::pages::composer::page(request)),
        Page::ConceptMapEditor => Box::pin(crate::pages::concept_map_editor::page(request)),
        Page::Lookup => Box::pin(crate::pages::lookup::page(request)),
        Page::Validate => Box::pin(crate::pages::validate::page(request)),
        Page::Expand => Box::pin(crate::pages::expand::page(request)),
        Page::Translate => Box::pin(crate::pages::translate::page(request)),
        Page::Server => Box::pin(crate::pages::server::page(request)),
        Page::Sync => Box::pin(crate::pages::sync::page(request)),
        Page::Run => Box::pin(crate::pages::sync::run_page(request)),
        Page::Configuration => drawn(crate::pages::configuration::page(&request)),
        Page::Preferences => drawn(crate::pages::preferences::page(&request)),
        Page::Shortcuts => drawn(crate::pages::shortcuts::page(&request)),
        Page::About => Box::pin(crate::pages::about::page(request)),
        Page::SignIn => drawn(crate::pages::sign_in::page(&request)),
    }
}

/// A page drawn with no read, as a drawing.
fn drawn(page: Html) -> Drawing {
    Box::pin(std::future::ready(page))
}

/// Every route a page answers at, as axum writes its path.
pub(crate) const ROUTES: &[&str] = &[
    "/",
    "/find",
    "/systems",
    "/systems/{system}",
    "/valuesets",
    "/valuesets/{id}",
    "/conceptmaps",
    "/conceptmaps/{id}",
    "/browse",
    "/history",
    "/codesystem",
    "/compose",
    "/conceptmap",
    "/lookup",
    "/validate",
    "/expand",
    "/translate",
    "/server",
    "/server/{tab}",
    "/sync",
    "/sync/{id}",
    "/sync/{id}/{tab}",
    "/configuration",
    "/configuration/{tab}",
    "/preferences",
    "/preferences/{section}",
    "/shortcuts",
    "/about",
    "/auth/sign-in",
];

#[cfg(test)]
mod tests {
    use super::{Page, ROUTES};

    #[test]
    fn every_route_names_a_page() {
        for route in ROUTES {
            let path = route
                .replace("{system}", "a")
                .replace("{id}", "b")
                .replace("{tab}", "c")
                .replace("{section}", "d");
            assert!(Page::of(&path).is_some(), "{route} names a page");
        }
        assert_eq!(Some(Page::Run), Page::of("/sync/abc/log"));
        assert_eq!(Some(Page::CodeSystemEditor), Page::of("/codesystem"));
        assert_eq!(None, Page::of("/no/such/page"));
        assert_eq!(None, Page::of("/systems/a/b"));
    }

    #[test]
    fn only_the_sign_in_stands_outside_the_shell() {
        assert!(!Page::SignIn.framed());
        assert!(Page::Dashboard.framed());
    }
}
