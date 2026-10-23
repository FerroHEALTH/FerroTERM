// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The capture pass: a PNG of every console screen, for the book and for the
//! visual review of a change.
//!
//! This is a capture, not an assertion. It drives the same deployment the
//! journeys drive, so every image is a picture of a console that was running.
//! It has two modes, each gated on its own variable, which
//! `scripts/ui-e2e.sh` sets only when it is asked for that mode, so an
//! ordinary journey run rewrites no tracked image:
//!
//! - [`GATE_ENV`], the book: one light image per screen the book page
//!   `website/book/src/operate/console.md` embeds, at [`BOOK_WIDTH`].
//! - [`REVIEW_ENV`], the review: every screen at [`REVIEW_WIDTHS`] in both
//!   themes, numbered in walking order, plus the hover, focus and open states,
//!   into the directory the variable names (dark images under `dark/`).
//!
//! Every shot is taken over the synthetic fixture in `e2e/fixtures/codesystems`,
//! which the script mounts. No licensed release is ever the subject of a
//! committed image: SNOMED CT and the other licensed terminologies are not
//! distributed by this repository, and a screenshot of their concepts would be
//! derived edition data.

use std::path::Path;
use std::path::PathBuf;

use thirtyfour::prelude::*;

use crate::harness::Journey;
use crate::harness::choose;
use crate::harness::query_param;
use crate::harness::server;
use crate::harness::session;
use crate::harness::sign_in;
use crate::harness::signed_in;

/// Names the book capture.
const GATE_ENV: &str = "FERROTERM_UI_E2E_DOCS_SHOTS";

/// Sends the book capture somewhere other than the book's image directory.
const SHOTS_DIR_ENV: &str = "FERROTERM_UI_E2E_SHOTS_DIR";

/// Names the directory of the review capture.
const REVIEW_ENV: &str = "FERROTERM_UI_E2E_REVIEW_DIR";

/// Where the book's images live, resolved from this crate's manifest.
const BOOK_IMAGES: &str = "../website/book/src/operate/img/console";

/// The FHIR version every capture is taken on.
///
/// R5 rather than the default, because the concept browser draws a tree only
/// for a version declaring `child-of`, and `filter-operator` defines that from
/// R5 onward (<https://hl7.org/fhir/R4B/codesystem-filter-operator.html>).
const CAPTURED_VERSION: &str = "r5";

/// A code the mounted fixture concept map maps, and so a code the fixture
/// code system holds.
const MAPPED_CODE: &str = "ca-leaf";

/// The cookie the console keeps its theme in.
const THEME_COOKIE: &str = "ferroterm_console_theme";

/// A code system row whose version declares the direct-child operator: the
/// row offers the concept browser, so the browse link is the mark.
const WALKABLE_ROW: &str =
    "//tr[.//a[contains(@href, '/browse')]]//a[starts-with(@href, '/systems/')]";

/// The canonical fact of a detail screen.
const CANONICAL_FACT: &str = "//dt[normalize-space()='Canonical']/following-sibling::dd[1]";

/// A listed value set, as the link that opens its screen.
const VALUE_SET_LINK: &str = "main tbody a[href^='/valuesets/']";

/// A listed concept map, as the link that opens its screen.
const CONCEPT_MAP_LINK: &str = "main tbody a[href^='/conceptmaps/']";

/// The link from a code system's screen into the concept browser.
const BROWSE_LINK: &str = "nav[aria-label^='Screens for'] a[href*='/browse']";

/// A concept the browser's search answered.
const SEARCH_RESULT: &str = "section[aria-labelledby='browse-search-heading'] ul li a";

/// One row of the tree.
const TREE_ROW: &str = "li[role='treeitem']";

/// The link from a value set's screen to its versions.
const HISTORY_LINK: &str = "main a[href^='/history']";

/// The first table row link of a listing, for the hover state.
const ROW_LINK: &str = "main tbody tr a";

/// The command bar, for the focus state.
const COMMAND_BAR: &str = "#command-bar";

/// The drawer button the shell draws at a narrow width.
const DRAWER_BUTTON: &str = "header button.fui-drawer-only";

/// The operator menu's button, drawn once a reader is signed in.
const MENU_BUTTON: &str = "header button[popovertarget='operator-menu']";

/// The translate screen's code system picker, a combobox.
const PICKER: &str = "#translate-system";

