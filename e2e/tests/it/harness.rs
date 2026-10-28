// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The browser session every journey drives, and the waits it is built from.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use thirtyfour::LoggingPrefsLogLevel;
use thirtyfour::common::keys::TypingData;
use thirtyfour::error::WebDriverErrorInner;
use thirtyfour::prelude::*;
use thirtyfour::stringmatch::Needle;
use thirtyfour::stringmatch::StringMatch;

/// Names the server under test, as a URL without a trailing slash.
pub const BASE_URL_ENV: &str = "FERROTERM_UI_E2E_BASE_URL";

/// Names the WebDriver endpoint the journeys drive the browser through.
pub const WEBDRIVER_ENV: &str = "FERROTERM_UI_E2E_WEBDRIVER";

/// Names the second server, the one with a SMART issuer configured.
pub const SIGNED_IN_BASE_URL_ENV: &str = "FERROTERM_UI_E2E_SIGNED_IN_BASE_URL";

/// Names the stub identity provider that second server trusts.
pub const ISSUER_ENV: &str = "FERROTERM_UI_E2E_ISSUER";

/// Names the Chrome binary the session starts, when it is not where
/// chromedriver looks for one.
pub const CHROME_ENV: &str = "FERROTERM_UI_E2E_CHROME";

/// The WebDriver endpoint used when [`WEBDRIVER_ENV`] is unset.
const DEFAULT_WEBDRIVER: &str = "http://127.0.0.1:4444";

/// Names the directory a failure writes its evidence into.
pub const FAILURES_ENV: &str = "FERROTERM_UI_E2E_FAILURES";

/// Where the evidence goes when nothing names a directory.
const DEFAULT_FAILURES: &str = "target/ui-e2e-failures";

/// How many words of a wait's description name its evidence files.
///
/// Enough to tell two failures apart in an artifact listing, short of a file
/// name nothing can read.
const SLUG_WORDS: usize = 8;

/// How much of a failing page's markup the log carries.
///
/// Enough to see which sections rendered and what they held, without burying the
/// assertion that failed. The whole document goes to the evidence file beside
/// it, so nothing is lost by trimming here.
const MARKUP_IN_A_FAILURE: usize = 20_000;

/// How long a wait keeps trying before the journey fails.
const WAIT: Duration = Duration::from_secs(30);

/// How many tab presses a walk to one control allows itself.
///
/// More than a screen has controls, so a control that is reachable is reached
/// whichever stop the walk starts from, and a walk that never arrives ends.
const TAB_LIMIT: usize = 200;

/// The control the keyboard is on, as its id, its name, and its own words.
const FOCUSED: &str = r"
const on = document.activeElement;
if (!on) { return ''; }
return [on.id || '', on.getAttribute('name') || '', (on.textContent || '').trim()]
  .filter((part) => part !== '').join(' ');
";

/// Tags the page in front of the browser before a press, and records on the
/// tag whether the press starts loading another page: a navigation the
/// Navigation API reports, or a form submission nothing cancelled, which
/// navigates only in a later task
/// (<https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#form-submission-algorithm>).
const TAG: &str = r#"
if (window.__journeyPress === undefined) {
  navigation.addEventListener("navigate", (event) => {
    if (!event.hashChange) {
      window.__journeyPress.navigating = true;
      window.__journeyPress.inPlace = event.destination.sameDocument;
    }
  });
  addEventListener("submit", (event) => { window.__journeyPress.submits.push(event); }, true);
}
window.__journeyPress = {
  navigating: false,
  inPlace: false,
  submits: [],
  page: document.querySelector("main .fui-column > *") ?? document.querySelector("main > *"),
};
"#;

/// Where the press [`TAG`] watched has left the browser: `loaded` once the
/// tagged document is gone, `drawn` once the router has drawn another page
/// in place of the tagged one, `leaving` while the tagged page is still in
/// front of the browser and another is on its way, `staying` when the press
/// loads nothing.
const LEFT: &str = r#"
const press = window.__journeyPress;
if (press === undefined) { return "loaded"; }
const posted = press.submits.some((event) => !event.defaultPrevented
  && (event.submitter?.getAttribute("formmethod") ?? event.target.method).toLowerCase() !== "dialog");
if (!press.navigating && !posted) { return "staying"; }
const now = document.querySelector("main .fui-column > *") ?? document.querySelector("main > *");
if (press.inPlace && !posted && now !== null && (press.page === null || !press.page.isConnected)) {
  return "drawn";
}
return "leaving";
"#;

