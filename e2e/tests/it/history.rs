// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The versions of a resource in the editor bundle: reading them, comparing
//! two, and putting an earlier one back.
//!
//! Every journey authors the resource it then reads the versions of, because
//! the repository distributes no terminology content and the version list is
//! only interesting once there is more than one version. Each one writes under
//! its own canonical, unique to the run, so the journeys run beside each other
//! against one server and a second run against the same store passes.
//!
//! The screen asks the synchronisation service's admin listener for the newest
//! run, which nothing answers on this deployment. The browser logs that
//! refusal as a severe network entry, and it is the screen working: the
//! findings section is then not in the document at all.

use thirtyfour::common::keys::TypingData;
use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::SignedIn;
use crate::harness::choose;
use crate::harness::run_canonical;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The sidebar link into the code system authoring screen.
const AUTHORING_LINK: &str = "//a[normalize-space()='Code system editor']";

/// The link each authoring screen carries to the versions of what it has open.
const HISTORY_LINK: &str = "//a[normalize-space()='History of this resource']";

/// The canonical the restore journey authors under.
fn restore_canonical() -> String {
    run_canonical("e2e-history-restore")
}

/// The canonical the read-only journey authors under.
fn reader_canonical() -> String {
    run_canonical("e2e-history-reader")
}

/// The canonical the keyboard journey authors under.
fn keyboard_canonical() -> String {
    run_canonical("e2e-history-keyboard")
}

/// The canonical field of the code system form.
const URL_FIELD: &str = "#editor-url";

/// The business version field.
const VERSION_FIELD: &str = "#editor-version";

/// Every concept's code control.
const CONCEPT_CODES: &str = "input[name='concept-code']";

/// Every concept's display control.
const CONCEPT_DISPLAYS: &str = "input[name='concept-display']";

/// The control that adds a concept.
const ADD_CONCEPT: &str = "//button[normalize-space()='Add a concept']";

/// The control that sends the whole code system.
const SAVE: &str = "//button[starts-with(normalize-space(), 'Create this code system') \
                    or starts-with(normalize-space(), 'Save this code system')]";

/// The live region the code system editor announces a save in.
const EDITOR_REPORT: &str = "#editor-report";

/// Every row of the version table.
const VERSION_ROWS: &str = "//section[@aria-labelledby='history-versions-heading']//tbody/tr";

/// Every restore control on the version table.
const RESTORE_CONTROLS: &str = "button[name='restore-version']";

/// The control that compares the two chosen versions.
const COMPARE: &str = "//button[normalize-space()='Compare these versions']";

/// The live count of what the two chosen versions differ by.
const DIFFERENCE_COUNT: &str = "#history-difference-count";

/// Every row of the difference table.
const DIFFERENCE_ROWS: &str = "//section[@aria-labelledby='history-difference-heading']//tbody/tr";

/// The live region the history screen announces a restore in.
const HISTORY_REPORT: &str = "#history-report";

/// The code every journey authors.
const CODE: &str = "red";

/// The display the first version carries.
const FIRST_DISPLAY: &str = "Red";

/// The display the second version carries.
const SECOND_DISPLAY: &str = "Crimson";

/// Signs in as a terminologist and opens the code system authoring screen.
///
/// Every move after the sign-in is a press rather than a load, because a load
/// would drop the token the sign-in just put in the page.
async fn open_authoring(journey: &Journey, deployment: &SignedIn, tag: &str) {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, "writer", tag).await;
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

/// Authors one code system with one concept and saves it.
async fn author_and_save(journey: &Journey, canonical: &str) -> WebDriverResult<()> {
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
        .send_keys(CODE)
        .await?;
    journey
        .element(By::Css(CONCEPT_DISPLAYS), "the concept's display control")
        .await
        .send_keys(FIRST_DISPLAY)
        .await?;
    save(journey, "the first save").await;
    Ok(())
}

/// Presses save and waits for the screen to announce what the server took.
async fn save(journey: &Journey, what: &str) {
    journey
        .press(By::XPath(SAVE), "the control that saves")
        .await
        .unwrap_or_else(|error| panic!("the save control refused the press: {error}"));
    journey
        .text_becoming(
            By::Css(EDITOR_REPORT),
            StringMatch::new("Saved").partial(),
            what,
        )
        .await;
}

/// Changes the concept's display and saves again, making a second version.
async fn change_the_display_and_save(journey: &Journey) -> WebDriverResult<()> {
    let display = journey
        .element(By::Css(CONCEPT_DISPLAYS), "the concept's display control")
        .await;
    display.clear().await?;
    display.send_keys(SECOND_DISPLAY).await?;
    save(journey, "the second save").await;
    Ok(())
}

/// Opens the versions of the resource the authoring screen has open.
async fn open_history(journey: &Journey) {
    journey
        .press(By::XPath(HISTORY_LINK), "the way to the versions")
        .await
        .unwrap_or_else(|error| panic!("the history link refused the press: {error}"));
    journey
        .element(By::Css("#history-versions-heading"), "the version list")
        .await;
}

