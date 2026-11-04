// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Opening pages, windows and cookies.

use thirtyfour::prelude::*;

use super::{Journey, LOADED, MODULES, POLL, READY, WAIT};

impl Journey {
    /// Opens `path` on `base` and returns the journey over it.
    pub async fn open(driver: WebDriver, base: &str, path: &str) -> Self {
        Self::opened(driver, base, path, true).await
    }

    /// Opens `path` on `base` in a browser refused every one of FerroUI's
    /// script modules, the loader among them, and returns the journey over
    /// it: the page as a reader whose modules never load reads it.
    pub async fn open_without_scripts(driver: WebDriver, base: &str, path: &str) -> Self {
        let devtools = driver.cdp();
        devtools
            .send_raw("Network.enable", serde_json::json!({}))
            .await
            .unwrap_or_else(|error| panic!("the browser refused its network domain: {error}"));
        devtools
            .send_raw(
                "Network.setBlockedURLs",
                serde_json::json!({ "urls": [MODULES] }),
            )
            .await
            .unwrap_or_else(|error| panic!("the browser refused to block the modules: {error}"));
        Self::opened(driver, base, path, false).await
    }

    /// Opens `path` on `base`, the browser loading the modules when
    /// `scripts`.
    pub(super) async fn opened(driver: WebDriver, base: &str, path: &str, scripts: bool) -> Self {
        let address = format!("{base}{path}");
        driver
            .goto(address.clone())
            .await
            .unwrap_or_else(|error| panic!("the browser could not open {address}: {error}"));
        let journey = Self { driver, scripts };
        journey.ready().await;
        journey
    }

    /// Waits until the page in front of the browser is ready for the next
    /// step: loaded, and on a console page with its modules running.
    ///
    /// The server sends a whole page, so its controls are on screen before
    /// the modules that answer them have run; a key typed in between reaches
    /// the field and never the module. FerroUI's loader marks `<html>` with
    /// `data-fui-script` once it has run, and each enhanced element with
    /// `data-fui-ready`. A page that is not the console's
    /// (the issuer's, a refusal in plain text) is not waited on, nor is a
    /// page whose browser refuses the modules, past its load.
    pub(super) async fn ready(&self) {
        // The page is read whole on every poll, so a navigation still settling
        // when the wait starts is judged on the page it lands on, and a script
        // the navigation interrupts reads as not yet ready.
        let script = if self.scripts { READY } else { LOADED };
        let started = std::time::Instant::now();
        while started.elapsed() <= WAIT {
            let ready = self
                .driver
                .execute(script, Vec::new())
                .await
                .is_ok_and(|answer| answer.convert::<bool>().is_ok_and(|ready| ready));
            if ready {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        panic!(
            "{}",
            self.failure(
                "the page to be ready",
                "no data-fui-script or data-fui-ready mark"
            )
            .await
        );
    }

    /// Opens `address` in the same session, as a fresh load.
    ///
    /// A click navigation and a typed address reach the router differently, so
    /// a journey that only clicks never exercises the address a reader shares.
    pub async fn reopen(&self, address: &str) {
        self.driver
            .goto(address)
            .await
            .unwrap_or_else(|error| panic!("the browser could not open {address}: {error}"));
        self.ready().await;
    }

    /// Drops every cookie the console set in this browser, so the next load
    /// is a visitor's: the sign-in lives in the console's session cookie.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused.
    pub async fn drop_cookies(&self) -> WebDriverResult<()> {
        self.driver.delete_all_cookies().await
    }

    /// The window the journey is driving now.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser does not answer.
    pub async fn window(&self) -> WebDriverResult<WindowHandle> {
        self.driver.window().await
    }

    /// Opens a second window in the same session and moves the journey into it.
    ///
    /// The windows share the session's cookies and nothing a page holds in
    /// memory, so a second window is a second reader of the same deployment
    /// (<https://www.w3.org/TR/webdriver2/#new-window>).
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused the window.
    pub async fn open_window(&self) -> WebDriverResult<WindowHandle> {
        let opened = self.driver.new_window().await?;
        self.driver.switch_to_window(opened.clone()).await?;
        Ok(opened)
    }

    /// Moves the journey into `window`.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser has no such window.
    pub async fn switch_to(&self, window: &WindowHandle) -> WebDriverResult<()> {
        self.driver.switch_to_window(window.clone()).await
    }
}
