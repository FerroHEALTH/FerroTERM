// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Journeys through the console's reading screens.
//!
//! Every page arrives whole from the console's server, so a step is a link
//! followed or a form submitted, and each journey checks what the next page
//! drew and the address it is at, which is the link a reader shares.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// The navigation the screens are reached from, by the landmark it announces.
const NAVIGATION: &str = "nav[aria-label='Console']";

/// The navigation entry marked as the screen the reader is on.
const CURRENT_SCREEN: &str = "nav[aria-label='Console'] a[aria-current='page']";

/// The FHIR version select in the top bar (M3), by the summary that opens it.
const VERSION_SELECT: &str = "summary.fui-version-select";

/// The canonical of the shaped taxonomy every journey reads.
const TAXONOMY: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// The value set over it.
const ALL_OF_IT: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";

/// A code system row's link into that system's own screen.
const SYSTEM_LINK: &str = "main tbody a[href^='/systems/']";

/// The page's title.
const TITLE: &str = "main h1";

/// The verdict line of an operation's answer (B4).
const VERDICT: &str = "main .fui-load-report";

/// Run, at the end of an operation's options row.
const RUN: &str = "main .fui-transform-run button[type='submit']";

/// The top bar's search field.
const FIND_FIELD: &str = "#find";

/// A navigation entry, by the words a reader reads on it.
fn screen_link(label: &str) -> String {
    format!("//nav[@aria-label='Console']//a[normalize-space()='{label}']")
}

/// A tab of a resource page, by its words.
fn tab(label: &str) -> String {
    format!("//nav[contains(@aria-label, 'Views of this')]//a[normalize-space()='{label}']")
}

