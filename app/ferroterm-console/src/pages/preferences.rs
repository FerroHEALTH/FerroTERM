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
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::preferences::{
    Post, PreferenceRow, Section, SectionRoutes, SelectPreference, preference_row,
    preferences_page, select_preference,
};

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::markup::{h, t};
use crate::pages::operations::hidden_field;
use crate::prefs::{DISPLAY_LANGUAGE, FHIR, PAGE_SIZE};
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
    let typed = |name: &'static str,
                 value: String,
                 input: &'static str,
                 placeholder: Option<String>| {
        // TODO(#946): draw these with FerroUI's typed preference once FerroUI#527 ships.
        h(format!(
            r#"<form method="post" action="{}" class="fui-preference-form">{}<input id="{name}" name="{name}" type="{input}" class="fui-input fui-setting-select" value="{}"{} autocomplete="off" spellcheck="false"><button type="submit" class="{}">{}</button></form>"#,
            t(&post.action),
            hidden_field(crate::routes::BACK_PARAM, &post.back),
            t(value),
            placeholder
                .map(|words| format!(r#" placeholder="{}""#, t(words)))
                .unwrap_or_default(),
            Variant::Secondary.small(),
            t(tr(locale, Msg::PreferencesSave)),
        ))
    };
    let language = typed(
        DISPLAY_LANGUAGE,
        request.prefs.display_language.clone(),
        "text",
        Some(tr(locale, Msg::PreferencesDisplayLanguageExample)),
    );
    let size = typed(
        PAGE_SIZE,
        request.prefs.page_size.to_string(),
        "number",
        None,
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
