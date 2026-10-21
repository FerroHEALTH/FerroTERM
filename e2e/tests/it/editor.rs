// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Authoring a local code system, and what a console without a sign-in does
//! not carry.
//!
//! The write journeys sign in through the stub issuer at the console that
//! configures one, because a write needs a token the server will take. The
//! console's server holds the token in the reader's session; the read-back
//! below drops the console's cookies first, so it reads as a visitor.

use thirtyfour::common::keys::TypingData;
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

/// The sidebar link into the authoring screen.
const AUTHORING_LINK: &str = "//a[normalize-space()='Code system editor']";

/// The canonical the round-trip journey authors under.
///
/// Synthetic: the repository distributes no code system content, and each
/// journey writes the resource it then reads back. One canonical per journey,
/// because the journeys run beside each other against one server, and one per
/// run, because a store may persist between runs.
fn canonical() -> String {
    run_canonical("e2e-colours")
}

/// The canonical the refused-write journey authors under.
fn refused_canonical() -> String {
    run_canonical("e2e-refused")
}

/// The canonical the keyboard journey authors under.
fn keyboard_canonical() -> String {
    run_canonical("e2e-keyboard")
}

/// The canonical field of the metadata form.
const URL_FIELD: &str = "#editor-url";

/// The business version field.
const VERSION_FIELD: &str = "#editor-version";

/// Every concept's code control.
const CONCEPT_CODES: &str = "input[name='concept-code']";

/// Every concept's display control.
const CONCEPT_DISPLAYS: &str = "input[name='concept-display']";

/// Every concept's lifecycle control.
const LIFECYCLES: &str = "select[name='concept-lifecycle']";

/// The control that adds a concept.
const ADD_CONCEPT: &str = "//button[normalize-space()='Add a concept']";

/// The control that sends the whole resource.
const SAVE: &str = "//button[starts-with(normalize-space(), 'Create this code system') \
                    or starts-with(normalize-space(), 'Save this code system')]";

/// The live region the screen announces every outcome in.
const REPORT: &str = "#editor-report";

/// The heading of the check a save runs on a retired concept.
const CHECK: &str = "#editor-check-heading";

/// The table that check draws, found by the parameter it reports.
const CHECK_TABLE: &str = "//table[.//th[normalize-space()='inactive']]";

/// The callout a refusal renders the server's own outcome in.
const REFUSAL: &str = "[role='alert']";

/// The first concept the journeys author.
const FIRST: &str = "red";

/// The second, which the round-trip journey retires.
const RETIRED: &str = "vermilion";

/// The `value` property of every element matching `selector`.
async fn values(journey: &Journey, selector: &str) -> WebDriverResult<Vec<String>> {
    let mut read = Vec::new();
    for element in journey.all(By::Css(selector)).await? {
        read.push(element.prop("value").await?.unwrap_or_default());
    }
    Ok(read)
}

/// Signs in under `profile` and opens the authoring screen from the sidebar.
///
/// The screen is reached by pressing the link, the way a reader reaches it.
async fn open_authoring(journey: &Journey, deployment: &SignedIn, profile: &str, tag: &str) {
    // The page the journey opened on is left for the issuer's, so it is
    // waited for first: a bundle still fetching its own assets logs an
    // aborted request when the address changes under it.
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, profile, tag).await;
    sign_in(journey, deployment).await;
    journey
        .element(By::XPath(AUTHORING_LINK), "the way to the authoring screen")
        .await
        .click()
        .await
        .unwrap_or_else(|error| panic!("the authoring link refused the press: {error}"));
    journey
        .element(By::Css(URL_FIELD), "the authoring form")
        .await;
}

/// Types the metadata and one concept into the form.
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
        .element(By::Css(VERSION_FIELD), "the business version field")
        .await
        .send_keys("1.0.0")
        .await?;
    journey
        .element(By::XPath(ADD_CONCEPT), "the control that adds a concept")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(CONCEPT_CODES), 1, "the concept row")
        .await;
    journey
        .element(By::Css(CONCEPT_CODES), "the concept's code control")
        .await
        .send_keys(code)
        .await?;
    journey
        .element(By::Css(CONCEPT_DISPLAYS), "the concept's display control")
        .await
        .send_keys(display)
        .await?;
    Ok(())
}

