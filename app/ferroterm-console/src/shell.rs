// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every screen: the top bar, the navigation, the banner
//! and the footer, drawn on the server from FerroUI's shell.
//!
//! The top bar carries what is true of every screen at once: the search, the
//! FHIR version the reader is reading through (M3), the server's state as
//! the status pill (F12), Sign in for a visitor, and the user menu (F13),
//! which holds the theme, since the top bar carries no theme control
//! (FerroUI's family rule 5). The navigation folds to its rail by a plain
//! form post the console keeps in a cookie. No specification governs the
//! chrome: our own design.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]

use std::borrow::Cow;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::search::{SearchAnswer, SearchBox, search_box};
use ferrohealth_ui::view::shell::{Footer, Shell, TopBar, brand_mark, lockup, shell, topbar};
use ferrohealth_ui::view::status::{Reach, StatusPill, banner, status_pill};
use ferrohealth_ui::widget::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::widget::nav::{Entry, Group, Rail, Sidebar, sidebar};
use ferrohealth_ui::widget::shortcuts::LegendWords;
use ferrohealth_ui::widget::version::{VersionRoot, VersionSelect, version_select};
use http::StatusCode;
use leptos::prelude::*;
use leptos_router::components::Outlet;

use crate::fhir::version::FhirVersion;
use crate::islands::Shortcuts;
use crate::locale::{Msg, tr, tr_with};
use crate::request::{Request, request};
use crate::routes;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// The console's chrome around the page the route draws.
///
/// It asks the server for its health once per page, for the status pill,
/// and, on a console with no sign-in, whether the server itself asks for
/// one, for the development banner.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ConsoleShell() -> impl IntoView {
    framed(|| view! { <Outlet /> }.into_any())
}

/// The console's chrome around the page `content` draws, for a page the
/// router reaches outside the shell's routes, such as an address that names
/// no screen.
pub(crate) fn framed(content: fn() -> AnyView) -> AnyView {
    let request = request();
    view! {
        <Suspense>
            {Suspend::new(async move {
                let health = request.client.probe().await;
                let open = if request.sign_in {
                    false
                } else {
                    request.client.serves_without_sign_in(request.version).await
                };
                chrome(&request, Probe::of(health), open, content())
            })}
        </Suspense>
    }
    .into_any()
}

/// The chrome of `request`, the server `probe`d as it answered, with the
/// development banner when `open` (no sign-in on either side).
fn chrome(request: &Request, probe: Probe, open: bool, content: AnyView) -> AnyView {
    let rail = Rail {
        collapsed: request.prefs.rail,
        action: String::from(routes::SIDEBAR_ROUTE),
        back: request.back(),
    };
    let nav = sidebar(Sidebar {
        id: NAV_ID,
        label: tr(Msg::ShellNavigation),
        groups: navigation(),
        current: request.path.clone(),
        rail: Some(rail),
        nest: None,
    });
    let bar = topbar(TopBar {
        sidebar: NAV_ID,
        lockup: lockup(
            routes::ui_link("", request.version),
            brand_mark(MARK_LIGHT, MARK_DARK),
            LOCKUP_NAME,
            tr(Msg::ShellConsole),
        )
        .into_any(),
        search: Some(search(request, None)),
        end: end(request, probe, open),
    });
    let page = shell(Shell {
        topbar: bar,
        banner: open.then(development_banner),
        sidebar: nav,
        content,
        dock: None,
        footer: footer(),
    });
    let legend = request.path == routes::ui_path(routes::SHORTCUTS_PATH);
    view! {
        {page}
        <Shortcuts
            groups=crate::pages::shortcuts::groups()
            words=LegendWords::of_page()
            goto=crate::pages::shortcuts::goto()
            open=legend
        />
    }
    .into_any()
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
/// marked whatever its query.
fn navigation() -> Vec<Group> {
    let entry = |path: &str, label: Msg, glyph: Glyph, exact: bool| Entry {
        href: Cow::Owned(routes::ui_path(path)),
        label: Cow::Owned(tr(label)),
        glyph,
        exact,
        count: None,
    };
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
            entries: vec![
                entry(routes::SERVER_PATH, Msg::ShellServer, Glyph::SERVER, false),
                entry(
                    routes::CONFIGURATION_PATH,
                    Msg::ShellConfiguration,
                    Glyph::FILE_TEXT,
                    false,
                ),
                entry(routes::ABOUT_PATH, Msg::ShellAbout, Glyph::INFO, true),
            ],
        },
    ]
}

/// The top bar's search, with `answer` under it when the page has one.
#[must_use]
pub(crate) fn search(request: &Request, answer: Option<SearchAnswer>) -> AnyView {
    let value = if request.path == routes::ui_path(routes::FIND_PATH) {
        request.text(crate::find::QUERY_PARAM)
    } else {
        String::new()
    };
    search_box(
        SearchBox {
            id: String::from(SEARCH_ID),
            name: String::from(crate::find::QUERY_PARAM),
            label: tr(Msg::ShellSearchLabel),
            placeholder: tr(Msg::ShellSearchPlaceholder),
            action: routes::ui_path(routes::FIND_PATH),
            value,
            ..SearchBox::default()
        },
        answer,
    )
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
/// a visitor, and the user menu.
fn end(request: &Request, probe: Probe, open: bool) -> AnyView {
    let select = versions(request);
    let choices = vec![select.choices()];
    let (reach, words) = probe.pill();
    let pill = status_pill(StatusPill {
        reach,
        words: tr(words),
        detail: Some(tr_with(
            Msg::ShellServerAt,
            &[("root", request.root.clone().into())],
        )),
    });
    let theme = ThemeChoice {
        current: request.prefs.family.theme,
        action: String::from(routes::PREFERENCE_ROUTE),
        back: request.back(),
    };
    let signed_in = request.session.signed_in();
    let sign_in = (!signed_in && request.sign_in).then(|| {
        view! {
            <a href=routes::sign_in_link(&request.back()) class=Variant::Secondary.small()>
                {label(Some(Glyph::LOG_IN), tr(Msg::ShellSignIn))}
            </a>
        }
        .into_any()
    });
    let menu = if signed_in {
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
    };
    view! {
        {version_select(select)}
        {pill}
        {sign_in}
        {user_menu(menu)}
    }
    .into_any()
}
