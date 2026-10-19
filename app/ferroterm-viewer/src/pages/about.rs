//! Everything about this server and this console, on one screen.
//!
//! Three tabs, one shown at a time. They are one screen because none of them
//! is about a code, and a reader arrives with one of the three questions.
//!
//! The tab being read is the `tab` parameter of the address, so the server
//! renders the pane a link names and the browser goes back to it. A link
//! written before they were one screen still opens the tab it named, through
//! the redirects in `shell.rs`.

use ferrohealth_ui::view::surface::{page_header, stack};
use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::detail::{TAB_PARAM, current_tab, detail_tabs};
use crate::pages::evidence;
use crate::pages::settings;
use crate::pages::versions;

/// The address parameter naming the tab being read.
pub(crate) const PANE_PARAM: &str = TAB_PARAM;

/// The tabs, in the order the screen draws them; the first is what a reader
/// who named none of them gets.
const TABS: &[(&str, &str)] = &[
    ("versions", "The four FHIR versions"),
    ("evidence", "The evidence this build ships"),
    ("settings", "Settings"),
];

/// Shows the three tabs about this server and this console.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn AboutPage() -> impl IntoView {
    let tab = current_tab(TABS, "versions");
    let body = move || {
        let key = tab.get();
        let (label, body) = match key {
            "evidence" => ("The evidence this build ships", evidence::pane()),
            "settings" => ("Settings", settings::pane()),
            _ => ("The four FHIR versions", versions::pane()),
        };
        pane(key, label, body)
    };
    view! {
        <Title text="About this server" />
        {page_header(
            "About this server",
            Some(
                String::from(
                    "What each FHIR root declares, the evidence this build ships, and this console's settings.",
                ),
            ),
            (),
        )}
        {stack(
            view! {
                {detail_tabs("About this server", TABS, tab)}
                {body}
            },
        )}
    }
}

/// The element id one tab's pane answers to.
fn anchor(id: &str) -> String {
    format!("about-{id}-heading")
}

/// One pane, named by its tab.
fn pane(id: &str, label: &'static str, body: AnyView) -> AnyView {
    let heading = anchor(id);
    view! {
        <section id=heading.clone() aria-labelledby=format!("{heading}-name") class="fui-stack">
            <h2 id=format!("{heading}-name") class="fui-sr-only">
                {label}
            </h2>
            {body}
        </section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::anchor;

    #[test]
    fn a_pane_answers_to_an_id_its_tab_names() {
        assert_eq!(anchor("settings"), "about-settings-heading");
    }
}
