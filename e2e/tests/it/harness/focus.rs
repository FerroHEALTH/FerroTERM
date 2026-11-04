// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keyboard: tabbing, focus and typing.

use std::time::Instant;
use thirtyfour::common::keys::TypingData;
use thirtyfour::prelude::*;

use super::{FOCUSED, Journey, POLL, TAB_LIMIT, WAIT};

impl Journey {
    /// Presses tab `times`, the way a keyboard reader walks a screen.
    ///
    /// The presses go in one WebDriver call. A walk that read the page back
    /// after each one spent two round trips per control, and the accessibility
    /// pass presses tab several hundred times.
    pub async fn tab(&self, times: usize) -> WebDriverResult<()> {
        let presses = TypingData::from(Key::Tab).to_string().repeat(times);
        self.driver.active_element().await?.send_keys(presses).await
    }

    /// What the keyboard is on right now, as the control's own name.
    ///
    /// WebDriver has no primitive for the element focus landed on, which is
    /// why this is one of the readings taken through a script.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser does not answer.
    pub async fn focused(&self) -> WebDriverResult<String> {
        self.evaluate(FOCUSED).await
    }

    /// Types `text` wherever the keyboard is, with no pointer involved.
    ///
    /// The keys go through the keyboard input source, as a reader's do, so
    /// focus left on a dialog that just closed still takes them
    /// (<https://www.w3.org/TR/webdriver2/#key-actions>). A leading Control
    /// is held down over the rest, as a chord.
    ///
    /// A key a module answers does so only once the loader has run, and the
    /// last press may have loaded a page whose controls are on screen before
    /// then, so the keys wait for the page. A key that submits a form returns
    /// once the page it loads is ready, as a press does.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused the keys.
    pub async fn type_here(&self, text: &str) -> WebDriverResult<()> {
        self.ready().await;
        let chain = self.driver.action_chain();
        self.acting("the keys typed", || async move {
            match text.strip_prefix('\u{e009}') {
                Some(chord) => {
                    chain
                        .key_down(Key::Control)
                        .send_keys(chord)
                        .key_up(Key::Control)
                        .perform()
                        .await
                }
                None => chain.send_keys(text).perform().await,
            }
        })
        .await
    }

    /// Waits until the keyboard is on the control `wanted` names, as a page
    /// that gives a field the focus back puts it there once its module runs.
    pub async fn focus_becoming(&self, wanted: &str, what: &str) {
        self.ready().await;
        let deadline = Instant::now() + WAIT;
        loop {
            let on = self.focused().await.unwrap_or_default();
            if on.contains(wanted) {
                return;
            }
            if Instant::now() >= deadline {
                let reason = format!("the keyboard is on `{on}`");
                panic!("{}", self.failure(what, &reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Presses tab until the keyboard is on the control `wanted` names.
    ///
    /// The walk is bounded, so a control the keyboard never reaches fails the
    /// journey with what it did reach rather than looping.
    pub async fn tab_to(&self, wanted: &str, what: &str) {
        let mut walked = Vec::new();
        for _ in 0..TAB_LIMIT {
            let on = self.focused().await.unwrap_or_default();
            if on.contains(wanted) {
                return;
            }
            walked.push(on);
            if self.tab(1).await.is_err() {
                break;
            }
        }
        let reason = format!("the keyboard walked over {walked:?} and never reached it");
        panic!("{}", self.failure(what, &reason).await);
    }

    /// How tall the viewport is, in CSS pixels.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser does not answer.
    pub async fn viewport_height(&self) -> WebDriverResult<f64> {
        let read = self.evaluate("return String(window.innerHeight);").await?;
        Ok(read.trim().parse::<f64>().unwrap_or_default())
    }
}
