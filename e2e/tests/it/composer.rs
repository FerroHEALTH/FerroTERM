// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Composing a value set: its clauses, the preview of what they select, and
//! the save, and what a reader without the scope or a console without a
//! sign-in gets.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::SignedIn;
use crate::harness::choose;
use crate::harness::run_canonical;
use crate::harness::server;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The code system the clause selects from, which the fixture mounts.
const TAXONOMY: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// The value set listing's way into a new value set.
const NEW: &str = "//main//a[normalize-space()='New value set']";

/// The canonical field.
const URL_FIELD: &str = "#vs-url";

/// The title field.
const TITLE_FIELD: &str = "#vs-title";

/// The first clause's code system.
const CLAUSE_SYSTEM: &str = "select[name^='clause.'][name$='.system']";

/// The first clause's Add filter.
const ADD_FILTER: &str = "button[name='intent'][value^='add-filter:']";

/// A filter row's value field.
const FILTER_VALUE: &str = "input[name^='filter.'][name$='.value']";

/// Run the preview.
const PREVIEW: &str = "button[name='intent'][value='preview']";

/// The docked Save.
const SAVE: &str = "button[name='intent'][value='save']";

/// Signs in under `profile` and opens a new value set from the listing.
async fn open_new(journey: &Journey, deployment: &SignedIn, profile: &str, tag: &str) {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, profile, tag).await;
    sign_in(journey, deployment).await;
    journey
        .reopen(&format!("{}/valuesets?fhir=r4b", deployment.base))
        .await;
    journey
        .press(By::XPath(NEW), "the way to a new value set")
        .await
        .unwrap_or_else(|error| panic!("the new value set link refused the press: {error}"));
    journey.element(By::Css(URL_FIELD), "the composer").await;
}

/// Writes the metadata and one include over the taxonomy, filtered to what
/// is below `code`.
async fn compose(journey: &Journey, canonical: &str, code: &str) -> WebDriverResult<()> {
    journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .send_keys(canonical)
        .await?;
    journey
        .element(By::Css(TITLE_FIELD), "the title field")
        .await
        .send_keys("Below the first branch")
        .await?;
    journey
        .press(
            By::Css("#vs-status option[value='active']"),
            "the active status",
        )
        .await?;
    journey
        .press(
            By::Css(format!("{CLAUSE_SYSTEM} option[value='{TAXONOMY}']")),
            "the taxonomy offered to the clause",
        )
        .await?;
    journey.press(By::Css(ADD_FILTER), "Add filter").await?;
    journey
        .element(By::Css(FILTER_VALUE), "the filter's value field")
        .await
        .send_keys(code)
        .await?;
    Ok(())
}

/// A value set over a published code system is composed, previewed and
/// saved.
#[tokio::test]
async fn a_value_set_is_composed_previewed_and_saved() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_new(&journey, &deployment, "writer", "composes").await;
            compose(&journey, &run_canonical("e2e-composed"), "ba-branch").await?;
            journey.press(By::Css(PREVIEW), "Run the preview").await?;
            journey
                .text_becoming(
                    By::Css("main .fui-aside-part"),
                    StringMatch::new("First leaf").partial(),
                    "the preview to list what the clause selects",
                )
                .await;
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

/// The preview and the save are reached and pressed from the keyboard
/// alone.
#[tokio::test]
async fn the_preview_and_the_save_are_reached_from_the_keyboard() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_new(&journey, &deployment, "writer", "composes-by-keyboard").await;
            compose(&journey, &run_canonical("e2e-composed-keys"), "ba-branch").await?;
            journey
                .press(By::Css(TITLE_FIELD), "the title field")
                .await?;
            journey.tab_to("Run the preview", "the preview's Run").await;
            journey.type_here("\u{e007}").await?;
            journey
                .text_becoming(
                    By::Css("main .fui-aside-part"),
                    StringMatch::new("First leaf").partial(),
                    "Enter on Run the preview",
                )
                .await;
            journey
                .press(By::Css(TITLE_FIELD), "the title field")
                .await?;
            journey
                .tab_to("Create this value set", "the docked Save")
                .await;
            journey.type_here("\u{e007}").await?;
            journey
                .text_becoming(
                    By::Css("main .fui-load-report"),
                    StringMatch::new("The server now holds version").partial(),
                    "Enter on the docked Save to save the value set",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader whose token may not write a value set is told so.
#[tokio::test]
async fn a_reader_without_the_scope_composes_nothing() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "reader", "composes-nothing").await;
            sign_in(&journey, &deployment).await;
            journey
                .reopen(&format!("{}/compose", deployment.base))
                .await;
            journey
                .text_becoming(
                    By::Css("h1"),
                    StringMatch::new("Compose a value set").partial(),
                    "the composer to say this account may not do this",
                )
                .await;
            assert_eq!(
                journey.count(By::Css(URL_FIELD)).await,
                0,
                "no form is drawn"
            );
            journey.no_console_errors_but(&["403"]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console with no sign-in carries no composer.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_composer() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/valuesets").await;
            journey.element(By::Css("main h1"), "the listing").await;
            assert_eq!(
                journey.count(By::XPath(NEW)).await,
                0,
                "no new value set is offered"
            );
            journey.reopen(&format!("{base}/compose")).await;
            journey
                .text_becoming(
                    By::Css("h1"),
                    StringMatch::new("Compose a value set").partial(),
                    "the composer to say this may not be done here",
                )
                .await;
            journey.no_console_errors_but(&["403"]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
