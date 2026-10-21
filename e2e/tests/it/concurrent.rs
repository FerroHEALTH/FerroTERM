// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Two writers on one resource: a save over a version another window
//! replaced is refused, and the screen offers the way back.
//!
//! Every editor screen states the version it replaces in `If-Match`, and the
//! server answers a stale one with `412 Precondition Failed`
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>). The stub issuer grants
//! only the authorization code flow, so the second writer is a second window
//! in the same browser: it shares the console's session cookie and none of
//! the first window's page state, which is where the draft lives.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::SignedIn;
use crate::harness::choose;
use crate::harness::run_canonical;
use crate::harness::run_slug;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The sidebar link into the code system authoring screen.
const AUTHORING_LINK: &str = "//a[normalize-space()='Code system editor']";

/// The stem of the canonical this journey authors under.
const STEM: &str = "e2e-concurrent-edit";

/// The last segment of that canonical, unique to the run.
///
/// The second window opens the resource by its canonical, and a copy left by
/// an earlier run against the same server would answer first.
fn slug() -> String {
    run_slug(STEM)
}

/// The canonical this journey authors under, and nothing else does.
fn canonical() -> String {
    run_canonical(STEM)
}

/// The overview's link to that code system's own screen.
///
/// The canonical is one percent-encoded path segment of the link, and its
/// last segment survives the encoding unchanged.
fn system_link() -> String {
    format!("a[href*='/systems/'][href*='{}']", slug())
}

/// The code system screen's link into the editor.
const EDIT_LINK: &str =
    "//nav[@aria-label='Screens for this code system']//a[normalize-space()='Edit']";

/// The canonical field of the metadata form.
const URL_FIELD: &str = "#editor-url";

/// The business version field.
const VERSION_FIELD: &str = "#editor-version";

/// Every concept's code control.
const CONCEPT_CODES: &str = "input[name='concept-code']";

/// Every concept's display control.
const CONCEPT_DISPLAYS: &str = "input[name='concept-display']";

/// The control that adds a concept.
const ADD_CONCEPT: &str = "//button[normalize-space()='Add a concept']";

/// The control that sends the whole resource.
const SAVE: &str = "//button[starts-with(normalize-space(), 'Create this code system') \
                    or starts-with(normalize-space(), 'Save this code system')]";

/// The control that sends an update, which is what a loaded resource offers.
const UPDATE: &str = "//button[starts-with(normalize-space(), 'Save this code system')]";

/// The live region the screen announces every outcome in.
const REPORT: &str = "#editor-report";

/// The id of that region, as the keyboard reading names it.
const REPORT_ID: &str = "editor-report";

/// The callout a refusal renders the server's own outcome in.
const REFUSAL: &str = "[role='alert']";

/// The control a `412` offers.
const RELOAD: &str = "//button[normalize-space()='Reload the current version']";

/// The two concepts, as code and first display.
const CONCEPTS: [(&str, &str); 2] = [("red", "Red"), ("blue", "Blue")];

/// The display the second window gives the first concept.
const SECOND_WINDOW_DISPLAY: &str = "Crimson";

/// The display the first window gives the second concept.
const FIRST_WINDOW_DISPLAY: &str = "Navy";

/// The severe entry the refused save makes the browser log, which is this
/// journey working.
const REFUSED_ENTRY: &str = "the server responded with a status of 412";

/// The `value` property of every element matching `selector`.
async fn values(journey: &Journey, selector: &str) -> WebDriverResult<Vec<String>> {
    let mut read = Vec::new();
    for element in journey.all(By::Css(selector)).await? {
        read.push(element.prop("value").await?.unwrap_or_default());
    }
    Ok(read)
}

/// Replaces what the display control at `position` holds with `display`.
async fn retype(journey: &Journey, position: usize, display: &str) -> WebDriverResult<()> {
    let controls = journey.all(By::Css(CONCEPT_DISPLAYS)).await?;
    let control = controls
        .get(position)
        .expect("the form carries a display control at that position");
    control.clear().await?;
    control.send_keys(display).await?;
    Ok(())
}

/// Presses save and returns what the live region then announces.
async fn save(journey: &Journey, needle: &str, what: &str) -> String {
    journey
        .element(By::XPath(SAVE), "the control that saves")
        .await
        .click()
        .await
        .unwrap_or_else(|error| panic!("the save control refused the press: {error}"));
    journey
        .text_becoming(By::Css(REPORT), StringMatch::new(needle).partial(), what)
        .await
}

