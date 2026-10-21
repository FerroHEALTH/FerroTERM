// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The WCAG 2.2 Level AA pass, walked across every screen rather than per
//! slice (<https://www.w3.org/TR/WCAG22/>).
//!
//! Two things here are read through a script, because WebDriver has no
//! primitive for either: which element focus landed on after a key press, and
//! the colour the browser actually painted. Everything else in the battery
//! stays on the primitives.
//!
//! The colour passes run over both themes. A palette that passes in light
//! and fails in dark is the failure this pass exists to catch, and it cannot
//! be seen from one of them.
//!
//! The keyboard walk and the markup pass also run over both densities. A
//! density moves padding, which is what a stop and a target are made of, and
//! leaves every ratio where it was, so the contrast pass stays on the themes.

use std::time::Duration;

use thirtyfour::prelude::*;

use crate::harness::Journey;
use crate::harness::server;
use crate::harness::session;

/// Every shipped screen, as the address a reader opens.
///
/// The code system screen is absent: its address carries a canonical this
/// module would have to name, and the sidebar does not offer it. The overview
/// links to it, and `viewer` walks that link.
///
/// The last four are the editor bundle's own screens, which the one server
/// serves at its own mount. The code system and concept map ones are
/// opened on the fixtures this deployment holds, because an empty form has
/// none of the rows whose markup this pass exists to read; the composer opens
/// on a new draft, which is where its own clause carries every control it
/// draws. This deployment publishes no issuer, so every form opens read-only,
/// and the controls a token opens are walked by the keyboard journeys in
/// `editor`, `composer`, `concept_map_editor` and `history` against the
/// deployment that has one.
///
/// The version screen names no resource, because this deployment has written
/// none: every resource it holds was loaded rather than written, and an id it
/// does not hold would make the screen render a refusal instead of itself.
/// What this pass reads there is the heading, the sentence that says there are
/// no versions, and the chrome around them; the table and the restore control
/// are walked by the keyboard journey in `history`.
const SCREENS: [&str; 19] = [
    "/",
    "/systems",
    "/systems/https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy",
    "/valuesets/e2e-taxonomy-all",
    "/conceptmaps/e2e-taxonomy-map",
    "/lookup",
    "/about?tab=settings",
    "/browse",
    "/expand",
    "/validate",
    "/valuesets",
    "/conceptmaps",
    "/translate",
    "/find",
    "/about",
    "/codesystem?system=https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy",
    "/compose",
    "/conceptmap?map=https%3A%2F%2Fferroterm.eu%2Ffhir%2FConceptMap%2Fe2e-taxonomy-map",
    "/history?type=CodeSystem",
];

/// The version every screen is opened on.
///
/// R5 declares the `child-of` filter operator, so the browser offers its tree
/// and the walk covers the one control with a non-obvious keyboard contract
/// (<https://www.w3.org/TR/WCAG22/#keyboard>).
const VERSION: &str = "r5";

/// The theme control in that pane.
const THEME_CONTROL: &str = "#viewer-theme";

/// The density control on the settings screen.
const DENSITY_CONTROL: &str = "#viewer-density";

/// The two densities a reader can choose between.
const DENSITIES: [&str; 2] = ["comfortable", "compact"];

/// The heading every screen renders once it has booted.
const HEADING: &str = "h1";

/// The ring a section draws while it is still reading.
///
/// Every read on a screen has to have landed before the walk counts controls,
/// because an answer that arrives mid-walk adds tab stops the count did not
/// budget for.
const SPINNER: &str = ".animate-spin";

/// How many focusable elements the screen offers right now.
///
/// The selector is `:enabled` rather than `:not([disabled])`, because a
/// control disabled through an ancestor `<fieldset disabled>` carries no
/// attribute of its own and is still skipped by the tab order
/// (<https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#concept-fe-disabled>).
const COUNT_FOCUSABLE: &str = r#"
return String(Array.from(document.querySelectorAll(
  'a[href], button:enabled, input:enabled, select:enabled,'
  + ' textarea:enabled, summary, [tabindex]:not([tabindex="-1"])'
)).filter((el) => el.checkVisibility({
  contentVisibilityAuto: true, opacityProperty: true, visibilityProperty: true,
})).length);
"#;

