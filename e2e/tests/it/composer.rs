// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Composing a local value set in the editor bundle, and what the reader
//! bundle does not carry.
//!
//! The shape is the one L-03 asks for: one include of a value set this server
//! already publishes, one include of the composer's own codes, a preview of
//! the unsaved definition, and a save. The published value set here is the
//! fixture the deployment loads from disk, which has no write path, so it also
//! stands for the national content the editor never offers to change.
//!
//! The write journeys sign in through the stub issuer at the console that
//! configures one, because a write needs a token the server will take; the
//! console's server holds it in the reader's session.

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

/// The sidebar link into the composer.
const COMPOSER_LINK: &str = "//a[normalize-space()='Value set composer']";

/// The canonical the round-trip journey composes under.
///
/// Synthetic: the repository distributes no terminology content, and each
/// journey writes the resource it then reads back. One canonical per journey,
/// because the journeys run beside each other against one server, and one per
/// run, because a store may persist between runs.
fn canonical() -> String {
    run_canonical("e2e-composed")
}

/// The canonical the keyboard journey composes under.
fn keyboard_canonical() -> String {
    run_canonical("e2e-composed-keyboard")
}

/// The value set the fixture publishes, which the composer draws in whole.
const PUBLISHED: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";

/// The code system the fixture publishes, which the own codes come from.
const SYSTEM: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// That system, percent-encoded into a query parameter.
const SYSTEM_PARAM: &str = "https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy";

/// One code of that system, which the composer names in its second include.
const CODE: &str = "ca-leaf";

/// What the search types to find it.
///
/// Nothing: an empty `filter` lists the first concepts the system answers
/// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), which is what
/// the concept browser's search does, and the code picks the row out of that
/// answer without the journey guessing what text the server indexes.
const SEARCH_TERM: &str = "";

/// The canonical field of the composer.
const URL_FIELD: &str = "#compose-url";

/// The live region the screen announces every outcome in.
const REPORT: &str = "#composer-report";

/// The control that saves the whole value set.
const SAVE: &str = "//button[normalize-space()='Save this value set']";

/// The control that runs the preview.
const PREVIEW: &str = "//button[normalize-space()='Run the preview']";

/// The control that adds an include.
const ADD_INCLUDE: &str = "//button[normalize-space()='Add an include']";

/// Every clause's own panel, which is one fieldset per include or exclude.
const CLAUSES: &str = "fieldset legend";

/// The picker that adds a value set this root publishes, on the first clause.
const PUBLISHED_PICKER: &str = "select[id^='compose-add-vs-']";

/// The picker that names a clause's code system.
const SYSTEM_PICKER: &str = "select[id^='compose-system-']";

/// The search that finds a code in that system.
const CODE_SEARCH: &str = "input[id^='compose-search-']";

/// A code system every deployment serves that declares implicit value set
/// templates (<https://terminology.hl7.org/UCUM.html>).
const UCUM: &str = "http://unitsofmeasure.org";

/// The implicit value set template picker of the second clause.
const SECOND_TEMPLATE_PICKER: &str = "(//select[starts-with(@id, 'compose-implicit-')])[2]";

/// The notice a screen that cannot be saved carries.
const READ_ONLY: &str = "//p[contains(text(), 'read-only') or contains(text(), 'no permission') \
     or contains(text(), 'Sign in to save')]";

/// Picks `value` in the `index`-th control matching `selector`.
///
/// The option itself is pressed, which is what a reader does and what fires
/// the control's own `change`, so the journey drives the real widget rather
/// than writing a property on it. The options arrive with the server's
/// answer, after the control, so the option is waited for.
async fn pick(journey: &Journey, selector: &str, index: usize, value: &str) -> WebDriverResult<()> {
    let controls = journey.all(By::Css(selector)).await?;
    let control = controls
        .get(index)
        .unwrap_or_else(|| panic!("the form draws a `{selector}` at position {index}"));
    let option = control
        .query(By::Css(format!("option[value='{value}']")))
        .first()
        .await
        .unwrap_or_else(|error| panic!("`{selector}` has no option `{value}`: {error}"));
    option.click().await?;
    Ok(())
}

/// Signs in under `profile` and opens the composer from the sidebar.
///
/// The screen is reached by pressing the link, the way a reader reaches it.
async fn open_composer(journey: &Journey, deployment: &SignedIn, profile: &str, tag: &str) {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, profile, tag).await;
    sign_in(journey, deployment).await;
    journey
        .element(By::XPath(COMPOSER_LINK), "the way to the composer")
        .await
        .click()
        .await
        .unwrap_or_else(|error| panic!("the composer link refused the press: {error}"));
    journey
        .element(By::Css(URL_FIELD), "the composer form")
        .await;
}

