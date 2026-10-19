// SPDX-License-Identifier: BUSL-1.1
//! Authoring a local concept map in the editor bundle, and previewing it.
//!
//! Every code on both sides is picked out of the system its group names,
//! through the same `ValueSet/$expand` search the concept browser runs, so the
//! journeys drive the search rather than typing a code the form would take
//! blind. The deployment serves one shaped, synthetic code system
//! (`e2e/fixtures/codesystems`), which is what the pickers search.
//!
//! The element a save writes differs per FHIR version: R4 and R4B state a
//! target's relation in `equivalence` and R5 and R6 in `relationship`
//! (<https://hl7.org/fhir/R4B/conceptmap.html>,
//! <https://hl7.org/fhir/R5/conceptmap.html>), so the round trip runs on both
//! and reads what the wire carries: R4B by reading the saved resource back,
//! R5 by previewing, since `$translate` carries the whole map in its own
//! parameter and a root refuses a resource whose elements it does not define.

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

/// The sidebar link into the map authoring screen.
const AUTHORING_LINK: &str = "//a[normalize-space()='Concept map editor']";

/// The code system the journeys map between.
///
/// It is the shaped fixture this deployment serves, so both pickers search a
/// system the server actually holds and the preview translates a real code.
const TAXONOMY: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// The canonical the R4B round trip authors under.
///
/// Synthetic: the repository distributes no terminology content, and each
/// journey writes the map it then reads back. One canonical per journey,
/// because the journeys run beside each other against one server, and one per
/// run, because a store may persist between runs.
fn r4b_canonical() -> String {
    run_canonical("e2e-map-r4b")
}

/// That canonical, percent-encoded into a query parameter.
fn r4b_canonical_param() -> String {
    query_param(&r4b_canonical())
}

/// The canonical the R5 round trip authors under.
fn r5_canonical() -> String {
    run_canonical("e2e-map-r5")
}

/// The canonical the keyboard journey authors under.
fn keyboard_canonical() -> String {
    run_canonical("e2e-map-keyboard")
}

/// The canonical field of the metadata form.
const URL_FIELD: &str = "#map-url";

/// Every group's source system control.
const GROUP_SOURCES: &str = "input[name='group-source']";

/// Every group's target system control.
const GROUP_TARGETS: &str = "input[name='group-target']";

/// Every code's own control.
const ELEMENT_CODES: &str = "input[name='element-code']";

/// Every target's code control.
const TARGET_CODES: &str = "input[name='target-code']";

/// Every target's relationship control.
const RELATIONSHIPS: &str = "select[name='target-relationship']";

/// The search field of a code's own picker.
const ELEMENT_SEARCH: &str = "input[type='search'][name^='element-']";

/// The search field of a target's picker.
const TARGET_SEARCH: &str = "input[type='search'][name^='target-']";

/// The control that adds a group.
const ADD_GROUP: &str = "//button[normalize-space()='Add a group']";

/// The control that adds a code to a group.
const ADD_CODE: &str = "//button[normalize-space()='Add a code']";

/// The control that adds a target to a code.
const ADD_TARGET: &str = "//button[normalize-space()='Add a target']";

/// The control that runs one preview.
const PREVIEW: &str = "//button[normalize-space()='Preview this code through $translate']";

/// The control that sends the whole resource.
const SAVE: &str = "//button[starts-with(normalize-space(), 'Create this concept map') \
                    or starts-with(normalize-space(), 'Save this concept map')]";

/// The live region the screen announces every outcome in.
const REPORT: &str = "#map-editor-report";

/// The code the journeys map from, and the phrase that finds it.
const SOURCE_CODE: &str = "ca-leaf";

/// The phrase that finds the source code in the taxonomy.
const SOURCE_PHRASE: &str = "First leaf";

/// The code the journeys map to, and the phrase that finds it.
const TARGET_CODE: &str = "bb-branch";

/// The phrase that finds the target code in the taxonomy.
const TARGET_PHRASE: &str = "Second branch";

/// The `value` property of every element matching `selector`.
async fn values(journey: &Journey, selector: &str) -> WebDriverResult<Vec<String>> {
    let mut read = Vec::new();
    for element in journey.all(By::Css(selector)).await? {
        read.push(element.prop("value").await?.unwrap_or_default());
    }
    Ok(read)
}

