// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every screen: the top bar, the navigation, the banner
//! and the footer, drawn once from FerroUI's shell around the routes.
//!
//! The top bar carries what is true of every screen at once: the search, the
//! FHIR version the reader is reading through (M3), the server's state as
//! the status pill (F12), Sign in for a visitor, and the user menu (F13),
//! which holds the theme, since the top bar carries no theme control
//! (FerroUI's family rule 5). The navigation folds to its rail by a form
//! post the console keeps in a cookie. The shell is drawn once and follows
//! the router (FerroUI `docs/consoles.md`, "How a console builds", step
//! 10): the navigation marks the page in view by itself, the version select
//! follows the address, and the parts that read the visit draw again when
//! it changes. No specification governs the chrome: our own design.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use std::borrow::Cow;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::island::{SessionWatch, ShortcutLegend};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::search::{SearchBox, search_box};
use ferrohealth_ui::view::shell::{Footer, Shell, TopBar, brand_mark, lockup, shell, topbar};
use ferrohealth_ui::view::status::{Reach, StatusPill, banner, status_pill};
use ferrohealth_ui::widget::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::widget::nav::{Entry, Group, Rail, Sidebar, sidebar};
use ferrohealth_ui::widget::session::{Left, Watch};
use ferrohealth_ui::widget::shortcuts::LegendWords;
use ferrohealth_ui::widget::version::{VersionRoot, VersionSelect, version_select};
#[cfg(not(target_arch = "wasm32"))]
use http::StatusCode;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use serde::{Deserialize, Serialize};

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::request::Request;
use crate::routes;
use crate::shortcuts::Shortcuts;
use crate::visit::Visits;

/// The product name as a reader reads it, the same in every language.
pub(crate) const PRODUCT: &str = "FerroTERM Console";

/// The product's name before the lockup's accent, the same in every
/// language.
const LOCKUP_NAME: &str = "FerroTERM";

/// The console's version, which the footer and the About page name.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The id of the navigation, which the drawer button opens.
const NAV_ID: &str = "sidebar";

/// The id of the user menu.
const USER_MENU_ID: &str = "user-menu";

/// The id of the top bar's search field.
pub(crate) const SEARCH_ID: &str = "find";

/// The id of the FHIR version select's menu.
const VERSION_MENU_ID: &str = "fhir-version";

/// The brand mark drawn on a light ground, from the site root.
const MARK_LIGHT: &str = "/mark-light.svg";

/// The brand mark drawn on a dark ground, from the site root.
const MARK_DARK: &str = "/mark-dark.svg";

/// How the terminology server answered the console's probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Probe {
    /// It answered `200`.
    Up,
    /// It answered, with a failure status.
    Degraded,
    /// It answered `401`, refusing the console.
    Refused,
    /// It did not answer.
    Unreachable,
}

impl Probe {
    #[cfg(not(target_arch = "wasm32"))]
    /// The state `status` names, or [`Probe::Unreachable`] for none.
    #[must_use]
    pub(crate) fn of(status: Option<StatusCode>) -> Self {
        match status {
            Some(StatusCode::OK) => Self::Up,
            Some(StatusCode::UNAUTHORIZED) => Self::Refused,
            Some(_) => Self::Degraded,
            None => Self::Unreachable,
        }
    }

    /// The status pill's shape and words.
    fn pill(self) -> (Reach, Msg) {
        match self {
            Self::Up => (Reach::Up, Msg::ShellServerUp),
            Self::Degraded => (Reach::Degraded, Msg::ShellServerDegraded),
            Self::Refused => (Reach::Refused, Msg::ShellServerRefused),
            Self::Unreachable => (Reach::Unreachable, Msg::ShellServerUnreachable),
        }
    }
}

/// How the terminology server answered the console, read once for the
/// shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reached {
    /// How it answered the probe of its health.
    probe: Probe,
    /// Whether it serves writes with no sign-in on a console that offers
    /// none, which the development banner says.
    open: bool,
}

