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
use ferrohealth_ui::island::PreferenceForm;
use ferrohealth_ui::view::preferences::{
    Post, PreferenceRow, Section, SectionRoutes, SelectPreference, preference_row,
    preferences_page, select_preference,
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::prefs::{DISPLAY_LANGUAGE, FHIR, PAGE_SIZE};
use crate::request::{Request, request};
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
fn own_rows(request: &Request, post: &Post, saved: Option<&str>) -> Vec<AnyView> {
    let fhir_saved = saved == Some(FHIR);
    let language_saved = saved == Some(DISPLAY_LANGUAGE);
    let size_saved = saved == Some(PAGE_SIZE);
    let fhir = select_preference(SelectPreference {
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
    });
    let typed = |name: &'static str,
                 value: String,
                 input: &'static str,
                 placeholder: Option<String>,
                 saved: bool| {
        let control = view! {
            <input
                id=name
                name=name
                type=input
                class="fui-input fui-setting-select"
                value=value
                placeholder=placeholder
                autocomplete="off"
                spellcheck="false"
            />
        }
        .into_any();
        view! {
            <PreferenceForm
                action=post.action.clone()
                back=post.back.clone()
                save=tr(Msg::PreferencesSave)
                saved
            >
                {control}
            </PreferenceForm>
        }
        .into_any()
    };
    let language = typed(
        DISPLAY_LANGUAGE,
        request.prefs.display_language.clone(),
        "text",
        Some(tr(Msg::PreferencesDisplayLanguageExample)),
        language_saved,
    );
    let size = typed(
        PAGE_SIZE,
        request.prefs.page_size.to_string(),
        "number",
        None,
        size_saved,
    );
    vec![
        preference_row(PreferenceRow {
            label: tr(Msg::PreferencesDefaultFhir).into(),
            description: tr(Msg::PreferencesDefaultFhirLine).into(),
            control_id: Some(String::from(FHIR)),
            control: fhir,
            saved: fhir_saved,
        }),
        preference_row(PreferenceRow {
            label: tr(Msg::PreferencesDisplayLanguage).into(),
            description: tr(Msg::PreferencesDisplayLanguageLine).into(),
            control_id: Some(String::from(DISPLAY_LANGUAGE)),
            control: language,
            saved: language_saved,
        }),
        preference_row(PreferenceRow {
            label: tr(Msg::PreferencesPageSize).into(),
            description: tr(Msg::PreferencesPageSizeLine).into(),
            control_id: Some(String::from(PAGE_SIZE)),
            control: size,
            saved: size_saved,
        }),
    ]
}

/// Your preferences, on the tab the route names.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn PreferencesPage() -> impl IntoView {
    let request = request();
    let params = use_params_map();
    let segment = params.with_untracked(|params| params.get("section"));
    let Some(section) = section(segment.as_deref()) else {
        return crate::pages::errors::not_found_page();
    };
    let routes = routes();
    let saved = request.param("saved");
    let post = Post {
        action: String::from(PREFERENCE_ROUTE),
        back: routes.of(section).to_owned(),
    };
    let own = match section {
        Section::Language => own_rows(&request, &post, saved.as_deref()),
        Section::Appearance | Section::Keyboard => Vec::new(),
    };
    let page = preferences_page(ferrohealth_ui::view::preferences::PreferencesPage {
        section,
        routes,
        reading: false,
        preferences: request.prefs.family.clone(),
        action: String::from(PREFERENCE_ROUTE),
        saved,
        accent: tr(Msg::PreferencesAccent),
        languages: crate::locale::languages(),
        browser: crate::locale::requested(),
        week_from_language: Weekday::Monday,
        shortcuts: Some(ui_path(SHORTCUTS_PATH)),
        reset: String::from(RESET_ROUTE),
        own,
    });
    view! {
        <Title text=tr(Msg::PreferencesTitle) />
        {page}
    }
    .into_any()
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
