// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Two writers on one resource: a save over a revision another window
//! replaced is refused, and the editor offers the way back.
//!
//! Every editor states the revision it replaces in `If-Match`, and the server
//! answers a stale one with `412 Precondition Failed`
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>). The second writer is a
//! second window in the same browser, sharing the console's session.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::choose;
use crate::harness::query_param;
use crate::harness::run_canonical;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The docked Save.
const SAVE: &str = "button[name='intent'][value='save']";

/// The verdict a saved page draws.
const SAVED: &str = "main .fui-load-report";

/// A save over a replaced revision is refused, the draft stays, and the
/// editor reloads the current revision on request.
#[tokio::test]
async fn a_save_over_a_replaced_revision_is_refused_and_the_current_one_reloads() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "concurrent").await;
            sign_in(&journey, &deployment).await;
            let canonical = run_canonical("e2e-concurrent-edit");
            journey
                .reopen(&format!("{}/codesystem?fhir=r4b", deployment.base))
                .await;
            journey
                .element(By::Css("#cs-url"), "the canonical field")
                .await
                .send_keys(&canonical)
                .await?;
            journey.press(By::Css(SAVE), "Save").await?;
            journey
                .text_becoming(
                    By::Css(SAVED),
                    StringMatch::new("version 1").partial(),
                    "the first revision",
                )
                .await;
            let editor = format!(
                "{}/codesystem?fhir=r4b&system={}",
                deployment.base,
                query_param(&canonical)
            );

            let first = journey.window().await?;
            journey.reopen(&editor).await;
            journey
                .element(By::Css("#cs-title"), "the first window's editor")
                .await;

            journey.open_window().await?;
            journey.reopen(&editor).await;
            journey
                .element(By::Css("#cs-title"), "the second window's title")
                .await
                .send_keys("Written in the second window")
                .await?;
            journey
                .press(By::Css(SAVE), "Save in the second window")
                .await?;
            journey
                .text_becoming(
                    By::Css(SAVED),
                    StringMatch::new("version 2").partial(),
                    "the second revision",
                )
                .await;

            journey.switch_to(&first).await?;
            journey
                .element(By::Css("#cs-title"), "the first window's title")
                .await
                .send_keys("Written in the first window")
                .await?;
            journey
                .press(By::Css(SAVE), "Save in the first window")
                .await?;
            journey
                .text_becoming(
                    By::Css("main [role='alert']"),
                    StringMatch::new("Someone changed this resource").partial(),
                    "the save to be refused as a conflict",
                )
                .await;
            assert!(
                journey
                    .element(By::Css("#cs-title"), "the draft's title")
                    .await
                    .prop("value")
                    .await?
                    .unwrap_or_default()
                    .contains("first window"),
                "the draft stays in the form"
            );
            journey
                .press(
                    By::XPath("//main//a[normalize-space()='Reload the current version']"),
                    "the reload",
                )
                .await?;
            let title = journey
                .element(By::Css("#cs-title"), "the reloaded title")
                .await
                .prop("value")
                .await?
                .unwrap_or_default();
            assert_eq!(
                title, "Written in the second window",
                "the current revision is read back"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
