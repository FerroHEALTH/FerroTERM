// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Journeys through the console's reading screens.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// The version switcher, found by the group name a screen reader announces.
const SWITCHER: &str = "[role='group'][aria-label='FHIR version']";

/// The switcher button marked as the version the reader is on.
const SELECTED_VERSION: &str =
    "[role='group'][aria-label='FHIR version'] button[aria-pressed='true']";

/// The sidebar the screens are reached from, by the landmark it announces.
const SIDEBAR: &str = "nav[aria-label='Screens']";

/// The sidebar entry marked as the screen the reader is on.
const CURRENT_SCREEN: &str = "nav[aria-label='Screens'] a[aria-current='page']";

/// The dashboard's line under its title, which leads with the FHIR base read.
const RENDERED_BASE: &str = ".fui-page-meta";

/// A code system row's link into that system's own screen.
const SYSTEM_LINK: &str = "a[href^='/systems/']";

/// The expansion runner's value set control, which offers what the root publishes.
const EXPAND_URL: &str = "#expand-url";

/// The expansion runner's submit control, scoped past the command bar.
const EXPAND_SUBMIT: &str = "main form button[type='submit']";

/// The disclosure holding the runs the page remembers.
const REMEMBERED: &str = "main details";

/// One remembered run, as the link that re-runs it.
const REMEMBERED_RUN: &str = "main details li a";

/// The command bar's own control, on every screen.
const COMMAND_BAR: &str = "#command-bar";

/// The command bar's submit control.
const COMMAND_SUBMIT: &str = "form[role='search'] button[type='submit']";

/// One offer the find screen made, as the link that takes it.
const OFFER: &str = "section[aria-labelledby='find-offers-heading'] li a";

/// The density control that pane carries.
const DENSITY_CONTROL: &str = "#viewer-density";

/// One drawn row of the code system listing's table.
const SYSTEM_ROW: &str = "section[aria-labelledby='systems-heading'] tbody tr";

/// How many code systems a deployment is read on one screen at.
///
/// The number is the bar issue #519 set: a real deployment serves twenty, and
/// a reader comparing them scrolls for none of them.
const SYSTEMS_ON_ONE_SCREEN: f64 = 20.0;

/// The desktop window the fit is measured in.
///
/// The battery's own window is shorter than a desktop screen, because the
/// browser draws its chrome inside it. The bar is a screen a reader has, so
/// the journey asks for one and measures the viewport it got.
const DESKTOP: (u32, u32) = (1280, 1024);

/// The sidebar entry onto the screen a code is checked on.
const CHECK_LINK: &str = "nav[aria-label='Screens'] a[href^='/validate']";

/// The control that names the code system that screen works over.
const CHECK_SYSTEM: &str = "#validate-system";

/// The first code system that control offers.
const FIRST_OFFER: &str = "#validate-system-list li[role='option']:first-child";

/// The field the code being checked is typed into.
const CHECK_CODE: &str = "#validate-code";

/// The submit of the runner on the screen, rather than the command bar's.
const CHECK_SUBMIT: &str = "main form button[type='submit']";

/// A live region on the screen, once it carries something.
const ANNOUNCEMENT: &str = "p[aria-live='polite']";

/// A code to check, which the journey never assumes is in the system it picks.
///
/// What is being proved is the path: three clicks reach an answer with no
/// canonical typed. A code the picked system does not have is answered with a
/// result of false, which is an answer the server gave.
const A_CODE: &str = "ca-leaf";

/// The code system listing's table, by the heading it is labelled by.
const SYSTEM_TABLE: &str = "section[aria-labelledby='systems-heading'] table";

/// The control that orders the table by the code system column.
const SORT_BY_SYSTEM: &str = "//section[@aria-labelledby='systems-heading']//th[1]/a";

/// The heading a screen reader hears as ordered upward.
const SORTED_UP: &str = "section[aria-labelledby='systems-heading'] th[aria-sort='ascending']";

/// What the table leads each system with, in the order it draws them.
const ROW_LABELS: &str = "section[aria-labelledby='systems-heading'] tbody a[href^='/systems/']";

/// The canonical a detail screen states in its facts.
const CANONICAL_FACT: &str = "//dt[normalize-space()='Canonical']/following-sibling::dd[1]";

/// The code system screen's tab onto what this server can do with it.
const SUPPORT_TAB: &str =
    "//nav[@aria-label='Views of this code system']//a[normalize-space()='Server support']";

