//! The chrome around every screen, and the routes it wraps.
//!
//! The screens are reached from FerroUI's navigation; the top bar carries
//! what is true of every screen at once: the command bar, the FHIR version
//! the reader is looking through, the server's health, the theme, and who is
//! signed in.

use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::menu::{OperatorMenu, operator_menu};
use ferrohealth_ui::view::search::{SearchBox, search_box};
use ferrohealth_ui::view::segmented::{SegmentedForm, segmented_form};
use ferrohealth_ui::view::shell::{
    Footer, Shell, brand_mark, drawer_button, lockup, shell, topbar_with,
};
use ferrohealth_ui::view::theme_switch::theme_toggle;
use ferrohealth_ui::widget::nav::{Entry, Group, sidebar};
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_router::components::{Redirect, Route, Routes};
use leptos_router::hooks::{use_location, use_query, use_query_map};
use leptos_router::params::Params;
use leptos_router::{SsrMode, path};

use crate::auth::Session;
use crate::components::health::health_indicator;
use crate::components::icon;
use crate::fhir::version::FhirVersion;
use crate::find::QUERY_PARAM;
use crate::locale::{Msg, tr};
use crate::pages::about::{AboutPage, PANE_PARAM};
use crate::pages::browse::BrowsePage;
use crate::pages::code_system::CodeSystemPage;
use crate::pages::concept_map::ConceptMapPage;
use crate::pages::concept_map_editor::ConceptMapEditorPage;
use crate::pages::concept_maps::ConceptMapsPage;
use crate::pages::editor::EditorPage;
use crate::pages::expand::ExpandPage;
use crate::pages::find::FindPage;
use crate::pages::history::HistoryPage;
use crate::pages::lookup::LookupPage;
use crate::pages::not_found::NotFoundPage;
use crate::pages::overview::OverviewPage;
use crate::pages::systems::SystemsPage;
use crate::pages::translate::TranslatePage;
use crate::pages::validate::ValidatePage;
use crate::pages::value_set::ValueSetPage;
use crate::pages::value_set_composer::ValueSetComposerPage;
use crate::pages::value_sets::ValueSetsPage;
use crate::routes::{ABOUT_PATH, FIND_PATH, ui_link};
use crate::settings::{SetTheme, SetVersion, Settings};

/// The product name as a reader reads it.
pub(crate) const PRODUCT: &str = "FerroTERM Console";

/// The console's version, which the footer and the About page name.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the sign-in route the server serves.
pub(crate) const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub(crate) const SIGN_OUT: &str = "/logout";

/// The FHIR version the current address selects, for every screen to read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectedVersion(pub(crate) Signal<FhirVersion>);

/// What the console's server told the page about itself.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Console {
    /// Whether the console offers a sign-in: an OpenID Provider is configured.
    pub(crate) sign_in: bool,
    /// Whether the console reaches a sync service.
    pub(crate) sync: bool,
}

/// The id of the navigation, which the drawer button opens.
const NAV_ID: &str = "screens";

/// The command bar's own control, which its label points at.
const COMMAND_ID: &str = "command-bar";

/// The screens that read what this server holds.
fn browse() -> [Entry; 4] {
    [
        Entry {
            href: "/",
            label: tr(Msg::Dashboard).into(),
            glyph: icon::OVERVIEW,
            exact: true,
        },
        Entry {
            href: "/systems",
            label: tr(Msg::CodeSystems).into(),
            glyph: icon::CODE_SYSTEMS,
            exact: false,
        },
        Entry {
            href: "/valuesets",
            label: tr(Msg::ValueSets).into(),
            glyph: icon::VALUE_SETS,
            exact: false,
        },
        Entry {
            href: "/conceptmaps",
            label: tr(Msg::ConceptMaps).into(),
            glyph: icon::CONCEPT_MAPS,
            exact: false,
        },
    ]
}

