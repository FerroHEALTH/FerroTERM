// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What the console offers when it configures no identity provider.
//!
//! A console with no `[oidc]` can complete no sign-in and offers none; it
//! reads every screen, as the server's read surface asks for no token. The
//! battery's first console is exactly that deployment, which is what these
//! journeys drive.

use thirtyfour::prelude::*;
use thirtyfour::stringmatch::StringMatch;

use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// The control that starts a sign-in, wherever on the chrome it sits.
const SIGN_IN: &str = "//a[normalize-space()='Sign in']";

/// The control that ends one.
const SIGN_OUT: &str = "//button[normalize-space()='Sign out']";

/// The shell's own mark, which proves the bundle booted.
const LOCKUP: &str = "header a.fui-lockup";

/// The screens a reader can reach, which between them carry every control.
const SCREENS: [&str; 6] = [
    "/",
    "/browse",
    "/expand",
    "/validate",
    "/valuesets",
    "/about",
];

/// A deployment with no issuer offers no sign-in on any screen.
///
/// The control is what every edit control will hang off, so its absence here
/// is the whole of "without an issuer the console stays read only".
#[tokio::test]
async fn no_issuer_means_no_sign_in_control_on_any_screen() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            for screen in SCREENS {
                journey.reopen(&format!("{base}{screen}")).await;
                // The shell is waited for first, so the count below is taken
                // on a booted page rather than on an empty body.
                journey
                    .element(By::Css(LOCKUP), &format!("the shell mark on {screen}"))
                    .await;
                assert_eq!(
                    journey.count(By::XPath(SIGN_IN)).await,
                    0,
                    "{screen} offers a sign-in the server publishes no issuer for"
                );
                assert_eq!(
                    journey.count(By::XPath(SIGN_OUT)).await,
                    0,
                    "{screen} offers a sign-out with nothing signed in"
                );
            }

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The callback address refuses a code that no sign-in of this console
/// started, and says so.
///
/// A reader can reach the address: it is a plain URL, and an identity
/// provider of another deployment could send someone to it. The console
/// matches the `state` against a sign-in it holds before it spends a code
/// (RFC 6749 §10.12), so this one is refused before anything is sent.
#[tokio::test]
async fn the_callback_says_there_is_no_sign_in_to_complete() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/auth/callback?code=abc&state=xyz").await;

            journey
                .text_becoming(
                    By::Css("body"),
                    StringMatch::new("no sign-in is pending").partial(),
                    "the callback to say there is no sign-in to complete",
                )
                .await;
            assert_eq!(
                journey.count(By::Css(LOCKUP)).await,
                0,
                "the refusal is the console's answer, not a screen that signed anyone in"
            );
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
