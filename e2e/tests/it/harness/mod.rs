// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The browser session every journey drives, and the waits it is built from.

use std::time::Duration;

use thirtyfour::LoggingPrefsLogLevel;
use thirtyfour::prelude::*;
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
/// tagged document is gone, `drawn` once a page was drawn in place of the
/// tagged one in the same document, `leaving` while the tagged page is still
/// in front of the browser and another is on its way, `staying` when the
/// press loads nothing.
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

/// The level chromedriver reports for `console.error`, an uncaught error in
/// a module, and a request the page made that failed.
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

/// The shell's own mark, which proves the page drew its shell.
const SHELL_MARK: &str = "header a.fui-lockup";

/// Whether the page in front of the browser has finished loading and, when
/// it is the console's, FerroUI's loader has run over it and marked `<html>`
/// with `data-fui-script` (`ferrohealth_ui::script::MARK`), which it writes
/// before any module arrives, and every element that asks for a module
/// carries `data-fui-ready` (`ferrohealth_ui::script::READY`), so the modules
/// answer the next press. A page that is not the console's (the issuer's, a
/// refusal in plain text) loads no FerroUI stylesheet and is not waited on.
const READY: &str = r#"
return document.readyState === "complete"
    && (document.querySelector("link[href^='/ferrohealth-ui/']") === null
        || (document.documentElement.hasAttribute("data-fui-script")
            && document.querySelector("[data-fui]:not([data-fui-ready])") === null));
"#;

/// Whether the page in front of the browser has finished loading, for a
/// journey whose browser refuses the modules: nothing marks the page.
const LOADED: &str = r#"return document.readyState === "complete";"#;

/// The address pattern of FerroUI's script modules, the loader among them,
/// which a journey without scripts refuses the browser.
const MODULES: &str = "*/ferrohealth-ui/*/scripts/*";

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
            By::Css("a[href^='/login']"),
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
    // it at creation, and that log is where an uncaught module error lands.
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
    /// Whether the browser loads FerroUI's script modules.
    scripts: bool,
}

mod elements;
mod evidence;
mod focus;
mod layout;
mod navigation;

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