/// The list that picker opens.
const PICKER_LIST: &str = "#translate-system-list";

/// A disclosure's summary.
const DISCLOSURE: &str = "main details summary";

/// The width of every book image, in CSS pixels; the height is fitted.
const BOOK_WIDTH: u32 = 1280;

/// The widths of the review, desktop and phone.
const REVIEW_WIDTHS: [u32; 2] = [1440, 390];

/// The window a state is captured in at each review width.
const STATE_WINDOWS: [(u32, u32); 2] = [(1440, 900), (390, 844)];

/// The shortest and tallest a shot may be, in CSS pixels.
const HEIGHT_BOUNDS: (u32, u32) = (700, 9000);

/// A window short enough that any screen overflows it, so the document
/// reports the height of the content rather than of the viewport.
const PROBE_WINDOW: u32 = 400;

/// How many times the window is grown to the document before a shot.
const FIT_ROUNDS: usize = 12;

/// The number the signed-in screens start from, past every reading screen.
const SIGNED_IN_FIRST: usize = 30;

/// One screen the walk captures.
#[derive(Clone, Debug)]
struct Screen {
    /// The image's name, without a number or an extension.
    name: &'static str,
    /// The book image this screen is, if the book embeds it.
    book: Option<&'static str>,
    /// The console address, from the root of the deployment.
    path: String,
}

/// What the walk learned from the listings, so it names no resource itself.
#[derive(Debug)]
struct Subjects {
    /// The canonical of a code system whose version declares `child-of`.
    system: String,
    /// The path of that code system's screen.
    system_path: String,
    /// The path of a value set's screen.
    value_set_path: String,
    /// The canonical of that value set.
    value_set: String,
    /// The path of a concept map's screen.
    concept_map_path: String,
}

/// Where a capture writes, and which variants it takes.
#[derive(Clone, Debug)]
enum Mode {
    /// The book's light images at one width.
    Book(PathBuf),
    /// Every variant, numbered, for review.
    Review(PathBuf),
}

impl Mode {
    /// The themes this mode captures.
    fn themes(&self) -> &'static [&'static str] {
        match self {
            Self::Book(_) => &["light"],
            Self::Review(_) => &["light", "dark"],
        }
    }
}

/// `path` on the captured version, keeping any query it already carries.
fn on_version(path: &str) -> String {
    let joiner = if path.contains('?') { '&' } else { '?' };
    format!("{path}{joiner}fhir={CAPTURED_VERSION}")
}

/// A listing's link without the query it carries, which [`on_version`] puts
/// back.
fn without_query(href: &str) -> String {
    href.split('?').next().unwrap_or_default().to_owned()
}

/// The directory a review image in `theme` goes to.
fn themed(dir: &Path, theme: &str) -> PathBuf {
    if theme == "dark" {
        dir.join("dark")
    } else {
        dir.to_path_buf()
    }
}

/// Fits the window to the screen in front of it at `width`, then shoots.
///
/// The window grows until the document is no taller than the viewport, a
/// few rounds at most, because a pane sized to the viewport grows with it.
/// Each round measures the page only once its height holds, so a list that
/// arrives after the resize (the concept browser's) is in the shot.
async fn fitted(journey: &Journey, width: u32, path: &Path) -> WebDriverResult<()> {
    journey.resize(width, PROBE_WINDOW).await;
    let frame = PROBE_WINDOW.saturating_sub(pixels(journey.viewport_height().await?));
    let mut viewport = PROBE_WINDOW.saturating_sub(frame);
    for _round in 0..FIT_ROUNDS {
        let content = journey
            .steady_height(&format!("{} to hold its height", path.display()))
            .await?;
        let wanted = content.clamp(HEIGHT_BOUNDS.0, HEIGHT_BOUNDS.1);
        if wanted == viewport {
            break;
        }
        viewport = wanted;
        journey.resize(width, viewport.saturating_add(frame)).await;
    }
    // A press earlier on the screen may have scrolled it, and a shot of a
    // scrolled page repeats the sticky top bar.
    journey
        .evaluate("window.scrollTo(0, 0); return '';")
        .await?;
    journey.shot(path).await;
    Ok(())
}

/// A length the browser reported, as whole CSS pixels.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` is the only defined float-to-integer conversion; it saturates at both ends, and the value is rounded and floored at zero first"
)]
fn pixels(length: f64) -> u32 {
    length.round().max(0.0) as u32
}

