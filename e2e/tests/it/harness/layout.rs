// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Sizes, heights, hover and screenshots.

use std::path::Path;
use std::time::Instant;
use thirtyfour::prelude::*;

use super::{Journey, NARROWEST_WINDOW, POLL, WAIT};

impl Journey {
    /// Sets the window, and with it the viewport a shot is taken of.
    ///
    /// The session starts at the size [`CHROME_ARGS`] fixes; a capture asks for
    /// a taller one so a whole screen lands in one image
    /// (<https://www.w3.org/TR/webdriver2/#set-window-rect>).
    ///
    /// Chrome keeps a window at least [`NARROWEST_WINDOW`] wide, so a phone
    /// width is emulated over the Chrome `DevTools` Protocol instead
    /// (<https://chromedevtools.github.io/devtools-protocol/tot/Emulation/#method-setDeviceMetricsOverride>).
    pub async fn resize(&self, width: u32, height: u32) {
        let devtools = self.driver.cdp();
        let window = width.max(NARROWEST_WINDOW);
        self.driver
            .set_window_rect(0, 0, window, height)
            .await
            .unwrap_or_else(|error| panic!("resizing to {width}x{height}: {error}"));
        let emulated = if width < NARROWEST_WINDOW {
            devtools
                .send_raw(
                    "Emulation.setDeviceMetricsOverride",
                    serde_json::json!({
                        "width": width,
                        "height": height,
                        "deviceScaleFactor": 1,
                        "mobile": true,
                    }),
                )
                .await
        } else {
            devtools
                .send_raw(
                    "Emulation.clearDeviceMetricsOverride",
                    serde_json::json!({}),
                )
                .await
        };
        emulated.unwrap_or_else(|error| panic!("emulating {width}x{height}: {error}"));
    }

    /// How tall the rendered document is, in CSS pixels: the root's scroll
    /// height, which is the taller of the content and the viewport.
    ///
    /// The body's box is no measure here, since the shell sizes it to the
    /// viewport and lets the content overflow it
    /// (<https://drafts.csswg.org/cssom-view/#dom-element-scrollheight>).
    pub async fn page_height(&self) -> WebDriverResult<u32> {
        let height = self
            .evaluate("return String(document.documentElement.scrollHeight);")
            .await?;
        Ok(height.trim().parse().unwrap_or_else(|error| {
            panic!("the document reported a height `{height}` that is no number: {error}")
        }))
    }

    /// Waits until the page's fonts have loaded and its height holds over
    /// three readings in a row, so a capture measures the page whole and not
    /// a list still arriving; fails the journey after [`WAIT`].
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused a reading.
    pub async fn steady_height(&self, what: &str) -> WebDriverResult<u32> {
        let deadline = Instant::now() + WAIT;
        let mut last = None;
        let mut held = 0_u8;
        loop {
            let fonts = self.evaluate("return document.fonts.status;").await?;
            let height = self.page_height().await?;
            if fonts.trim() == "loaded" && last == Some(height) {
                held = held.saturating_add(1);
                if held >= 2 {
                    return Ok(height);
                }
            } else {
                held = 0;
            }
            last = Some(height);
            if Instant::now() >= deadline {
                let reason = format!("the page height never held, last {height}");
                panic!("{}", self.failure(what, &reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Sets the cookie `name` to `value` on the page's own origin, the way a
    /// form post of the console's preferences would.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused the cookie.
    pub async fn set_cookie(&self, name: &str, value: &str) -> WebDriverResult<()> {
        let mut cookie = Cookie::new(name.to_owned(), value.to_owned());
        cookie.set_path("/");
        self.driver.add_cookie(cookie).await
    }

    /// Moves the pointer onto the element matching `selector`, so a capture
    /// shows its hover state.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused the move.
    pub async fn hover(&self, selector: By, what: &str) -> WebDriverResult<()> {
        let element = self.visible(selector, what).await;
        self.driver
            .action_chain()
            .move_to_element_center(&element)
            .perform()
            .await
    }

    /// Waits until the page shows no spinner, skeleton or busy region, so a
    /// capture is never of a screen still answering.
    pub async fn settled(&self, what: &str) {
        self.count_becoming(
            By::Css(".fui-spinner-ring, .fui-skeleton, [aria-busy='true']"),
            0,
            what,
        )
        .await;
    }

    /// Writes a PNG of what the browser is showing to `path`.
    ///
    /// The image is the viewport, whose size the session fixes on the command
    /// line, so a capture taken twice is the same size twice. WebDriver hands
    /// the picture back as base64 over the wire
    /// (<https://www.w3.org/TR/webdriver2/#take-screenshot>), so the file is
    /// written where the journeys run rather than inside the browser.
    pub async fn shot(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("creating {}: {error}", parent.display()));
        }
        self.driver
            .screenshot(path)
            .await
            .unwrap_or_else(|error| panic!("writing {}: {error}", path.display()));
        println!("captured {}", path.display());
    }
}
