// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The revisions of a stored resource: a restore, what a reader without the
//! scope sees, and the comparison of two revisions.
//!
//! Each journey writes the code system whose revisions it reads, through the
//! editor, because the deployment holds no stored resource of its own.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::SignedIn;
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

/// A Restore control in the revisions.
const RESTORE: &str =
    "//main//tbody//button[starts-with(normalize-space(), 'Restore')][not(ancestor::dialog)]";

/// Writes a code system at `canonical` in two revisions, the second
/// retitled, and opens its revisions.
async fn two_revisions(
    journey: &Journey,
    deployment: &SignedIn,
    canonical: &str,
) -> WebDriverResult<()> {
    journey
        .reopen(&format!("{}/codesystem?fhir=r4b", deployment.base))
        .await;
    journey
        .element(By::Css("#cs-url"), "the canonical field")
        .await
        .send_keys(canonical)
        .await?;
    journey
        .element(By::Css("#cs-title"), "the title field")
        .await
        .send_keys("First title")
        .await?;
    journey.press(By::Css(SAVE), "Save").await?;
    journey
        .text_becoming(
            By::Css(SAVED),
            StringMatch::new("version 1").partial(),
            "the first revision",
        )
        .await;
    journey
        .reopen(&format!(
            "{}/codesystem?fhir=r4b&system={}",
            deployment.base,
            query_param(canonical)
        ))
        .await;
    let title = journey
        .element(By::Css("#cs-title"), "the title field")
        .await;
    title.clear().await?;
    title.send_keys("Second title").await?;
    journey.press(By::Css(SAVE), "Save").await?;
    journey
        .text_becoming(
            By::Css(SAVED),
            StringMatch::new("version 2").partial(),
            "the second revision",
        )
        .await;
    journey
        .reopen(&format!(
            "{}/systems/{}?fhir=r4b",
            deployment.base,
            query_param(canonical)
        ))
        .await;
    journey
        .press(
            By::XPath(
                "//nav[contains(@aria-label, 'Views of this')]//a[normalize-space()='History']",
            ),
            "the History tab",
        )
        .await?;
    journey.address_carrying("/history", "the revisions").await;
    Ok(())
}

/// An earlier revision is restored through its confirmation, and the page
/// says which revision it wrote.
#[tokio::test]
async fn an_earlier_revision_is_restored() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "restores").await;
            sign_in(&journey, &deployment).await;
            two_revisions(&journey, &deployment, &run_canonical("e2e-restored")).await?;
            let restores = journey.all(By::XPath(RESTORE)).await?;
            let restore = restores
                .last()
                .expect("the first revision offers a restore");
            journey
                .press_on(restore, "the first revision's restore")
                .await?;
            let confirmation = journey
                .visible(
                    By::Css("dialog[open] button[type='submit']"),
                    "the confirmation",
                )
                .await;
            journey.press_on(&confirmation, "the confirmation").await?;
            journey
                .text_becoming(
                    By::Css("main [role='status']"),
                    StringMatch::new("Revision 1 is restored as revision 3").partial(),
                    "the page to say which revision the restore wrote",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Two revisions are compared, and the comparison names what changed.
#[tokio::test]
async fn two_revisions_are_compared() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "compares").await;
            sign_in(&journey, &deployment).await;
            two_revisions(&journey, &deployment, &run_canonical("e2e-compared")).await?;
            journey
                .press(
                    By::Css("#history-from option[value='1']"),
                    "the first revision",
                )
                .await?;
            journey
                .press(
                    By::Css("#history-to option[value='2']"),
                    "the second revision",
                )
                .await?;
            journey
                .press(
                    By::XPath("//main//button[normalize-space()='Compare']"),
                    "Compare",
                )
                .await?;
            journey
                .address_carrying("from=1", "the comparison's address")
                .await;
            journey
                .element(
                    By::XPath("//main//table//code[normalize-space()='title']"),
                    "the title changed",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader whose token may not update the resource sees its revisions and
/// no restore.
#[tokio::test]
async fn a_reader_sees_the_revisions_and_no_restore_control() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "writes-for-a-reader").await;
            sign_in(&journey, &deployment).await;
            two_revisions(&journey, &deployment, &run_canonical("e2e-read-revisions")).await?;
            let revisions = journey.address().await;
            journey.drop_cookies().await?;
            choose(&journey, &deployment, "reader", "reads-revisions").await;
            sign_in(&journey, &deployment).await;
            journey.reopen(&revisions).await;
            journey
                .element(
                    By::XPath("//main//tbody//th[contains(., 'Revision 2')]"),
                    "the revisions",
                )
                .await;
            assert_eq!(
                journey.count(By::XPath(RESTORE)).await,
                0,
                "a reader is offered no restore"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