/// Takes the shots of the screen in front of the browser: the book image, or
/// every review width.
async fn shots(
    journey: &Journey,
    mode: &Mode,
    theme: &str,
    number: usize,
    screen: &Screen,
) -> WebDriverResult<()> {
    match mode {
        Mode::Book(dir) => {
            if let Some(book) = screen.book {
                fitted(journey, BOOK_WIDTH, &dir.join(format!("{book}.png"))).await?;
            }
        }
        Mode::Review(dir) => {
            for width in REVIEW_WIDTHS {
                let file = format!("{number:02}-{}-{width}.png", screen.name);
                fitted(journey, width, &themed(dir, theme).join(file)).await?;
            }
        }
    }
    Ok(())
}

/// Opens `path` on `base` and waits for it to finish answering.
async fn open(journey: &Journey, base: &str, path: &str) {
    journey.reopen(&format!("{base}{}", on_version(path))).await;
    journey
        .element(By::Css("main h1"), "the screen's heading")
        .await;
    journey
        .settled(&format!("{path} to finish answering"))
        .await;
}

/// Reads the subjects from the listings.
async fn subjects(journey: &Journey, base: &str) -> WebDriverResult<Subjects> {
    open(journey, base, "/systems").await;
    let row = journey
        .element(By::XPath(WALKABLE_ROW), "a code system with a hierarchy")
        .await;
    let system_path = row.attr("href").await?.unwrap_or_default();
    row.click().await?;
    let system = journey
        .element(By::XPath(CANONICAL_FACT), "the code system's canonical")
        .await
        .text()
        .await?;
    open(journey, base, "/valuesets").await;
    let link = journey
        .element(By::Css(VALUE_SET_LINK), "a listed value set")
        .await;
    let value_set_path = link.attr("href").await?.unwrap_or_default();
    link.click().await?;
    let value_set = journey
        .element(By::XPath(CANONICAL_FACT), "the value set's canonical")
        .await
        .text()
        .await?;
    open(journey, base, "/conceptmaps").await;
    let concept_map_path = journey
        .element(By::Css(CONCEPT_MAP_LINK), "a listed concept map")
        .await
        .attr("href")
        .await?
        .unwrap_or_default();
    Ok(Subjects {
        system: system.trim().to_owned(),
        system_path: without_query(&system_path),
        value_set_path: without_query(&value_set_path),
        value_set: value_set.trim().to_owned(),
        concept_map_path: without_query(&concept_map_path),
    })
}

/// The screens a visitor reads, in walking order, the concept browser apart.
fn read_screens(subjects: &Subjects) -> Vec<Screen> {
    let system = query_param(&subjects.system);
    let value_set = query_param(&subjects.value_set);
    let screen = |name, book, path: String| Screen { name, book, path };
    let system_path = &subjects.system_path;
    let value_set_path = &subjects.value_set_path;
    let concept_map_path = &subjects.concept_map_path;
    vec![
        screen("dashboard", Some("dashboard"), "/".to_owned()),
        screen("code-systems", Some("code-systems"), "/systems".to_owned()),
        screen("code-system", Some("code-system"), system_path.clone()),
        screen(
            "code-system-support",
            None,
            format!("{system_path}?tab=support"),
        ),
        screen("code-system-json", None, format!("{system_path}?tab=json")),
        screen("value-sets", None, "/valuesets".to_owned()),
        screen("value-set", Some("value-set"), value_set_path.clone()),
        screen(
            "value-set-expansion",
            None,
            format!("{value_set_path}?tab=expansion"),
        ),
        screen("concept-maps", None, "/conceptmaps".to_owned()),
        screen("concept-map", None, concept_map_path.clone()),
        screen(
            "concept-map-mappings",
            Some("concept-map"),
            format!("{concept_map_path}?tab=mappings"),
        ),
        screen(
            "lookup",
            Some("lookup"),
            format!("/lookup?system={system}&code={MAPPED_CODE}"),
        ),
        screen(
            "validate",
            None,
            format!("/validate?on=codesystem&system={system}&code={MAPPED_CODE}"),
        ),
        screen("expand", Some("expand"), format!("/expand?url={value_set}")),
        screen(
            "translate",
            None,
            format!("/translate?system={system}&code={MAPPED_CODE}"),
        ),
        screen("find", None, format!("/find?q={system}")),
        screen("about", Some("about"), "/about".to_owned()),
        screen("about-evidence", None, "/about?tab=evidence".to_owned()),
        screen("about-settings", None, "/about?tab=settings".to_owned()),
        screen("not-found", None, "/no-such-screen".to_owned()),
    ]
}