/// The code system screen's capability pane, by the heading it is labelled by.
const CAPABILITY_PANE: &str = "section[aria-labelledby='system-capability-heading']";

/// The code system screen's published-resource pane.
const PUBLISHED_PANE: &str = "section[aria-labelledby='system-published-heading']";

/// The links a declared code system offers into the other screens.
const SYSTEM_TOOLS: &str = "nav[aria-label^='Screens for']";

/// A code system row whose version declares a hierarchy.
///
/// The row is chosen by what the capability statement declares rather than by
/// which system it names. A row offers the concept browser only where its
/// version declares the direct-child operator, so the browse link is the mark.
const WALKABLE_ROW: &str =
    "//tr[.//a[contains(@href, '/browse')]]//a[starts-with(@href, '/systems/')]";

/// The surface the tree journey drives.
///
/// `filter-operator` defines `child-of` from R5 onward, so an R4-family
/// surface states a hierarchy without it
/// (<https://hl7.org/fhir/R4B/codesystem-filter-operator.html>) and the
/// browser draws no tree to walk. The journey opens the version that can
/// express the filter it exercises.
const TREE_SURFACE: &str = "/systems?fhir=r5";

/// The link from a code system's screen into the concept browser.
const BROWSE_LINK: &str = "nav[aria-label^='Screens for'] a[href*='/browse']";

/// A concept the search answered, as the link that reads it.
const SEARCH_RESULT: &str = "section[aria-labelledby='browse-search-heading'] ul li a";

/// The tree once its first level has been answered: a row, or the sentence
/// saying the server answered no child.
///
/// The sentence is drawn from the answer rather than from the address, so its
/// arrival is what separates a concept with no children from a level that has
/// not been read yet.
const TREE_SETTLED: &str = "li[role='treeitem'], #browse-tree-empty";

/// One row of the tree.
const TREE_ROW: &str = "li[role='treeitem']";

/// The code a tree row draws after its words.
const TREE_CODE: &str = "code.fui-tree-code";

/// The row that holds the tree's one tab stop.
const TAB_STOP: &str = "li[role='treeitem'][tabindex='0']";

/// The row the tree announces as selected.
const SELECTED_ROW: &str = "li[role='treeitem'][aria-selected='true']";

/// A parent of the concept being read, as the link that moves onto it.
const PARENT_LINK: &str = "nav[aria-label='Parents of this concept'] a";

/// The sidebar entry onto the screen the version comparison is a tab of.
const ABOUT_LINK: &str = "nav[aria-label='Screens'] a[href^='/about']";

/// The tab that shows that comparison.
const VERSIONS_TAB: &str = "nav[aria-label='About this server'] a[data-tab='versions']";

/// The version comparison, by the heading it is labelled by.
const COMPARISON: &str = "section[aria-labelledby='comparison-heading']";

/// The two tables that comparison draws.
///
/// Scoped to the comparison, because it is a pane of the About screen now and
/// the panes beside it draw tables of their own.
const COMPARISON_TABLE: &str = "section[aria-labelledby='comparison-heading'] table";

/// The listing screens a row is opened from, the heading link of a listed
/// row, the canonical beside it, and the fact its detail screen states it in.
///
/// Both screens draw that link from `listing.rs`, which renders a row unlinked
/// when the resource it lists carries no `Resource.id`, so a link found here is
/// the wire carrying one.
const LISTINGS: [(&str, &str, &str, &str); 2] = [
    (
        "/valuesets",
        "section[aria-labelledby='valuesets-heading'] tbody th a",
        "section[aria-labelledby='valuesets-heading'] tbody th span",
        "//dt[normalize-space()='Canonical']/following-sibling::dd[1]",
    ),
    (
        "/conceptmaps",
        "section[aria-labelledby='conceptmaps-heading'] tbody th a",
        "section[aria-labelledby='conceptmaps-heading'] tbody th span",
        "//dt[normalize-space()='Canonical']/following-sibling::dd[1]",
    ),
];

/// A switcher button, by the version name a reader reads on it.
fn version_link(label: &str) -> String {
    format!("//*[@role='group'][@aria-label='FHIR version']//button[normalize-space()='{label}']")
}

/// One root's cell on the `CodeSystem/$lookup` row, counting from the left.
///
/// The columns are the roots in release order, so the first cell is R4's and
/// the third is R5's.
fn lookup_cell(column: u8) -> String {
    format!("//th[normalize-space()='CodeSystem/$lookup']/following-sibling::td[{column}]")
}

