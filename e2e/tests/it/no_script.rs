// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Every screen with FerroUI's script modules refused: its content is all
//! there and its links and forms still lead where they lead.
//!
//! The browser is refused every module of the library, the loader among
//! them (`/ferrohealth-ui/<version>/scripts/`), as on a page whose modules
//! have not loaded or will not. The console's floor is what this holds:
//! complete server-drawn pages, and a link or a form behind every important
//! action (no specification governs this: our own design). A refused module
//! is the one severe entry a page may log here.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::accessibility::{SCREENS, VERSION};
use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// What the browser logs for a module it was refused, which these journeys
/// provoke on purpose.
const REFUSED: [&str; 1] = ["/scripts/"];

/// The canonical of the shaped taxonomy every journey reads.
const TAXONOMY: &str = "https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy";

/// Every screen arrives whole: its shell, its heading and its content.
#[tokio::test]
async fn every_screen_is_whole_with_no_script() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open_without_scripts(driver, &base, "/").await;
            for screen in SCREENS {
                let separator = if screen.contains('?') { '&' } else { '?' };
                journey
                    .reopen(&format!("{base}{screen}{separator}fhir={VERSION}"))
                    .await;
                journey
                    .element(By::Css("header a.fui-lockup"), "the shell's lockup")
                    .await;
                let heading = journey
                    .element(By::Css("main h1"), &format!("the heading of {screen}"))
                    .await
                    .text()
                    .await?;
                assert!(!heading.trim().is_empty(), "{screen} names itself");
                assert_eq!(
                    0,
                    journey.count(By::Css("[aria-busy='true']")).await,
                    "{screen} waits on nothing"
                );
            }
            journey.no_console_errors_but(&REFUSED).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The navigation's links lead to their screens and an operation's form
/// runs, both with no script.
#[tokio::test]
async fn a_link_and_a_run_form_work_with_no_script() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open_without_scripts(driver, &base, "/?fhir=r5").await;
            journey
                .press(
                    By::XPath("//nav[@aria-label='Console']//a[normalize-space()='Operations']"),
                    "the operations entry",
                )
                .await?;
            journey.address_carrying("/lookup", "the Look up tab").await;
            journey
                .press(By::Css("#lookup-code"), "the code field")
                .await?;
            journey.type_here("ca-leaf").await?;
            journey
                .press(
                    By::Css("main .fui-transform-run button[type='submit']"),
                    "Look up",
                )
                .await?;
            journey
                .address_carrying("code=ca-leaf", "the run's address")
                .await;
            journey
                .text_becoming(
                    By::Css("main .fui-load-report"),
                    StringMatch::new("First leaf").partial(),
                    "the answer to name the concept found",
                )
                .await;
            journey.no_console_errors_but(&REFUSED).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A closed level of the concept tree opens through its toggle's link, the
/// page drawn again with the level read.
#[tokio::test]
async fn a_tree_level_opens_through_its_link_with_no_script() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open_without_scripts(
                driver,
                &base,
                &format!("/browse?fhir=r5&system={TAXONOMY}&code=aa-root"),
            )
            .await;
            let branch = "li[role='treeitem'][data-key='aa-root/ba-branch']";
            journey
                .element(
                    By::Css(format!("{branch}[aria-expanded='false']")),
                    "the first branch, closed",
                )
                .await;
            journey
                .press(
                    By::Css(format!("{branch} > .fui-tree-row > a.fui-tree-toggle")),
                    "the first branch's toggle",
                )
                .await?;
            journey
                .address_carrying("open=ba-branch", "the opened level's address")
                .await;
            journey
                .element(
                    By::Css(format!("{branch}[aria-expanded='true']")),
                    "the first branch, open",
                )
                .await;
            journey
                .visible(
                    By::XPath("//li[@role='treeitem']//a[contains(., 'First leaf')]"),
                    "a child of the opened level",
                )
                .await;
            journey.no_console_errors_but(&REFUSED).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A preference is kept by its form's Save, which shows when no module hides
/// it, and the next page is drawn in it.
#[tokio::test]
async fn a_preference_is_kept_by_its_form_with_no_script() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open_without_scripts(driver, &base, "/preferences").await;
            let form = "//form[.//input[@name='density'][@value='compact']]";
            journey
                .press(
                    By::XPath(format!(
                        "{form}//label[.//input[@name='density'][@value='compact']]"
                    )),
                    "the compact choice",
                )
                .await?;
            journey
                .press(
                    By::XPath(format!("{form}//button[@type='submit']")),
                    "the density's Save",
                )
                .await?;
            journey
                .element(
                    By::Css("html[data-density='compact']"),
                    "the page drawn compact",
                )
                .await;
            journey.reopen(&format!("{base}/systems")).await;
            journey
                .element(
                    By::Css("html[data-density='compact']"),
                    "the next page drawn compact",
                )
                .await;
            journey.no_console_errors_but(&REFUSED).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