/// Signs in as a terminologist and opens the map authoring screen.
///
/// `fhir` is the label of the version the journey runs on, reached through
/// the switcher, whose post keeps it as the version an address naming none
/// reads; the sidebar link then lands on it.
async fn open_authoring(journey: &Journey, deployment: &SignedIn, tag: &str, fhir: &str) {
    journey
        .element(By::Css("header a.fui-lockup"), "the shell mark")
        .await;
    choose(journey, deployment, "writer", tag).await;
    sign_in(journey, deployment).await;
    let switch = format!(
        "//*[@role='group'][@aria-label='FHIR version']//button[normalize-space()='{fhir}']"
    );
    journey
        .element(By::XPath(&switch), "the version this journey runs on")
        .await
        .click()
        .await
        .unwrap_or_else(|error| panic!("the version switcher refused the press: {error}"));
    journey
        .text_becoming(
            By::Css("[role='group'][aria-label='FHIR version'] button[aria-pressed='true']"),
            fhir.to_owned(),
            "the switcher to mark the version this journey runs on",
        )
        .await;
    journey
        .address_carrying(
            &format!("fhir={}", fhir.to_lowercase()),
            "the switcher's post to return to the page on that version",
        )
        .await;
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

/// Types a phrase into `search`, runs it, and presses the offer it found.
///
/// The offer is a real control the server's own `$expand` produced, so this is
/// what proves a code arrives with its display rather than typed blind.
async fn pick(journey: &Journey, search: &str, phrase: &str) -> WebDriverResult<()> {
    journey
        .element(By::Css(search), "the picker's search field")
        .await
        .send_keys(phrase)
        .await?;
    journey
        .element(By::Css(search), "the picker's search field")
        .await
        .send_keys(TypingData::from(Key::Enter).to_string())
        .await?;
    let offer = format!("//button[starts-with(normalize-space(), '{phrase}')]");
    journey
        .element(By::XPath(&offer), "the code the search found")
        .await
        .click()
        .await?;
    Ok(())
}

/// Authors one group with one mapping, both codes chosen through search.
async fn author(journey: &Journey, canonical: &str, relationship: &str) -> WebDriverResult<()> {
    journey
        .element(By::Css(URL_FIELD), "the canonical field")
        .await
        .send_keys(canonical)
        .await?;
    journey
        .element(By::XPath(ADD_GROUP), "the control that adds a group")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(GROUP_SOURCES), 1, "the group row")
        .await;
    journey
        .element(By::Css(GROUP_SOURCES), "the group's source system")
        .await
        .send_keys(TAXONOMY)
        .await?;
    journey
        .element(By::Css(GROUP_TARGETS), "the group's target system")
        .await
        .send_keys(TAXONOMY)
        .await?;

    journey
        .element(By::XPath(ADD_CODE), "the control that adds a code")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(ELEMENT_CODES), 1, "the code row")
        .await;
    pick(journey, ELEMENT_SEARCH, SOURCE_PHRASE).await?;
    assert_eq!(
        values(journey, ELEMENT_CODES).await?,
        vec![SOURCE_CODE.to_owned()],
        "the source code came out of the search rather than the keyboard"
    );

    journey
        .element(By::XPath(ADD_TARGET), "the control that adds a target")
        .await
        .click()
        .await?;
    journey
        .count_becoming(By::Css(TARGET_CODES), 1, "the target row")
        .await;
    pick(journey, TARGET_SEARCH, TARGET_PHRASE).await?;
    assert_eq!(
        values(journey, TARGET_CODES).await?,
        vec![TARGET_CODE.to_owned()],
        "the target code came out of the search too"
    );

    // The option is the served version's own: the control offers what the root
    // expanded the element's value set to, so pressing one by value proves the
    // code came off the wire rather than out of the bundle.
    journey
        .element(By::Css(RELATIONSHIPS), "the relationship control")
        .await
        .find(By::Css(format!("option[value='{relationship}']")))
        .await?
        .click()
        .await?;
    Ok(())
}

/// The console entry a raw FHIR navigation provokes.
///
/// A JSON document carries no `<link rel="icon">`, so the browser asks for the
/// origin's `/favicon.ico` and the server answers `404`. It belongs to
/// [`stored_resource`] and to nothing on any screen, so the journeys that read
/// the wire exempt exactly this one and still fail on anything else.
const RAW_FAVICON: &str = "/favicon.ico - Failed to load resource";