/// The screens only a signed-in reader is offered.
fn write_screens() -> Vec<Screen> {
    let screen = |name, path: &str| Screen {
        name,
        book: None,
        path: path.to_owned(),
    };
    vec![
        screen("signed-in-dashboard", "/"),
        screen("code-system-editor", "/codesystem"),
        screen("value-set-composer", "/compose"),
        screen("concept-map-editor", "/conceptmap"),
    ]
}

/// Captures `screens` on `base`, numbering from `first`, and returns the next
/// number.
async fn capture(
    journey: &Journey,
    mode: &Mode,
    base: &str,
    screens: &[Screen],
    first: usize,
) -> WebDriverResult<usize> {
    let mut number = first;
    for screen in screens {
        let wanted = matches!(mode, Mode::Review(_)) || screen.book.is_some();
        if wanted {
            for theme in mode.themes() {
                journey.set_cookie(THEME_COOKIE, theme).await?;
                open(journey, base, &screen.path).await;
                shots(journey, mode, theme, number, screen).await?;
            }
        }
        number = number.saturating_add(1);
    }
    journey.set_cookie(THEME_COOKIE, "light").await?;
    Ok(number)
}

/// The concept browser, reached the way a reader reaches it: from a code
/// system, through a search, onto a concept with rows below it.
async fn browse(
    journey: &Journey,
    mode: &Mode,
    base: &str,
    subjects: &Subjects,
    number: usize,
) -> WebDriverResult<()> {
    let screen = Screen {
        name: "browse",
        book: Some("browse"),
        path: subjects.system_path.clone(),
    };
    for theme in mode.themes() {
        journey.set_cookie(THEME_COOKIE, theme).await?;
        open(journey, base, &screen.path).await;
        journey
            .press(By::Css(BROWSE_LINK), "the link into the concept browser")
            .await?;
        journey
            .press(By::Css(SEARCH_RESULT), "a concept the search answered")
            .await?;
        journey
            .address_carrying("code=", "the address to carry the concept")
            .await;
        journey
            .element(By::Css(TREE_ROW), "a row of the tree")
            .await;
        journey
            .settled("the concept browser to finish answering")
            .await;
        shots(journey, mode, theme, number, &screen).await?;
    }
    journey.set_cookie(THEME_COOKIE, "light").await
}

/// The versions of the value set, reached from its screen.
async fn history(
    journey: &Journey,
    mode: &Mode,
    base: &str,
    subjects: &Subjects,
    number: usize,
) -> WebDriverResult<()> {
    let screen = Screen {
        name: "history",
        book: None,
        path: subjects.value_set_path.clone(),
    };
    for theme in mode.themes() {
        journey.set_cookie(THEME_COOKIE, theme).await?;
        open(journey, base, &screen.path).await;
        journey
            .press(
                By::Css(HISTORY_LINK),
                "the link to the value set's versions",
            )
            .await?;
        journey
            .address_carrying("/history", "the versions screen")
            .await;
        journey
            .element(By::Css("main h1"), "the versions screen's heading")
            .await;
        journey.settled("the versions to finish answering").await;
        shots(journey, mode, theme, number, &screen).await?;
    }
    journey.set_cookie(THEME_COOKIE, "light").await
}