/// Adds the second concept of the round-trip journey and retires it.
async fn add_and_retire(journey: &Journey) -> WebDriverResult<()> {
    journey
        .element(By::XPath(ADD_CONCEPT), "the control that adds a concept")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(CONCEPT_CODES), 2, "the second concept row")
        .await;
    let codes = journey.all(By::Css(CONCEPT_CODES)).await?;
    let displays = journey.all(By::Css(CONCEPT_DISPLAYS)).await?;
    codes
        .last()
        .expect("the form carries two concept rows")
        .send_keys(RETIRED)
        .await?;
    displays
        .last()
        .expect("the form carries two display controls")
        .send_keys("Vermilion")
        .await?;

    // The concept's own legend follows the code that was typed, and the retire
    // control is named after it, so finding that control proves the press
    // lands on the row the journey means.
    let retire = format!("//button[normalize-space()='Retire {RETIRED}']");
    journey
        .element(By::XPath(&retire), "the control that retires the concept")
        .await
        .click()
        .await?;
    assert_eq!(
        values(journey, LIFECYCLES).await?,
        vec!["active".to_owned(), "retired".to_owned()],
        "the retire control moves one concept's lifecycle and leaves the other"
    );
    Ok(())
}

/// Reads the code system back off the server and checks the retirement stuck.
///
/// The console's cookies are dropped first, so the form opens read-only:
/// what it shows is the server's own answer, seen the way a reader who never
/// signed in sees it.
async fn the_retirement_survives(journey: &Journey, base: &str) -> WebDriverResult<()> {
    journey.drop_cookies().await?;
    journey
        .reopen(&format!(
            "{base}/codesystem?fhir=r4b&system={}",
            query_param(&canonical())
        ))
        .await;
    journey
        .count_becoming(
            By::Css(CONCEPT_CODES),
            2,
            "the saved code system to be read back",
        )
        .await;
    let read = values(journey, CONCEPT_CODES).await?;
    assert!(
        read.contains(&RETIRED.to_owned()) && read.contains(&FIRST.to_owned()),
        "both concepts came back: {read:?}"
    );
    let states = values(journey, LIFECYCLES).await?;
    let retired: Vec<&String> = read
        .iter()
        .zip(states.iter())
        .filter(|(_, state)| state.as_str() == "retired")
        .map(|(code, _)| code)
        .collect();
    assert_eq!(
        retired.first().map(|code| code.as_str()),
        Some(RETIRED),
        "the retirement survived the round trip through the server: {read:?} {states:?}"
    );
    assert_eq!(
        retired.len(),
        1,
        "and nothing else was retired with it: {read:?} {states:?}"
    );
    Ok(())
}

/// A whole local code system is authored, retired, saved, and read back.
#[tokio::test]
async fn a_local_code_system_is_authored_retired_and_read_back() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "writer", "authors-a-code-system").await;

            author(&journey, &canonical(), FIRST, "Red").await?;
            add_and_retire(&journey).await?;

            journey
                .element(By::XPath(SAVE), "the control that saves")
                .await
                .click()
                .await?;
            let said = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Saved").partial(),
                    "the screen to announce the save",
                )
                .await;
            assert!(
                said.contains("version"),
                "the announcement says which version the server now holds: `{said}`"
            );

            // The check the save runs is the server's own answer about the
            // concept that was retired.
            journey
                .element(By::Css(CHECK), "the check the save ran")
                .await;
            let reported = journey
                .element(By::XPath(CHECK_TABLE), "the table the check draws")
                .await
                .text()
                .await?;
            assert!(
                reported.contains(RETIRED),
                "the check names the concept that was retired: `{reported}`"
            );
            assert!(
                reported.contains("true"),
                "$validate-code reports it as inactive: `{reported}`"
            );

            the_retirement_survives(&journey, &deployment.base).await?;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A write the token carries no scope for is refused, in the server's words.