/// The resource as the server holds it, read straight off the FHIR API.
///
/// `_format` names the representation, so a browser navigation gets FHIR JSON
/// rather than whatever its own `Accept` header would negotiate
/// (<https://hl7.org/fhir/R4B/http.html#mime-type>). Reading it is what proves
/// which element the save wrote, which no control on the screen can show.
async fn stored_resource(journey: &Journey, base: &str, canonical: &str) -> String {
    journey
        .reopen(&format!("{base}/conceptmaps?fhir=r4b&url={canonical}"))
        .await;
    journey
        .press(
            By::Css("section[aria-labelledby='conceptmaps-heading'] tbody th a"),
            "the stored map in the listing",
        )
        .await
        .unwrap_or_else(|error| panic!("opening the stored map: {error}"));
    journey
        .press(
            By::XPath(
                "//nav[@aria-label='Views of this concept map']//a[normalize-space()='JSON']",
            ),
            "the stored map's JSON tab",
        )
        .await
        .unwrap_or_else(|error| panic!("opening the JSON tab: {error}"));
    journey
        .text_becoming(
            By::Css("#conceptmap-json-highlighted"),
            StringMatch::new("ConceptMap").partial(),
            "the resource the server holds",
        )
        .await
}

/// Reads the saved map back as a visitor, with the console's cookies dropped.
///
/// What comes back is the server's own answer seen the way a reader who never
/// signed in sees it, which is also what a built or national map looks like:
/// the form is there and the save control is not.
async fn the_map_comes_back_read_only(journey: &Journey, base: &str) -> WebDriverResult<()> {
    journey.drop_cookies().await?;
    journey
        .reopen(&format!(
            "{base}/conceptmap?fhir=r4b&map={}",
            r4b_canonical_param()
        ))
        .await;
    journey
        .count_becoming(By::Css(TARGET_CODES), 1, "the saved map to be read back")
        .await;
    assert_eq!(
        values(journey, TARGET_CODES).await?,
        vec![TARGET_CODE.to_owned()],
        "the target the search picked came back off the server"
    );
    assert_eq!(
        values(journey, RELATIONSHIPS).await?,
        vec!["equivalent".to_owned()],
        "the relationship survived the round trip"
    );
    assert_eq!(
        journey.count(By::XPath(SAVE)).await,
        0,
        "a visitor is offered no save"
    );

    let stored = stored_resource(journey, base, &r4b_canonical_param()).await;
    assert!(
        stored.contains("equivalence"),
        "R4B states a target's relation in `equivalence`: `{stored}`"
    );
    assert!(
        !stored.contains("\"relationship\""),
        "and never in R5's element: `{stored}`"
    );
    Ok(())
}

/// Translates the source code through the map that was just saved.
///
/// It is the same operation the preview ran inline, over the map the server
/// now holds, so a reader who was sent the address sees what the author saw.
async fn the_runner_translates_through_it(journey: &Journey, base: &str) {
    journey
        .reopen(&format!(
            "{base}/translate?fhir=r4b&map={}\
             &system=https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy\
             &code={SOURCE_CODE}",
            r4b_canonical_param()
        ))
        .await;
    let translated = journey
        .text_becoming(
            // The section outlives the answer inside it: waiting on a child the
            // answer replaces reads a stale element the moment it re-renders.
            By::Css("section[aria-labelledby='translate-answer-heading']"),
            StringMatch::new(TARGET_CODE).partial(),
            "the runner to translate through the saved map",
        )
        .await;
    assert!(translated.contains(TARGET_CODE), "`{translated}`");
}

