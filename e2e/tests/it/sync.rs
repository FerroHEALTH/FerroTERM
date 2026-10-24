// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The Synchronisation screens, over the stub sync listener the harness
//! starts beside the signed-in console (`e2e/src/bin/stub_sync.rs`).
//!
//! The listener refuses a request without a bearer credential, so a run list
//! that draws proves the console sent the reader's token; the issuer grants
//! `__ferroterm.sync` to the writer profile and to nobody else. The runs are
//! invented and name the synthetic fixture.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::choose;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The navigation entry of the screens.
const NAV_ENTRY: &str = "nav a[href='/sync']";

/// The run the stub listener holds that found one local code changed.
const TAKEN_RUN: &str = "20261021T030000Z";

/// The value set of the synthetic fixture the finding names.
const FIXTURE_VALUE_SET: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";

/// A reader whose token carries the listener's scope reaches the runs from
/// the navigation, opens one, and reads its finding, linked to the value set
/// it names.
#[tokio::test]
async fn the_runs_and_a_finding_are_read() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "sync-runs").await;
            sign_in(&journey, &deployment).await;
            journey
                .press(By::Css(NAV_ENTRY), "the Synchronisation entry")
                .await?;
            journey
                .text_becoming(
                    By::Css("main h1"),
                    StringMatch::new("Synchronisation"),
                    "the runs to open",
                )
                .await;
            journey
                .text_becoming(
                    By::Css("main tbody"),
                    StringMatch::new("1 finding").partial(),
                    "the run list to name the run that found a changed code",
                )
                .await;
            journey
                .press(
                    By::Css(format!("main tbody a[href='/sync/{TAKEN_RUN}']")),
                    "the run that took a release",
                )
                .await?;
            journey
                .text_becoming(
                    By::Css("main dl"),
                    StringMatch::new("Synthetic taxonomy 2").partial(),
                    "the summary to name the release the run took",
                )
                .await;
            journey
                .press(
                    By::Css(format!("a[href='/sync/{TAKEN_RUN}/findings']")),
                    "the Findings tab",
                )
                .await?;
            let row = journey
                .text_becoming(
                    By::Css("main tbody tr"),
                    StringMatch::new("The release made this code inactive").partial(),
                    "the finding to say what the release did",
                )
                .await;
            assert!(row.contains("ca-leaf"), "the finding names its code: {row}");
            assert!(
                row.contains(FIXTURE_VALUE_SET),
                "the finding names its value set: {row}"
            );
            journey
                .press(
                    By::Css("main tbody a[href^='/valuesets/e2e-taxonomy-all']"),
                    "the value set the finding names",
                )
                .await?;
            journey
                .address_carrying("/valuesets/e2e-taxonomy-all", "the value set screen")
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Run now posts to the listener behind its confirmation, and the list names
/// the run that finished.
#[tokio::test]
async fn run_now_answers_with_the_finished_run() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "sync-run-now").await;
            sign_in(&journey, &deployment).await;
            journey.reopen(&format!("{}/sync", deployment.base)).await;
            journey
                .press(
                    By::XPath(
                        "//main//*[normalize-space()='Run now…' or normalize-space()='Run now']",
                    ),
                    "Run now",
                )
                .await?;
            let confirm = journey
                .visible(
                    By::Css("dialog[open] button[type='submit']"),
                    "the Run now confirmation",
                )
                .await;
            journey.press_on(&confirm, "the confirmation").await?;
            journey
                .text_becoming(
                    By::Css("main [role='status']"),
                    StringMatch::new("finished and succeeded").partial(),
                    "the list to name the run that finished",
                )
                .await;
            journey
                .text_becoming(
                    By::Css("main tbody tr"),
                    StringMatch::new("Run now").partial(),
                    "the finished run to lead the list",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader whose token lacks the listener's scope is offered no
/// Synchronisation entry, and its address is not found.
#[tokio::test]
async fn a_reader_without_the_scope_sees_no_synchronisation() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "reader", "sync-hidden").await;
            sign_in(&journey, &deployment).await;
            assert_eq!(
                journey.count(By::Css(NAV_ENTRY)).await,
                0,
                "the navigation offers no Synchronisation entry"
            );
            journey.reopen(&format!("{}/sync", deployment.base)).await;
            journey
                .text_becoming(
                    By::Css("main h1"),
                    StringMatch::new("No such screen"),
                    "the address to be not found",
                )
                .await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