/// A sidebar entry, by the label a reader reads on it.
fn screen_link(label: &str) -> String {
    format!("//nav[@aria-label='Screens']//a[normalize-space()='{label}']")
}

/// The sidebar reaches another screen and marks the one it reached.
///
/// The mark is the `aria-current="page"` a screen reader announces, and it is
/// computed from the address rather than from the click, so a reader who typed
/// the address gets the same mark. The click also proves the screen it
/// reached reads the FHIR version the reader switched to: the switcher keeps
/// it as the version an address naming none reads.
#[tokio::test]
async fn the_sidebar_reaches_another_screen_and_marks_the_one_it_reached() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;

            journey
                .element(By::Css(SIDEBAR), "the sidebar the screens are reached from")
                .await;
            journey
                .text_becoming(
                    By::Css(CURRENT_SCREEN),
                    "Dashboard",
                    "the sidebar to mark the dashboard the reader opened",
                )
                .await;
            journey
                .element(By::XPath(version_link("R5")), "the R5 button")
                .await
                .click()
                .await?;
            journey
                .text_becoming(
                    By::Css(SELECTED_VERSION),
                    "R5",
                    "the switcher to mark R5 after the press",
                )
                .await;

            journey
                .element(By::XPath(screen_link("Value sets")), "the value sets entry")
                .await
                .click()
                .await?;

            journey
                .text_becoming(
                    By::Css(CURRENT_SCREEN),
                    "Value sets",
                    "the sidebar to mark value sets after the click",
                )
                .await;
            journey
                .address_carrying("/valuesets", "the value sets screen's own address")
                .await;
            journey
                .text_becoming(
                    By::Css(SELECTED_VERSION),
                    "R5",
                    "the value sets screen to read the version the reader switched to",
                )
                .await;
            assert_eq!(
                journey.count(By::Css(CURRENT_SCREEN)).await,
                1,
                "one entry is the screen the reader is on"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The shell renders, and the switcher moves the whole page onto another FHIR
/// version.
///
/// The rendered FHIR base below the switcher is read from the same signal the
/// address carries, so the press is proved by what the page reads after it.
#[tokio::test]
async fn the_shell_renders_and_the_switcher_moves_the_page_onto_another_version() {
    let Some(base) = server() else {
        return;
    };
    // The session is quit whether the body returns, errors, or panics; a
    // session left behind holds the browser's only slot, and the next journey
    // then queues behind it until the grid reclaims it.
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;

            journey
                .element(By::Css(SWITCHER), "the FHIR version switcher")
                .await;
            journey
                .text_becoming(
                    By::Css(SELECTED_VERSION),
                    "R4B",
                    "the switcher to mark R4B, the default version, as current",
                )
                .await;
            let base_before = journey
                .text_becoming(
                    By::Css(RENDERED_BASE),
                    StringMatch::new("/r4b ").partial(),
                    "the overview to render the R4B FHIR base",
                )
                .await;
            let base_before = base_before
                .split(" · ")
                .next()
                .unwrap_or_default()
                .to_owned();
            let address_before = journey.address().await;
            assert!(
                base_before.ends_with("/r4b"),
                "the rendered FHIR base is the R4B root, not `{base_before}`"
            );
            assert!(
                !address_before.contains("fhir=r5"),
                "the journey starts off R5, and the address says `{address_before}`"
            );

            journey
                .element(By::XPath(version_link("R5")), "the R5 button")
                .await
                .click()
                .await?;

            journey
                .text_becoming(
                    By::Css(SELECTED_VERSION),
                    "R5",
                    "the switcher to mark R5 as current after the click",
                )
                .await;
            let base_after = journey
                .text_becoming(
                    By::Css(RENDERED_BASE),
                    StringMatch::new("/r5 ").partial(),
                    "the overview to render the R5 FHIR base",
                )
                .await;
            let base_after = base_after
                .split(" · ")
                .next()
                .unwrap_or_default()
                .to_owned();
            let address_after = journey.address().await;
            assert!(
                base_after.ends_with("/r5"),
                "the rendered FHIR base moved to the R5 root, not to `{base_after}`"
            );
            assert_ne!(
                base_before, base_after,
                "switching version re-renders the page, so the FHIR base is not the one it was"
            );
            assert!(
                address_after.contains("fhir=r5"),
                "the switcher returns the reader with the version in the address: `{address_after}`"
            );
            assert_ne!(
                address_before, address_after,
                "a switched version is shareable, so the address changed with the page"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A row on the overview opens that code system's own screen, and both panes
/// draw from the canonical the route carried.
///
/// This is the journey that proves the percent-encoded canonical survives the
/// route: the detail screen finds the system in the capability statement only
/// if the segment it was linked with decoded back to the canonical the row
/// named. The links row exists only for a system the capabilities declare.
#[tokio::test]
async fn a_row_opens_the_code_system_screen_and_both_panes_draw() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;

            let row = journey
                .element(By::Css(SYSTEM_LINK), "a code system link on the overview")
                .await;
            let canonical = row_canonical(&row).await?;
            assert!(
                !canonical.is_empty(),
                "the row names the canonical it links to"
            );
            row.click().await?;

            journey
                .text_becoming(
                    By::XPath(CANONICAL_FACT),
                    StringMatch::new(canonical.clone()).full(),
                    "the detail screen to state the canonical the row named",
                )
                .await;
            journey
                .element(By::Css(PUBLISHED_PANE), "the published-resource pane")
                .await;
            journey
                .element(By::XPath(SUPPORT_TAB), "the server support tab")
                .await
                .click()
                .await?;
            journey
                .element(By::Css(CAPABILITY_PANE), "the capability pane")
                .await;
            // The links row is drawn only for a system the capability
            // statement names, so finding it proves the canonical round-tripped
            // through the percent-encoded route segment.
            journey
                .element(By::Css(SYSTEM_TOOLS), "the links into the other screens")
                .await;

            let address = journey.address().await;
            assert!(
                address.contains("/systems/"),
                "the screen is addressed under the systems route: `{address}`"
            );
            assert!(
                address.contains("fhir=r4b"),
                "the link carries the version the reader was on: `{address}`"
            );
            assert!(
                !address.contains("/systems//"),
                "the canonical is one encoded segment, so it never splits: `{address}`"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The address a row links to opens the same screen when it is loaded fresh.
///
/// A click and a typed address reach the router by different routes: the click
/// handler pushes the path after one `decodeURI` pass, while a fresh load is
/// read straight off `window.location`. The canonical is a percent-encoded
/// segment on both, so the shareable address has to be driven, not modelled.
#[tokio::test]
async fn the_address_a_row_links_to_opens_the_same_screen_when_it_is_loaded_fresh() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;

            let row = journey
                .element(By::Css(SYSTEM_LINK), "a code system link on the overview")
                .await;
            let canonical = row_canonical(&row).await?;
            let address = row
                .prop("href")
                .await?
                .expect("an anchor resolves its own href");
            assert!(
                address.contains("/systems/"),
                "the row links into the systems route: `{address}`"
            );

            // The server has never served this path, so this also drives the
            // single-page fallback that makes a deep link work at all.
            journey.reopen(&address).await;

            journey
                .text_becoming(
                    By::XPath(CANONICAL_FACT),
                    StringMatch::new(canonical.clone()).full(),
                    "the freshly loaded screen to state the same canonical",
                )
                .await;
            journey
                .element(By::Css(SYSTEM_TOOLS), "the links into the other screens")
                .await;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A run is remembered in the page and re-run from the list.
///
/// A run is already a URL, because every runner puts its parameters in the
/// address, so the list holds links and nothing else. That is what makes a
/// remembered run safe: it is re-run when a reader returns to it, and can
/// never show a stale answer beside a live one. Nothing is sent anywhere and
/// nothing is stored in the browser.
#[tokio::test]
async fn a_run_is_remembered_in_the_page_and_re_run_from_the_list() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/expand?fhir=r5").await;
            journey
                .element(By::Css(EXPAND_URL), "the runner's value set control")
                .await
                .click()
                .await?;
            journey
                .visible(
                    By::Css("#expand-url-list li[role='option']:first-child"),
                    "the first value set the list offers",
                )
                .await
                .click()
                .await?;
            journey
                .element(By::Css(EXPAND_SUBMIT), "the runner's submit control")
                .await
                .click()
                .await?;
            let ran = journey
                .address_carrying("url=", "the run the reader made")
                .await;

            let held = journey
                .element(By::Css(REMEMBERED), "the runs the page remembers")
                .await;
            held.click().await?;
            let remembered = journey
                .element(By::Css(REMEMBERED_RUN), "the run the list holds")
                .await;
            let href = remembered.attr("href").await?.unwrap_or_default();
            assert!(
                href.contains("url="),
                "the list holds the address that made the run: `{href}`"
            );

            // Somewhere else and back by the sidebar, then through the list:
            // the list lives in the page, so it outlasts a move between
            // screens and is re-run from a runner holding no run.
            journey
                .element(By::XPath(screen_link("Check a code")), "the check entry")
                .await
                .click()
                .await?;
            journey
                .address_carrying("/validate", "the check screen")
                .await;
            journey
                .element(
                    By::XPath(screen_link("Expand a value set")),
                    "the expansion entry",
                )
                .await
                .click()
                .await?;
            journey
                .element(By::Css(REMEMBERED), "the runs the page remembers")
                .await
                .click()
                .await?;
            journey
                .element(By::Css(REMEMBERED_RUN), "the run the list still holds")
                .await
                .click()
                .await?;
            let returned = journey
                .address_carrying("url=", "the run the reader returned to")
                .await;
            assert_eq!(
                returned, ran,
                "the list re-runs the address it remembered, exactly"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The command bar reads what a reader typed and offers addresses for it.
///
/// The reading is the console's own, of the string's shape, before any request:
/// a canonical, a code, or a phrase. What gates the offers is the one request
/// the screen makes, the root's own `CapabilityStatement`, so an operation the
/// root does not declare is never offered. Every offer is a real link, which
/// is what makes a run something a reader can send on.
#[tokio::test]
async fn the_command_bar_reads_what_was_typed_and_offers_addresses_for_it() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r5").await;
            let row = journey
                .element(By::Css(SYSTEM_LINK), "a code system on the overview")
                .await;
            let system = row_canonical(&row).await?;

            journey
                .element(By::Css(COMMAND_BAR), "the command bar")
                .await
                .send_keys(&system)
                .await?;
            journey
                .element(By::Css(COMMAND_SUBMIT), "the command bar's submit")
                .await
                .click()
                .await?;
            journey
                .address_carrying("/find", "the screen the command bar opens")
                .await;
            journey
                .text_becoming(
                    By::Css(".fui-page-subtitle"),
                    StringMatch::new("a canonical".to_owned()).partial(),
                    "the console to say it read a canonical",
                )
                .await;

            let offer = journey
                .element(By::Css(OFFER), "an offer the screen made")
                .await;
            let href = offer.attr("href").await?.unwrap_or_default();
            assert!(
                href.starts_with('/'),
                "an offer is an address inside the console: `{href}`"
            );
            offer.click().await?;
            journey
                .element(By::Css("h1"), "the screen the offer opened")
                .await;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The overview's table is ordered from its own headings, and the order is a
/// link.
///
/// The sort is a navigation rather than a private signal, so the ordered table
/// is a URL a reader can send. `aria-sort` on the header cell is what a screen
/// reader announces (<https://www.w3.org/WAI/ARIA/apg/patterns/table/>), and
/// the control inside it is a link, so the column is reordered from the
/// keyboard with no key handler of ours.
#[tokio::test]
async fn the_overview_table_orders_itself_and_the_order_is_a_link() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/systems?fhir=r5").await;
            journey
                .element(By::Css(SYSTEM_TABLE), "the table of served systems")
                .await;
            let declared = labels(&journey).await?;
            assert!(
                declared.len() > 1,
                "the fixture serves more than one system, so an order is visible"
            );

            journey
                .element(By::XPath(SORT_BY_SYSTEM), "the code system heading control")
                .await
                .click()
                .await?;
            let address = journey
                .address_carrying("sort=system", "the order the reader asked for")
                .await;
            assert!(
                !address.contains("dir=desc"),
                "a first click orders the column upward: `{address}`"
            );
            journey
                .element(By::Css(SORTED_UP), "the heading announced as ascending")
                .await;
            let mut upward = declared.clone();
            upward.sort_by_key(|label| label.to_lowercase());
            assert_eq!(
                labels(&journey).await?,
                upward,
                "the table draws the systems in the order the address asked for"
            );

            journey
                .element(By::XPath(SORT_BY_SYSTEM), "the code system heading control")
                .await
                .click()
                .await?;
            journey
                .address_carrying("dir=desc", "the second click turning the column around")
                .await;
            let mut downward = upward.clone();
            downward.reverse();
            assert_eq!(
                labels(&journey).await?,
                downward,
                "a second click on one column turns it around"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Twenty code systems are read on one screen in compact mode.
///
/// The fixture serves a handful of systems and a real deployment serves
/// twenty, so the journey measures what the screen costs rather than counting
/// what this deployment happens to hold: where the first row starts, and what
/// the tallest row drawn takes. Twenty rows of that height have to end above
/// under the foot of the viewport.
#[tokio::test]
async fn the_overview_reads_twenty_systems_on_one_screen_in_compact_mode() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/about?fhir=r5").await;
            journey.resize(DESKTOP.0, DESKTOP.1).await;
            journey
                .keep_setting(&base, DENSITY_CONTROL, "compact", Some("data-density"))
                .await?;

            journey.reopen(&format!("{base}/systems?fhir=r5")).await;
            journey
                .element(By::Css(SYSTEM_TABLE), "the table of served systems")
                .await;
            let rows = journey.all(By::Css(SYSTEM_ROW)).await?;
            assert!(
                !rows.is_empty(),
                "the fixture serves a code system, so the table has a row to measure"
            );
            let mut top = f64::MAX;
            let mut tallest: f64 = 0.0;
            for row in &rows {
                let rect = row.rect().await?;
                top = top.min(rect.y);
                tallest = tallest.max(rect.height);
            }
            let viewport = journey.viewport_height().await?;
            let taken = top + SYSTEMS_ON_ONE_SCREEN * tallest;
            assert!(
                taken <= viewport,
                "twenty systems take {taken:.0}px of a {viewport:.0}px viewport at a row of \
                 {tallest:.0}px under a header of {top:.0}px, so a reader comparing twenty \
                 scrolls for some of them"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A code is checked in three clicks, with no canonical typed.
///
/// The runners take a canonical, and almost no reader knows one by heart, so
/// each offers what this root publishes. The journey is the whole path a
/// reader who has never seen the console takes: the sidebar entry, the code
/// system, the code. Nothing here names a code system: the offer it picks is
/// whatever the root published first, so a deployment this server has never
/// served drives the same way.
#[tokio::test]
async fn a_code_is_checked_in_three_clicks_with_no_canonical_typed() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/?fhir=r5").await;

            journey
                .element(By::Css(CHECK_LINK), "the sidebar entry onto the check")
                .await
                .click()
                .await?;

            journey
                .element(By::Css(CHECK_SYSTEM), "the code system control")
                .await
                .click()
                .await?;
            let offer = journey
                .visible(By::Css(FIRST_OFFER), "the first code system offered")
                .await;
            let picked = offer
                .find(By::Css(".fui-combobox-label"))
                .await?
                .text()
                .await?;
            assert!(
                !picked.is_empty(),
                "an offer a reader cannot read is an offer they cannot pick"
            );
            let canonical = offer
                .find(By::Css(".fui-combobox-detail"))
                .await?
                .text()
                .await?;
            assert!(
                !canonical.is_empty(),
                "the offer carries the canonical the run sends"
            );
            offer.click().await?;

            journey
                .element(By::Css(CHECK_CODE), "the code field")
                .await
                .send_keys(A_CODE)
                .await?;
            journey
                .element(By::Css(CHECK_SUBMIT), "the runner's submit")
                .await
                .click()
                .await?;

            let address = journey
                .address_carrying("system=", "the picked system in the address")
                .await;
            assert!(
                address.contains("code=") && address.contains("/validate"),
                "a run is a link a reader can send: `{address}`"
            );
            journey
                .live_region_saying(
                    By::Css(ANNOUNCEMENT),
                    "result",
                    "the validation to announce what the server answered",
                )
                .await;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// What the table leads each system with, in the order it draws them.
///
/// This is the text the column is ordered by: the name the code system was
/// published under, or the canonical where the server published no name.
async fn labels(journey: &Journey) -> WebDriverResult<Vec<String>> {
    let mut found = Vec::new();
    for element in journey.all(By::Css(ROW_LABELS)).await? {
        found.push(element.text().await?);
    }
    Ok(found)
}

/// The canonical an overview row carries, wherever the row put it.
///
/// A row leads with the name its code system was published under and sets the
/// canonical beside it, so the canonical is the link's own sibling. A system
/// the server published no name for leads with the canonical itself.
async fn row_canonical(row: &WebElement) -> WebDriverResult<String> {
    match row.find(By::XPath("following-sibling::span[1]")).await {
        Ok(beside) => beside.text().await,
        Err(_absent) => row.text().await,
    }
}

/// Walks from the overview to a browse screen whose tree has a level drawn.
///
/// The row is chosen by the operator its version declares, so the walk knows
/// no code system. Where the concept the search answers first turns out to be
/// a leaf, the walk moves onto one of its parents, because a tree with no row
/// has no tab stop to press a key on.
async fn tree_with_a_level(journey: &Journey) -> WebDriverResult<()> {
    journey
        .element(
            By::XPath(WALKABLE_ROW),
            "a code system whose version declares the direct-child operator",
        )
        .await
        .click()
        .await?;
    journey
        .element(By::Css(BROWSE_LINK), "the link into the concept browser")
        .await
        .click()
        .await?;

    // An empty filter lists the first concepts the server answers, so the
    // search gives the tree an anchor with nothing typed into it.
    journey
        .element(By::Css(SEARCH_RESULT), "a concept the search answered")
        .await
        .click()
        .await?;
    journey
        .address_carrying("code=", "the address to carry the concept that was read")
        .await;

    // Waiting on the answered tree rather than on the busy indicator: the
    // indicator is gone the moment the empty walk before this concept
    // resolved, so a count taken then would read a level still in flight.
    journey
        .element(By::Css(TREE_SETTLED), "the first level of the tree")
        .await;
    if journey.count(By::Css(TREE_ROW)).await == 0 {
        journey
            .element(By::Css(PARENT_LINK), "a parent of this leaf concept")
            .await
            .click()
            .await?;
        journey
            .element(By::Css(TREE_ROW), "a tree row below the parent")
            .await;
    }
    Ok(())
}

/// The taxonomy tree opens a node and selects a concept from the keyboard
/// alone, and the address carries both moves.
///
/// This is the journey that proves the ARIA tree view pattern is implemented
/// rather than described: the tree keeps one tab stop, the right arrow opens
/// the node under it, `Enter` selects the concept, and both moves are
/// navigations, so a walk a reader made is a link they can share. Nothing here
/// names a code system. The row is picked by the operator its version
/// declares, which is the same fact the screen draws the tree from.
#[tokio::test]
async fn the_taxonomy_tree_is_walked_by_keyboard_alone() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, TREE_SURFACE).await;
            tree_with_a_level(&journey).await?;

            let stop = journey
                .element(By::Css(TAB_STOP), "the tree's one tab stop")
                .await;
            assert_eq!(
                journey.count(By::Css(TAB_STOP)).await,
                1,
                "a tree is one tab stop, however many rows it draws \
                 (https://www.w3.org/WAI/ARIA/apg/patterns/treeview/)"
            );
            let row_id = stop
                .attr("id")
                .await?
                .expect("every tree row carries the id its focus is moved by");
            let row_code = stop.find(By::Css(TREE_CODE)).await?.text().await?;
            assert!(
                !row_code.is_empty(),
                "the row names the code it draws, and the journey selects by it"
            );

            let closed_rows = journey.count(By::Css(TREE_ROW)).await;
            stop.send_keys(Key::Right).await?;
            let opened = journey
                .address_carrying(
                    "open=",
                    "the right arrow to open the node under the tab stop",
                )
                .await;
            assert!(
                opened.contains("code="),
                "opening a node leaves the concept being read alone: `{opened}`"
            );

            // The navigation redrew the tree once the opened level answered, so
            // that level is waited for and the tab stop read again.
            journey
                .element(
                    By::Css("li[role='treeitem'] li[role='treeitem']"),
                    "the level the right arrow opened",
                )
                .await;
            journey
                .element(By::Css(TAB_STOP), "the tab stop after the node opened")
                .await
                .send_keys(Key::Enter)
                .await?;
            let selected = journey
                .address_carrying(
                    &format!("code={row_code}"),
                    "Enter to select the concept the tab stop was on",
                )
                .await;
            assert!(
                selected.contains("open="),
                "selecting a concept leaves the opened node open: `{selected}`"
            );

            let announced = journey
                .element(
                    By::Css(SELECTED_ROW),
                    "the row the tree announces as selected",
                )
                .await;
            assert_eq!(
                announced.attr("id").await?.as_deref(),
                Some(row_id.as_str()),
                "the row Enter selected is the row that held the tab stop"
            );
            assert_eq!(
                journey.count(By::Css(SELECTED_ROW)).await,
                1,
                "a tree announces one selected row, so a reader is never told of two"
            );

            // The left arrow closes the node the right arrow opened, which is
            // the other half of the pattern's walk. The tree draws the levels
            // an address names, so the rows going back to what they were is
            // the node having closed.
            assert!(
                journey.count(By::Css(TREE_ROW)).await > closed_rows,
                "the opened node drew a level, or there is nothing to close again"
            );
            closed_again(&journey, closed_rows).await?;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Presses the left arrow on the tab stop and waits for the tree to draw
/// `closed_rows` rows again, with one tab stop.
async fn closed_again(journey: &Journey, closed_rows: usize) -> WebDriverResult<()> {
    journey
        .element(By::Css(TAB_STOP), "the tab stop before the node is closed")
        .await
        .send_keys(Key::Left)
        .await?;
    journey
        .count_becoming(
            By::Css(TREE_ROW),
            closed_rows,
            "the left arrow to close the level the right arrow opened",
        )
        .await;
    assert_eq!(
        journey.count(By::Css(TAB_STOP)).await,
        1,
        "closing a node leaves the tree with one tab stop"
    );
    Ok(())
}

/// The version comparison reads all four roots and shows where they differ.
///
/// The row this asserts on is the one the screen exists for: R4 and R4B
/// declare `$lookup` at the type level and R5 added the instance level
/// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>). Every cell is
/// read from the capability statement the browser fetched from that root, so a
/// screen that rendered a remembered answer, or that blanked while one root
/// was still answering, fails here.
#[tokio::test]
async fn the_four_roots_are_compared_and_their_lookup_levels_differ() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;

            journey
                .element(
                    By::Css(ABOUT_LINK),
                    "the sidebar entry onto the screen the comparison is a pane of",
                )
                .await
                .click()
                .await?;
            // A reader coming for the comparison opens its tab, which is the
            // one the screen opens on and a link they can send.
            journey
                .element(By::Css(VERSIONS_TAB), "the tab the comparison is on")
                .await
                .click()
                .await?;

            journey
                .element(By::Css(COMPARISON), "the comparison the four reads fill")
                .await;
            let r4 = journey
                .text_becoming(
                    By::XPath(lookup_cell(1)),
                    "type",
                    "R4 to state the levels it answers $lookup at",
                )
                .await;
            let r5 = journey
                .text_becoming(
                    By::XPath(lookup_cell(3)),
                    StringMatch::new("instance").partial(),
                    "R5 to state the instance level R4 does not declare",
                )
                .await;
            assert_ne!(
                r4, r5,
                "the screen exists to show that two roots answer one operation differently"
            );
            assert_eq!(
                journey.count(By::Css(COMPARISON_TABLE)).await,
                2,
                "both comparison tables drew, so nothing blanked while the four reads settled"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A listed value set and a listed concept map each open the resource they
/// name.
///
/// The row heads with a link only where the searchset entry carried a
/// `Resource.id`, the logical id the read interaction addresses
/// (<https://hl7.org/fhir/R4B/resource.html#id>). A row without one renders as
/// plain text, so a click here would find nothing to click. Opening it proves
/// the id round-tripped: the detail screen reads the resource back by the id
/// its path carries, and states the canonical the row carried beside its name.
#[tokio::test]
async fn a_listed_row_opens_the_resource_it_names() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            for (path, row, subject, heading) in LISTINGS {
                journey.reopen(&format!("{base}{path}")).await;
                let link = journey
                    .element(By::Css(row), "a listed row that links to its detail")
                    .await;
                let title = link.text().await?;
                assert!(
                    !title.is_empty(),
                    "the row leads with the name of the resource, on {path}"
                );
                // The row leads with the title and carries the canonical
                // beside it, so the canonical is read from the row rather than
                // from the link that opens it.
                let canonical = journey
                    .element(By::Css(subject), "the canonical the row carries")
                    .await
                    .text()
                    .await?;
                link.click().await?;

                let address = journey
                    .address_carrying(
                        &format!("{path}/"),
                        "the address to name the row that was opened",
                    )
                    .await;
                assert!(
                    address.contains("fhir="),
                    "opening a row keeps the version the reader was on: `{address}`"
                );
                journey
                    .text_becoming(
                        By::XPath(heading),
                        StringMatch::new(canonical.clone()).partial(),
                        "the detail screen to state the canonical the row named",
                    )
                    .await;
            }

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