/// The screens that run one terminology operation, each named for the
/// question it answers.
fn operations() -> [Entry; 4] {
    [
        Entry {
            href: "/lookup",
            label: tr(Msg::LookUpACode).into(),
            glyph: icon::LOOKUP,
            exact: false,
        },
        Entry {
            href: "/validate",
            label: tr(Msg::CheckACode).into(),
            glyph: icon::VALIDATE,
            exact: false,
        },
        Entry {
            href: "/expand",
            label: tr(Msg::ExpandAValueSet).into(),
            glyph: icon::EXPAND,
            exact: false,
        },
        Entry {
            href: "/translate",
            label: tr(Msg::TranslateACode).into(),
            glyph: icon::TRANSLATE,
            exact: false,
        },
    ]
}

/// The screens that write, on a console that offers a sign-in.
fn authoring() -> [Entry; 3] {
    [
        Entry {
            href: "/codesystem",
            label: tr(Msg::CodeSystemEditor).into(),
            glyph: icon::EDIT,
            exact: false,
        },
        Entry {
            href: "/compose",
            label: tr(Msg::ValueSetComposer).into(),
            glyph: icon::EDIT,
            exact: false,
        },
        Entry {
            href: "/conceptmap",
            label: tr(Msg::ConceptMapEditor).into(),
            glyph: icon::EDIT,
            exact: false,
        },
    ]
}

/// The one screen about this server and this console.
fn about() -> [Entry; 1] {
    [Entry {
        href: "/about",
        label: tr(Msg::AboutThisServer).into(),
        glyph: icon::ABOUT,
        exact: false,
    }]
}

/// The navigation of a console that offers a sign-in: read, run, write, and
/// the server itself.
fn nav_groups() -> Vec<Group> {
    vec![
        Group {
            id: "screens-browse",
            title: tr(Msg::Browse).into(),
            entries: browse().into(),
        },
        Group {
            id: "screens-operations",
            title: tr(Msg::Operations).into(),
            entries: operations().into(),
        },
        Group {
            id: "screens-authoring",
            title: tr(Msg::Authoring).into(),
            entries: authoring().into(),
        },
        Group {
            id: "screens-server",
            title: tr(Msg::Server).into(),
            entries: about().into(),
        },
    ]
}

/// The navigation of a console that offers no sign-in: the same groups, with
/// no entry onto a screen that writes, since nobody there can save.
fn reader_groups() -> Vec<Group> {
    vec![
        Group {
            id: "screens-browse",
            title: tr(Msg::Browse).into(),
            entries: browse().into(),
        },
        Group {
            id: "screens-operations",
            title: tr(Msg::Operations).into(),
            entries: operations().into(),
        },
        Group {
            id: "screens-server",
            title: tr(Msg::Server).into(),
            entries: about().into(),
        },
    ]
}

/// The shell's own query parameters.
#[derive(Clone, Debug, Params, PartialEq)]
struct ShellQuery {
    /// The FHIR version, as its path segment.
    fhir: Option<String>,
}

/// The address the reader is on, path and query, which a preference post
/// returns to.
fn here() -> Signal<String> {
    let location = use_location();
    Signal::derive(move || {
        let path = location.pathname.get();
        let search = location.search.get();
        if search.is_empty() {
            path
        } else {
            format!("{path}?{}", search.trim_start_matches('?'))
        }
    })
}

/// The command bar, on every screen: what a reader types is read by its
/// shape alone, in `crate::find`, and the screen it opens offers what this
/// root can do with it.
fn command_bar() -> AnyView {
    let query = use_query_map();
    let typed = move || query.with(|query| query.get(QUERY_PARAM).unwrap_or_default());
    view! {
        {move || {
            search_box(SearchBox {
                id: COMMAND_ID,
                name: QUERY_PARAM,
                label: tr(Msg::FindCodeCanonicalPhrase).into(),
                placeholder: tr(Msg::CodeCanonicalPhrase).into(),
                action: format!("/{FIND_PATH}"),
                value: typed(),
            })
        }}
    }
    .into_any()
}

/// The switch over the four FHIR roots the server mounts: each segment posts
/// its version to the console, which keeps it as the one an address naming
/// none reads and returns the reader to the same page on it.
fn version_switch(version: Signal<FhirVersion>, back: Signal<String>) -> AnyView {
    view! {
        {move || {
            segmented_form(SegmentedForm {
                name: "fhir",
                label: tr(Msg::FhirVersion).into(),
                prefix: Some((icon::VERSION, tr(Msg::Fhir).into())),
                choices: FhirVersion::ALL
                    .into_iter()
                    .map(|choice| (choice.segment().to_owned(), choice.label().to_owned()))
                    .collect(),
                current: version.get().segment().to_owned(),
                action: SetVersion::url().to_owned(),
                back: back.get(),
            })
        }}
    }
    .into_any()
}

