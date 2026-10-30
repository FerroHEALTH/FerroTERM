// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every screen: the top bar, the navigation, the banner
//! and the footer, drawn from FerroUI's shell around the page.
//!
//! The top bar carries what is true of every screen at once: the search, the
//! FHIR version the reader is reading through (M3), the server's state as
//! the status pill (F12), Sign in for a visitor, and the user menu (F13),
//! which holds the theme, since the top bar carries no theme control
//! (FerroUI's family rule 5). The navigation folds to its rail by a form
//! post the console keeps in a cookie. Every page draws the whole shell
//! from its own request, so the navigation marks the page in view and the
//! version select names its address. No specification governs the chrome:
//! our own design.

use std::borrow::Cow;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::view::nav::{Entry, Group, Rail, Sidebar, sidebar};
use ferrohealth_ui::view::search::{SearchBox, search_box};
use ferrohealth_ui::view::session::{Watch, session_watch};
use ferrohealth_ui::view::shell::{Footer, Shell, TopBar, brand_mark, lockup, shell, topbar};
use ferrohealth_ui::view::shortcuts::shortcut_legend;
use ferrohealth_ui::view::status::{Reach, StatusPill, banner, status_pill};
use ferrohealth_ui::view::toast::toast_region;
use ferrohealth_ui::view::version::{VersionRoot, VersionSelect, version_select};
use http::StatusCode;

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{h, t};
use crate::request::Request;
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

/// The brand mark drawn on a light ground.
pub(crate) const MARK_LIGHT: &str = "/mark-light.svg";

/// The brand mark drawn on a dark ground.
pub(crate) const MARK_DARK: &str = "/mark-dark.svg";

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

/// How the terminology server answered the console, read once for the
/// shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub(crate) async fn reached(request: &Request) -> Reached {
    let health = request.client.probe().await;
    let open = if request.sign_in {
        false
    } else {
        request.client.serves_without_sign_in(request.version).await
    };
    Reached {
        probe: Probe::of(health),
        open,
    }
}

/// The console's chrome around `content`, the page `request` names, with
/// the server as it was `reached`: the shell, the session watch of a
/// signed-in reader and the shortcut legend.
pub(crate) fn console_shell(request: &Request, reached: Reached, content: Html) -> Html {
    let locale = &request.locale;
    let bar = top_bar(request, reached);
    let nav = navigation_of(request);
    let banner = reached.open.then(|| development_banner(request));
    // NOTE: FerroUI `docs/consoles.md`, "The session's end": the watch starts from what
    // the session had left as the server drew the page.
    let watch = request.left.map(|left| {
        session_watch(&Watch::new(
            locale,
            routes::SESSION_ROUTE,
            routes::SIGN_IN_PAGE,
            routes::SIGN_OUT_ROUTE,
            left,
        ))
    });
    let legend = shortcut_legend(locale, crate::pages::shortcuts::groups(locale));
    let content = Html::join([content, watch.unwrap_or_default(), legend]);
    shell(
        locale,
        Shell {
            topbar: bar,
            banner,
            sidebar: nav,
            content,
            dock: None,
            toasts: toast_region(locale, Vec::new()),
            footer: footer(),
        },
    )
}

/// The navigation of `request`, its rail as the reader left it.
fn navigation_of(request: &Request) -> Html {
    let rail = Rail {
        collapsed: request.prefs.rail,
        action: String::from(routes::SIDEBAR_ROUTE),
        back: request.back(),
    };
    sidebar(
        &request.locale,
        Sidebar {
            id: NAV_ID,
            label: request.tr(Msg::ShellNavigation),
            groups: navigation(&request.locale, request.reads_sync()),
            current: request.path.clone(),
            rail: Some(rail),
            nest: None,
        },
    )
}

