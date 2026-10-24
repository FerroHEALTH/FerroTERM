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
/// Each resource page is opened on every tab, because a tab is its own
/// address and its own markup; the concept browser on a concept, so its tree
/// is drawn. This deployment publishes no issuer, so the editors are walked
/// by the keyboard journeys of `editor`, `composer`, `concept_map_editor`
/// and `history` against the deployment that has one. The shortcuts' own
/// address opens their legend as a modal dialog, which the walk would only
/// tab inside, so `console` reads it instead.
const SCREENS: [&str; 24] = [
    "/",
    "/systems",
    "/systems/https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy",
    "/systems/https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy?pane=versions",
    "/valuesets",
    "/valuesets/e2e-taxonomy-all",
    "/valuesets/e2e-taxonomy-all?pane=expansion",
    "/conceptmaps",
    "/conceptmaps/e2e-taxonomy-map",
    "/conceptmaps/e2e-taxonomy-map?pane=overview",
    "/browse?system=https%3A%2F%2Fferroterm.eu%2Ffhir%2FCodeSystem%2Fe2e-taxonomy&code=ba-branch",
    "/lookup",
    "/validate",
    "/expand",
    "/translate",
    "/find?q=ca-leaf",
    "/server",
    "/server/operations",
    "/server/evidence",
    "/configuration",
    "/preferences",
    "/preferences/reading",
    "/preferences/keyboard",
    "/about",
];

/// The version every screen is opened on.
///
/// R5 declares the `child-of` filter operator, so the browser offers its tree
/// and the walk covers the one control with a non-obvious keyboard contract
/// (<https://www.w3.org/TR/WCAG22/#keyboard>).
const VERSION: &str = "r5";

/// The two densities a reader can choose between.
const DENSITIES: [&str; 2] = ["comfortable", "compact"];

/// The heading every screen renders once it has booted.
const HEADING: &str = "h1";

/// A part of the page still waiting on an answer.
///
/// The server sends a page whole, so none is ever drawn; the wait stays so a
/// page that one day streams a part in is still measured once it has landed.
const SPINNER: &str = "[aria-busy='true']";

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
// A ring counts only where focus changes it, so each candidate part's outline
// and shadow are read at rest, before any control holds focus, and compared
// with what it draws once focus arrives: a row whose cells always carry an
// inset shadow (a stripe, a selection) shows nothing new on focus.
const look = (node) => {
  const style = getComputedStyle(node);
  return [style.outlineStyle, style.outlineWidth, style.outlineColor, style.boxShadow].join(' ');
};
const parts = (el) => {
  const found = [el, ...el.children];
  for (let node = el.parentElement, depth = 0; node && depth < 4; node = node.parentElement, depth += 1) {
    found.push(node);
  }
  const cell = el.closest('tr') && el.closest('tr').firstElementChild;
  if (cell) { found.push(cell); }
  return found;
};
const resting = new WeakMap();
const rest = (node) => {
  if (!resting.has(node) && !node.matches(':focus')) { resting.set(node, look(node)); }
};
// Every element, read before any disclosure is opened below: a stop the tags
// below leave out (a skip link drawn off screen until it is focused) still
// needs its resting look, and a disclosure's summary rests closed.
document.querySelectorAll('body, body *').forEach(rest);
// An island that hydrates may put new nodes in place of the ones read above,
// so every element added later is read at rest as it arrives.
new MutationObserver((changes) => changes.forEach((change) => change.addedNodes.forEach((added) => {
  if (added.nodeType !== Node.ELEMENT_NODE) { return; }
  [added, ...added.querySelectorAll('*')].forEach(rest);
}))).observe(document.body, { childList: true, subtree: true });
// Every disclosure is opened first. A control inside a closed <details> is
// not in the tab order and a reader reaches it by opening the disclosure, so
// walking with them shut would either miss those controls or call them
// unreachable (https://html.spec.whatwg.org/multipage/interactive-elements.html#the-details-element).
// A summary that declares a popup (a select's listbox) is a widget, whose
// options are reached with the arrow keys once it is open, so it stays shut
// (https://www.w3.org/WAI/ARIA/apg/patterns/listbox/), and its summary is
// walked as a reader meets it.
document.querySelectorAll('details:not([open]):not(:has(> summary[aria-haspopup]))')
  .forEach((el) => { el.open = true; });
