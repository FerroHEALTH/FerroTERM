// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Authoring a local code system, and what a console without a sign-in does
//! not carry.
//!
//! The write journeys sign in through the stub issuer at the console that
//! configures one, because a write needs a token the server will take. The
//! editor is one form posted to its own address: every Add and Save posts
//! the whole draft, and the page comes back drawn from it.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::SignedIn;
use crate::harness::choose;
use crate::harness::query_param;
use crate::harness::run_canonical;
use crate::harness::server;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The code systems listing's way into a new code system.
const NEW: &str = "//main//a[normalize-space()='New code system']";

/// The canonical field of the metadata.
const URL_FIELD: &str = "#cs-url";

/// The title field.
const TITLE_FIELD: &str = "#cs-title";

/// The business version field.
const VERSION_FIELD: &str = "#cs-version";

/// Every concept's code field: a new concept's, which the reader types.
const NEW_CODES: &str = "input[name^='concept.'][name$='.code']:not([type='hidden'])";

/// Every concept's display field.
const DISPLAYS: &str = "input[name^='concept.'][name$='.display']";

/// Every concept's lifecycle.
const LIFECYCLES: &str = "select[name^='concept.'][name$='.lifecycle']";

/// The button that adds a concept.
const ADD_CONCEPT: &str = "//button[normalize-space()='Add a concept']";

/// The docked Save.
const SAVE: &str = "button[name='intent'][value='save']";

/// The verdict a saved page draws.
const SAVED: &str = "main .fui-load-report";

/// The refusal a save the server refused draws, in the server's words.
const REFUSAL: &str = "main [role='alert']";

/// Signs in under `profile` and opens a new code system from the listing.
async fn open_new(journey: &Journey, deployment: &SignedIn, profile: &str, tag: &str) {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, profile, tag).await;
    sign_in(journey, deployment).await;
    journey
        .reopen(&format!("{}/systems?fhir=r4b", deployment.base))
        .await;
    journey
        .press(By::XPath(NEW), "the way to a new code system")
        .await
        .unwrap_or_else(|error| panic!("the new code system link refused the press: {error}"));
    journey.element(By::Css(URL_FIELD), "the editor").await;
}

/// Types the metadata and adds one concept, `code` shown as `display`.
async fn author(
    journey: &Journey,
    canonical: &str,
    code: &str,
    display: &str,
) -> WebDriverResult<()> {
    journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .send_keys(canonical)
        .await?;
    journey
        .element(By::Css(TITLE_FIELD), "the title field")
        .await
        .send_keys("Synthetic colours")
        .await?;
    journey
        .element(By::Css(VERSION_FIELD), "the business version field")
        .await
        .send_keys("1.0.0")
        .await?;
    add_concept(journey, code, display).await
}

/// Adds a concept, `code` shown as `display`, as the last row.
async fn add_concept(journey: &Journey, code: &str, display: &str) -> WebDriverResult<()> {
    let before = journey.count(By::Css(DISPLAYS)).await;
    journey
        .press(By::XPath(ADD_CONCEPT), "the control that adds a concept")
        .await?;
    journey
        .count_becoming(By::Css(DISPLAYS), before + 1, "the new concept row")
        .await;
    let codes = journey.all(By::Css(NEW_CODES)).await?;
    codes
        .last()
        .expect("the form carries the new concept's code field")
        .send_keys(code)
        .await?;
    let displays = journey.all(By::Css(DISPLAYS)).await?;
    displays
        .last()
        .expect("the form carries the new concept's display field")
        .send_keys(display)
        .await?;
    Ok(())
}

/// Presses Save and waits for the page that says what the server holds.
async fn save(journey: &Journey) -> WebDriverResult<()> {
    journey.press(By::Css(SAVE), "Save").await?;
    journey
        .text_becoming(
            By::Css(SAVED),
            StringMatch::new("The server now holds version").partial(),
            "the save to answer with the version the server holds",
        )
        .await;
    Ok(())
}