/// Who is signed in, or the way to sign in, on a console that offers it.
fn operator(console: Console) -> AnyView {
    let session = expect_context::<Session>();
    if !console.sign_in {
        return ().into_any();
    }
    let drawn = move || {
        if !session.signed_in() {
            return view! {
                <a href=SIGN_IN rel="external" class=Variant::Secondary.class()>
                    {icon(Glyph::LOG_IN, "16")}
                    {tr(Msg::SignIn)}
                </a>
            }
            .into_any();
        }
        let menu = OperatorMenu {
            id: "operator-menu",
            name: session.who().unwrap_or_else(|| tr(Msg::SignedIn)),
            subtitle: session.standing().into(),
            sign_out: Some(String::from(SIGN_OUT)),
        };
        operator_menu(menu, ()).into_any()
    };
    drawn.into_any()
}

/// The top bar: the lockup, the command bar in the middle, and the version,
/// the server's health, the reader and the theme at the end.
fn bar(version: Signal<FhirVersion>, back: Signal<String>, console: Console) -> AnyView {
    let start = view! {
        {drawer_button(NAV_ID)}
        {lockup("/", brand_mark("/mark-light.svg", "/mark-dark.svg"), "FerroTERM", tr(Msg::Console))}
    }
    .into_any();
    let end = view! {
        {version_switch(version, back)}
        {health_indicator()}
        {operator(console)}
        {move || theme_toggle(SetTheme::url().to_owned(), back.get())}
    }
    .into_any();
    topbar_with(start, command_bar(), end)
}

/// The routes, every screen rendered whole on the server before it is sent
/// (the Leptos book, `ssr/23_ssr_modes`), so no content waits on a script.
fn routes(console: Console) -> AnyView {
    view! {
        <Routes fallback=|| view! { <NotFoundPage /> }>
            <Route path=path!("/") view=OverviewPage ssr=SsrMode::Async />
            <Route path=path!("/browse") view=BrowsePage ssr=SsrMode::Async />
            <Route path=path!("/expand") view=ExpandPage ssr=SsrMode::Async />
            <Route path=path!("/lookup") view=LookupPage ssr=SsrMode::Async />
            <Route path=path!("/validate") view=ValidatePage ssr=SsrMode::Async />
            <Route path=path!("/about") view=AboutPage ssr=SsrMode::Async />
            <Route path=path!("/systems") view=SystemsPage ssr=SsrMode::Async />
            <Route path=path!("/systems/:url") view=CodeSystemPage ssr=SsrMode::Async />
            <Route path=path!("/conceptmaps") view=ConceptMapsPage ssr=SsrMode::Async />
            <Route path=path!("/conceptmaps/:id") view=ConceptMapPage ssr=SsrMode::Async />
            <Route path=path!("/translate") view=TranslatePage ssr=SsrMode::Async />
            <Route path=path!("/valuesets") view=ValueSetsPage ssr=SsrMode::Async />
            <Route path=path!("/valuesets/:id") view=ValueSetPage ssr=SsrMode::Async />
            <Route path=path!("/find") view=FindPage ssr=SsrMode::Async />
            <Route
                path=path!("/codesystem")
                view=move || writes(console, || view! { <EditorPage /> }.into_any())
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/compose")
                view=move || writes(console, || view! { <ValueSetComposerPage /> }.into_any())
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/conceptmap")
                view=move || writes(console, || view! { <ConceptMapEditorPage /> }.into_any())
                ssr=SsrMode::Async
            />
            <Route path=path!("/history") view=HistoryPage ssr=SsrMode::Async />
            // The three panes were three screens. A link written then still
            // opens the pane it named.
            <Route path=path!("/versions") view=|| moved_to("versions") />
            <Route path=path!("/evidence") view=|| moved_to("evidence") />
            <Route path=path!("/settings") view=|| moved_to("settings") />
        </Routes>
    }
    .into_any()
}

