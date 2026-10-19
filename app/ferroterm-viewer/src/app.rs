// SPDX-License-Identifier: BUSL-1.1
//! The page tree the server renders and the browser hydrates.
//!
//! The document carries the reader's theme and density, written by the server
//! from its cookies so the page arrives drawn in them, and the request's
//! locale as `lang` (`crate::locale`); the three stylesheets
//! are the components' (`ferrohealth-ui`), the FerroTERM brand, and the
//! console's own layout utilities, all of this origin, so `style-src 'self'`
//! holds.

// The diagnostic lands on macro output rather than on an item written here,
// so the expectation covers the module.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]

use ferrohealth_ui::i18n::{Locale, provide_locale};
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::Router;
use serde::{Deserialize, Serialize};

use crate::auth::{Operator, Session};
use crate::components::shell::{Console, PRODUCT, console_shell};
use crate::fhir::FhirClient;
use crate::runs::RunList;
use crate::settings::Settings;

/// The path of the stylesheet cargo-leptos writes into the site bundle.
pub const STYLESHEET: &str = "/pkg/ferroterm-viewer.css";

/// What the server tells the page about the request it renders: carried to
/// the browser half serialized, so both halves draw the same page (the Leptos
/// book, `ssr/22_life_cycle`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// The terminology server's public root, which every request a screen
    /// discloses starts with, without a final `/`.
    pub root: String,
    /// Whether the console offers a sign-in.
    pub sign_in: bool,
    /// Whether the console reaches a sync service, whose runs the history
    /// screen reads.
    pub sync: bool,
    /// The signed-in reader, when there is one.
    pub operator: Option<Operator>,
    /// The reader's preferences.
    pub settings: Settings,
    /// The tag of the locale the request is served in (`crate::locale`).
    pub locale: String,
}

/// The whole HTML document the server renders around [`App`], in the theme
/// and density the request's cookies name.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let settings = crate::settings::server::requested();
    let theme = settings.theme.key();
    let density = settings.density.key();
    let locale = crate::locale::requested();
    let lang = locale.tag();
    let description = crate::locale::tr_in(
        crate::locale::catalog(),
        &locale,
        crate::locale::Msg::Description,
    );
    view! {
        <!DOCTYPE html>
        <html lang=lang data-theme=theme data-density=density>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="color-scheme" content="light dark" />
                <meta name="description" content=description />
                <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
                <link rel="stylesheet" href=crate::server::COMPONENTS_CSS />
                <link rel="stylesheet" href=crate::server::THEME_CSS />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// What the server knows of the request: on the server half read from the
/// configuration, the session and the cookies.
fn facts() -> Facts {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(state) = use_context::<crate::server::ViewerState>() else {
            return Facts::default();
        };
        Facts {
            root: state.settings().ferroterm.shown_root(),
            sign_in: state.settings().oidc.is_some(),
            sync: state.settings().ferroterm.sync.is_some(),
            operator: crate::session::request::operator(),
            settings: crate::settings::server::requested(),
            locale: crate::locale::requested().tag(),
        }
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the page a visitor sees.
    #[cfg(target_arch = "wasm32")]
    {
        Facts::default()
    }
}

/// The root component: the stylesheet, the title, the contexts every screen
/// reads, and the shell.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let facts = SharedValue::new(facts).into_inner();
    provide_context(FhirClient::new(facts.root));
    provide_context(Session::new(facts.operator));
    provide_context(facts.settings);
    provide_locale(Locale::parse(&facts.locale).unwrap_or_default());
    let _help = ferrohealth_ui::widget::help::provide_help();
    provide_context(RunList::default());
    let console = Console {
        sign_in: facts.sign_in,
        sync: facts.sync,
    };
    provide_context(console);
    view! {
        <Stylesheet id="leptos" href=STYLESHEET />
        <Title formatter=|text: String| format!("{text} · {PRODUCT}") />
        <Router>{console_shell(console)}</Router>
    }
}
