// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keyboard shortcuts: the groups FerroUI's legend lists, and the page
//! that lists them with no script.
//!
//! Every key here is FerroUI's: `/` focuses the search (`search` module),
//! `?` opens the legend (`shortcuts` module), Escape closes what is open,
//! and the concept tree walks on the arrows (`tree` module). The console
//! listens for no key of its own.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::kbd::{Shortcut, ShortcutGroup, shortcut_keys};
use ferrohealth_ui::view::surface::{card, definitions, grid_start};

use crate::locale::{Msg, tr};
use crate::pages::parts::heading;
use crate::request::Request;

/// A shortcut of `keys` doing `does`.
fn shortcut(locale: &Locale, keys: &str, does: Msg) -> Shortcut {
    Shortcut {
        keys: keys.to_owned(),
        does: tr(locale, does),
    }
}

/// The groups the legend lists, their words in `locale`.
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
    ]
}

/// The keyboard shortcuts, as a page.
pub(crate) fn page(request: &Request) -> Html {
    let locale = &request.locale;
    let or = tr(locale, Msg::KeysOr);
    let cards = Html::join(groups(locale).into_iter().map(|group| {
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
    use ferrohealth_ui::view::shortcuts::shortcut_legend;

    use super::groups;

    #[test]
    fn the_legend_lists_the_console_s_keys_in_a_closed_dialog() {
        let english = Locale::english();
        let html = shortcut_legend(&english, groups(&english));
        let html = html.as_str();
        assert!(html.contains("<dialog id=\"shortcuts\""), "{html}");
        assert!(!html.contains(" open"), "drawn closed: {html}");
        assert!(
            html.contains("Find a resource, a code or a phrase"),
            "{html}"
        );
        assert!(html.contains("Read the concept"), "{html}");
    }
}
