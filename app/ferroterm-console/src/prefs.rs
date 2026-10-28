// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The reader's preferences: the family's six and the console's own three.
//!
//! Each preference is a cookie of its own (FerroUI's family rule 9), named by
//! its key after [`PREFIX`]: the family's theme, density, motion, language,
//! first day of the week and shortcut hints
//! (`ferrohealth_ui::theme::Preferences`), and the console's own default
//! FHIR version, display language and page size, which Your preferences
//! draws inside the library's tabs. The server reads them while it renders,
//! so the theme, the density, the motion and the hints are on `<html>`
//! before any script runs, and the browser keeps no storage of its own. A
//! change is a plain form post the server checks against the values each
//! preference offers. No specification governs this: our own design.

use ferrohealth_ui::theme::Preferences;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::theme::{Density, Hints, Language, Motion, Preference, Theme, WeekStart};

use crate::fhir::version::FhirVersion;

/// The largest page the console asks a server for in one request.
///
/// No specification caps `count`: this is our own design, a bound on how much
/// one page asks the browser to render.
pub(crate) const MAX_COUNT: u32 = 1_000;

/// What every preference cookie's name starts with, before the
/// `__Host-` prefix a `Secure` console adds.
pub(crate) const PREFIX: &str = "ferroterm_console_";

/// The cookie key of the FHIR version an address that names none reads.
pub(crate) const FHIR: &str = "fhir";

/// The cookie key of the language concept names are asked for in.
pub(crate) const DISPLAY_LANGUAGE: &str = "display-language";

/// The cookie key of how many rows a paged screen asks for.
pub(crate) const PAGE_SIZE: &str = "page-size";

/// The cookie key of the sidebar's rail, which is no preference of Your
/// preferences but is kept the same way.
pub(crate) const SIDEBAR: &str = "sidebar";

#[cfg(not(target_arch = "wasm32"))]
/// The console's own preferences, each its cookie key.
pub(crate) const OWN: [&str; 3] = [FHIR, DISPLAY_LANGUAGE, PAGE_SIZE];

/// The page size a reader who set none reads at.
pub(crate) const DEFAULT_PAGE_SIZE: u32 = 25;

/// The longest display language tag the console keeps.
const LANGUAGE_TAG_MAX: usize = 64;

/// What the reader chose, as the request's cookies say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Prefs {
    /// The family's preferences.
    pub(crate) family: Preferences,
    /// The FHIR version an address that names none reads.
    pub(crate) fhir: FhirVersion,
    /// The BCP 47 tag sent as `displayLanguage`, empty for the server's own.
    pub(crate) display_language: String,
    /// How many rows a paged screen asks for.
    pub(crate) page_size: u32,
    /// Whether the navigation is folded to its rail.
    pub(crate) rail: bool,
    /// The FHIR version the top bar chose for the browser's session, which
    /// an address that names none reads before the default.
    pub(crate) session_fhir: Option<FhirVersion>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            family: Preferences::default(),
            fhir: FhirVersion::default(),
            display_language: String::new(),
            page_size: DEFAULT_PAGE_SIZE,
            rail: false,
            session_fhir: None,
        }
    }
}

impl Prefs {
    /// The preferences the `Cookie` header value `cookies` keeps, each under
    /// its key after `prefix`; a missing or unknown value is the default.
    #[must_use]
    pub(crate) fn read(cookies: &str, prefix: &str) -> Self {
        let defaults = Self::default();
        let value = |key: &str| ferrohealth_ui::theme::cookie(cookies, &format!("{prefix}{key}"));
        // NOTE: no specification governs this: our own design; a cookie that names
        // no choice is the choice not made, as a missing one is.
        Self {
            family: Preferences::read(cookies, prefix),
            fhir: value(FHIR)
                .and_then(|key| key.parse::<FhirVersion>().ok())
                .unwrap_or(defaults.fhir),
            display_language: value(DISPLAY_LANGUAGE)
                .and_then(display_language)
                .unwrap_or(defaults.display_language),
            page_size: value(PAGE_SIZE)
                .and_then(page_size)
                .unwrap_or(defaults.page_size),
            rail: value(SIDEBAR) == Some("rail"),
            session_fhir: value(crate::routes::SESSION_FHIR)
                .and_then(|key| key.parse::<FhirVersion>().ok()),
        }
    }
}

/// The page size `text` names, when it is a whole number from 1 to
/// [`MAX_COUNT`].
#[must_use]
pub(crate) fn page_size(text: &str) -> Option<u32> {
    let size = text.trim().parse::<u32>().ok()?;
    (1..=MAX_COUNT).contains(&size).then_some(size)
}