/// Picks `value` in the version control `selector` names.
///
/// The option itself is pressed, which is what a reader does and what fires
/// the control's own `change`, so the journey drives the real widget rather
/// than writing a property on it.
async fn pick(journey: &Journey, selector: &str, value: &str) -> WebDriverResult<()> {
    let control = journey
        .element(By::Css(selector), "a version control")
        .await;
    let option = control
        .find(By::Css(format!("option[value='{value}']")))
        .await
        .unwrap_or_else(|error| panic!("`{selector}` has no option `{value}`: {error}"));
    option.click().await?;
    Ok(())
}

/// Chooses two versions and presses the control that compares them.
async fn compare(journey: &Journey, from: &str, to: &str) -> WebDriverResult<()> {
    pick(journey, "#history-from", from).await?;
    pick(journey, "#history-to", to).await?;
    journey
        .element(By::XPath(COMPARE), "the control that compares")
        .await
        .click()
        .await?;
    Ok(())
}

/// Two versions are compared, the first is restored, and it comes back whole.
#[tokio::test]
async fn an_earlier_version_is_restored_and_the_resource_comes_back_as_it_was() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "restores-a-version").await;
            author_and_save(&journey, &restore_canonical()).await?;
            change_the_display_and_save(&journey).await?;

            open_history(&journey).await;
            journey
                .count_becoming(By::XPath(VERSION_ROWS), 2, "both versions")
                .await;

            compare(&journey, "1", "2").await?;
            let counted = journey
                .text_becoming(
                    By::Css(DIFFERENCE_COUNT),
                    StringMatch::new("differs").partial(),
                    "the comparison to report what changed",
                )
                .await;
            assert!(
                counted.starts_with('1'),
                "one element differs between the two versions: `{counted}`"
            );
            let changed = journey
                .element(By::XPath(DIFFERENCE_ROWS), "the difference table")
                .await
                .text()
                .await?;
            assert!(
                changed.contains("display") && changed.contains(FIRST_DISPLAY),
                "the row names the element and what the earlier version held: `{changed}`"
            );

            journey
                .element(
                    By::XPath("//button[normalize-space()='Restore version 1']"),
                    "the control that restores the first version",
                )
                .await
                .click()
                .await?;
            let said = journey
                .text_becoming(
                    By::Css(HISTORY_REPORT),
                    StringMatch::new("Restored version 1").partial(),
                    "the screen to announce the restore",
                )
                .await;
            assert!(
                said.contains('3'),
                "the restore is a new version the server now holds: `{said}`"
            );
            journey
                .count_becoming(By::XPath(VERSION_ROWS), 3, "the restore's own version")
                .await;

            compare(&journey, "1", "3").await?;
            journey
                .text_becoming(
                    By::Css(DIFFERENCE_COUNT),
                    StringMatch::new("state the same elements").partial(),
                    "the restored version to equal the one it restored",
                )
                .await;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader who holds no token sees the versions and no way to restore one.
#[tokio::test]
async fn a_reader_sees_the_versions_and_no_restore_control() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "reads-the-versions").await;
            author_and_save(&journey, &reader_canonical()).await?;
            change_the_display_and_save(&journey).await?;
            open_history(&journey).await;
            journey
                .count_becoming(By::XPath(VERSION_ROWS), 2, "both versions")
                .await;
            assert_eq!(
                journey.count(By::Css(RESTORE_CONTROLS)).await,
                1,
                "a terminologist may restore the earlier version and not the current one"
            );
            let address = journey.address().await;

            // Without the console's cookies the load is a visitor's.
            journey.drop_cookies().await?;
            journey.reopen(&address).await;
            journey
                .count_becoming(
                    By::XPath(VERSION_ROWS),
                    2,
                    "the versions read without a token",
                )
                .await;
            assert_eq!(
                journey.count(By::Css(RESTORE_CONTROLS)).await,
                0,
                "a reader sees what changed and is offered no way to change it"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The comparison is reachable and runnable with the keyboard alone.
#[tokio::test]
async fn the_comparison_runs_from_the_keyboard_alone() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "compares-from-the-keyboard").await;
            author_and_save(&journey, &keyboard_canonical()).await?;
            change_the_display_and_save(&journey).await?;
            open_history(&journey).await;
            journey
                .count_becoming(By::XPath(VERSION_ROWS), 2, "both versions")
                .await;

            // A closed select chooses the option whose label starts with the
            // typed letter, and the same letter again moves to the next such
            // option. That holds on every platform, where an arrow key on macOS
            // opens the native picker instead of moving the selection.
            journey
                .tab_to("history-from", "the earlier version control")
                .await;
            journey.type_here("v").await?;
            journey
                .tab_to("history-to", "the later version control")
                .await;
            journey.type_here("vv").await?;
            journey
                .tab_to("Compare these versions", "the control that compares")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;

            let counted = journey
                .text_becoming(
                    By::Css(DIFFERENCE_COUNT),
                    StringMatch::new("differs").partial(),
                    "the keyboard to run the comparison",
                )
                .await;
            assert!(
                counted.starts_with('1'),
                "the keyboard reached the same comparison the pointer does: `{counted}`"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
