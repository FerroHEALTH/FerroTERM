// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Finding elements, pressing them and waiting for their text and counts.

use std::time::Instant;
use thirtyfour::error::WebDriverErrorInner;
use thirtyfour::prelude::*;
use thirtyfour::stringmatch::Needle;

use super::{Journey, LEFT, POLL, STALE_TRIES, TAG, WAIT};

impl Journey {
    /// The first element matching `selector`, once the page carries it.
    ///
    /// The wait is the library's own poller over the element condition, so
    /// nothing here sleeps for a fixed time and hopes. A press may have loaded
    /// a whole new page, so the wait starts with that page being ready.
    pub async fn element(&self, selector: By, what: &str) -> WebElement {
        // A page a press reloads can replace the element between finding it
        // and centring it, so a stale element is looked up again.
        for _ in 0..STALE_TRIES {
            self.ready().await;
            let element = match self
                .driver
                .query(selector.clone())
                .wait(WAIT, POLL)
                .first()
                .await
            {
                Ok(element) => element,
                Err(error) => panic!("{}", self.failure(what, &error.to_string()).await),
            };
            // NOTE: WebDriver 2, Element Click: a press on an element whose centre
            // is under the sticky top bar is refused, so the element is centred.
            match element.scroll_into_view().await {
                Ok(()) => return element,
                Err(error)
                    if matches!(
                        error.as_inner(),
                        WebDriverErrorInner::StaleElementReference(_)
                    ) => {}
                Err(error) => panic!("{}", self.failure(what, &error.to_string()).await),
            }
        }
        panic!(
            "{}",
            self.failure(what, "the element went stale on every look")
                .await
        )
    }

    /// The first element matching `selector` once the browser draws it.
    ///
    /// An element the page holds hidden, such as a control in a closed menu,
    /// is in the document before it is drawn, and a press on it is refused.
    pub async fn visible(&self, selector: By, what: &str) -> WebElement {
        self.ready().await;
        match self
            .driver
            .query(selector)
            .wait(WAIT, POLL)
            .and_displayed()
            .first()
            .await
        {
            Ok(element) => element,
            Err(error) => panic!("{}", self.failure(what, &error.to_string()).await),
        }
    }

    /// Presses the element matching `selector` once it is on the page, and
    /// returns once the page the press loads, if any, is ready.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused the press.
    pub async fn press(&self, selector: By, what: &str) -> WebDriverResult<()> {
        let element = self.element(selector, what).await;
        self.press_on(&element, what).await
    }

    /// Presses `element`, which a journey already holds, and returns once the
    /// page the press loads, if any, is ready.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused the press.
    pub async fn press_on(&self, element: &WebElement, what: &str) -> WebDriverResult<()> {
        self.acting(what, || element.click()).await
    }