/// A whole local code system is authored, saved, one of its concepts
/// retired, and the retirement read back.
#[tokio::test]
async fn a_local_code_system_is_authored_retired_and_read_back() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_new(&journey, &deployment, "writer", "authors-a-system").await;
            let canonical = run_canonical("e2e-colours");
            author(&journey, &canonical, "red", "Red").await?;
            add_concept(&journey, "vermilion", "Vermilion").await?;
            save(&journey).await?;

            let editor = format!(
                "{}/codesystem?fhir=r4b&system={}",
                deployment.base,
                query_param(&canonical)
            );
            journey.reopen(&editor).await;
            journey
                .count_becoming(By::Css(LIFECYCLES), 2, "both concepts read back")
                .await;
            let lifecycles = journey.all(By::Css(LIFECYCLES)).await?;
            let retired = lifecycles
                .last()
                .expect("the second concept's lifecycle")
                .find(By::Css("option[value='retired']"))
                .await?;
            journey
                .press_on(&retired, "the second concept's retired lifecycle")
                .await?;
            save(&journey).await?;

            journey.reopen(&editor).await;
            journey
                .count_becoming(By::Css(LIFECYCLES), 2, "both concepts read back again")
                .await;
            let mut states = Vec::new();
            for lifecycle in journey.all(By::Css(LIFECYCLES)).await? {
                states.push(lifecycle.prop("value").await?.unwrap_or_default());
            }
            assert_eq!(
                states,
                vec!["active".to_owned(), "retired".to_owned()],
                "the retirement survived the round trip, and nothing else was retired"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A write the token does not carry the scope for is refused by the server,
/// and the refusal is drawn in the server's own words over the draft.
///
/// The `response-only` grant names the write scopes in the token answer and
/// leaves them out of the token, so the console offers the editor and the
/// server refuses the save.
#[tokio::test]
async fn a_write_without_the_scope_is_refused_in_the_server_s_own_words() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_new(&journey, &deployment, "response-only", "refused-write").await;
            let canonical = run_canonical("e2e-refused");
            author(&journey, &canonical, "red", "Red").await?;
            journey.press(By::Css(SAVE), "Save").await?;
            journey
                .element(By::Css(REFUSAL), "the server's refusal")
                .await;
            assert!(
                journey
                    .element(By::Css(URL_FIELD), "the canonical kept in the form")
                    .await
                    .prop("value")
                    .await?
                    .unwrap_or_default()
                    .contains("e2e-refused"),
                "the draft stays in the form after the refusal"
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// One concept is authored and saved with the keyboard alone.
#[tokio::test]
async fn one_concept_is_authored_with_the_keyboard_alone() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_new(&journey, &deployment, "writer", "keyboard-system").await;
            journey.tab_to("cs-url", "the canonical field").await;
            journey.type_here(&run_canonical("e2e-keyboard")).await?;
            journey
                .tab_to("Add a concept", "the control that adds a concept")
                .await;
            journey.type_here("\u{e007}").await?;
            journey
                .count_becoming(By::Css(DISPLAYS), 1, "the new concept row")
                .await;
            journey.tab_to("code", "the new concept's code field").await;
            journey.type_here("key").await?;
            journey.tab(1).await?;
            journey.type_here("Typed with the keyboard").await?;
            journey
                .tab_to("Create this code system", "the docked Save")
                .await;
            journey.type_here("\u{e007}").await?;
            journey
                .text_becoming(
                    By::Css(SAVED),
                    StringMatch::new("The server now holds version").partial(),
                    "Enter on the docked Save to save the code system",
                )
                .await;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console with no sign-in offers no way into an editor, and the editor's
/// address says the account may not edit.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_authoring_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/systems").await;
            journey.element(By::Css("main table"), "the listing").await;
            assert_eq!(
                journey.count(By::XPath(NEW)).await,
                0,
                "a console that signs no one in offers no new code system"
            );
            journey.reopen(&format!("{base}/codesystem")).await;
            journey
                .text_becoming(
                    By::Css("h1"),
                    StringMatch::new("Edit a code system").partial(),
                    "the editor to say this may not be done here",
                )
                .await;
            assert_eq!(
                journey.count(By::Css(URL_FIELD)).await,
                0,
                "no editing form is drawn"
            );
            journey.no_console_errors_but(&["403"]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