/// Asks the server for its health, for the status pill, and, on a console
/// with no sign-in, whether the server itself asks for one, for the
/// development banner.
///
/// # Errors
///
/// When the server function cannot be reached or answered.
#[server(endpoint = "reached")]
pub(crate) async fn server_reached() -> Result<Reached, ServerFnError> {
    let request = Request::read();
    let health = request.client.probe().await;
    let open = if request.sign_in {
        false
    } else {
        request.client.serves_without_sign_in(request.version).await
    };
    Ok(Reached {
        probe: Probe::of(health),
        open,
    })
}

/// The request of the address in view, as the visit the shell knows read
/// it.
fn in_view(visits: Option<Visits>, path: String, query: String) -> Request {
    let visit = visits.map_or_else(crate::visit::server_visit, |visits| visits.0.get());
    Request::of(&visit, path, query)
}

/// The console's chrome around `content`, the routes: drawn once, each part
/// following the router or the visit as it changes.
pub(crate) fn console_shell(content: AnyView) -> AnyView {
    ferrohealth_ui::widget::nav::follow_path();
    let location = use_location();
    let visits = use_context::<Visits>();
    let path = location.pathname.get_untracked();
    let query = location
        .search
        .get_untracked()
        .trim_start_matches('?')
        .to_owned();
    let reached = OnceResource::new(server_reached());
    let drawn_at = (path.clone(), query.clone());
    let bar = view! {
        <Suspense>
            {move || {
                let request = in_view(visits, drawn_at.0.clone(), drawn_at.1.clone());
                Suspend::new(async move {
                    let reached = reached.await.ok();
                    top_bar(&request, reached)
                })
            }}
        </Suspense>
    }
    .into_any();
    let drawn_at = (path.clone(), query.clone());
    let nav = (move || {
        let request = in_view(visits, drawn_at.0.clone(), drawn_at.1.clone());
        navigation_of(&request)
    })
    .into_any();
    let banner = view! {
        <Suspense>
            {Suspend::new(async move {
                reached.await.ok().filter(|reached| reached.open).map(|_| development_banner())
            })}
        </Suspense>
    }
    .into_any();
    let page = shell(Shell {
        topbar: bar,
        banner: Some(banner),
        sidebar: nav,
        content,
        dock: None,
        footer: footer(),
    });
    // NOTE: FerroUI `docs/consoles.md`, "The session's end": the watch starts from what
    // the session had left as the server drew the page, carried to the browser.
    let left = *SharedValue::new(session_left);
    let watch = left.map(|left| {
        let watch = Watch::new(
            routes::SESSION_ROUTE,
            routes::SIGN_IN_PAGE,
            routes::SIGN_OUT_ROUTE,
            left,
        );
        view! { <SessionWatch watch /> }
    });
    view! {
        {page}
        {watch}
        <Shortcuts goto=crate::pages::shortcuts::goto() />
        <ShortcutLegend groups=crate::pages::shortcuts::groups() words=LegendWords::of_page() />
    }
    .into_any()
}

/// What the reader's session had left as the server drew the page; none for
/// a visitor, and in the browser, which reads the value the server
/// serialized.
fn session_left() -> Option<Left> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::session::request::left()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// The navigation of `request`, its rail as the reader left it.
fn navigation_of(request: &Request) -> AnyView {
    let rail = Rail {
        collapsed: request.prefs.rail,
        action: String::from(routes::SIDEBAR_ROUTE),
        back: request.back(),
    };
    sidebar(Sidebar {
        id: NAV_ID,
        label: tr(Msg::ShellNavigation),
        groups: navigation(request.reads_sync()),
        current: request.path.clone(),
        rail: Some(rail),
        nest: None,
    })
}

/// The top bar of `request`, the server as it was `reached`.
fn top_bar(request: &Request, reached: Option<Reached>) -> AnyView {
    let probe = reached.map_or(Probe::Unreachable, |reached| reached.probe);
    let open = reached.is_some_and(|reached| reached.open);
    topbar(TopBar {
        sidebar: NAV_ID,
        lockup: lockup(
            routes::ui_link("", request.version),
            brand_mark(MARK_LIGHT, MARK_DARK),
            LOCKUP_NAME,
            tr(Msg::ShellConsole),
        )
        .into_any(),
        search: Some(search(request)),
        end: end(request, probe, open),
    })
}

