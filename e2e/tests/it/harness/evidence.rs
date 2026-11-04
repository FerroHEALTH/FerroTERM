// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The address, the console and the evidence a failed wait leaves.

use std::path::PathBuf;
use std::time::Instant;

use super::{
    DEFAULT_FAILURES, FAILURES_ENV, Journey, MARKUP_IN_A_FAILURE, POLL, SEVERE, WAIT, slug,
};

impl Journey {
    /// The address the browser is on.
    pub async fn address(&self) -> String {
        self.driver
            .current_url()
            .await
            .expect("the browser reports the address it is on")
            .to_string()
    }

    /// The address, once it carries `needle`.
    ///
    /// A key press that navigates does so a paint after the press, so an
    /// address read straight afterwards is the one before it. The wait polls
    /// the same way the element waits do, and reports what the browser logged
    /// when the address never arrives.
    pub async fn address_carrying(&self, needle: &str, what: &str) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let address = self.address().await;
            if address.contains(needle) {
                return address;
            }
            if Instant::now() >= deadline {
                let reason =
                    format!("the address is still `{address}`, which carries no `{needle}`");
                panic!("{}", self.failure(what, &reason).await);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Everything the browser logged as severe since the last read.
    ///
    /// Each read drains the buffer, so a journey reads it only where it
    /// reports it. Nothing is passed over: a failed request for an asset or a
    /// code is exactly the defect a rendering test is here to catch, and the
    /// document declares its own icon, so the browser issues no probe of its
    /// own (<https://html.spec.whatwg.org/multipage/links.html#rel-icon>).
    pub(super) async fn console_errors(&self) -> Vec<String> {
        match self.driver.browser_log().await {
            Ok(entries) => entries
                .iter()
                .filter(|entry| entry.level == SEVERE)
                .map(|entry| {
                    let source = entry.source.as_deref().unwrap_or("unattributed");
                    format!("[{source}] {}", entry.message)
                })
                .collect(),
            Err(error) => vec![format!("the browser log could not be read: {error}")],
        }
    }

    /// The message for a wait that ran out, with what the browser logged.
    ///
    /// A boot failure shows up as an element that never appears, so the
    /// console goes into the message: without it the report says only that a
    /// selector did not match.
    pub(super) async fn failure(&self, what: &str, reason: &str) -> String {
        let console = self.console_errors().await;
        let logged = if console.is_empty() {
            "the browser logged nothing severe".to_owned()
        } else {
            format!("the browser logged:\n  {}", console.join("\n  "))
        };
        // A selector that matched nothing says nothing about why. The address
        // and the markup say which page was actually open and what it held, so
        // a failure that cannot be reproduced locally is still diagnosable
        // from the log alone.
        let address = self.driver.current_url().await.map_or_else(
            |error| format!("(the address could not be read: {error})"),
            |url| url.to_string(),
        );
        let source = self.driver.source().await;
        let markup = match &source {
            Ok(text) => {
                let trimmed: String = text.chars().take(MARKUP_IN_A_FAILURE).collect();
                format!("the page held:\n{trimmed}")
            }
            Err(error) => format!("(the markup could not be read: {error})"),
        };
        let evidence = self.evidence(what, source.ok().as_deref()).await;
        format!(
            "waiting for {what} failed: {reason}\nthe address was {address}\n\
             {logged}\n{evidence}\n{markup}"
        )
    }

    /// Writes a screenshot and the whole document to the failure directory.
    ///
    /// The `ui-e2e` CI job uploads that directory when the run fails, so a
    /// failure that reproduces only on the runner is looked at rather than
    /// re-run. Nothing here is allowed to raise: a browser that cannot answer
    /// a screenshot request is already the failure being reported, and a
    /// second panic on the way out would hide it. Each outcome is returned as
    /// a line of the report instead.
    pub(super) async fn evidence(&self, what: &str, source: Option<&str>) -> String {
        let dir = std::env::var(FAILURES_ENV)
            .map_or_else(|_absent| PathBuf::from(DEFAULT_FAILURES), PathBuf::from);
        if let Err(error) = std::fs::create_dir_all(&dir) {
            return format!("(no evidence written: creating {}: {error})", dir.display());
        }
        // Two journeys can fail on the same wait, and nextest gives each test
        // its own process, so the pid is what keeps one from overwriting the
        // other's evidence.
        let stem = format!("{}-{}", slug(what), std::process::id());
        let shot = dir.join(format!("{stem}.png"));
        let page = dir.join(format!("{stem}.html"));
        let mut written = Vec::new();
        match self.driver.screenshot(&shot).await {
            Ok(()) => written.push(format!("a screenshot in {}", shot.display())),
            Err(error) => written.push(format!("(no screenshot: {error})")),
        }
        match source {
            Some(text) => match std::fs::write(&page, text) {
                Ok(()) => written.push(format!("the whole document in {}", page.display())),
                Err(error) => written.push(format!(
                    "(no document: writing {}: {error})",
                    page.display()
                )),
            },
            None => written.push("(no document: the markup could not be read)".to_owned()),
        }
        format!("the failure left {}", written.join(", and "))
    }

    /// Fails the journey when the browser logged anything severe.
    pub async fn no_console_errors(&self) {
        self.no_console_errors_but(&[]).await;
    }

    /// The same, for a journey that provoked a refusal on purpose.
    ///
    /// A journey that asks the server for something it refuses makes the
    /// browser log the answer as a severe network entry, and that entry is the
    /// journey working rather than a defect. `expected` names the entries the
    /// journey provoked, as text they contain; every other severe entry still
    /// fails it.
    pub async fn no_console_errors_but(&self, expected: &[&str]) {
        let console: Vec<String> = self
            .console_errors()
            .await
            .into_iter()
            .filter(|entry| !expected.iter().any(|wanted| entry.contains(wanted)))
            .collect();
        assert!(
            console.is_empty(),
            "the browser logged {} severe entries, and a rendered page that logs one is a defect:\n  {}",
            console.len(),
            console.join("\n  ")
        );
    }
}