/// How often a wait re-reads the page while it waits.
const POLL: Duration = Duration::from_millis(100);

/// How often [`Journey::element`] looks an element up again after a reload
/// replaced it.
const STALE_TRIES: usize = 3;

/// The level chromedriver reports for `console.error` and for an uncaught
/// panic, which `console_error_panic_hook` turns into one.
const SEVERE: &str = "SEVERE";

/// The flags the browser is started with.
///
/// `--headless=new` is Chrome's current headless mode; the other two are what
/// a browser inside a container needs, because the kernel sandbox and the
/// default 64 MB `/dev/shm` are both unavailable there
/// (<https://developer.chrome.com/docs/chromium/headless>).
const CHROME_ARGS: [&str; 4] = [
    "--headless=new",
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--window-size=1280,900",
];

/// The narrowest window Chrome draws, in CSS pixels; a narrower viewport is
/// emulated.
const NARROWEST_WINDOW: u32 = 500;

/// The `fhirUser` the stub issuer signs everyone in as.
const SIGNED_IN_AS: &str = "Practitioner/e2e-terminologist";

/// The control that starts a sign-in: a link to the console's own sign-in
/// route, in the top bar.
const SIGN_IN_CONTROL: &str = "//a[normalize-space()='Sign in']";

/// The shell's own mark, which proves a bundle booted.
const SHELL_MARK: &str = "header a.fui-lockup";

/// Whether the page in front of the browser has finished loading and, when
/// it is the console's, FerroUI has marked it hydrated
/// (`data-ferroui-hydrated`). A page that is not the console's (the issuer's,
/// a refusal in plain text) loads no FerroUI stylesheet and is not waited on.
const HYDRATED: &str = r#"
return document.readyState === "complete"
    && (document.documentElement.hasAttribute("data-ferroui-hydrated")
        || document.querySelector("link[href='/ferrohealth-ui.css']") === null);
"#;

/// Opens the issuer's profile address, which decides what the sign-in grants.
///
/// `profile` is the issuer's own vocabulary: `writer` for the full grant,
/// `reader` for the identity scopes alone, `response-only` for a grant the
/// answer states and the token does not carry. `tag` is the journey's name,
/// which keeps the revocations it reads back its own while the others run
/// beside it.
pub async fn choose(journey: &Journey, deployment: &SignedIn, profile: &str, tag: &str) {
    let address = format!("{}/profile?name={profile}&tag={tag}", deployment.issuer);
    journey.reopen(&address).await;
    journey
        .text_becoming(
            By::Css("#profile"),
            StringMatch::new(profile).partial(),
            "the issuer to take the grant this journey signs in for",
        )
        .await;
}

/// Presses the sign-in control and waits for the shell to name the reader.
///
/// The console signs the reader in on its own server: the control leads to
/// its sign-in page, whose control leads to the sign-in route, the issuer
/// sends the browser back to the console's callback, and the page the
/// console then draws names the reader.
pub async fn sign_in(journey: &Journey, deployment: &SignedIn) -> String {
    journey.reopen(&format!("{}/", deployment.base)).await;
    journey
        .element(By::Css(SHELL_MARK), "the shell mark on the overview")
        .await;
    let control = journey
        .element(By::XPath(SIGN_IN_CONTROL), "the sign-in control")
        .await;
    journey
        .press_on(&control, "the sign-in control")
        .await
        .unwrap_or_else(|error| {
            panic!("the sign-in control refused the press: {error}");
        });
    journey
        .press(
            By::Css("a[href^='/auth/login']"),
            "the sign-in page's way to the identity provider",
        )
        .await
        .unwrap_or_else(|error| {
            panic!("the sign-in page's control refused the press: {error}");
        });
    journey
        .text_becoming(
            By::XPath("//*[contains(text(), 'Practitioner/')]"),
            StringMatch::new(SIGNED_IN_AS).partial(),
            "the shell to name whoever signed in",
        )
        .await
}

/// The server under test, or `None` when nothing names one.
pub fn server() -> Option<String> {
    match std::env::var(BASE_URL_ENV) {
        Ok(base) => Some(base.trim_end_matches('/').to_owned()),
        Err(_absent) => {
            println!(
                "skipped: {BASE_URL_ENV} is unset, so there is no served console to drive. \
                 scripts/ui-e2e.sh sets it, and the ui-e2e CI job always runs that."
            );
            None
        }
    }
}