/// The top bar of `request`, the server as it was `reached`.
fn top_bar(request: &Request, reached: Reached) -> Html {
    topbar(
        &request.locale,
        TopBar {
            sidebar: NAV_ID,
            lockup: lockup(
                routes::ui_link("", request.version),
                brand_mark(MARK_LIGHT, MARK_DARK),
                LOCKUP_NAME,
                request.tr(Msg::ShellConsole),
            ),
            search: Some(search(request)),
            end: end(request, reached.probe, reached.open),
        },
    )
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
fn development_banner(request: &Request) -> Html {
    banner(
        Tone::Warn,
        Glyph::TRIANGLE_ALERT,
        request.tr(Msg::ShellDevelopmentTitle),
        &request.tr(Msg::ShellDevelopmentBody),
        None,
    )
}

/// The navigation's groups, each entry a plain path, so the page in view is
/// marked whatever its query, with Synchronisation when `sync` (the reader
/// may read the sync service's runs).
fn navigation(locale: &ferrohealth_ui::i18n::Locale, sync: bool) -> Vec<Group> {
    let entry = |path: &str, label: Msg, glyph: Glyph, exact: bool| Entry {
        href: Cow::Owned(routes::ui_path(path)),
        label: Cow::Owned(tr(locale, label)),
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
            title: Cow::Owned(tr(locale, Msg::ShellGroupOverview)),
            entries: vec![entry(
                "",
                Msg::ShellDashboard,
                Glyph::LAYOUT_DASHBOARD,
                true,
            )],
        },
        Group {
            id: "nav-terminology",
            title: Cow::Owned(tr(locale, Msg::ShellGroupTerminology)),
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
            title: Cow::Owned(tr(locale, Msg::ShellGroupOperations)),
            entries: vec![entry(
                routes::OPERATIONS_PATH,
                Msg::ShellOperations,
                Glyph::FLASK_CONICAL,
                false,
            )],
        },
        Group {
            id: "nav-console",
            title: Cow::Owned(tr(locale, Msg::ShellGroupConsole)),
            entries: console,
        },
    ]
}

/// The top bar's search, whose panel asks the find's answer route as you
/// type ([`crate::pages::find::answer_route`]).
#[must_use]
pub(crate) fn search(request: &Request) -> Html {
    search_box(&request.locale, &search_of(request))
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
        label: request.tr(Msg::ShellSearchLabel),
        placeholder: request.tr(Msg::ShellSearchPlaceholder),
        action: routes::ui_path(routes::FIND_PATH),
        value,
        suggest: Some(String::from(crate::pages::find::ANSWER_ROUTE)),
        glyphs: crate::pages::find::GLYPHS.to_vec(),
        ..SearchBox::default()
    }
}

/// The FHIR version select of `request`: each root a link to the same page
/// read through it, the reader's default marked.
fn versions(request: &Request) -> VersionSelect {
    let (path, query) = (&request.path, &request.query);
    VersionSelect {
        id: String::from(VERSION_MENU_ID),
        label: request.tr(Msg::ShellFhirVersion),
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
fn end(request: &Request, probe: Probe, open: bool) -> Html {
    let locale = &request.locale;
    let (reach, words) = probe.pill();
    let pill = status_pill(StatusPill {
        reach,
        words: tr(locale, words),
        detail: Some(tr_with(
            locale,
            Msg::ShellServerAt,
            &[("root", request.root.clone().into())],
        )),
    });
    let select = version_select(locale, versions(request));
    let signed_in = request.session.signed_in();
    let sign_in = (!signed_in && request.sign_in).then(|| {
        h(format!(
            r#"<a href="{}" class="{}">{}</a>"#,
            t(routes::sign_in_link(&request.back())),
            Variant::Secondary.small(),
            label(Some(Glyph::LOG_IN), tr(locale, Msg::ShellSignIn)),
        ))
    });
    let menu = user_menu(locale, menu_of(request, open));
    Html::join([select, pill, sign_in.unwrap_or_default(), menu])
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
            .unwrap_or_else(|| request.tr(Msg::ShellSignedIn));
        UserMenu {
            id: USER_MENU_ID,
            label: who.clone(),
            name: Some(who.clone()),
            who,
            line: request.session.standing(&request.locale),
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
            label: request.tr(label),
            name: None,
            who: request.tr(Msg::ShellNotSignedIn),
            line: request.tr(line),
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