/// Composes the L-03 shape: a published value set, and one own code.
async fn compose(journey: &Journey, canonical: &str) -> WebDriverResult<()> {
    journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .send_keys(canonical)
        .await?;

    // The first include draws the published value set in whole.
    journey
        .count_becoming(By::Css(PUBLISHED_PICKER), 1, "the first clause")
        .await;
    pick(journey, PUBLISHED_PICKER, 0, PUBLISHED).await?;
    journey
        .element(
            By::XPath("(//button[normalize-space()='Add'])[1]"),
            "the control that adds the picked value set",
        )
        .await
        .click()
        .await?;
    journey
        .count_becoming(
            By::Css("input[id^='compose-valueset-']"),
            1,
            "the value set the clause draws in",
        )
        .await;

    // The second include names one code of the fixture's own code system.
    journey
        .element(By::XPath(ADD_INCLUDE), "the control that adds an include")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(SYSTEM_PICKER), 2, "the second clause")
        .await;
    // A system that declares implicit value set templates offers them on the
    // clause, and the fixture's own code system declares none, so its clause
    // hides the template picker.
    pick(journey, SYSTEM_PICKER, 1, UCUM).await?;
    journey
        .element(
            By::XPath(format!(
                "{SECOND_TEMPLATE_PICKER}[not(ancestor::div[@hidden])]/option[@value='{UCUM}/vs']"
            )),
            "the template UCUM declares",
        )
        .await;
    pick(journey, SYSTEM_PICKER, 1, SYSTEM).await?;
    journey
        .count_becoming(
            By::XPath(format!("{SECOND_TEMPLATE_PICKER}[ancestor::div[@hidden]]")),
            1,
            "the template picker hidden for a system that declares none",
        )
        .await;
    let searches = journey.all(By::Css(CODE_SEARCH)).await?;
    searches
        .get(1)
        .expect("the second clause carries its own search")
        .send_keys(format!("{SEARCH_TERM}\n"))
        .await?;
    journey
        .element(
            By::XPath(format!(
                "//li[contains(., '{CODE}')]//button[normalize-space()='Add']"
            )),
            "the search result for that code",
        )
        .await
        .click()
        .await?;
    journey
        .count_becoming(
            By::Css("input[id^='compose-display-']"),
            1,
            "the code the clause now names",
        )
        .await;
    Ok(())
}

/// Reads the value set back off the server and validates a code through it.
///
/// The console's cookies are dropped first, so the form opens read-only:
/// what it shows is the server's own answer, seen the way a reader who never
/// signed in sees it.
async fn the_definition_survives(
    journey: &Journey,
    base: &str,
    address: &str,
) -> WebDriverResult<()> {
    journey.drop_cookies().await?;
    journey.reopen(address).await;
    journey
        .count_becoming(
            By::Css("input[id^='compose-valueset-']"),
            1,
            "the saved value set to be read back",
        )
        .await;
    let read = journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .prop("value")
        .await?
        .unwrap_or_default();
    assert_eq!(read, canonical(), "the canonical came back as it was saved");
    let clauses = journey.count(By::Css(CLAUSES)).await;
    assert!(
        clauses >= 2,
        "both includes came back: {clauses} clause legends"
    );

    journey
        .reopen(&format!(
            "{base}/validate?fhir=r4b&on=valueset&valueSet={}&code={CODE}&system={SYSTEM_PARAM}",
            query_param(&canonical())
        ))
        .await;
    let validated = journey
        .text_becoming(
            By::Css("main"),
            StringMatch::new("result").partial(),
            "$validate-code to answer through the saved value set",
        )
        .await;
    assert!(
        validated.contains("true"),
        "the code is in the value set the composer saved: `{validated}`"
    );
    Ok(())
}