/// A value percent-encoded as one query component.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The navigation reaches another screen in the session's FHIR version and
/// marks the one it reached.
///
/// The mark is the `aria-current="page"` a screen reader announces, computed
/// on the server from the address, so a reader who typed the address gets the
/// same mark. An entry is a plain path, and the version comes from the one
/// the version menu kept for the session.
#[tokio::test]
async fn the_navigation_reaches_another_screen_and_marks_the_one_it_reached() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            journey
                .reopen(&format!("{base}/fhir-version?fhir=r4&back=%2F"))
                .await;
            journey.element(By::Css(NAVIGATION), "the navigation").await;
            journey
                .press(
                    By::XPath(screen_link("Code systems")),
                    "the code systems entry",
                )
                .await?;
            let address = journey
                .address_carrying("/systems", "the code systems screen")
                .await;
            assert!(
                !address.contains("fhir="),
                "the entry is a plain path: {address}"
            );
            journey
                .text_becoming(
                    By::Css(VERSION_SELECT),
                    StringMatch::new("R4").case_insensitive(),
                    "the screen to read the session's version",
                )
                .await;
            journey
                .text_becoming(
                    By::Css(CURRENT_SCREEN),
                    StringMatch::new("Code systems").partial(),
                    "the entry to be marked as the screen the reader is on",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The version select moves the page onto another FHIR version and keeps it
/// for the session, so a screen whose address names none reads it.
#[tokio::test]
async fn the_version_select_moves_the_page_onto_another_version() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/systems?fhir=r4").await;
            journey
                .press(By::Css(VERSION_SELECT), "the FHIR version select")
                .await?;
            let r5 = journey
                .visible(
                    By::XPath("//a[@role='menuitemradio'][.//span[normalize-space()='R5']]"),
                    "the R5 choice",
                )
                .await;
            journey.press_on(&r5, "the R5 choice").await?;
            let address = journey.address_carrying("fhir=r5", "the page on R5").await;
            assert!(
                address.contains("/systems"),
                "the select stays on the page: {address}"
            );
            journey
                .text_becoming(
                    By::Css(VERSION_SELECT),
                    StringMatch::new("R5").partial(),
                    "the select to name the version in force",
                )
                .await;
            journey.reopen(&format!("{base}/valuesets")).await;
            journey
                .text_becoming(
                    By::Css(VERSION_SELECT),
                    StringMatch::new("R5").partial(),
                    "a page whose address names no version to read the session's",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A row opens the code system screen, its tabs each draw their pane, and
/// the address of every tab opens the same pane when it is loaded fresh.
#[tokio::test]
async fn a_row_opens_the_code_system_screen_and_each_tab_is_a_link() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/systems?fhir=r4").await;
            journey
                .press(By::Css(SYSTEM_LINK), "the first code system's link")
                .await?;
            journey
                .text_becoming(
                    By::Css("main .fui-page-subtitle"),
                    StringMatch::new(TAXONOMY).partial(),
                    "the code system screen to state its canonical",
                )
                .await;
            journey
                .press(By::XPath(tab("Versions")), "the Versions tab")
                .await?;
            let versions = journey
                .address_carrying("pane=versions", "the Versions pane")
                .await;
            journey
                .press(By::XPath(tab("JSON")), "the JSON tab")
                .await?;
            journey
                .text_becoming(
                    By::Css("main .fui-tab-panel"),
                    StringMatch::new("\"resourceType\"").partial(),
                    "the JSON pane to show the resource as the server sent it",
                )
                .await;
            journey.reopen(&versions).await;
            journey
                .text_becoming(
                    By::Css("nav[aria-label='Views of this code system'] a[aria-current='page']"),
                    StringMatch::new("Versions").partial(),
                    "the Versions address to open on its pane",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The listing orders itself by a column, and the order is in the address.
#[tokio::test]
async fn the_listing_orders_itself_and_the_order_is_a_link() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/systems?fhir=r4").await;
            journey
                .element(
                    By::Css("main th[aria-sort='ascending']"),
                    "the ordered column",
                )
                .await;
            journey
                .press(
                    By::Css("main th button.fui-sort"),
                    "the column's order control",
                )
                .await?;
            let address = journey
                .address_carrying("order=desc", "the listing ordered downward")
                .await;
            journey
                .element(
                    By::Css("main th[aria-sort='descending']"),
                    "the column ordered downward",
                )
                .await;
            journey.reopen(&address).await;
            journey
                .element(
                    By::Css("main th[aria-sort='descending']"),
                    "the order to survive a fresh load",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A listed value set and concept map each open the resource the row names.
#[tokio::test]
async fn a_listed_row_opens_the_resource_it_names() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/valuesets?fhir=r4").await;
            for (listing, link) in [
                ("/valuesets?fhir=r4", "main tbody a[href^='/valuesets/']"),
                (
                    "/conceptmaps?fhir=r4",
                    "main tbody a[href^='/conceptmaps/']",
                ),
            ] {
                journey.reopen(&format!("{base}{listing}")).await;
                let row = journey.element(By::Css(link), "the first listed row").await;
                let named = row.text().await?;
                journey.press_on(&row, "the first listed row").await?;
                journey
                    .text_becoming(
                        By::Css(TITLE),
                        StringMatch::new(named.trim()).partial(),
                        "the resource page to carry the name its row read",
                    )
                    .await;
            }
            journey
                .press(By::XPath(tab("Overview")), "the concept map's Overview")
                .await?;
            journey
                .address_carrying("pane=overview", "the Overview pane")
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The top bar's search reads what was typed and offers the screens that
/// take it, each a link with the words filled in.
#[tokio::test]
async fn the_search_reads_what_was_typed_and_offers_addresses_for_it() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r4").await;
            journey
                .press(By::Css(FIND_FIELD), "the search field")
                .await?;
            journey.type_here("ca-leaf\n").await?;
            journey
                .address_carrying("/find?q=ca-leaf", "the find page")
                .await;
            journey
                .press(
                    By::XPath("//main//a[normalize-space()='Look up ca-leaf']"),
                    "the look up offer",
                )
                .await?;
            journey
                .address_carrying("code=ca-leaf", "Look up with the code filled in")
                .await;
            journey
                .element(
                    By::Css("#lookup-code[value='ca-leaf']"),
                    "the code in its field",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A code is looked up with no canonical typed: the code system is picked
/// from what the root publishes, and the answer names what it found.
#[tokio::test]
async fn a_code_is_looked_up_from_a_picked_code_system() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r4").await;
            journey
                .press(By::XPath(screen_link("Operations")), "the operations entry")
                .await?;
            journey.address_carrying("/lookup", "the Look up tab").await;
            journey
                .press(
                    By::Css(format!("#lookup-system option[value='{TAXONOMY}']")),
                    "the taxonomy offered",
                )
                .await?;
            journey
                .press(By::Css("#lookup-code"), "the code field")
                .await?;
            journey.type_here("ba-branch").await?;
            journey.press(By::Css(RUN), "Look up").await?;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("First branch").partial(),
                    "the answer to name the concept found",
                )
                .await;
            journey
                .element(
                    By::Css("main [role='status']"),
                    "the verdict announced as a status",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A code is checked against a value set and the relation of two codes is
/// checked, each from the Check tab.
#[tokio::test]
async fn a_code_is_checked_in_a_value_set_and_two_codes_are_related() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(
                driver,
                &base,
                &format!(
                    "/validate?fhir=r4&url={}&system={}",
                    encoded(ALL_OF_IT),
                    encoded(TAXONOMY)
                ),
            )
            .await;
            journey
                .press(By::Css("#check-code"), "the code field")
                .await?;
            journey.type_here("ca-leaf").await?;
            journey.press(By::Css(RUN), "Check").await?;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("Valid").partial(),
                    "the answer to say the code is in the value set",
                )
                .await;
            journey
                .press(
                    By::XPath(
                        "//main//a[contains(@class, 'fui-segment')][normalize-space()='Relation']",
                    ),
                    "the Relation choice",
                )
                .await?;
            journey
                .address_carrying("mode=relation", "the relation form")
                .await;
            journey.press(By::Css("#check-code-a"), "code A").await?;
            journey.type_here("aa-root").await?;
            journey.press(By::Css("#check-code-b"), "code B").await?;
            journey.type_here("ca-leaf").await?;
            journey.press(By::Css(RUN), "Check the relation").await?;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("subsumes").partial(),
                    "the answer to say how the two codes relate",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// An expansion is read a page at a time, the next page a link.
#[tokio::test]
async fn an_expansion_is_read_a_page_at_a_time() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(
                driver,
                &base,
                &format!("/expand?fhir=r4&url={}&count=2", encoded(ALL_OF_IT)),
            )
            .await;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("1 to 2 of 5").partial(),
                    "the first page and its total",
                )
                .await;
            journey
                .press(
                    By::XPath("//main//a[normalize-space()='Next']"),
                    "the next page",
                )
                .await?;
            journey
                .address_carrying("offset=2", "the second page's address")
                .await;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("3 to 4 of 5").partial(),
                    "the second page",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A code is translated through the map the root publishes.
#[tokio::test]
async fn a_code_is_translated_through_a_published_map() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(
                driver,
                &base,
                &format!(
                    "/translate?fhir=r4&code=cb-leaf&system={}",
                    encoded(TAXONOMY)
                ),
            )
            .await;
            journey.press(By::Css(RUN), "Translate").await?;
            journey
                .text_becoming(
                    By::Css(VERDICT),
                    StringMatch::new("1 match").partial(),
                    "the answer to count the match",
                )
                .await;
            journey
                .element(
                    By::XPath("//main//td[contains(., 'wider')]"),
                    "the relationship in words",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The concept browser walks the taxonomy: a search finds a concept, its
/// tree opens the path above it, and a tree item reads another concept.
#[tokio::test]
async fn the_taxonomy_tree_is_walked() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(
                driver,
                &base,
                &format!("/browse?fhir=r5&system={}", encoded(TAXONOMY)),
            )
            .await;
            journey
                .press(By::Css("#browse-find"), "the search field")
                .await?;
            journey.type_here("branch\n").await?;
            journey
                .address_carrying("filter=branch", "the search's address")
                .await;
            journey
                .press(
                    By::XPath(
                        "//ul[contains(@class, 'fui-match-list')]//a[contains(., 'First branch')]",
                    ),
                    "the first branch among the matches",
                )
                .await?;
            journey
                .address_carrying("code=ba-branch", "the concept read")
                .await;
            journey
                .text_becoming(
                    By::Css("main .fui-concept-title"),
                    StringMatch::new("First branch").partial(),
                    "the concept pane to read it",
                )
                .await;
            journey
                .element(
                    By::Css("li[role='treeitem'][aria-selected='true']"),
                    "the concept selected in the tree",
                )
                .await;
            journey
                .press(
                    By::XPath("//li[@role='treeitem']//a[contains(., 'First leaf')]"),
                    "a child in the tree",
                )
                .await?;
            journey
                .address_carrying("code=ca-leaf", "the child read")
                .await;
            journey
                .text_becoming(
                    By::Css("main .fui-concept-title"),
                    StringMatch::new("First leaf").partial(),
                    "the concept pane to read the child",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The four roots are compared side by side on the Server screen.
#[tokio::test]
async fn the_four_roots_are_compared() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r4").await;
            journey
                .press(By::XPath(screen_link("Server")), "the server entry")
                .await?;
            journey
                .address_carrying("/server", "the server screen")
                .await;
            for root in ["R4", "R4B", "R5", "R6"] {
                journey
                    .element(
                        By::XPath(format!("//main//table//th[contains(., '{root}')]")),
                        "a column per root",
                    )
                    .await;
            }
            journey
                .press(
                    By::XPath("//main//nav//a[normalize-space()='Operations']"),
                    "the Operations tab",
                )
                .await?;
            journey
                .element(
                    By::XPath("//main//th[contains(., 'CodeSystem/$lookup')]"),
                    "the $lookup row",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The keyboard shortcuts open their legend and go to a screen, once the
/// page has hydrated; with no WebAssembly the same legend is a page.
#[tokio::test]
async fn the_shortcuts_open_their_legend_and_go_to_a_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r4").await;
            journey
                .element(By::Css(TITLE), "the dashboard's title")
                .await;
            journey.type_here("?").await?;
            journey
                .visible(By::Css("dialog[open]"), "the shortcuts legend")
                .await;
            journey.type_here("\u{e00c}").await?;
            journey
                .count_becoming(By::Css("dialog[open]"), 0, "the legend to close")
                .await;
            journey.type_here("gc").await?;
            journey
                .address_carrying("/systems", "the code systems screen")
                .await;
            journey.reopen(&format!("{base}/shortcuts")).await;
            journey
                .visible(
                    By::Css("dialog[open]"),
                    "the legend drawn open at its own address",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Configuration says "How configuration works" once, at the end of the
/// line that names the file, and that button opens its dialog with no
/// script.
#[tokio::test]
async fn the_configuration_file_line_opens_how_configuration_works() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let how = "//main//*[normalize-space()='How configuration works']\
                       [self::a or self::button][not(ancestor::dialog)]";
            let journey = Journey::open(driver, &base, "/configuration").await;
            journey
                .element(By::Css(TITLE), "the configuration's title")
                .await;
            assert_eq!(
                1,
                journey.count(By::XPath(how)).await,
                "How configuration works is offered once, on the file line"
            );
            journey
                .press(
                    By::Css("button.fui-link-button[commandfor='config-how']"),
                    "How configuration works on the file line",
                )
                .await?;
            journey
                .visible(By::Css("dialog#config-how[open]"), "the dialog it opens")
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Configuration's title row offers More, and its one item copies the path
/// of the file the console was started with, shown under the item; the item
/// shows once the menu's island runs, since only script can copy.
#[tokio::test]
async fn configuration_more_copies_the_file_path() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/configuration").await;
            journey
                .press(
                    By::Css("main summary[aria-label='More configuration actions']"),
                    "the title row's More",
                )
                .await?;
            let copy = journey
                .visible(
                    By::XPath(
                        "//button[@role='menuitem'][.//*[normalize-space()=\"Copy the file's path\"]]",
                    ),
                    "Copy the file's path",
                )
                .await;
            let path = copy.attr("data-fui-copy").await?.unwrap_or_default();
            assert!(
                path.ends_with("/console.toml"),
                "the item copies the configuration file's path: {path:?}"
            );
            let shown = copy.text().await?;
            assert!(
                shown.contains(&path),
                "the path is shown under the item: {shown:?}"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A preference is kept in its cookie and every page is drawn in it.
#[tokio::test]
async fn a_preference_is_kept_and_every_page_is_drawn_in_it() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r4").await;
            journey
                .keep_setting(&base, "density", "compact", "data-density")
                .await?;
            journey.reopen(&format!("{base}/systems")).await;
            journey
                .element(
                    By::Css("html[data-density='compact']"),
                    "the next page drawn compact",
                )
                .await;
            journey
                .keep_setting(&base, "density", "comfortable", "data-density")
                .await?;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