/// A path segment unique to this run: `stem`, then the process id.
///
/// nextest runs each journey in its own process, so the id holds for one
/// journey and differs on the next run. A store that persists between runs
/// then never answers with the copy an earlier run left under the same name.
pub fn run_slug(stem: &str) -> String {
    format!("{stem}-{}", std::process::id())
}

/// A canonical unique to this run, on a synthetic host.
///
/// Synthetic: the repository distributes no terminology content, and every
/// write journey authors the resource it then reads back.
pub fn run_canonical(stem: &str) -> String {
    format!("https://terminology.example/{}", run_slug(stem))
}

/// `value` percent-encoded as one query parameter.
pub fn query_param(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// A deployment that asks a reader to sign in, and the issuer it trusts.
#[derive(Clone, Debug)]
pub struct SignedIn {
    /// The server under test, as a URL without a trailing slash.
    pub base: String,
    /// The stub identity provider, as a URL without a trailing slash.
    pub issuer: String,
}

/// The signed-in deployment, or `None` when nothing names one.
///
/// It is a second server beside the one [`server`] names: that one configures
/// no issuer, which is the deployment the read-only journeys drive, and a
/// server cannot be both at once.
pub fn signed_in() -> Option<SignedIn> {
    // An empty value is what the harness script passes when it started no
    // issuer, and `var` answers that as a value rather than as absent.
    let named = |variable| std::env::var(variable).ok().filter(|set| !set.is_empty());
    match (named(SIGNED_IN_BASE_URL_ENV), named(ISSUER_ENV)) {
        (Some(base), Some(issuer)) => Some(SignedIn {
            base: base.trim_end_matches('/').to_owned(),
            issuer: issuer.trim_end_matches('/').to_owned(),
        }),
        _absent => {
            println!(
                "skipped: {SIGNED_IN_BASE_URL_ENV} and {ISSUER_ENV} are unset, so there is no \
                 signed-in deployment to drive. scripts/ui-e2e.sh sets both in its managed mode, \
                 and the ui-e2e CI job always runs that."
            );
            None
        }
    }
}

/// A browser session on the configured WebDriver endpoint.
pub async fn session() -> WebDriver {
    let endpoint =
        std::env::var(WEBDRIVER_ENV).unwrap_or_else(|_absent| DEFAULT_WEBDRIVER.to_owned());
    let mut capabilities = DesiredCapabilities::chrome();
    for argument in CHROME_ARGS {
        capabilities
            .add_arg(argument)
            .expect("chrome accepts a command-line argument");
    }
    if let Ok(binary) = std::env::var(CHROME_ENV)
        && !binary.is_empty()
    {
        capabilities
            .set_binary(&binary)
            .expect("chrome accepts a binary path");
    }
    // chromedriver records the browser log only for a session that asked for
    // it at creation, and that log is where an uncaught panic lands.
    capabilities
        .set_browser_log_level(LoggingPrefsLogLevel::All)
        .expect("chrome accepts a logging preference");
    WebDriver::new(&endpoint, capabilities)
        .await
        .unwrap_or_else(|error| panic!("no browser session at {endpoint}: {error}"))
}

/// `what` as a file name: lowercase words joined by hyphens.
///
/// A wait describes itself in a sentence, and that sentence is what names the
/// evidence, so the file in the artifact says which wait wrote it.
fn slug(what: &str) -> String {
    let name: String = what
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed: String = name
        .split('-')
        .filter(|word| !word.is_empty())
        .take(SLUG_WORDS)
        .collect::<Vec<&str>>()
        .join("-");
    if trimmed.is_empty() {
        "failure".to_owned()
    } else {
        trimmed
    }
}

/// One browser session pointed at the server under test.
#[derive(Debug)]
pub struct Journey {
    /// The session every step drives.
    driver: WebDriver,
}

impl Journey {
    /// Opens `path` on `base` and returns the journey over it.
    pub async fn open(driver: WebDriver, base: &str, path: &str) -> Self {
        let address = format!("{base}{path}");
        driver
            .goto(address.clone())
            .await
            .unwrap_or_else(|error| panic!("the browser could not open {address}: {error}"));
        let journey = Self { driver };
        journey.hydrated().await;
        journey
    }

    /// Waits, on a console page, until the browser half has attached to it.
    ///
    /// The server sends a whole page, so its controls are on screen before
    /// their handlers are; a key typed in between reaches the field and never
    /// the form's state. FerroUI marks `<html>` with `data-ferroui-hydrated`
    /// once the page and its lazy route have hydrated. A page that is not the
    /// console's (the issuer's, a refusal in plain text) is not waited on. A
    /// document still loading is waited on, because its script may not have
    /// run yet.
    async fn hydrated(&self) {
        // The page is read whole on every poll, so a navigation still settling
        // when the wait starts is judged on the page it lands on, and a script
        // the navigation interrupts reads as not yet hydrated.
        let started = std::time::Instant::now();
        while started.elapsed() <= WAIT {
            let ready = self
                .driver
                .execute(HYDRATED, Vec::new())
                .await
                .is_ok_and(|answer| answer.convert::<bool>().is_ok_and(|ready| ready));
            if ready {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        panic!(
            "{}",
            self.failure("the page to hydrate", "no data-ferroui-hydrated mark")
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
        self.hydrated().await;
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

    /// The first element matching `selector`, once the bundle has rendered it.
    ///
    /// The wait is the library's own poller over the element condition, so
    /// nothing here sleeps for a fixed time and hopes. A press may have loaded
    /// a whole new page, so the wait starts with that page's hydration.
    pub async fn element(&self, selector: By, what: &str) -> WebElement {
        // A page a press reloads can replace the element between finding it
        // and centring it, so a stale element is looked up again.
        for _ in 0..STALE_TRIES {
            self.hydrated().await;
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
        self.hydrated().await;
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
    /// returns once the page the press loads, if any, has hydrated.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused the press.
    pub async fn press(&self, selector: By, what: &str) -> WebDriverResult<()> {
        let element = self.element(selector, what).await;
        self.press_on(&element, what).await
    }

    /// Presses `element`, which a journey already holds, and returns once the
    /// page the press loads, if any, has hydrated.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error if the browser refused the press.
    pub async fn press_on(&self, element: &WebElement, what: &str) -> WebDriverResult<()> {
        self.acting(what, || element.click()).await
    }

    /// Runs `act`, a press or keys, on a tagged page, and returns once the
    /// page it loads, if any, has hydrated.
    ///
    /// A click can return while the page it leaves is still in front of the
    /// browser, hydrated, and carrying the controls the next step looks for,
    /// so every wait after it would pass on a page about to go away. The tag
    /// goes with that document and marks the page drawn in it, which is how
    /// the next one is told apart: a new document, or a page the router drew
    /// in place of the marked one.
    async fn acting<F, A>(&self, what: &str, act: F) -> WebDriverResult<()>
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
                    self.hydrated().await;
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
    /// carrying `value` is chosen, the hydrated page keeps it on the change,
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
        self.hydrated().await;
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
        self.hydrated().await;
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
    /// document has finished loading, which `hydrated` reads, and whether a
    /// press is loading another page, which `acting` reads. They are read
    /// through a script (<https://www.w3.org/TR/webdriver2/#execute-script>)
    /// and nothing else is.
    pub async fn evaluate(&self, script: &str) -> WebDriverResult<String> {
        self.driver.execute(script, Vec::new()).await?.convert()
    }

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
    /// A shortcut answers only once the page has hydrated, and the last press
    /// may have loaded a page whose controls are on screen before then, so
    /// the keys wait for the page. A key that submits a form returns once the
    /// page it loads has hydrated, as a press does.
    ///
    /// # Errors
    ///
    /// Returns the WebDriver error when the browser refused the keys.
    pub async fn type_here(&self, text: &str) -> WebDriverResult<()> {
        self.hydrated().await;
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
    async fn console_errors(&self) -> Vec<String> {
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
    async fn failure(&self, what: &str, reason: &str) -> String {
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
    async fn evidence(&self, what: &str, source: Option<&str>) -> String {
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

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_waits_description_names_a_file_that_a_filesystem_accepts() {
        assert_eq!(
            slug("the switcher to mark R4B, the default version, as current"),
            "the-switcher-to-mark-r4b-the-default-version"
        );
        assert_eq!(
            slug("a code system card on the overview"),
            "a-code-system-card-on-the-overview"
        );
        assert_eq!(
            slug("/systems/x?fhir=r4b"),
            "systems-x-fhir-r4b",
            "a description carrying an address leaves no separator in the name"
        );
        assert_eq!(slug("..."), "failure", "a name is never empty");
    }
}