/// A whole local value set is composed, previewed, saved, and read back.
#[tokio::test]
async fn a_value_set_over_a_published_one_is_composed_previewed_and_saved() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_composer(&journey, &deployment, "writer", "composes-a-value-set").await;
            compose(&journey, &canonical()).await?;

            // The rules the server reads the definition by are on the screen.
            let rules = journey
                .element(
                    By::Css("section[aria-labelledby='compose-includes-heading']"),
                    "the includes",
                )
                .await
                .text()
                .await?;
            assert!(
                rules.contains("union of them all"),
                "the includes say they union: `{rules}`"
            );
            assert!(
                rules.contains("is in the value set named here"),
                "and the criteria inside one include say they intersect: `{rules}`"
            );

            journey
                .element(By::XPath(PREVIEW), "the control that runs the preview")
                .await
                .click()
                .await?;
            let counted = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("selects").partial(),
                    "the preview to announce what the definition selects",
                )
                .await;
            assert!(
                counted.contains("concepts"),
                "the count is announced in the live region: `{counted}`"
            );
            let previewed = journey
                .element(
                    By::Css("section[aria-labelledby='compose-preview-heading']"),
                    "the preview",
                )
                .await
                .text()
                .await?;
            assert!(
                previewed.contains(CODE),
                "the code the composer named is in the expansion: `{previewed}`"
            );

            journey
                .press(By::XPath(SAVE), "the control that saves")
                .await?;
            journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Saved").partial(),
                    "the screen to announce the save",
                )
                .await;
            let address = journey
                .address_carrying(
                    "id=",
                    "the address to name the value set the server created",
                )
                .await;

            the_definition_survives(&journey, &deployment.base, &address).await?;

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// One include is composed with the keyboard alone.
///
/// Every control is reached by tabbing and operated by typing or by Enter, so
/// nothing on the screen is mouse-only (<https://www.w3.org/TR/WCAG22/#keyboard>).
#[tokio::test]
async fn one_include_is_composed_with_the_keyboard_alone() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_composer(
                &journey,
                &deployment,
                "writer",
                "composes-with-the-keyboard",
            )
            .await;

            journey.tab_to("compose-url", "the canonical field").await;
            journey.type_here(&keyboard_canonical()).await?;
            // The harness reports the control the keyboard is on as its id,
            // its name, and its own words, so the id is one part of that.
            let on = journey.focused().await?;
            assert!(
                on.contains("compose-url"),
                "typing leaves the focus where it was: `{on}`"
            );

            // The picker that adds a published value set is reached by tabbing
            // on from the canonical, and chosen by typing its name.
            journey
                .tab_to(
                    "compose-add-vs-0",
                    "the picker that adds a published value set",
                )
                .await;
            journey.type_here("Every concept").await?;
            let picked = journey
                .element(By::Css(PUBLISHED_PICKER), "the picker")
                .await
                .prop("value")
                .await?
                .unwrap_or_default();
            assert_eq!(
                picked, PUBLISHED,
                "typing the name of an option selects it, with no pointer"
            );

            journey.tab(1).await?;
            let next = journey.focused().await?;
            assert!(
                next.contains("Add"),
                "the control that adds it is the next tab stop: `{next}`"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader who signed in without a write scope is offered no save.
///
/// The other half of the read-only rule, that a resource the server holds no
/// writable record of opens read-only whatever the token carries, is pinned by
/// `fhir::compose::tests::a_resource_the_server_stamped_no_version_on_is_read_only`:
/// this deployment publishes no such `ValueSet` to open here.
#[tokio::test]
async fn a_reader_without_the_scope_composes_nothing() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_composer(&journey, &deployment, "reader", "composes-nothing").await;

            let said = journey
                .element(By::XPath(READ_ONLY), "the notice that nothing can be saved")
                .await
                .text()
                .await?;
            assert!(
                said.contains("no permission"),
                "the reader is told why nothing can be saved: `{said}`"
            );
            assert_eq!(
                journey.count(By::XPath(SAVE)).await,
                0,
                "a reader with no write scope is offered no save"
            );

            // The form itself still takes what they type, because composing
            // and previewing are reads; only the save is the write.
            let enabled = journey
                .element(By::Css(URL_FIELD), "the canonical field")
                .await
                .is_enabled()
                .await?;
            assert!(
                enabled,
                "a new draft is composed without a write scope, and only the save is gated"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console that offers no sign-in carries no composer and no way to reach
/// it, since nobody there can save.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_composer() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            journey
                .element(By::Css("header a.fui-lockup"), "the shell mark")
                .await;
            assert_eq!(
                journey.count(By::XPath(COMPOSER_LINK)).await,
                0,
                "the sidebar of a console without a sign-in leads to no composer"
            );

            journey.reopen(&format!("{base}/compose")).await;
            journey
                .element(By::Css("h1"), "the console's answer to an address it lacks")
                .await;
            assert_eq!(
                journey.count(By::Css(URL_FIELD)).await,
                0,
                "a console without a sign-in draws no composer"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
