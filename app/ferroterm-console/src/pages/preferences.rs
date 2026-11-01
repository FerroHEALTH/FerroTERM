// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Your preferences: the library's three tabs, with the console's own
//! preferences in Language and region.
//!
//! The page is server HTML; each preference posts to the console's
//! preference route, which keeps it in its cookie and returns with `?saved=`
//! naming it, so the row says Saved. The console's own are the FHIR version
//! an address that names none reads, the language concept names are asked
//! for in, and how many rows a paged screen asks for. No specification
//! governs the page: our own design.

use ferrohealth_ui::calendar::Weekday;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::preferences::{
    Post, PreferenceRow, Section, SectionRoutes, SelectPreference, Typed, TypedPreference,
    preference_row, preferences_page, select_preference, typed_preference,
};

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::prefs::{DISPLAY_LANGUAGE, FHIR, LANGUAGE_TAG_MAX, MAX_COUNT, PAGE_SIZE};
use crate::request::Request;
use crate::routes::{PREFERENCE_ROUTE, RESET_ROUTE, SHORTCUTS_PATH, ui_path};

/// The route segment of Language and region, as the console names it.
const READING: &str = "reading";

/// The tab the route's segment names: none for Appearance, `reading` for
/// Language and region, `keyboard` for Keyboard.
#[must_use]
pub(crate) fn section(segment: Option<&str>) -> Option<Section> {
    match segment {
        None => Some(Section::Appearance),
        Some(READING) => Some(Section::Language),
        Some(key) => Section::parse(key).filter(|section| *section != Section::Language),
    }
}

/// The routes of the three tabs.
fn routes() -> SectionRoutes {
    let root = ui_path(crate::routes::PREFERENCES_PATH);
    SectionRoutes {
        appearance: root.clone(),
        language: format!("{root}/{READING}"),
        keyboard: format!("{root}/{}", Section::Keyboard.key()),
    }
}

/// The console's own rows of Language and region.
fn own_rows(request: &Request, post: &Post, saved: Option<&str>) -> Vec<Html> {
    let locale = &request.locale;
    let fhir_saved = saved == Some(FHIR);
    let language_saved = saved == Some(DISPLAY_LANGUAGE);
    let size_saved = saved == Some(PAGE_SIZE);
    let fhir = select_preference(
        locale,
        SelectPreference {
            name: String::from(FHIR),
            choices: FhirVersion::ALL
                .into_iter()
                .map(|version| {
                    (
                        version.segment().to_owned(),
                        format!("{} · {}", version.label(), version.release()).into(),
                    )
                })
                .collect(),
            current: request.prefs.fhir.segment().to_owned(),
            post: post.clone(),
            saved: fhir_saved,
        },
    );
    // NOTE: FerroUI `docs/consoles.md`, "Your own preferences": the browser holds each
    // field to the bounds `prefs::kept` checks again on the post.
    let language = typed_preference(
        locale,
        TypedPreference {
            name: String::from(DISPLAY_LANGUAGE),
            kind: Typed::Text {
                min_length: None,
                max_length: Some(LANGUAGE_TAG_MAX),
            },
            value: request.prefs.display_language.clone(),
            placeholder: Some(tr(locale, Msg::PreferencesDisplayLanguageExample)),
            post: post.clone(),
            saved: language_saved,
        },
    );
    let size = typed_preference(
        locale,
        TypedPreference {
            name: String::from(PAGE_SIZE),
            kind: Typed::Number {
                min: Some(1),
                max: Some(i64::from(MAX_COUNT)),
                step: None,
            },
            value: request.prefs.page_size.to_string(),
            placeholder: None,
            post: post.clone(),
            saved: size_saved,
        },
    );
    vec![
        preference_row(
            locale,
            PreferenceRow {
                label: tr(locale, Msg::PreferencesDefaultFhir).into(),
                description: tr(locale, Msg::PreferencesDefaultFhirLine).into(),
                control_id: Some(String::from(FHIR)),
                control: fhir,
                saved: fhir_saved,
            },
        ),
        preference_row(
            locale,
            PreferenceRow {
                label: tr(locale, Msg::PreferencesDisplayLanguage).into(),
                description: tr(locale, Msg::PreferencesDisplayLanguageLine).into(),
                control_id: Some(String::from(DISPLAY_LANGUAGE)),
                control: language,
                saved: language_saved,
            },
        ),
        preference_row(
            locale,
            PreferenceRow {
                label: tr(locale, Msg::PreferencesPageSize).into(),
                description: tr(locale, Msg::PreferencesPageSizeLine).into(),
                control_id: Some(String::from(PAGE_SIZE)),
                control: size,
                saved: size_saved,
            },
        ),
    ]
}

/// Your preferences, on the tab the route names.
pub(crate) fn page(request: &Request) -> Html {
    let locale = &request.locale;
    let Some(section) = section(request.segment(1).as_deref()) else {
        return crate::pages::errors::not_found_page(request);
    };
    let routes = routes();
    let saved = request.param("saved");
    let post = Post {
        action: String::from(PREFERENCE_ROUTE),
        back: routes.of(section).to_owned(),
    };
    let own = match section {
        Section::Language => own_rows(request, &post, saved.as_deref()),
        Section::Appearance | Section::Keyboard => Vec::new(),
    };
    request.answering.title(&tr(locale, Msg::PreferencesTitle));
    preferences_page(
        locale,
        ferrohealth_ui::view::preferences::PreferencesPage {
            section,
            routes,
            reading: false,
            preferences: request.prefs.family.clone(),
            action: String::from(PREFERENCE_ROUTE),
            saved,
            accent: tr(locale, Msg::PreferencesAccent),
            languages: crate::locale::languages(),
            browser: request.browser.clone(),
            week_from_language: Weekday::Monday,
            shortcuts: Some(ui_path(SHORTCUTS_PATH)),
            reset: String::from(RESET_ROUTE),
            own,
        },
    )
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::preferences::Section;

    use super::section;

    #[test]
    fn each_tab_is_a_route_and_language_is_named_reading() {
        assert_eq!(Some(Section::Appearance), section(None));
        assert_eq!(Some(Section::Language), section(Some("reading")));
        assert_eq!(Some(Section::Keyboard), section(Some("keyboard")));
        assert_eq!(
            None,
            section(Some("language")),
            "the console names the tab Reading's route"
        );
        assert_eq!(None, section(Some("account")), "a console never adds a tab");
    }
}