/// The footer every page carries.
#[must_use]
pub(crate) fn footer() -> Footer {
    Footer {
        product: Cow::Borrowed(PRODUCT),
        version: Cow::Borrowed(VERSION),
        about: Cow::Owned(routes::ABOUT_ROUTE.to_owned()),
    }
}

/// The banner of a development deployment: no sign-in on the console, and
/// none on the server, so anyone who reaches it can write.
fn development_banner() -> AnyView {
    banner(
        Tone::Warn,
        Glyph::TRIANGLE_ALERT,
        tr(Msg::ShellDevelopmentTitle),
        &tr(Msg::ShellDevelopmentBody),
        None,
    )
}

/// The navigation's groups, each entry a plain path, so the page in view is
/// marked whatever its query, with Synchronisation when `sync` (the reader
/// may read the sync service's runs).
fn navigation(sync: bool) -> Vec<Group> {
    let entry = |path: &str, label: Msg, glyph: Glyph, exact: bool| Entry {
        href: Cow::Owned(routes::ui_path(path)),
        label: Cow::Owned(tr(label)),
        glyph,
        exact,
        count: None,
    };
    let mut console = vec![entry(
        routes::SERVER_PATH,
        Msg::ShellServer,
        Glyph::SERVER,
        false,
    )];
    if sync {
        console.push(entry(
            routes::SYNC_PATH,
            Msg::ShellSynchronisation,
            Glyph::REFRESH_CW,
            false,
        ));
    }
    console.push(entry(
        routes::CONFIGURATION_PATH,
        Msg::ShellConfiguration,
        Glyph::FILE_TEXT,
        false,
    ));
    console.push(entry(
        routes::ABOUT_PATH,
        Msg::ShellAbout,
        Glyph::INFO,
        true,
    ));
    vec![
        Group {
            id: "nav-overview",
            title: Cow::Owned(tr(Msg::ShellGroupOverview)),
            entries: vec![entry(
                "",
                Msg::ShellDashboard,
                Glyph::LAYOUT_DASHBOARD,
                true,
            )],
        },
        Group {
            id: "nav-terminology",
            title: Cow::Owned(tr(Msg::ShellGroupTerminology)),
            entries: vec![
                entry(
                    routes::SYSTEMS_PATH,
                    Msg::ShellCodeSystems,
                    Glyph::BOOK_OPEN,
                    false,
                ),
                entry(
                    routes::VALUE_SETS_PATH,
                    Msg::ShellValueSets,
                    Glyph::LAYERS,
                    false,
                ),
                entry(
                    routes::CONCEPT_MAPS_PATH,
                    Msg::ShellConceptMaps,
                    Glyph::ARROW_LEFT_RIGHT,
                    false,
                ),
                entry(
                    routes::BROWSE_PATH,
                    Msg::ShellConceptBrowser,
                    Glyph::NETWORK,
                    false,
                ),
            ],
        },
        Group {
            id: "nav-operations",
            title: Cow::Owned(tr(Msg::ShellGroupOperations)),
            entries: vec![entry(
                routes::OPERATIONS_PATH,
                Msg::ShellOperations,
                Glyph::FLASK_CONICAL,
                false,
            )],
        },
        Group {
            id: "nav-console",
            title: Cow::Owned(tr(Msg::ShellGroupConsole)),
            entries: console,
        },
    ]
}

/// The top bar's search, whose panel asks the find's answer route as you
/// type ([`crate::pages::find::find_answer`]).
#[must_use]
pub(crate) fn search(request: &Request) -> AnyView {
    search_box(search_of(request))
}

/// The top bar's search box of `request`, holding the words of the find
/// page it is on.
fn search_of(request: &Request) -> SearchBox {
    let value = if request.path == routes::ui_path(routes::FIND_PATH) {
        request.text(crate::find::QUERY_PARAM)
    } else {
        String::new()
    };
    SearchBox {
        id: String::from(SEARCH_ID),
        name: String::from(crate::find::QUERY_PARAM),
        label: tr(Msg::ShellSearchLabel),
        placeholder: tr(Msg::ShellSearchPlaceholder),
        action: routes::ui_path(routes::FIND_PATH),
        value,
        suggest: Some(String::from(crate::pages::find::ANSWER_ROUTE)),
        ..SearchBox::default()
    }
}

