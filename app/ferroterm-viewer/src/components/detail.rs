// SPDX-License-Identifier: BUSL-1.1
//! What every detail screen shares: the tabs it keeps in its address, and
//! the pane that shows the resource as the server sent it.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::view::tabs::{KeyedTab, keyed_tabs};
use ferrohealth_ui::widget::document::{DocumentPane, document_pane};
use leptos::prelude::*;

use crate::locale::{Msg, tr};
use leptos_router::hooks::{use_location, use_query_map};

/// The address parameter naming the tab a detail screen shows.
pub(crate) const TAB_PARAM: &str = "tab";

/// The tab the address names, or `default` when it names none of `tabs`.
pub(crate) fn current_tab(
    tabs: &'static [(&'static str, Msg)],
    default: &'static str,
) -> Memo<&'static str> {
    let query = use_query_map();
    Memo::new(move |_| {
        query.with(|query| {
            let asked = query.get_str(TAB_PARAM).unwrap_or_default();
            tabs.iter()
                .map(|(key, _)| *key)
                .find(|key| *key == asked)
                .unwrap_or(default)
        })
    })
}

/// The address of this same screen with the tab `key`, every other
/// parameter kept: `search` is the query without its `?`, already encoded.
pub(crate) fn tab_link(pathname: &str, search: &str, key: &str) -> String {
    let mut kept: Vec<&str> = search
        .split('&')
        .filter(|pair| {
            !pair.is_empty() && pair.split_once('=').map_or(*pair, |(name, _)| name) != TAB_PARAM
        })
        .collect();
    let named = format!("{TAB_PARAM}={key}");
    kept.push(&named);
    format!("{pathname}?{}", kept.join("&"))
}

/// The tabs `tabs` of a detail screen named `label`, the one `current` names
/// marked; each is a link onto this same screen, so the server renders the
/// tab an address names and the browser goes back to it.
pub(crate) fn detail_tabs(
    label: String,
    tabs: &'static [(&'static str, Msg)],
    current: Memo<&'static str>,
) -> AnyView {
    let location = use_location();
    view! {
        {move || {
            let pathname = location.pathname.get();
            let search = location.search.get();
            let search = search.trim_start_matches('?').to_owned();
            let links = tabs
                .iter()
                .map(|(key, words)| KeyedTab {
                    key,
                    href: tab_link(&pathname, &search, key),
                    label: tr(*words).into(),
                })
                .collect();
            keyed_tabs(label.clone(), links, current.get())
        }}
    }
    .into_any()
}

/// The resource `text` as the server sent it, highlighted or raw, with a
/// copy, in the pane `id` named `label`.
pub(crate) fn json_pane(id: &'static str, label: String, text: String) -> AnyView {
    document_pane(
        DocumentPane {
            id,
            label: label.into(),
            formats: Vec::new(),
            format: Signal::stored("json"),
            text: Signal::stored(text),
            language: Signal::stored(Language::Json),
            rendered: None,
        },
        |_| (),
    )
}

#[cfg(test)]
mod tests {
    use super::tab_link;

    #[test]
    fn a_tab_link_names_its_tab_and_keeps_every_other_parameter() {
        assert_eq!(tab_link("/systems/x", "", "json"), "/systems/x?tab=json");
        assert_eq!(
            tab_link("/valuesets/a", "fhir=r5&tab=overview", "json"),
            "/valuesets/a?fhir=r5&tab=json"
        );
    }
}
