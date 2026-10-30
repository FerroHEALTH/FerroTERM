// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The concept tree's folds, with FerroUI's modules loaded: a level the
//! server drew closes and opens in place, loading no document, and the
//! `tree` module keeps the folds over a reload for the session.

use thirtyfour::prelude::*;

use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// The concept browser on the first branch of the shaped taxonomy, whose
/// path the server draws open.
const BRANCH_PAGE: &str = "/browse?fhir=r5&system=https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy&code=ba-branch";

/// The first branch's item, by its path key.
const BRANCH: &str = "li[role='treeitem'][data-key='aa-root/ba-branch']";

/// Sets a marker on the window, which survives only while no new document
/// loads.
const MARK: &str = "window.__journeyKept = true; return 'set';";

/// Whether the marker [`MARK`] set is still there.
const MARKED: &str = "return String(window.__journeyKept === true);";

/// Clicks the toggle of the first branch, in place.
async fn toggle(journey: &Journey) -> WebDriverResult<()> {
    let toggle = journey
        .element(
            By::Css(format!("{BRANCH} > .fui-tree-row > .fui-tree-toggle")),
            "the first branch's toggle",
        )
        .await;
    toggle.click().await
}

/// A level the server drew folds in place, its fold survives a reload, and
/// it opens again in place.
#[tokio::test]
async fn a_tree_level_folds_in_place_and_its_fold_survives_a_reload() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, BRANCH_PAGE).await;
            journey
                .element(
                    By::Css(format!("{BRANCH}[aria-expanded='true']")),
                    "the branch the server drew open",
                )
                .await;
            journey.evaluate(MARK).await?;
            toggle(&journey).await?;
            journey
                .count_becoming(
                    By::Css(format!("{BRANCH}[aria-expanded='false']")),
                    1,
                    "the branch to close in place",
                )
                .await;
            assert_eq!(
                "true",
                journey.evaluate(MARKED).await?,
                "the fold loaded no document"
            );
            let address = journey.address().await;
            journey.reopen(&address).await;
            journey
                .count_becoming(
                    By::Css(format!("{BRANCH}[aria-expanded='false']")),
                    1,
                    "the reload to keep the branch closed",
                )
                .await;
            journey.evaluate(MARK).await?;
            toggle(&journey).await?;
            journey
                .count_becoming(
                    By::Css(format!("{BRANCH}[aria-expanded='true']")),
                    1,
                    "the branch to open again in place",
                )
                .await;
            journey
                .visible(
                    By::Css(format!("{BRANCH} > ul[role='group']")),
                    "its children",
                )
                .await;
            assert_eq!(
                "true",
                journey.evaluate(MARKED).await?,
                "the open loaded no document"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