/// The hover, focus and open states a full-page capture never shows, in the
/// window a reader has, in both themes.
async fn states(journey: &Journey, dir: &Path, base: &str) -> WebDriverResult<()> {
    for theme in ["light", "dark"] {
        let into = themed(dir, theme).join("states");
        journey.set_cookie(THEME_COOKIE, theme).await?;
        let (width, height) = STATE_WINDOWS[0];
        open(journey, base, "/systems").await;
        journey.resize(width, height).await;
        journey
            .hover(By::Css(ROW_LINK), "a code system row")
            .await?;
        journey.shot(&into.join("hover-row-1440.png")).await;
        journey
            .press(By::Css(COMMAND_BAR), "the command bar")
            .await?;
        journey.shot(&into.join("focus-command-bar-1440.png")).await;
        open(journey, base, "/about").await;
        journey.resize(width, height).await;
        journey.press(By::Css(DISCLOSURE), "a disclosure").await?;
        journey.shot(&into.join("disclosure-open-1440.png")).await;
        for (width, height) in STATE_WINDOWS {
            open(journey, base, "/translate").await;
            journey.resize(width, height).await;
            journey
                .press(By::Css(PICKER), "the code system picker")
                .await?;
            journey
                .visible(By::Css(PICKER_LIST), "the open list of code systems")
                .await;
            journey
                .shot(&into.join(format!("combobox-open-{width}.png")))
                .await;
        }
        let (width, height) = STATE_WINDOWS[1];
        open(journey, base, "/").await;
        journey.resize(width, height).await;
        journey
            .visible(By::Css(DRAWER_BUTTON), "the drawer button")
            .await
            .click()
            .await?;
        journey.shot(&into.join("drawer-open-390.png")).await;
    }
    journey.set_cookie(THEME_COOKIE, "light").await
}

/// The operator menu, open, on a signed-in console.
async fn menu_open(journey: &Journey, dir: &Path, base: &str) -> WebDriverResult<()> {
    for theme in ["light", "dark"] {
        journey.set_cookie(THEME_COOKIE, theme).await?;
        for (width, height) in STATE_WINDOWS {
            open(journey, base, "/").await;
            journey.resize(width, height).await;
            journey
                .visible(By::Css(MENU_BUTTON), "the operator menu's button")
                .await
                .click()
                .await?;
            journey
                .visible(By::Css("#operator-menu"), "the open operator menu")
                .await;
            let file = format!("menu-open-{width}.png");
            journey
                .shot(&themed(dir, theme).join("states").join(file))
                .await;
        }
    }
    journey.set_cookie(THEME_COOKIE, "light").await
}

/// Which mode this run captures in, or `None` when neither is asked for.
fn mode() -> Option<Mode> {
    if let Some(dir) = std::env::var_os(REVIEW_ENV) {
        return Some(Mode::Review(PathBuf::from(dir)));
    }
    std::env::var_os(GATE_ENV)?;
    Some(Mode::Book(match std::env::var(SHOTS_DIR_ENV) {
        Ok(named) => PathBuf::from(named),
        Err(_absent) => Path::new(env!("CARGO_MANIFEST_DIR")).join(BOOK_IMAGES),
    }))
}

/// Captures every screen in walking order, one session on the reading
/// console and, for a review, one on the signed-in console.
#[tokio::test]
async fn the_documentation_screenshots_are_captured() {
    let Some(base) = server() else {
        return;
    };
    let Some(mode) = mode() else {
        println!(
            "skipped: neither {GATE_ENV} nor {REVIEW_ENV} is set, so no image is written. \
             scripts/ui-e2e.sh sets one when it is asked for a capture."
        );
        return;
    };
    let reading = mode.clone();
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &base, "/").await;
            let subjects = subjects(&journey, &base).await?;
            let screens = read_screens(&subjects);
            let (before, after) = screens.split_at(3);
            let next = capture(&journey, &reading, &base, before, 1).await?;
            browse(&journey, &reading, &base, &subjects, next).await?;
            let next = capture(&journey, &reading, &base, after, next.saturating_add(1)).await?;
            if let Mode::Review(dir) = &reading {
                history(&journey, &reading, &base, &subjects, next).await?;
                states(&journey, dir, &base).await?;
            }
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the reading capture ran and the browser session ended cleanly");
    let (Mode::Review(dir), Some(deployment)) = (mode, signed_in()) else {
        return;
    };
    let outcome = session()
        .await
        .run_and_quit(|driver| async move {
            let journey = Journey::open(driver, &deployment.base, "/").await;
            choose(&journey, &deployment, "writer", "docs-shots").await;
            sign_in(&journey, &deployment).await;
            let review = Mode::Review(dir.clone());
            capture(
                &journey,
                &review,
                &deployment.base,
                &write_screens(),
                SIGNED_IN_FIRST,
            )
            .await?;
            menu_open(&journey, &dir, &deployment.base).await?;
            Ok::<(), WebDriverError>(())
        })
        .await;
    outcome.expect("the signed-in capture ran and the browser session ended cleanly");
}