/// How long the count has to hold still before the walk trusts it.
const SETTLE: Duration = Duration::from_millis(400);

/// How many times the walk re-reads the count before it gives up waiting.
const SETTLE_TRIES: usize = 20;

/// How many tab presses past the number of controls the walk allows itself.
///
/// The document itself takes a stop on the way round, so a walk that pressed
/// tab only as often as there are controls would end short of the first one.
const EXTRA_STOPS: usize = 6;

/// Tags every visible focusable element on the screen and says how many there
/// are.
///
/// The tag is what the walk below compares against: an element with no id
/// cannot otherwise be named, and a selector built from position would move
/// the moment the screen re-renders.
const TAG_FOCUSABLE: &str = r#"
// Every disclosure is opened first. A control inside a closed <details> is
// not in the tab order and a reader reaches it by opening the disclosure, so
// walking with them shut would either miss those controls or call them
// unreachable (https://html.spec.whatwg.org/multipage/interactive-elements.html#the-details-element).
document.querySelectorAll('details:not([open])').forEach((el) => { el.open = true; });
const focusable = Array.from(document.querySelectorAll(
  'a[href], button:enabled, input:enabled, select:enabled,'
  + ' textarea:enabled, summary, [tabindex]:not([tabindex="-1"])'
)).filter((el) => {
  const box = el.getBoundingClientRect();
  if (box.width < 2 || box.height < 2) { return false; }
  return el.checkVisibility({
    contentVisibilityAuto: true, opacityProperty: true, visibilityProperty: true,
  });
});
focusable.forEach((el, index) => el.setAttribute('data-walk', String(index)));
// Every stop is recorded as focus reaches it, so the walk below presses tab in
// one call and reads the whole sequence back once.
window.walkStops = [];
document.addEventListener('focusin', (event) => {
  const el = event.target;
  // The outline is judged only where `:focus-visible` matches. A burst of tab
  // presses can record a stop before the browser has resolved the pseudo-class
  // for it, and an outline read then is the resting one, not the focused one.
  if (!el.matches(':focus-visible')) { return; }
  const style = getComputedStyle(el);
  const tag = el.getAttribute('data-walk');
  window.walkStops.push([tag === null ? '' : tag, style.outlineStyle, style.outlineWidth,
    el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')].join('|'));
});
// The walk starts from the document rather than from wherever the last screen
// left focus, so the first control is reached inside the budget below.
document.body.setAttribute('tabindex', '-1');
document.body.focus();
return focusable.map((el) => el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')
  + ' "' + (el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 30) + '"'
).join('\n');
"#;

/// Every stop the walk reached, one per line, as the recorder wrote them.
const WALK_STOPS: &str = "return (window.walkStops || []).join('\\n');";

/// Every text this screen paints whose contrast falls under the AA bar.
///
/// The ratio is WCAG's own, over the relative luminance of the two colours the
/// browser computed (<https://www.w3.org/TR/WCAG22/#contrast-minimum>). The
/// background is the first ancestor that paints one, because a transparent
/// element shows what is behind it. Large text is 18.66px bold or 24px, which
/// is the threshold the success criterion names.
const CONTRAST_FAILURES: &str = r#"
// Tailwind 4 writes its palette in oklch, and a browser computes `color` to
// the space the author wrote (https://www.w3.org/TR/css-color-4/#resolving-color-values),
// so a reader of getComputedStyle sees oklch and never rgb. Both are converted
// to linear-light sRGB here, which is what the luminance formula wants anyway.
const linear = (value) => {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
};
const gamma = (c) => {
  const v = c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(Math.max(c, 0), 1 / 2.4) - 0.055;
  return Math.round(Math.min(Math.max(v, 0), 1) * 255);
};
// OKLab to linear sRGB, the matrices of the colour space's own definition
// (https://bottosson.github.io/posts/oklab/).
const from_oklab = (L, a, b, alpha) => {
  const l = Math.pow(L + 0.3963377774 * a + 0.2158037573 * b, 3);
  const m = Math.pow(L - 0.1055613458 * a - 0.0638541728 * b, 3);
  const s = Math.pow(L - 0.0894841775 * a - 1.2914855480 * b, 3);
  return {
    r: 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    g: -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    b: -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    a: alpha,
  };
};
const numbers = (colour) => (colour.match(/-?[\d.]+(?:e-?\d+)?%?/g) || []).map((part) =>
  part.endsWith('%') ? Number(part.slice(0, -1)) / 100 : Number(part)
);
const parse = (colour) => {
  const text = colour.trim();
  if (text === 'transparent') { return { r: 0, g: 0, b: 0, a: 0 }; }
  const parts = numbers(text);
  if (text.startsWith('rgb')) {
    if (parts.length < 3) { return null; }
    return {
      r: linear(parts[0]), g: linear(parts[1]), b: linear(parts[2]),
      a: parts.length > 3 ? parts[3] : 1,
    };
  }
  if (text.startsWith('oklch')) {
    if (parts.length < 3) { return null; }
    const hue = (parts[2] * Math.PI) / 180;
    return from_oklab(parts[0], parts[1] * Math.cos(hue), parts[1] * Math.sin(hue),
      parts.length > 3 ? parts[3] : 1);
  }
  if (text.startsWith('oklab')) {
    if (parts.length < 3) { return null; }
    return from_oklab(parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1);
  }
  return null;
};
// A colour with alpha shows what is behind it, so it is composited onto the
// colour behind before either is measured
// (https://www.w3.org/TR/compositing-1/#simplealphacompositing).
const over = (top, bottom) => ({
  r: top.r * top.a + bottom.r * (1 - top.a),
  g: top.g * top.a + bottom.g * (1 - top.a),
  b: top.b * top.a + bottom.b * (1 - top.a),
  a: 1,
});
const luminance = (c) => 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
const shown = (c) => 'rgb(' + gamma(c.r) + ', ' + gamma(c.g) + ', ' + gamma(c.b) + ')';
const unknown = new Set();
const behind = (el) => {
  let painted = { r: 1, g: 1, b: 1, a: 1 };
  const stack = [];
  for (let node = el; node; node = node.parentElement) {
    const raw = getComputedStyle(node).backgroundColor;
    const colour = parse(raw);
    if (!colour) { unknown.add(raw); continue; }
    if (colour.a === 0) { continue; }
    stack.push(colour);
    if (colour.a === 1) { break; }
  }
  while (stack.length) { painted = over(stack.pop(), painted); }
  return painted;
};
const owns_text = (el) => Array.from(el.childNodes).some(
  (node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim() !== ''
);
const failures = [];
for (const el of document.querySelectorAll('body *')) {
  if (!owns_text(el)) { continue; }
  if (el.closest('.sr-only') || el.tagName === 'OPTION') { continue; }
  const box = el.getBoundingClientRect();
  if (box.width < 2 || box.height < 2) { continue; }
  const style = getComputedStyle(el);
  if (style.visibility === 'hidden' || style.display === 'none') { continue; }
  const raw = style.color;
  const fg = parse(raw);
  if (!fg) { unknown.add(raw); continue; }
  if (fg.a === 0) { continue; }
  const bg = behind(el);
  const painted = fg.a === 1 ? fg : over(fg, bg);
  const lighter = Math.max(luminance(painted), luminance(bg));
  const darker = Math.min(luminance(painted), luminance(bg));
  const ratio = (lighter + 0.05) / (darker + 0.05);
  const size = parseFloat(style.fontSize);
  const weight = Number(style.fontWeight) || 400;
  const large = size >= 24 || (size >= 18.66 && weight >= 700);
  const bar = large ? 3 : 4.5;
  if (ratio + 0.005 < bar) {
    failures.push(el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')
      + ' "' + el.textContent.trim().slice(0, 40) + '" '
      + ratio.toFixed(2) + ':1 needs ' + bar + ':1, ' + shown(painted) + ' on ' + shown(bg));
  }
}
// A colour space this cannot read is a measurement that did not happen, so it
// is reported rather than passed over.
for (const raw of unknown) { failures.push('unreadable colour: ' + raw); }
return failures.join('\n');
"#;

/// Everything a browser can see about a screen's markup that WCAG governs and
/// no wait can prove.
///
/// A table with no `<tbody>`, a header cell with no `scope`, a control with no
/// accessible name, or two elements sharing one id all render fine and all
/// break a screen reader, so each is read from the DOM the browser built
/// rather than from the source that produced it.
const SEMANTICS: &str = r#"
const findings = [];
const name = (el) => el.tagName.toLowerCase() + (el.id ? '#' + el.id : '');
for (const table of document.querySelectorAll('table')) {
  if (!table.querySelector(':scope > tbody')) {
    findings.push('table with no tbody: ' + table.textContent.trim().slice(0, 40));
  }
}
for (const cell of document.querySelectorAll('th')) {
  if (!cell.getAttribute('scope')) {
    findings.push('th with no scope: "' + cell.textContent.trim().slice(0, 40) + '"');
  }
}
for (const control of document.querySelectorAll('input, select, textarea')) {
  if (control.type === 'hidden') { continue; }
  const labelled = control.id && document.querySelector('label[for="' + CSS.escape(control.id) + '"]');
  const named = labelled || control.closest('label')
    || control.getAttribute('aria-label') || control.getAttribute('aria-labelledby');
  if (!named) { findings.push('control with no label: ' + name(control)); }
  if (!control.id) { findings.push('control with no id: ' + name(control)); }
}
const seen = new Set();
for (const el of document.querySelectorAll('[id]')) {
  if (seen.has(el.id)) { findings.push('id used twice: ' + el.id); }
  seen.add(el.id);
}
for (const paragraph of document.querySelectorAll('p')) {
  const block = paragraph.querySelector('p, div, ul, ol, table, section, h1, h2, h3, h4');
  if (block) { findings.push('block element inside a p: ' + block.tagName.toLowerCase()); }
}
const headings = document.querySelectorAll('h1');
if (headings.length !== 1) { findings.push(headings.length + ' h1 elements, not 1'); }
for (const image of document.querySelectorAll('img')) {
  if (image.getAttribute('alt') === null) { findings.push('img with no alt: ' + image.src); }
}
return findings.join('\n');
"#;

/// Whether the document ships a rule for readers who ask for less motion.
///
/// The check is on the CSSOM rather than on the source, so a rule the build
/// dropped is caught (<https://www.w3.org/TR/WCAG22/#animation-from-interactions>).
const REDUCED_MOTION: &str = r"
// The rule is written inside a cascade layer, and a layer block nests its own
// rules, so the walk recurses rather than reading one flat list
// (https://drafts.csswg.org/css-cascade-5/#cssom).
const declared = (rules) => {
  for (const rule of rules) {
    if (rule.conditionText && rule.conditionText.includes('prefers-reduced-motion')) {
      return true;
    }
    if (rule.cssRules && declared(rule.cssRules)) { return true; }
  }
  return false;
};
for (const sheet of document.styleSheets) {
  let rules;
  try { rules = sheet.cssRules; } catch (unreadable) { continue; }
  if (declared(rules)) { return 'declared'; }
}
return 'absent';
";

/// The value set the expansion runner is given, which the fixture publishes.
const EXPANDED: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";

/// The code system the validation runner is given.
const SYSTEM: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// A code that system holds.
const CODE: &str = "ca-leaf";

/// A live region on the screen, once it carries something.
const ANNOUNCEMENT: &str = "p[aria-live='polite']";

/// The address of one screen on the version the pass runs.
fn address(base: &str, path: &str) -> String {
    let opener = if path.contains('?') { '&' } else { '?' };
    format!("{base}{path}{opener}fhir={VERSION}")
}

/// The first live region on the screen, once it says something.
///
/// The wait is on the region having text at all rather than on wording, so
/// this reads every screen the same way and none of it is pinned to a sentence.
async fn announcement(journey: &Journey, what: &str) -> String {
    let region = journey.element(By::Css(ANNOUNCEMENT), what).await;
    // NOTE: a live region may be visually hidden, which WebDriver's element
    // text leaves out, so its words are read from `textContent` (DOM, Node).
    for _ in 0..SETTLE_TRIES {
        match region.prop("textContent").await {
            Ok(Some(said)) if !said.trim().is_empty() => return said,
            Ok(_) | Err(_) => tokio::time::sleep(SETTLE).await,
        }
    }
    panic!("{what}: the live region stayed empty");
}

/// Puts the console in `mode`, which the settings pane keeps in a cookie, so
/// every screen opened afterwards in this session is drawn in it.
async fn choose_theme(journey: &Journey, base: &str, mode: &str) -> WebDriverResult<()> {
    journey
        .keep_setting(base, THEME_CONTROL, mode, Some("data-theme"))
        .await
}

/// Puts the console in `density`, which the settings pane keeps in a cookie,
/// so every screen opened afterwards in this session is drawn at it.
async fn choose_density(journey: &Journey, base: &str, density: &str) -> WebDriverResult<()> {
    journey
        .keep_setting(base, DENSITY_CONTROL, density, Some("data-density"))
        .await
}

/// Opens every disclosure on the screen, so what is behind one is measured.
///
/// A remembered run list, an evidence table and a root's notes are each a
/// `<details>`. A pass that measured only what is open would report on a
/// screen no reader ends up looking at.
const OPEN_DISCLOSURES: &str = r"
document.querySelectorAll('details:not([open])').forEach((el) => { el.open = true; });
return 'opened';
";

/// Opens one screen and waits for every read on it to have landed.
///
/// The heading arrives with the bundle, the rings go when the reads answer,
/// and the count of controls holding still is what proves nothing else is
/// about to draw.
async fn open(journey: &Journey, base: &str, path: &str) -> WebDriverResult<()> {
    journey.reopen(&address(base, path)).await;
    journey
        .element(By::Css(HEADING), &format!("the heading of {path}"))
        .await;
    journey
        .count_becoming(
            By::Css(SPINNER),
            0,
            &format!("every read on {path} to land"),
        )
        .await;
    journey.evaluate(OPEN_DISCLOSURES).await?;
    let mut last = String::new();
    for _ in 0..SETTLE_TRIES {
        let now = journey.evaluate(COUNT_FOCUSABLE).await?;
        if now == last {
            return Ok(());
        }
        last = now;
        tokio::time::sleep(SETTLE).await;
    }
    panic!("{path} kept drawing new controls, so its tab order never held still");
}

/// Tabs through one screen and reports every control the keyboard never
/// reached, and every stop the browser drew no outline on.
async fn walk(journey: &Journey, path: &str, theme: &str) -> WebDriverResult<Vec<String>> {
    let named = journey.evaluate(TAG_FOCUSABLE).await?;
    let controls: Vec<&str> = named.lines().collect();
    assert!(
        !controls.is_empty(),
        "{path} in the {theme} theme offers no focusable control at all, which no screen does"
    );
    let mut reached = vec![false; controls.len()];
    let mut findings = Vec::new();
    journey.tab(controls.len() + EXTRA_STOPS).await?;
    for stop in journey.evaluate(WALK_STOPS).await?.lines() {
        let mut fields = stop.split('|');
        let (Some(tag), Some(style), Some(width), Some(named)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if style == "none" || width.starts_with('0') {
            findings.push(format!(
                "{path} ({theme}): focus on {named} draws no outline (outline-style {style}, width {width})"
            ));
        }
        if let Ok(index) = tag.parse::<usize>()
            && let Some(seen) = reached.get_mut(index)
        {
            *seen = true;
        }
    }
    for (index, seen) in reached.iter().enumerate() {
        if !seen {
            let named = controls.get(index).copied().unwrap_or("?");
            findings.push(format!("{path} ({theme}): tabbing never reached {named}"));
        }
    }
    Ok(findings)
}

/// Every control on every screen is reached by the keyboard alone, and every
/// stop the keyboard lands on is drawn with an outline.
///
/// WCAG 2.2 requires both: operation by keyboard
/// (<https://www.w3.org/TR/WCAG22/#keyboard>) and a focus indicator that is
/// visible (<https://www.w3.org/TR/WCAG22/#focus-visible>). A screen built one
/// slice at a time can satisfy each slice and still leave a control only a
/// mouse reaches, which is what walking the whole of it catches.
#[tokio::test]
async fn every_control_on_every_screen_is_reached_by_the_keyboard_alone() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;
            let mut findings = Vec::new();
            for density in DENSITIES {
                choose_density(&journey, &base, density).await?;
                for theme in ["light", "dark"] {
                    choose_theme(&journey, &base, theme).await?;
                    let drawn = format!("{theme}, {density}");
                    for path in SCREENS {
                        open(&journey, &base, path).await?;
                        findings.extend(walk(&journey, path, &drawn).await?);
                    }
                }
            }
            assert!(
                findings.is_empty(),
                "the keyboard walk found {} failures:\n{}",
                findings.len(),
                findings.join("\n")
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// Every text on every screen meets the AA contrast bar, in both themes.
///
/// The ratio is computed over what the browser painted rather than over the
/// tokens the source names, so a colour inherited from an ancestor, or one a
/// transparent background lets through, is measured as a reader sees it
/// (<https://www.w3.org/TR/WCAG22/#contrast-minimum>).
#[tokio::test]
async fn every_screen_meets_the_aa_contrast_bar_in_both_themes() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;
            let mut findings = Vec::new();
            for theme in ["light", "dark"] {
                choose_theme(&journey, &base, theme).await?;
                for path in SCREENS {
                    open(&journey, &base, path).await?;
                    let failures = journey.evaluate(CONTRAST_FAILURES).await?;
                    if !failures.is_empty() {
                        findings.push(format!("{path} ({theme}):\n{failures}"));
                    }
                }
            }
            assert!(
                findings.is_empty(),
                "text below the AA contrast bar:\n{}",
                findings.join("\n")
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// The markup every screen renders satisfies the structural obligations WCAG
/// puts on it, and the document asks for less motion when a reader does.
///
/// Tables, labels and ids are checked on the DOM the browser built rather than
/// on the source, because a control whose label went missing in a re-render
/// renders the same as one that never had one
/// (<https://www.w3.org/TR/WCAG22/#name-role-value>).
#[tokio::test]
async fn every_screen_renders_the_markup_a_screen_reader_needs() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;
            let mut findings = Vec::new();
            for density in DENSITIES {
                choose_density(&journey, &base, density).await?;
                for path in SCREENS {
                    open(&journey, &base, path).await?;
                    let reported = journey.evaluate(SEMANTICS).await?;
                    if !reported.is_empty() {
                        findings.push(format!("{path} ({density}):\n{reported}"));
                    }
                }
            }
            assert_eq!(
                journey.evaluate(REDUCED_MOTION).await?,
                "declared",
                "the bundle ships no rule for a reader who asks for less motion"
            );
            assert!(
                findings.is_empty(),
                "markup a screen reader cannot use:\n{}",
                findings.join("\n")
            );
            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}

/// A search, an expansion and a validation each announce their outcome in a
/// live region.
///
/// A reader who cannot see the answer arrive is told it did
/// (<https://www.w3.org/TR/WCAG22/#status-messages>). The announcement is read
/// out of the region the page marks `aria-live`, so a count printed somewhere
/// else on the screen does not satisfy this.
#[tokio::test]
async fn a_search_an_expansion_and_a_validation_are_each_announced() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;

            journey
                .reopen(&format!("{base}/browse?fhir={VERSION}&system={SYSTEM}"))
                .await;
            let searched = announcement(&journey, "the search to announce what it matched").await;
            assert!(
                searched.chars().any(|glyph| glyph.is_ascii_digit()),
                "the browse search announced `{searched}`, which carries no count"
            );

            journey
                .reopen(&format!("{base}/expand?fhir={VERSION}&url={EXPANDED}"))
                .await;
            let expanded =
                announcement(&journey, "the expansion to announce how much it answered").await;
            assert!(
                expanded.chars().any(|glyph| glyph.is_ascii_digit()),
                "the expansion announced `{expanded}`, which carries no count"
            );

            journey
                .reopen(&format!(
                    "{base}/validate?fhir={VERSION}&system={SYSTEM}&code={CODE}"
                ))
                .await;
            let validated = announcement(&journey, "the validation to announce its verdict").await;
            assert!(
                !validated.is_empty(),
                "the validation announced nothing, so a reader is never told it ran"
            );

            journey.no_console_errors().await;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the journey ran and the browser session ended cleanly");
}