///
/// The issuer's `response-only` profile states the write scope in the token
/// response and leaves it out of the access token, so the viewer draws the
/// control its answer opened and the server refuses the request that control
/// sent. What the screen does with the refusal is the point: the outcome text
/// is announced in the live region, and the outcome itself is rendered whole
/// (<https://hl7.org/fhir/R4B/operationoutcome.html>).
#[tokio::test]
async fn a_write_without_the_scope_is_refused_in_the_server_s_own_words() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "response-only", "refused-write").await;

            author(&journey, &refused_canonical(), "refused", "Refused").await?;
            journey
                .element(By::XPath(SAVE), "the control that saves")
                .await
                .click()
                .await?;

            let announced = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("permission").partial(),
                    "the screen to announce the refusal",
                )
                .await;
            assert!(
                announced.contains("scope"),
                "the server's own wording reaches the live region: `{announced}`"
            );

            let rendered = journey
                .element(By::Css(REFUSAL), "the refusal the screen renders")
                .await
                .text()
                .await?;
            assert!(
                rendered.contains("403"),
                "the status the server answered is shown: `{rendered}`"
            );
            assert!(
                rendered.contains("scope"),
                "and so is its OperationOutcome: `{rendered}`"
            );

            // The refusal the journey provoked is logged as a network entry,
            // which is this journey working. The exemption names the request
            // it belongs to, so another refusal in the same journey still
            // fails it.
            journey
                .no_console_errors_but(&[&format!(
                    "{}/r4b/CodeSystem - Failed to load resource: the server responded with a \
                     status of 403",
                    deployment.base
                )])
                .await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// One concept is authored with the keyboard alone.
///
/// Every step of the authoring is a tab press or a typed character: the walk
/// reaches each control by tabbing to it and drives it where the keyboard is,
/// so nothing here is reachable only with a pointer
/// (<https://www.w3.org/TR/WCAG22/#keyboard>).
#[tokio::test]
async fn one_concept_is_authored_with_the_keyboard_alone() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "writer", "keyboard-authoring").await;

            journey.tab_to("editor-url", "the canonical field").await;
            journey.type_here(&keyboard_canonical()).await?;

            journey
                .tab_to("Add a concept", "the control that adds a concept")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;
            journey
                .count_becoming(By::Css(CONCEPT_CODES), 1, "the concept the key press added")
                .await;

            journey.tab_to("concept-code", "the concept's code").await;
            journey.type_here("keyed").await?;
            journey
                .tab_to("concept-display", "the concept's display")
                .await;
            journey.type_here("Keyed").await?;

            assert_eq!(
                values(&journey, CONCEPT_CODES).await?,
                vec!["keyed".to_owned()],
                "the code was typed rather than clicked in"
            );
            assert_eq!(
                values(&journey, CONCEPT_DISPLAYS).await?,
                vec!["Keyed".to_owned()],
                "and so was the display"
            );

            journey
                .tab_to("Create this code system", "the control that saves")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;
            journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Saved").partial(),
                    "the save the keyboard sent",
                )
                .await;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console that offers no sign-in carries no authoring screen and no way to
/// reach one, since nobody there can save.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_authoring_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/codesystem?fhir=r4b").await;
            journey
                .element(By::Css("h1"), "the console's answer to an address it lacks")
                .await;
            assert_eq!(
                journey.count(By::Css(URL_FIELD)).await,
                0,
                "a console without a sign-in draws no authoring form"
            );

            journey.reopen(&format!("{base}/?fhir=r4b")).await;
            journey
                .element(By::Css("header a.fui-lockup"), "the shell mark")
                .await;
            assert_eq!(
                journey.count(By::XPath(AUTHORING_LINK)).await,
                0,
                "the sidebar of a console without a sign-in leads to no authoring screen"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
