// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keyboard shortcuts: the groups the legend lists, the go-to targets
//! the console's shortcuts island follows, and the page that lists them
//! with no WebAssembly.

use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::kbd::{Shortcut, ShortcutGroup, shortcut_keys};
use ferrohealth_ui::view::surface::{card, definitions, grid_start};
use leptos::prelude::*;

use crate::locale::{Msg, tr};
use crate::pages::parts::heading;
use crate::routes::{
    BROWSE_PATH, CONCEPT_MAPS_PATH, OPERATIONS_PATH, SERVER_PATH, SYSTEMS_PATH, VALUE_SETS_PATH,
    ui_path,
};

/// The go-to targets: the second key of each `g` pair and where it leads.
#[must_use]
pub(crate) fn goto() -> Vec<(String, String)> {
    [
        ("d", ""),
        ("c", SYSTEMS_PATH),
        ("v", VALUE_SETS_PATH),
        ("m", CONCEPT_MAPS_PATH),
        ("b", BROWSE_PATH),
        ("o", OPERATIONS_PATH),
        ("s", SERVER_PATH),
    ]
    .into_iter()
    .map(|(letter, path)| (letter.to_owned(), ui_path(path)))
    .collect()
}

/// A shortcut of `keys` doing `does`.
fn shortcut(keys: &str, does: Msg) -> Shortcut {
    Shortcut {
        keys: keys.to_owned(),
        does: tr(does),
    }
}

/// The groups the legend lists, their words in the page's locale.
#[must_use]
pub(crate) fn groups() -> Vec<ShortcutGroup> {
    vec![
        ShortcutGroup {
            title: tr(Msg::KeysEverywhere),
            shortcuts: vec![
                shortcut("/", Msg::KeysFind),
                shortcut("?", Msg::KeysLegend),
                shortcut("Esc", Msg::KeysClose),
                shortcut("g d", Msg::KeysDashboard),
                shortcut("g c", Msg::KeysCodeSystems),
                shortcut("g v", Msg::KeysValueSets),
                shortcut("g m", Msg::KeysConceptMaps),
                shortcut("g b", Msg::KeysBrowser),
                shortcut("g o", Msg::KeysOperations),
                shortcut("g s", Msg::KeysServer),
            ],
        },
        ShortcutGroup {
            title: tr(Msg::KeysTree),
            shortcuts: vec![
                shortcut("up | down", Msg::KeysTreeMove),
                shortcut("right", Msg::KeysTreeOpen),
                shortcut("left", Msg::KeysTreeClose),
                shortcut("Enter", Msg::KeysTreeRead),
            ],
        },
        ShortcutGroup {
            title: tr(Msg::KeysOperationsGroup),
            shortcuts: vec![
                shortcut("Ctrl Enter", Msg::KeysRun),
                shortcut("Ctrl s", Msg::KeysSave),
            ],
        },
    ]
}

/// The keyboard shortcuts, as a page.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ShortcutsPage() -> impl IntoView {
    let or = tr(Msg::KeysOr);
    let cards = groups()
        .into_iter()
        .map(|group| {
            let rows: Vec<(String, AnyView)> = group
                .shortcuts
                .iter()
                .map(|shortcut| {
                    (
                        shortcut.does.clone(),
                        shortcut_keys(&shortcut.keys, or.clone()),
                    )
                })
                .collect();
            card(group.title, definitions(rows)).into_any()
        })
        .collect_view();
    view! {
        {heading(tr(Msg::KeysTitle), Some(tr(Msg::KeysIntro)), Vec::new(), ActionGroup::none())}
        {grid_start(cards)}
    }
}