/// An authoring screen, on a console that offers a sign-in; elsewhere nobody
/// can save, so the address answers as one the console does not have.
fn writes(console: Console, page: impl FnOnce() -> AnyView) -> AnyView {
    if console.sign_in {
        page()
    } else {
        view! { <NotFoundPage /> }.into_any()
    }
}

/// Sends an address a pane used to have to the pane it named: a redirect the
/// server answers before the page renders, or a replacing navigation once it
/// has hydrated (`leptos_router`, `Redirect`).
fn moved_to(pane: &'static str) -> impl IntoView {
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let target = format!(
        "{}&{PANE_PARAM}={pane}",
        ui_link(ABOUT_PATH, version.get_untracked())
    );
    view! { <Redirect path=target /> }
}

/// The chrome, the routed screen, and the navigation beside it.
///
/// The version is read reactively from the query on every render, because a
/// link to another version is a navigation that matches the same route:
/// `leptos_router` then updates the params without re-running this body, so
/// an untracked read taken at setup would go stale.
pub(crate) fn console_shell(console: Console) -> AnyView {
    let settings = expect_context::<Settings>();
    let query = use_query::<ShellQuery>();
    let default = settings.version;
    // NOTE: A `?fhir=` value naming no served version is a link a reader typed,
    // so it reads as absent and the reader's own default applies.
    let version = Signal::derive(move || {
        query
            .read()
            .as_ref()
            .ok()
            .and_then(|query| query.fhir.as_deref())
            .and_then(|text| text.parse::<FhirVersion>().ok())
            .unwrap_or(default)
    });
    provide_context(SelectedVersion(version));
    let back = here();
    shell(Shell {
        topbar: bar(version, back, console),
        banner: None,
        sidebar: sidebar(
            NAV_ID,
            tr(Msg::Screens),
            if console.sign_in {
                nav_groups()
            } else {
                reader_groups()
            },
        ),
        content: routes(console),
        footer: Footer {
            product: PRODUCT.into(),
            version: VERSION,
            about: "/about",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{about, browse, nav_groups, reader_groups};

    /// The path of every entry, in the order the navigation draws them.
    fn entries() -> Vec<&'static str> {
        nav_groups()
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.href))
            .collect()
    }

    #[test]
    fn the_navigation_opens_on_the_overview_and_ends_on_the_server_itself() {
        let listed = entries();
        assert_eq!(listed.first().copied(), Some("/"), "{listed:?}");
        assert_eq!(listed.last().copied(), Some("/about"), "{listed:?}");
    }

    #[test]
    fn no_two_entries_lead_to_the_same_screen() {
        let listed = entries();
        for (index, href) in listed.iter().enumerate() {
            assert!(
                !listed.iter().skip(index + 1).any(|other| other == href),
                "two entries lead to `{href}`: {listed:?}"
            );
        }
    }

    #[test]
    fn a_console_without_a_sign_in_leads_to_no_screen_that_writes() {
        let listed: Vec<&str> = reader_groups()
            .iter()
            .flat_map(|group| group.entries.iter().map(|entry| entry.href))
            .collect();
        for href in ["/codesystem", "/compose", "/conceptmap"] {
            assert!(!listed.contains(&href), "{href}: {listed:?}");
        }
        assert_eq!(
            listed.len() + 3,
            entries().len(),
            "it leaves out the three authoring screens and nothing else"
        );
    }

    #[test]
    fn the_navigation_offers_the_authoring_screens() {
        let listed = entries();
        for href in ["/codesystem", "/compose", "/conceptmap"] {
            assert!(listed.contains(&href), "{href}: {listed:?}");
        }
    }

    #[test]
    fn every_group_is_labelled_and_holds_a_screen() {
        for group in &nav_groups() {
            assert!(
                !group.title.is_empty(),
                "a group with no label reads as a run of links"
            );
            assert!(
                !group.entries.is_empty(),
                "the `{}` group is empty",
                group.title
            );
        }
        assert_eq!(
            nav_groups().first().map(|group| group.entries.len()),
            Some(browse().len())
        );
        assert_eq!(
            nav_groups().last().map(|group| group.entries.len()),
            Some(about().len())
        );
    }
}