/// A whole local map is authored, previewed, saved, and read back on R4B.
#[tokio::test]
async fn a_local_concept_map_is_authored_previewed_and_saved_on_r4b() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "authors-a-map-r4b", "R4B").await;

            // `equivalent` is a code of the value set R4B binds
            // `target.equivalence` to, which the control offered because the
            // server expanded it.
            author(&journey, &r4b_canonical(), "equivalent").await?;

            journey
                .element(By::XPath(PREVIEW), "the control that previews")
                .await
                .click()
                .await?;
            let previewed = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Translated through").partial(),
                    "the screen to announce the preview",
                )
                .await;
            assert!(
                previewed.contains("the map on this screen"),
                "the preview went through the unsaved map: `{previewed}`"
            );
            let shown = journey
                .element(
                    By::XPath("//table[.//th[normalize-space()='Relationship']]"),
                    "the preview",
                )
                .await
                .text()
                .await?;
            assert!(
                shown.contains(TARGET_CODE),
                "the preview names the target the map maps to: `{shown}`"
            );

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

            the_map_comes_back_read_only(&journey, &deployment.base).await?;
            the_runner_translates_through_it(&journey, &deployment.base).await;

            journey.no_console_errors_but(&[RAW_FAVICON]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The same map on R5, where the element and its value set both differ.
///
/// The R5 shape is proven over the wire by the preview rather than by a save.
/// `$translate` carries the map itself in the `conceptMap` parameter, so the
/// server reads the whole resource: an R5 root refuses `target.equivalence` as
/// an unknown property, and an answer means the form wrote `relationship` and
/// `sourceScope` instead. The relationship code is one R4B's own value set
/// does not contain, so the control offered the served version's codes.
// TODO(#659): save here as well, once a resource written on one served version
// stops failing the other versions' type-level search.
#[tokio::test]
async fn the_same_map_writes_r5_s_own_relationship_element() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "authors-a-map-r5", "R5").await;

            author(&journey, &r5_canonical(), "source-is-narrower-than-target").await?;

            journey
                .element(By::XPath(PREVIEW), "the control that previews")
                .await
                .click()
                .await?;
            let previewed = journey
                .text_becoming(
                    By::Css(REPORT),
                    StringMatch::new("Translated through").partial(),
                    "the screen to announce the preview",
                )
                .await;
            assert!(
                previewed.contains("the map on this screen"),
                "an R5 root took the map this form wrote: `{previewed}`"
            );
            let shown = journey
                .element(
                    By::XPath("//table[.//th[normalize-space()='Relationship']]"),
                    "the preview",
                )
                .await
                .text()
                .await?;
            assert!(
                shown.contains(TARGET_CODE),
                "the preview names the target the map maps to: `{shown}`"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// One mapping is authored with the keyboard alone.
///
/// Every step is a tab press or a typed character, so nothing on the screen is
/// reachable only with a pointer (<https://www.w3.org/TR/WCAG22/#keyboard>).
#[tokio::test]
async fn one_mapping_is_authored_with_the_keyboard_alone() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            open_authoring(&journey, &deployment, "keyboard-map", "R4B").await;

            journey.tab_to("map-url", "the canonical field").await;
            journey.type_here(&keyboard_canonical()).await?;

            journey
                .tab_to("Add a group", "the control that adds a group")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;
            journey
                .count_becoming(By::Css(GROUP_SOURCES), 1, "the group the key press added")
                .await;

            journey
                .tab_to("group-source", "the group's source system")
                .await;
            journey.type_here(TAXONOMY).await?;
            journey
                .tab_to("group-target", "the group's target system")
                .await;
            journey.type_here(TAXONOMY).await?;

            journey
                .tab_to("Add a code", "the control that adds a code")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;
            journey
                .count_becoming(By::Css(ELEMENT_CODES), 1, "the code the key press added")
                .await;

            // The picker is driven from the keyboard too: the phrase is typed
            // and Enter submits the search, which is what a form does.
            // The walk matches the search field by its own id suffix: tabbing to
            // "element-" would stop on the code control, which comes first.
            journey
                .tab_to("-search", "the code's own search field")
                .await;
            journey.type_here(SOURCE_PHRASE).await?;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;
            let offer = format!("//button[starts-with(normalize-space(), '{SOURCE_PHRASE}')]");
            journey
                .element(By::XPath(&offer), "the code the search found")
                .await;
            journey
                .tab_to(SOURCE_PHRASE, "the offer the search drew")
                .await;
            journey
                .type_here(&TypingData::from(Key::Enter).to_string())
                .await?;

            assert_eq!(
                values(&journey, ELEMENT_CODES).await?,
                vec![SOURCE_CODE.to_owned()],
                "the code was chosen with the keyboard alone"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A console that offers no sign-in carries no map authoring screen and no
/// way to reach one.
#[tokio::test]
async fn a_console_without_a_sign_in_carries_no_map_authoring_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/conceptmap?fhir=r4b").await;
            journey
                .element(By::Css("h1"), "the console's answer to an address it lacks")
                .await;
            assert_eq!(
                journey.count(By::Css(URL_FIELD)).await,
                0,
                "a console without a sign-in draws no map authoring form"
            );

            journey.reopen(&format!("{base}/?fhir=r4b")).await;
            journey
                .element(By::Css("header a.fui-lockup"), "the shell mark")
                .await;
            assert_eq!(
                journey.count(By::XPath(AUTHORING_LINK)).await,
                0,
                "the sidebar of a console without a sign-in leads to no map authoring screen"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