// A negative tabindex takes an element out of the tab order on purpose: the
// items of a menu or a tree are reached with the arrow keys once the widget
// holds focus (https://html.spec.whatwg.org/multipage/interaction.html#attr-tabindex).
const focusable = Array.from(document.querySelectorAll(
  'a[href], button:enabled, input:enabled, select:enabled,'
  + ' textarea:enabled, summary, [tabindex]'
)).filter((el) => el.getAttribute('tabindex') !== '-1').filter((el) => {
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
  // A composite control draws its ring on the part that shows it: the search
  // field's box, a tree item's row, and a table row whose name is the link,
  // whose cells draw the ring as an inset shadow on a wide screen.
  // A ring that fades in is read at its end: each part's running transitions
  // are finished first, or a stop read at its first frame looks like rest.
  parts(el).forEach((node) => node.getAnimations().forEach((animation) => animation.finish()));
  const changed = (node) => resting.has(node) && resting.get(node) !== look(node);
  const draws = (node) => {
    const style = getComputedStyle(node);
    return style.outlineStyle !== 'none' && !style.outlineWidth.startsWith('0') && changed(node);
  };
  let ring = Array.from(el.children).find(draws) || el;
  for (let node = el.parentElement, depth = 0; node && depth < 4; node = node.parentElement, depth += 1) {
    if (draws(ring)) { break; }
    if (node.matches(':focus-within') && draws(node)) { ring = node; }
  }
  const cell = el.closest('tr') && el.closest('tr').firstElementChild;
  const shadowed = !draws(ring) && cell && getComputedStyle(cell).boxShadow.includes('inset')
    && changed(cell);
  const style = shadowed
    ? { outlineStyle: 'inset-shadow', outlineWidth: '2px' }
    : draws(ring) ? getComputedStyle(ring) : { outlineStyle: 'none', outlineWidth: '0px' };
  const tag = el.getAttribute('data-walk');
  window.walkStops.push([tag === null ? '' : tag, style.outlineStyle, style.outlineWidth,
    el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')].join('|'));
// The capture phase, so a widget that stops the event on its way up (the tree
// handles its own focus) is still recorded.
}, true);
// The walk starts from the document rather than from wherever the last screen
// left focus, so the first control is reached inside the budget below.
document.body.setAttribute('tabindex', '-1');
document.body.focus();
return focusable.map((el) => el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')
  + ' "' + (el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 30) + '"'
).join('\n');
"#;

/// Two table rows the console never draws, added to a screen to prove the
/// walk: in the first the cell carries an inset shadow at rest and focus
/// changes nothing, in the second the shadow arrives with focus.
///
/// The rules go in through a constructed style sheet, which the console's
/// nonce-based Content Security Policy leaves to the page's own script.
const ROW_FIXTURES: &str = r##"
const sheet = new CSSStyleSheet();
sheet.replaceSync('#walk-fixtures a { outline: none; }'
  + ' #walk-fixtures tr.permanent td { box-shadow: inset 3px 0 0 #000; }'
  + ' #walk-fixtures tr.on-focus:focus-within td { box-shadow: inset 3px 0 0 #000; }');
document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
const table = document.createElement('table');
table.id = 'walk-fixtures';
table.innerHTML = '<caption>Walk fixtures</caption><tbody>'
  + '<tr class="permanent"><td><a id="walk-permanent" href="#permanent">Permanent</a></td></tr>'
  + '<tr class="on-focus"><td><a id="walk-on-focus" href="#on-focus">On focus</a></td></tr>'
  + '</tbody>';
document.querySelector('main').appendChild(table);
return '';
"##;

/// Shuts every popup a summary declares (a select's listbox), which
/// [`OPEN_DISCLOSURES`] opened for the other checks, so the walk meets each
/// select as a reader does: shut, its options reached with the arrow keys
/// once it is open (<https://www.w3.org/WAI/ARIA/apg/patterns/listbox/>).
const SHUT_POPUPS: &str = r"
document.querySelectorAll('details[open]:has(> summary[aria-haspopup])')
  .forEach((el) => { el.open = false; });
return 'shut';
";

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
  // A colour the page mixes (`color-mix()` in srgb) computes to the `color()`
  // form, whose channels are gamma-encoded on 0 to 1
  // (https://www.w3.org/TR/css-color-4/#color-function).
  if (text.startsWith('color(srgb ')) {
    if (parts.length < 3) { return null; }
    return {
      r: linear(parts[0] * 255), g: linear(parts[1] * 255), b: linear(parts[2] * 255),
      a: parts.length > 3 ? parts[3] : 1,
    };
  }
  if (text.startsWith('oklab')) {
    if (parts.length < 3) { return null; }
    return from_oklab(parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1);
  }
  return null;
};
// A colour with alpha shows what is behind it, so it is composited onto the
// colour behind before either is measured
// (https://www.w3.org/TR/compositing-1/#simplealphacompositing). A browser
// blends the gamma-encoded sRGB values, so the blend is taken there.
const encoded = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(Math.max(c, 0), 1 / 2.4) - 0.055);
const blend = (top, bottom, alpha) =>
  linear(255 * (encoded(top) * alpha + encoded(bottom) * (1 - alpha)));
