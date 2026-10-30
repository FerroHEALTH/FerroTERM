// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Every screen at 320 CSS pixels wide, with the modules loaded and with
//! them refused: the page reflows into one column with no horizontal scroll
//! (WCAG 2.2 SC 1.4.10 Reflow, <https://www.w3.org/TR/WCAG22/#reflow>).

use thirtyfour::prelude::*;

use crate::accessibility::{SCREENS, VERSION};
use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// The width SC 1.4.10 names, in CSS pixels.
const WIDTH: u32 = 320;

/// A height that leaves the width the only constraint.
const HEIGHT: u32 = 800;

/// How far the document reaches past the viewport to the right, in CSS
/// pixels, then the outermost elements that reach past it outside any box
/// that scrolls or clips sideways, each as its tag, its classes and its
/// width; `0` alone is a page that does not scroll sideways.
const OVERFLOW: &str = r"
const root = document.documentElement;
const over = Math.max(0, root.scrollWidth - root.clientWidth);
if (over === 0) { return '0'; }
const clipped = (element) => {
  for (let up = element.parentElement; up && up !== document.body; up = up.parentElement) {
    const style = getComputedStyle(up);
    if (style.position === 'fixed' || style.overflowX !== 'visible') { return true; }
  }
  return getComputedStyle(element).position === 'fixed';
};
const past = [...document.querySelectorAll('body *')].filter((element) => {
  const box = element.getBoundingClientRect();
  return box.width > 0 && box.right > root.clientWidth + 0.5 && !clipped(element);
});
const outermost = past.filter((element) => !past.some((other) => other !== element && other.contains(element)));
const named = outermost.slice(0, 6).map((element) =>
  element.tagName.toLowerCase() + [...element.classList].map((name) => '.' + name).join('')
    + ' ' + Math.round(element.getBoundingClientRect().width) + 'px');
return String(over) + ' px, by ' + named.join(', ');
";

/// The screens the walk leaves out while FerroUI's stylesheet widens them.
const AWAITING_FERROUI: [&str; 5] = [
    // TODO(#947): walk these once FerroUI#528 and FerroUI#529 ship.
    "/lookup",
    "/validate",
    "/expand",
    "/translate",
    "/conceptmaps/e2e-taxonomy-map",
];

/// Walks every screen at [`WIDTH`] and fails on each that scrolls sideways.
async fn walk(journey: &Journey, base: &str) -> WebDriverResult<()> {
    journey.resize(WIDTH, HEIGHT).await;
    let mut wide = Vec::new();
    for screen in SCREENS
        .into_iter()
        .filter(|screen| !AWAITING_FERROUI.contains(screen))
    {
        let separator = if screen.contains('?') { '&' } else { '?' };
        journey
            .reopen(&format!("{base}{screen}{separator}fhir={VERSION}"))
            .await;
        journey
            .element(By::Css("main h1"), &format!("the heading of {screen}"))
            .await;
        let over = journey.evaluate(OVERFLOW).await?;
        if over.trim() != "0" {
            wide.push(format!(
                "{screen} reaches {} px past the viewport",
                over.trim()
            ));
        }
    }
    assert!(
        wide.is_empty(),
        "a screen scrolls sideways at {WIDTH} px:\n  {}",
        wide.join("\n  ")
    );
    Ok(())
}

/// Every screen reflows at 320 px with the modules loaded.
#[tokio::test]
async fn every_screen_reflows_at_320_px() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            walk(&journey, &base).await?;
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Every screen reflows at 320 px with the modules refused.
#[tokio::test]
async fn every_screen_reflows_at_320_px_with_no_script() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open_without_scripts(driver, &base, "/").await;
            walk(&journey, &base).await?;
            journey.no_console_errors_but(&["/scripts/"]).await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
