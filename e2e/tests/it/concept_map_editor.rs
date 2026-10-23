// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Authoring a concept map, and what a console without a sign-in does not
//! carry.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::choose;
use crate::harness::run_canonical;
use crate::harness::server;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The code system the map translates from, which the fixture mounts.
const TAXONOMY: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// The concept map listing's way into a new concept map.
const NEW: &str = "//main//a[normalize-space()='New concept map']";

/// The canonical field.
const URL_FIELD: &str = "#cm-url";

/// The docked Save.
const SAVE: &str = "button[name='intent'][value='save']";

/// One mapping is authored and saved: a group between two systems, a source
/// code and its target.
#[tokio::test]
async fn one_mapping_is_authored_and_saved() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "maps").await;
            sign_in(&journey, &deployment).await;
            journey
                .reopen(&format!("{}/conceptmaps?fhir=r4", deployment.base))
                .await;
            journey
                .press(By::XPath(NEW), "the way to a new concept map")
                .await?;
            journey
                .element(By::Css(URL_FIELD), "the canonical field")
                .await
                .send_keys(&run_canonical("e2e-map"))
                .await?;
            journey
                .press(
                    By::Css("button[name='intent'][value='add-group']"),
                    "Add a group",
                )
                .await?;
            for side in ["source", "target"] {
                journey
                    .press(
                        By::Css(format!(
                            "select[name^='group.'][name$='.{side}'] option[value='{TAXONOMY}']"
                        )),
                        "the taxonomy offered to the group",
                    )
                    .await?;
            }
            journey
                .press(
                    By::Css("button[name='intent'][value^='add-element:']"),
                    "Add a code",
                )
                .await?;
            journey
                .element(
                    By::Css("input[name^='element.'][name$='.code']"),
                    "the source code",
                )
                .await
                .send_keys("ca-leaf")
                .await?;
            journey
                .element(
                    By::Css("input[name^='target.'][name$='.code']"),
                    "the target code",
                )
                .await
                .send_keys("cb-leaf")
                .await?;
            journey
                .press(
                    By::Css(
                        "select[name^='target.'][name$='.relationship'] option[value='relatedto']",
                    ),
                    "the relationship",
                )
                .await?;
            journey.press(By::Css(SAVE), "Save").await?;
            journey
                .text_becoming(
                    By::Css("main .fui-load-report"),
                    StringMatch::new("The server now holds version").partial(),
                    "the save to answer with the version the server holds",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console with no sign-in carries no concept map editor.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_map_authoring_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/conceptmaps").await;
            journey.element(By::Css("main h1"), "the listing").await;
            assert_eq!(
                journey.count(By::XPath(NEW)).await,
                0,
                "no new concept map is offered"
            );
            journey.reopen(&format!("{base}/conceptmap")).await;
            journey
                .text_becoming(
                    By::Css("h1"),
                    StringMatch::new("Edit a concept map").partial(),
                    "the editor to say this may not be done here",
                )
                .await;
            journey.no_console_errors_but(&["403"]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