    /// Runs `act`, a press or keys, on a tagged page, and returns once the
    /// page it loads, if any, is ready.
    ///
    /// A click can return while the page it leaves is still in front of the
    /// browser, ready, and carrying the controls the next step looks for, so
    /// every wait after it would pass on a page about to go away. The tag goes
    /// with that document, which is how the next one is told apart.
    pub(super) async fn acting<F, A>(&self, what: &str, act: F) -> WebDriverResult<()>
    where
        F: FnOnce() -> A,
        A: Future<Output = WebDriverResult<()>>,
    {
        self.driver.execute(TAG, Vec::new()).await?;
        act().await?;
        let deadline = Instant::now() + WAIT;
        loop {
            // NOTE: a script the navigation interrupts has no answer, which reads
            // as the page still leaving.
            let left = self
                .driver
                .execute(LEFT, Vec::new())
                .await
                .ok()
                .and_then(|answer| answer.convert::<String>().ok());
            match left.as_deref() {
                Some("staying") => return Ok(()),
                Some("loaded" | "drawn") => {
                    self.ready().await;
                    return Ok(());
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                let reason = "the page the press left never went away";
                panic!("{}", self.failure(what, reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Keeps one preference of Your preferences: the choice card of `key`
    /// carrying `value` is chosen, the `preference` module keeps it on the change,
    /// and the console's answer is waited for on `<html>`, where `drawn`
    /// carries the theme or the density.
    ///
    /// The preferences are the console's cookies, so every screen opened
    /// afterwards in this session reads them.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused a press.
    pub async fn keep_setting(
        &self,
        base: &str,
        key: &str,
        value: &str,
        drawn: &str,
    ) -> WebDriverResult<()> {
        self.reopen(&format!("{base}/preferences")).await;
        let kept = By::Css(format!("html[{drawn}='{value}']"));
        if self.count(kept.clone()).await == 1 {
            return Ok(());
        }
        let choice = self
            .visible(
                By::Css(format!("label:has(input[name='{key}'][value='{value}'])")),
                "the choice of the preference",
            )
            .await;
        self.press_on(&choice, "the choice of the preference")
            .await?;
        self.count_becoming(kept, 1, "the console to draw the kept setting")
            .await;
        self.ready().await;
        Ok(())
    }

    /// The text of the element matching `selector` once `needle` matches it.
    ///
    /// Waiting on the text rather than on the element alone is what makes a
    /// re-render observable: the element is already on the page while it still
    /// carries the previous version's text.
    pub async fn text_becoming<N>(&self, selector: By, needle: N, what: &str) -> String
    where
        N: Needle + Clone + Send + Sync + 'static,
    {
        self.ready().await;
        let deadline = Instant::now() + WAIT;
        loop {
            // NOTE: a press that posts a form loads a new page under the poll,
            // so an element read during the swap is stale and read again.
            if let Ok(found) = self.driver.find_all(selector.clone()).await {
                for element in found {
                    if let Ok(text) = element.text().await
                        && needle.is_match(&text)
                    {
                        return text;
                    }
                }
            }
            if Instant::now() >= deadline {
                let reason = format!("no element matching {selector:?} carried the text");
                panic!("{}", self.failure(what, &reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Every element matching `selector` right now, in document order.
    ///
    /// The reading is of the page as it stands, so a journey takes it only
    /// after a wait has proven the section it describes has settled.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused the query.
    pub async fn all(&self, selector: By) -> WebDriverResult<Vec<WebElement>> {
        self.driver.find_all(selector).await
    }

    /// How many elements match `selector` right now.
    ///
    /// The count is a reading of the page as it stands, so a journey takes it
    /// only after a wait has proven the section it describes has settled.
    pub async fn count(&self, selector: By) -> usize {
        self.driver
            .find_all(selector)
            .await
            .map(|found| found.len())
            .unwrap_or_default()
    }

    /// Waits until `selector` matches `wanted` elements, and says so if it
    /// never does.
    ///
    /// A screen that fills from several reads draws its parts one at a time,
    /// so the first match is on a page that is still filling. Waiting on the
    /// count is what makes the whole of it observable.
    pub async fn count_becoming(&self, selector: By, wanted: usize, what: &str) {
        let deadline = Instant::now() + WAIT;
        loop {
            let found = self.count(selector.clone()).await;
            if found == wanted {
                return;
            }
            if Instant::now() >= deadline {
                let reason = format!("{found} elements match, not {wanted}");
                panic!("{}", self.failure(what, &reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// What one script evaluated to in the page, as the string it returned.
    ///
    /// Every other reading here is a WebDriver primitive, because inert markup
    /// satisfies a source assertion even when the control it describes is
    /// unreachable. Five things have no primitive: the element focus is on
    /// after a key press, the colour a browser actually painted, the height
    /// of the viewport, because Get Window Rect reports the window around it
    /// (<https://www.w3.org/TR/webdriver2/#get-window-rect>), whether a
    /// document has finished loading, which `ready` reads, and whether a
    /// press is loading another page, which `acting` reads. They are read
    /// through a script (<https://www.w3.org/TR/webdriver2/#execute-script>)
    /// and nothing else is.
    pub async fn evaluate(&self, script: &str) -> WebDriverResult<String> {
        self.driver.execute(script, Vec::new()).await?.convert()
    }
}