/// The FHIR version select of `request`: each root a link to the same page
/// read through it, the reader's default marked.
fn versions(request: &Request) -> VersionSelect {
    let (path, query) = (&request.path, &request.query);
    VersionSelect {
        id: String::from(VERSION_MENU_ID),
        label: tr(Msg::ShellFhirVersion),
        roots: FhirVersion::ALL
            .into_iter()
            .map(|version| VersionRoot {
                name: version.label().to_owned(),
                release: version.release().to_owned(),
                href: routes::version_choice(path, query, version),
                default: version == request.prefs.fhir,
            })
            .collect(),
        current: request.version.label().to_owned(),
    }
}

/// The end of the top bar: the version select, the status pill, Sign in for
/// a visitor, and the user menu, each part that names the page in view
/// following the router.
fn end(request: &Request, probe: Probe, open: bool) -> AnyView {
    let location = use_location();
    let at = {
        let request = request.clone();
        move || {
            request.at(
                location.pathname.get(),
                location.search.get().trim_start_matches('?').to_owned(),
            )
        }
    };
    let (reach, words) = probe.pill();
    let pill = status_pill(StatusPill {
        reach,
        words: tr(words),
        detail: Some(tr_with(
            Msg::ShellServerAt,
            &[("root", request.root.clone().into())],
        )),
    });
    let select = {
        let at = at.clone();
        move || version_select(versions(&at()))
    };
    let signed_in = request.session.signed_in();
    let sign_in = (!signed_in && request.sign_in).then(|| {
        let at = at.clone();
        (move || {
            view! {
                <a href=routes::sign_in_link(&at().back()) class=Variant::Secondary.small()>
                    {label(Some(Glyph::LOG_IN), tr(Msg::ShellSignIn))}
                </a>
            }
        })
        .into_any()
    });
    let menu = move || user_menu(menu_of(&at(), open));
    view! {
        {select}
        {pill}
        {sign_in}
        {menu}
    }
    .into_any()
}

/// The user menu of `request`, saying the server is `open` to a visitor.
fn menu_of(request: &Request, open: bool) -> UserMenu {
    let choices = vec![versions(request).choices()];
    let theme = ThemeChoice {
        current: request.prefs.family.theme,
        action: String::from(routes::PREFERENCE_ROUTE),
        back: request.back(),
    };
    if request.session.signed_in() {
        let who = request
            .session
            .who()
            .unwrap_or_else(|| tr(Msg::ShellSignedIn));
        UserMenu {
            id: USER_MENU_ID,
            label: who.clone(),
            name: Some(who.clone()),
            who,
            line: request.session.standing(),
            preferences: routes::ui_path(routes::PREFERENCES_PATH),
            theme,
            choices,
            shortcuts: Some(routes::ui_path(routes::SHORTCUTS_PATH)),
            sign_out: Some(String::from(routes::SIGN_OUT_ROUTE)),
            links: Vec::new(),
        }
    } else {
        let (label, line) = if open {
            (Msg::ShellPreferencesMenu, Msg::ShellOpenLine)
        } else {
            (Msg::ShellVisitorMenu, Msg::ShellVisitorLine)
        };
        UserMenu {
            id: USER_MENU_ID,
            label: tr(label),
            name: None,
            who: tr(Msg::ShellNotSignedIn),
            line: tr(line),
            preferences: routes::ui_path(routes::PREFERENCES_PATH),
            theme,
            choices,
            shortcuts: Some(routes::ui_path(routes::SHORTCUTS_PATH)),
            sign_out: None,
            links: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::search_of;
    use crate::request::Request;
    use crate::visit::Visit;

    #[test]
    fn the_search_asks_the_find_answer_route_and_keeps_the_find_page_s_words() {
        let visit = Visit::default();
        let find = Request::of(&visit, String::from("/find"), String::from("q=respir"));
        let search = search_of(&find);
        assert_eq!(Some(String::from("/find/answer")), search.suggest);
        assert_eq!("respir", search.value);
        let other = Request::of(&visit, String::from("/systems"), String::from("q=respir"));
        assert_eq!("", search_of(&other).value, "only the find page fills it");
    }
}
