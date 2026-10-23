// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What a reader who signs in gets, driven through a real identity provider.
//!
//! The deployment here is the second console the harness starts: one whose
//! `[oidc]` names the stub issuer, over a server that trusts the same issuer,
//! so the console signs the reader in on its own server with the
//! authorization code flow and PKCE and calls the server with the token
//! (<https://openid.net/specs/openid-connect-core-1_0.html#CodeFlowAuth>).
//! The journeys in `sign_in` drive the other console, which offers none.
//!
//! Each journey chooses what it is granted by opening the issuer's `/profile`
//! address first, and tags itself so its own revocations are what it reads
//! back. The sign-in itself shows nothing and asks nothing: the issuer
//! redirects straight back with a code, because what these journeys are about
//! is what the console does with what comes back.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::choose;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// The control that starts a sign-in.
const SIGN_IN: &str = "//a[normalize-space()='Sign in']";

/// The button that opens the reader's menu.
const MENU: &str = "button[popovertarget='user-menu']";

/// The line under the reader's name in that menu, which says what they may
/// change.
const STANDING: &str = "#user-menu .fui-menu-header span";

/// The control that ends a sign-in, inside the menu.
const SIGN_OUT: &str = "//button[normalize-space()='Sign out']";

/// A control that saves a resource, on any authoring screen.
const SAVE: &str = "//button[starts-with(normalize-space(), 'Save this') \
                    or starts-with(normalize-space(), 'Create this')]";

/// The code system the deployment loads at start.
const TAXONOMY_PAGE: &str =
    "/systems/https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy";

/// Opens the signed-in reader's menu.
async fn open_menu(journey: &Journey) {
    journey
        .press(By::Css(MENU), "the signed-in reader's menu button")
        .await
        .unwrap_or_else(|error| panic!("the menu button refused the press: {error}"));
}

/// The `fhirUser` the stub issuer signs everyone in as.
const WHO: &str = "Practitioner/e2e-terminologist";

/// A sign-in completes without anything being typed, and the shell says who
/// is signed in and what they may change.
///
/// The name is the `fhirUser` the identity token carries, which is the FHIR
/// identity of the person
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_completed_sign_in_names_the_reader_and_what_they_may_edit() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "names-the-reader").await;
            let named = sign_in(&journey, &deployment).await;
            assert!(named.contains(WHO), "the shell names the reader: `{named}`");

            open_menu(&journey).await;
            let editable = journey
                .text_becoming(
                    By::Css(STANDING),
                    StringMatch::new("May edit").partial(),
                    "the menu to say what this account may change",
                )
                .await;
            for resource_type in ["CodeSystem", "ValueSet", "ConceptMap"] {
                assert!(
                    editable.contains(resource_type),
                    "{resource_type} is in the granted scopes: `{editable}`"
                );
            }
            assert_eq!(
                journey.count(By::XPath(SIGN_IN)).await,
                0,
                "the sign-in control gives way to the signed-in chrome"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A reader the issuer grants no write scope is told so, and is offered
/// nothing to change.
///
/// The console draws a control only where the granted scopes open it, so a
/// grant of `openid` and `fhirUser` alone leaves the chrome read-only
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_reader_without_a_write_scope_is_offered_nothing_to_change() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "reader", "no-write-scope").await;
            let named = sign_in(&journey, &deployment).await;
            assert!(named.contains(WHO), "the reader is signed in: `{named}`");

            open_menu(&journey).await;
            let said = journey
                .text_becoming(
                    By::Css(STANDING),
                    StringMatch::new("Read only").partial(),
                    "the menu to say this account changes nothing",
                )
                .await;
            assert!(
                said.contains("no editing permission"),
                "the reader is told why: `{said}`"
            );
            journey
                .reopen(&format!("{}/codesystem", deployment.base))
                .await;
            journey
                .text_becoming(
                    By::Css("main h1, body h1"),
                    StringMatch::new("Edit a code system").partial(),
                    "the editor to say this account may not do this",
                )
                .await;
            assert_eq!(
                journey.count(By::XPath(SAVE)).await,
                0,
                "nothing offers this account a save"
            );

            // The editor answers a reader with 403 Forbidden, the refusal the
            // journey asked for.
            journey
                .no_console_errors_but(&["/codesystem - Failed to load resource: the server responded with a status of 403"])
                .await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Signing out revokes the token at the issuer and ends the session here.
///
/// RFC 7009 §2.1 has the client tell the issuer the credential is finished
/// with. The console's server holds the token, so it sends the revocation as
/// it ends the session; the issuer's own count is what proves it was sent.
#[tokio::test]
async fn signing_out_revokes_the_token_and_drops_it() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            // The count below is exact, so the tag is this run's own: an
            // issuer left running from an earlier run still holds the last
            // one's revocations, and nextest gives each test its own process.
            let tag = format!("revokes-on-sign-out-{}", std::process::id());
            let tag = tag.as_str();
            choose(&journey, &deployment, "writer", tag).await;
            let named = sign_in(&journey, &deployment).await;
            assert!(named.contains(WHO), "the reader is signed in: `{named}`");

            open_menu(&journey).await;
            let out = journey
                .visible(By::XPath(SIGN_OUT), "the sign-out control")
                .await;
            journey
                .press_on(&out, "the sign-out control")
                .await
                .unwrap_or_else(|error| {
                    panic!("the sign-out control refused the press: {error}");
                });
            journey
                .text_becoming(
                    By::Css("body"),
                    StringMatch::new("You are signed out").partial(),
                    "the sign-in page to say the session ended",
                )
                .await;
            assert_eq!(
                journey
                    .count(By::XPath("//*[contains(text(), 'Practitioner/')]"))
                    .await,
                0,
                "nothing still names the reader who signed out"
            );

            journey.no_console_errors().await;

            journey
                .reopen(&format!("{}/revocations?tag={tag}", deployment.issuer))
                .await;
            let counted = journey
                .text_becoming(
                    By::Css("#revocations"),
                    StringMatch::new("revoked 1").partial(),
                    "the issuer to have been told the token is finished with",
                )
                .await;
            assert!(
                counted.contains(tag),
                "this journey's own count: `{counted}`"
            );
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A visitor reads without signing in, is offered the sign-in, and is
/// offered no control that writes.
///
/// The server's read surface asks for no token, so the console reads every
/// screen for a visitor; a save would be refused, so none is drawn.
#[tokio::test]
async fn a_visitor_is_offered_the_sign_in_and_nothing_that_writes() {
    let Some(deployment) = signed_in() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, TAXONOMY_PAGE).await;
            let offered = journey
                .element(By::XPath(SIGN_IN), "the sign-in control")
                .await;
            let href = offered.attr("href").await?.unwrap_or_default();
            assert!(
                href.starts_with("/auth/sign-in?back="),
                "the control opens the console's own sign-in, returning here: {href}"
            );
            assert_eq!(
                journey
                    .count(By::XPath("//main//a[normalize-space()='Edit']"))
                    .await,
                0,
                "a visitor is offered no edit"
            );
            assert_eq!(
                journey.count(By::XPath(SAVE)).await,
                0,
                "a visitor is offered no save"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