const over = (top, bottom) => ({
  r: blend(top.r, bottom.r, top.a),
  g: blend(top.g, bottom.g, top.a),
  b: blend(top.b, bottom.b, top.a),
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
  if (el.closest('.sr-only, .fui-sr-only') || el.tagName === 'OPTION') { continue; }
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
  // An id is what a `label for` names; a control inside its label is named by
  // containment (https://html.spec.whatwg.org/multipage/forms.html#the-label-element).
  if (!control.id && !control.closest('label')) {
    findings.push('control with no id: ' + name(control));
  }
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

/// The verdict of a run, which the page marks a status message.
const ANNOUNCEMENT: &str = "main [role='status'].fui-load-report";

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

/// Puts the console in `mode`, which Your preferences keeps in a cookie, so
/// every screen opened afterwards in this session is drawn in it.
async fn choose_theme(journey: &Journey, base: &str, mode: &str) -> WebDriverResult<()> {
    journey
        .keep_setting(base, "theme", mode, "data-theme")
        .await
}

/// Puts the console in `density`, which Your preferences keeps in a cookie,
/// so every screen opened afterwards in this session is drawn at it.
async fn choose_density(journey: &Journey, base: &str, density: &str) -> WebDriverResult<()> {
    journey
        .keep_setting(base, "density", density, "data-density")
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
    journey.evaluate(SHUT_POPUPS).await?;
    // The select's island answers the toggle a moment later, and the resting
    // looks are read after it has.
    tokio::time::sleep(SETTLE).await;
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

/// The walk counts a row's inset shadow as a focus indicator only when focus
/// draws it, so a row whose shadow is there at rest fails Focus Visible
/// (<https://www.w3.org/TR/WCAG22/#focus-visible>) and a row whose shadow
/// arrives with focus passes.
#[tokio::test]
async fn a_row_shadow_counts_as_a_focus_ring_only_when_focus_draws_it() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;
            open(&journey, &base, "/about").await?;
            journey.evaluate(ROW_FIXTURES).await?;
            let findings = walk(&journey, "/about", "row fixtures").await?;
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.contains("a#walk-permanent")
                        && finding.contains("draws no outline")),
                "a row whose shadow is there at rest passed the walk: {findings:#?}"
            );
            assert!(
                !findings
                    .iter()
                    .any(|finding| finding.contains("a#walk-on-focus")),
                "a row whose shadow arrives with focus failed the walk: {findings:#?}"
            );
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

/// A lookup, an expansion and a validation each announce their outcome as a
/// status message.
///
/// A reader who cannot see the answer arrive is told it did
/// (<https://www.w3.org/TR/WCAG22/#status-messages>). The verdict carries
/// `role="status"`, so a count printed somewhere else on the screen does not
/// satisfy this.
#[tokio::test]
async fn a_lookup_an_expansion_and_a_validation_are_each_announced() {
    let Some(base) = server() else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, &format!("/?fhir={VERSION}")).await;

            journey
                .reopen(&format!(
                    "{base}/lookup?fhir={VERSION}&system={SYSTEM}&code={CODE}"
                ))
                .await;
            let found = announcement(&journey, "the lookup to announce what it found").await;
            assert!(
                found.contains(CODE),
                "the lookup announced `{found}`, which names no code"
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
                    "{base}/validate?fhir={VERSION}&url={EXPANDED}&system={SYSTEM}&code={CODE}"
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