/// Signs in as a terminologist and authors the code system both windows edit.
async fn author(journey: &Journey, deployment: &SignedIn) -> WebDriverResult<()> {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, "writer", "concurrent-edit").await;
    sign_in(journey, deployment).await;
    journey
        .element(By::XPath(AUTHORING_LINK), "the way to the authoring screen")
        .await
        .click()
        .await?;
    journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .send_keys(canonical())
        .await?;
    journey
        .element(By::Css(VERSION_FIELD), "the business version field")
        .await
        .send_keys("1.0.0")
        .await?;
    for (count, (code, display)) in CONCEPTS.iter().enumerate() {
        journey
            .element(By::XPath(ADD_CONCEPT), "the control that adds a concept")
            .await
            .click()
            .await?;
        journey
            .count_becoming(By::Css(CONCEPT_CODES), count + 1, "the new concept row")
            .await;
        let codes = journey.all(By::Css(CONCEPT_CODES)).await?;
        let displays = journey.all(By::Css(CONCEPT_DISPLAYS)).await?;
        codes
            .last()
            .expect("the form carries the row just added")
            .send_keys(*code)
            .await?;
        displays
            .last()
            .expect("the form carries the row just added")
            .send_keys(*display)
            .await?;
    }
    let said = save(journey, "Saved", "the first window's create").await;
    assert!(
        said.contains("version 1"),
        "the create is the first version the server holds: `{said}`"
    );
    Ok(())
}

/// Opens the same code system in a second window and saves a change there.
///
/// The sign-in is the console's session, which the second window shares, so
/// it opens the code system listing and moves by pressing links as a reader
/// would.
async fn replace_from_a_second_window(
    journey: &Journey,
    deployment: &SignedIn,
) -> WebDriverResult<()> {
    journey.open_window().await?;
    journey
        .reopen(&format!("{}/systems", deployment.base))
        .await;
    journey
        .element(
            By::Css(system_link()),
            "the listing's row for the code system",
        )
        .await
        .click()
        .await?;
    journey
        .element(By::XPath(EDIT_LINK), "the way into the editor")
        .await
        .click()
        .await?;
    journey
        .count_becoming(
            By::Css(CONCEPT_DISPLAYS),
            CONCEPTS.len(),
            "the code system read into the second window",
        )
        .await;
    journey
        .element(By::XPath(UPDATE), "the second window's save control")
        .await;
    assert_eq!(
        values(journey, CONCEPT_DISPLAYS).await?,
        vec!["Red".to_owned(), "Blue".to_owned()],
        "the second window reads what the first one saved"
    );
    retype(journey, 0, SECOND_WINDOW_DISPLAY).await?;
    let said = save(journey, "Saved", "the second window's save").await;
    assert!(
        said.contains("version 2"),
        "the second window's save is the second version: `{said}`"
    );
    journey.no_console_errors().await;
    Ok(())
}

/// A save over a version another window replaced is refused, keeps what was
/// typed, and reloads the current version on request.
#[tokio::test]
async fn a_save_over_a_replaced_version_is_refused_and_the_current_one_reloads() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            author(&journey, &deployment).await?;
            let first = journey.window().await?;

            replace_from_a_second_window(&journey, &deployment).await?;
            journey.switch_to(&first).await?;

            // The first window still holds version 1, so its save states a
            // version the server has moved past.
            retype(&journey, 1, FIRST_WINDOW_DISPLAY).await?;
            let announced = save(
                &journey,
                "does not hold",
                "the screen to announce the refusal",
            )
            .await;
            assert!(
                announced.contains("If-Match"),
                "the server's own wording reaches the live region: `{announced}`"
            );
            let rendered = journey
                .element(By::Css(REFUSAL), "the refusal the screen renders")
                .await
                .text()
                .await?;
            assert!(
                rendered.contains("412"),
                "the status the server answered is shown: `{rendered}`"
            );
            assert!(
                rendered.contains("does not hold"),
                "and so is its OperationOutcome: `{rendered}`"
            );
            assert_eq!(
                values(&journey, CONCEPT_DISPLAYS).await?,
                vec!["Red".to_owned(), FIRST_WINDOW_DISPLAY.to_owned()],
                "the refused edit is still in the form, and nothing was lost"
            );
            journey.no_console_errors_but(&[REFUSED_ENTRY]).await;

            journey
                .element(By::XPath(RELOAD), "the control a 412 offers")
                .await
                .click()
                .await?;
            let reloaded = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Reloaded").partial(),
                    "the screen to announce the reload",
                )
                .await;
            assert!(
                reloaded.contains("version 2"),
                "the reload names the version it now shows: `{reloaded}`"
            );
            journey
                .focus_becoming(REPORT_ID, "the keyboard to land on the announcement")
                .await;
            assert_eq!(
                values(&journey, CONCEPT_DISPLAYS).await?,
                vec![SECOND_WINDOW_DISPLAY.to_owned(), "Blue".to_owned()],
                "the form shows the second window's version"
            );
            journey
                .address_carrying(&slug(), "the address to name what is open")
                .await;

            // The change applied again over the current version is taken.
            retype(&journey, 1, FIRST_WINDOW_DISPLAY).await?;
            let said = save(&journey, "Saved", "the save after the reload").await;
            assert!(
                said.contains("version 3"),
                "the save states the version the reload read: `{said}`"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
