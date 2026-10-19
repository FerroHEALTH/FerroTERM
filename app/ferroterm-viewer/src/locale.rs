// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the crate's
//! `locales/` directory, one directory per language
//! (`locales/en/ferroterm-viewer.ftl`), embedded at build time by
//! `fluent_templates::static_loader!`, the mechanism `ferrohealth_ui`
//! catalogs its own words with. English is the one catalog that ships and the
//! one every lookup falls back to; a language joins as one catalog file and
//! no code change.
//!
//! The server chooses the locale once per request: the best language of the
//! request's `Accept-Language` the catalog holds, else the configured
//! `[server] locale`. [`crate::app::App`] provides it to both halves through
//! `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
//! components' words follow the same choice, and the document writes it as
//! `<html lang>` (WCAG 2.2 SC 3.1.1). No specification governs the catalog
//! layout or the choice: our own design.

use ferrohealth_ui::i18n::{Locale, locale};
use fluent_templates::Loader;

fluent_templates::static_loader! {
    static CATALOG = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// The console's catalogs, every language it ships.
#[must_use]
pub(crate) fn catalog() -> &'static dyn Loader {
    &*CATALOG
}

/// Declares the console's messages: each variant names one message of the
/// catalog by its id.
macro_rules! messages {
    ($($(#[$meta:meta])+ $name:ident = $id:literal,)+) => {
        /// A string the console draws, by its message in the catalog.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Msg {
            $($(#[$meta])+ $name,)+
        }

        impl Msg {
            /// Every message, in declaration order.
            #[cfg(test)]
            pub(crate) const ALL: &[Self] = &[$(Self::$name,)+];

            /// The message's id in the catalog.
            #[must_use]
            pub(crate) const fn id(self) -> &'static str {
                match self {
                    $(Self::$name => $id,)+
                }
            }
        }
    };
}

messages! {
    /// The document's description of the console.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "the server half alone writes the document")
    )]
    Description = "description",
    /// The title of the screen an address that names no page reaches.
    NotFoundTitle = "not-found-title",
    /// That screen's heading.
    NotFoundHeading = "not-found-heading",
    /// The sentence of that screen.
    NotFoundBody = "not-found-body",
    /// That screen's way back.
    NotFoundBack = "not-found-back",
}

/// The console's message `msg` in the page's locale.
#[must_use]
pub(crate) fn tr(msg: Msg) -> String {
    tr_in(catalog(), &locale(), msg)
}

/// The message `msg` of `catalog` in `locale`.
#[must_use]
pub(crate) fn tr_in(catalog: &dyn Loader, locale: &Locale, msg: Msg) -> String {
    locale.lookup(catalog, msg.id())
}

/// The locale of the request being served: the best language of its
/// `Accept-Language` the catalog holds, else the configured one, English
/// with no configuration in reach.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub(crate) fn requested() -> Locale {
    use leptos::context::use_context;
    let configured = use_context::<crate::server::ViewerState>()
        .map_or_else(Locale::english, |state| state.settings().locale.clone());
    let header = use_context::<http::request::Parts>()
        .and_then(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT_LANGUAGE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    Locale::negotiate_with(&header, catalog(), configured)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;

    use super::{Msg, catalog, tr_in};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// The directories under `locales/`, each a language.
    fn catalog_directories(root: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let found = catalog_directories(&root);
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in found {
            let language = tag.parse().expect("a language");
            for msg in Msg::ALL {
                let words = catalog().try_lookup(&language, msg.id());
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!("Not found", tr_in(catalog(), &english, Msg::NotFoundTitle));
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qot qound",
            tr_in(&*THROWAWAY, &throwaway, Msg::NotFoundTitle)
        );
        assert_eq!(
            "Back to the dashboard",
            tr_in(catalog(), &throwaway, Msg::NotFoundBack),
            "the shipped catalogs fall back to English"
        );
    }
}