/// The display language `text` names: empty for the server's own choice,
/// or a BCP 47 tag of ASCII letters, digits and hyphens.
#[must_use]
pub(crate) fn display_language(text: &str) -> Option<String> {
    let tag = text.trim();
    // NOTE: RFC 5646 §2.1: a language tag is ASCII letters, digits and hyphens,
    // so anything else is no tag and never reaches a cookie.
    let plain = tag.len() <= LANGUAGE_TAG_MAX
        && tag
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    plain.then(|| tag.to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
/// The value the cookie of the preference `key` keeps for the posted
/// `value`, when the preference offers it, and `None` for anything else.
///
/// `offered` says whether a language tag is one the console ships a catalog
/// for, which the language preference holds a choice to.
#[must_use]
pub(crate) fn kept(key: &str, value: &str, offered: impl Fn(&str) -> bool) -> Option<String> {
    let keyed = |choice: Option<&str>| choice.map(str::to_owned);
    match key {
        Theme::COOKIE => keyed(Theme::from_cookie(value).map(Theme::key)),
        Density::COOKIE => keyed(Density::from_cookie(value).map(Density::key)),
        Motion::COOKIE => keyed(Motion::from_cookie(value).map(Motion::key)),
        WeekStart::COOKIE => keyed(WeekStart::from_cookie(value).map(WeekStart::key)),
        Hints::COOKIE => keyed(Hints::from_cookie(value).map(Hints::key)),
        Language::COOKIE => Language::from_cookie(value)
            .filter(|language| language.tag().is_none_or(&offered))
            .map(|language| language.cookie_value().to_owned()),
        FHIR => value
            .parse::<FhirVersion>()
            .ok()
            .map(|version| version.segment().to_owned()),
        DISPLAY_LANGUAGE => display_language(value),
        PAGE_SIZE => page_size(value).map(|size| size.to_string()),
        _unknown => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Every cookie key Reset to defaults clears: the family's and the
/// console's own.
#[must_use]
pub(crate) fn every_key() -> Vec<&'static str> {
    Preferences::COOKIES.into_iter().chain(OWN).collect()
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::theme::{Density, Theme};

    use super::{DEFAULT_PAGE_SIZE, Prefs, display_language, every_key, kept, page_size};
    use crate::fhir::version::FhirVersion;

    #[test]
    fn a_typed_page_size_is_read() {
        assert_eq!(page_size(" 25 "), Some(25), "surrounding space is trimmed");
    }

    #[test]
    fn a_page_size_outside_the_bounds_is_refused_rather_than_clamped() {
        assert_eq!(page_size("0"), None, "a zero-row page never advances");
        assert_eq!(
            page_size("1001"),
            None,
            "the reader is told the value was not taken"
        );
    }

    #[test]
    fn text_that_is_not_a_number_names_no_page_size() {
        assert_eq!(page_size("twenty"), None);
        assert_eq!(page_size("-5"), None, "a negative count is not a size");
        assert_eq!(page_size(""), None);
    }

    #[test]
    fn a_display_language_is_a_tag_or_nothing() {
        assert_eq!(display_language(" nl-NL "), Some(String::from("nl-NL")));
        assert_eq!(
            display_language(""),
            Some(String::new()),
            "empty is the server's own"
        );
        assert_eq!(display_language("nl NL"), None);
        assert_eq!(display_language("<script>"), None);
    }

    #[test]
    fn the_cookies_are_read_under_their_prefix() {
        let prefs = Prefs::read(
            "x=1; p_theme=dark; p_density=compact; p_fhir=r5; p_display-language=nl; p_page-size=50; p_sidebar=rail",
            "p_",
        );
        assert_eq!(prefs.family.theme, Theme::Dark);
        assert_eq!(prefs.family.density, Density::Compact);
        assert_eq!(prefs.fhir, FhirVersion::R5);
        assert_eq!(prefs.display_language, "nl");
        assert_eq!(prefs.page_size, 50);
        assert!(prefs.rail, "the rail is kept as a preference is");
    }

    #[test]
    fn a_cookie_naming_no_choice_reads_as_the_default() {
        let prefs = Prefs::read("p_theme=sepia; p_fhir=r9; p_page-size=0", "p_");
        assert_eq!(prefs.family.theme, Theme::System);
        assert_eq!(prefs.fhir, FhirVersion::default());
        assert_eq!(prefs.page_size, DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn a_post_keeps_only_a_value_its_preference_offers() {
        let english = |tag: &str| tag == "en";
        assert_eq!(kept("theme", "dark", english), Some(String::from("dark")));
        assert_eq!(kept("theme", "sepia", english), None);
        assert_eq!(kept("language", "en", english), Some(String::from("en")));
        assert_eq!(
            kept("language", "browser", english),
            Some(String::from("browser"))
        );
        assert_eq!(
            kept("language", "nl", english),
            None,
            "no Dutch catalog ships"
        );
        assert_eq!(kept("fhir", "R6", english), Some(String::from("r6")));
        assert_eq!(kept("page-size", "100", english), Some(String::from("100")));
        assert_eq!(kept("page-size", "0", english), None);
        assert_eq!(
            kept("sidebar", "rail", english),
            None,
            "the rail has its own route"
        );
    }

    #[test]
    fn reset_clears_the_familys_and_the_consoles_own() {
        let keys = every_key();
        for key in [
            "theme",
            "density",
            "motion",
            "language",
            "week-start",
            "hints",
            "fhir",
            "display-language",
            "page-size",
        ] {
            assert!(keys.contains(&key), "{key}");
        }
    }
}
