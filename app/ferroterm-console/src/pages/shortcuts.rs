// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keyboard shortcuts: the groups and go-to pairs FerroUI's legend
//! lists, and the page that lists them with no script.
//!
//! Every key here is bound by FerroUI: `/` focuses the search (`search`
//! module), `?` opens the legend (`shortcuts` module), Escape closes what is
//! open, the concept tree walks on the arrows (`tree` module), and the
//! `keys` module presses the control that announces [`RUN_KEYS`] or
//! [`SAVE_KEYS`] and follows a go-to pair (FerroUI `docs/consoles.md`,
//! "Keyboard shortcuts"). The console listens for no key of its own.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::kbd::{Shortcut, ShortcutGroup, shortcut_keys};
use ferrohealth_ui::view::shortcuts::{GoTo, GoToPair};
use ferrohealth_ui::view::surface::{card, definitions, grid_start};

use crate::locale::{Msg, tr};
use crate::pages::parts::heading;
use crate::request::Request;
use crate::routes::{
    BROWSE_PATH, CONCEPT_MAPS_PATH, OPERATIONS_PATH, SERVER_PATH, SYSTEMS_PATH, VALUE_SETS_PATH,
    ui_path,
};

/// The chord that presses an operation's Run and the composer's Run the
/// preview, from inside a field of its form too.
pub(crate) const RUN_KEYS: &str = "Ctrl Enter";

/// The chord that presses an editor's docked Save.
pub(crate) const SAVE_KEYS: &str = "Ctrl s";

/// The first key of every go-to pair.
const GO_TO_LEAD: &str = "g";

/// A shortcut of `keys` doing `does`.
fn shortcut(locale: &Locale, keys: &str, does: Msg) -> Shortcut {
    Shortcut {
        keys: keys.to_owned(),
        does: tr(locale, does),
    }
}

/// The go-to pairs, each a letter after [`GO_TO_LEAD`] and a link of the
/// navigation, named as the navigation names it in `locale`.
#[must_use]
pub(crate) fn go_to(locale: &Locale) -> GoTo {
    let pair = |key: &str, path: &str, does: Msg| GoToPair {
        key: key.to_owned(),
        href: ui_path(path),
        does: tr(locale, does),
    };
    GoTo {
        title: tr(locale, Msg::KeysGoTo),
        lead: GO_TO_LEAD.to_owned(),
        pairs: vec![
            pair("d", "", Msg::ShellDashboard),
            pair("c", SYSTEMS_PATH, Msg::ShellCodeSystems),
            pair("v", VALUE_SETS_PATH, Msg::ShellValueSets),
            pair("m", CONCEPT_MAPS_PATH, Msg::ShellConceptMaps),
            pair("b", BROWSE_PATH, Msg::ShellConceptBrowser),
            pair("o", OPERATIONS_PATH, Msg::ShellOperations),
            pair("s", SERVER_PATH, Msg::ShellServer),
        ],
    }
}

/// The groups the legend lists before the go-to pairs, their words in
/// `locale`.
#[must_use]
pub(crate) fn groups(locale: &Locale) -> Vec<ShortcutGroup> {
    vec![
        ShortcutGroup {
            title: tr(locale, Msg::KeysEverywhere),
            shortcuts: vec![
                shortcut(locale, "/", Msg::KeysFind),
                shortcut(locale, "?", Msg::KeysLegend),
                shortcut(locale, "Esc", Msg::KeysClose),
            ],
        },
        ShortcutGroup {
            title: tr(locale, Msg::KeysTree),
            shortcuts: vec![
                shortcut(locale, "up | down", Msg::KeysTreeMove),
                shortcut(locale, "right", Msg::KeysTreeOpen),
                shortcut(locale, "left", Msg::KeysTreeClose),
                shortcut(locale, "Enter", Msg::KeysTreeRead),
            ],
        },
        ShortcutGroup {
            title: tr(locale, Msg::KeysOperationsGroup),
            shortcuts: vec![
                shortcut(locale, RUN_KEYS, Msg::KeysRun),
                shortcut(locale, SAVE_KEYS, Msg::KeysSave),
            ],
        },
    ]
}

/// The keyboard shortcuts, as a page.
pub(crate) fn page(request: &Request) -> Html {
    let locale = &request.locale;
    let or = tr(locale, Msg::KeysOr);
    let mut listed = groups(locale);
    listed.push(go_to(locale).group());
    let cards = Html::join(listed.into_iter().map(|group| {
        let rows: Vec<(String, Html)> = group
            .shortcuts
            .iter()
            .map(|shortcut| {
                (
                    shortcut.does.clone(),
                    shortcut_keys(&shortcut.keys, or.clone()),
                )
            })
            .collect();
        card(group.title, definitions(rows))
    }));
    Html::join([
        heading(
            request,
            tr(locale, Msg::KeysTitle),
            Some(tr(locale, Msg::KeysIntro)),
            Vec::new(),
            ActionGroup::none(),
        ),
        grid_start(cards),
    ])
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;
    use ferrohealth_ui::view::shortcuts::shortcut_legend_with_go_to;

    use super::{go_to, groups};

    #[test]
    fn the_legend_lists_the_console_s_keys_in_a_closed_dialog() {
        let english = Locale::english();
        let html = shortcut_legend_with_go_to(&english, groups(&english), go_to(&english));
        let html = html.as_str();
        assert!(html.contains("<dialog id=\"shortcuts\""), "{html}");
        assert!(!html.contains(" open"), "drawn closed: {html}");
        assert!(
            html.contains("Find a resource, a code or a phrase"),
            "{html}"
        );
        assert!(html.contains("Read the concept"), "{html}");
        assert!(html.contains("Run, or run the preview"), "{html}");
        assert!(html.contains("Save"), "{html}");
    }

    #[test]
    fn every_go_to_pair_is_a_link_of_the_navigation() {
        let english = Locale::english();
        let html = shortcut_legend_with_go_to(&english, groups(&english), go_to(&english));
        let html = html.as_str();
        assert!(html.contains("Go to"), "{html}");
        for path in [
            "/",
            "/systems",
            "/valuesets",
            "/conceptmaps",
            "/browse",
            "/lookup",
            "/server",
        ] {
            assert!(html.contains(&format!("href=\"{path}\"")), "{path}: {html}");
        }
    }

    #[test]
    fn the_shortcuts_page_lists_the_go_to_pairs_as_keys() {
        let english = Locale::english();
        let group = go_to(&english).group();
        let keys: Vec<&str> = group.shortcuts.iter().map(|s| s.keys.as_str()).collect();
        assert_eq!(keys, ["g d", "g c", "g v", "g m", "g b", "g o", "g s"]);
    }
}
