// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Look up: one code read in the code system it belongs to, with
//! `CodeSystem/$lookup` (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>).

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::small_action_button;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::fhir::CODE_SYSTEM;
use crate::fhir::concept::{Concept, LookupRequest};
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, p, strong, t};
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::failure;
use crate::request::Request;
use crate::routes::{BROWSE_PATH, SYSTEM_PARAM, SYSTEM_VERSION_PARAM, system_tool_link};

/// The address parameter carrying the code to read.
pub(crate) const CODE_PARAM: &str = "code";

/// The address parameter carrying the language displays are wanted in.
const LANGUAGE_PARAM: &str = "displayLanguage";

/// The Look up tab.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let named = request
        .client
        .published_names(request.version, CODE_SYSTEM)
        .await
        .ok();
    let systems = choices(named.as_ref());
    let system = request.text(SYSTEM_PARAM);
    let system = if system.is_empty() {
        systems
            .first()
            .map(|(value, _)| value.clone())
            .unwrap_or_default()
    } else {
        system
    };
    let card = request
        .client
        .terminology_capabilities(request.version)
        .await
        .ok()
        .and_then(|capabilities| capabilities.card(&system));
    let versions = versions(
        locale,
        named
            .as_ref()
            .map(|named| named.versions(&system))
            .unwrap_or_default(),
        card.as_ref(),
    );
    let version = request.text(SYSTEM_VERSION_PARAM);
    let code = request.text(CODE_PARAM);
    let language = request.text(LANGUAGE_PARAM);
    let languages = languages(card.as_ref());
    let fields = Html::join([
        select(
            "lookup-system",
            SYSTEM_PARAM,
            tr(locale, Msg::LookupSystem),
            &systems,
            &system,
            None,
        ),
        select(
            "lookup-version",
            SYSTEM_VERSION_PARAM,
            tr(locale, Msg::LookupVersion),
            &versions,
            &version,
            Some(tr(locale, Msg::LookupServerVersion)),
        ),
        text(
            "lookup-code",
            CODE_PARAM,
            tr(locale, Msg::LookupCode),
            &code,
            tr(locale, Msg::LookupCodeHint),
        ),
        select(
            "lookup-language",
            LANGUAGE_PARAM,
            tr(locale, Msg::LookupLanguage),
            &languages,
            &language,
            Some(tr(locale, Msg::LookupServerLanguage)),
        ),
    ]);
    let form = options(
        &request,
        &[],
        fields,
        run(
            locale,
            "lookup-run",
            tr(locale, Msg::OpsLookUp),
            Glyph::SEARCH,
            "$lookup",
        ),
    );
    let answer = if system.is_empty() || code.is_empty() {
        before(locale, tr(locale, Msg::LookupBefore))
    } else {
        let asked = LookupRequest {
            system: system.clone(),
            system_version: Some(version.clone()).filter(|text| !text.is_empty()),
            code: code.clone(),
            display_language: Some(language).filter(|text| !text.is_empty()),
        };
        answer(
            &request,
            &asked,
            named
                .as_ref()
                .and_then(|named| named.names().get(&system).cloned()),
        )
        .await
    };
    frame(&request, Op::Lookup, form, answer)
}

/// The languages a picker offers: every language a served version of the
/// system `card` declares, once each.
fn languages(card: Option<&SystemCard>) -> Vec<(String, String)> {
    card.map(|card| {
        let mut tags: Vec<String> = card
            .versions
            .iter()
            .flat_map(|row| row.languages.clone())
            .collect();
        tags.sort();
        tags.dedup();
        tags.into_iter()
            .map(|tag| (tag.clone(), tag))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

/// The versions a picker offers: each published version, the one an
/// unversioned request resolves to marked as the default.
fn versions(
    locale: &Locale,
    published: Vec<crate::fhir::named::Choice>,
    card: Option<&SystemCard>,
) -> Vec<(String, String)> {
    let default = card
        .and_then(|card| card.versions.iter().find(|row| row.is_default))
        .and_then(|row| row.code.clone());
    let mut offered: Vec<String> = published
        .into_iter()
        .map(|choice| choice.canonical)
        .collect();
    if let Some(card) = card {
        offered.extend(card.versions.iter().filter_map(|row| row.code.clone()));
    }
    offered.sort();
    offered.dedup();
    offered
        .into_iter()
        .map(|version| {
            let words = if default.as_deref() == Some(version.as_str()) {
                tr_with(
                    locale,
                    Msg::LookupDefaultVersion,
                    &[("version", version.clone().into())],
                )
            } else {
                version.clone()
            };
            (version, words)
        })
        .collect()
}

/// The answer to `asked`: the concept found, or the refusal.
async fn answer(request: &Request, asked: &LookupRequest, name: Option<String>) -> Html {
    let locale = &request.locale;
    let url = request.client.lookup_url(request.version, asked);
    match request.client.lookup(request.version, asked).await {
        Err(error) => Html::join([
            panel_section(locale, None, failure(locale, &error)),
            sent(locale, &url, None),
        ]),
        Ok(document) => {
            let concept = document.value.concept();
            let system = name
                .or_else(|| concept.name.clone())
                .unwrap_or_else(|| asked.system.clone());
            let open = small_action_button(
                locale,
                Action::new(
                    Some(Glyph::NETWORK),
                    tr(locale, Msg::LookupOpenInBrowser),
                    Act::Link(format!(
                        "{}&{CODE_PARAM}={}",
                        system_tool_link(
                            BROWSE_PATH,
                            &asked.system,
                            asked.system_version.as_deref(),
                            request.version
                        ),
                        percent(&asked.code)
                    )),
                ),
                Variant::Secondary,
            );
            let found = found(locale, &concept, &asked.code, &system);
            Html::join([
                verdict(locale, Outcome::Ok, found, Some(open)),
                concept_sections(locale, &concept),
                sent(
                    locale,
                    &url,
                    Some((tr(locale, Msg::LookupRaw), document.text)),
                ),
            ])
        }
    }
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The verdict's sentence: the display and code found in the named system.
fn found(locale: &Locale, concept: &Concept, code: &str, system: &str) -> Html {
    let display = concept.display.clone().unwrap_or_default();
    let place = match &concept.version {
        Some(version) => tr_with(
            locale,
            Msg::LookupInVersion,
            &[
                ("system", system.to_owned().into()),
                ("version", version.clone().into()),
            ],
        ),
        None => tr_with(
            locale,
            Msg::LookupIn,
            &[("system", system.to_owned().into())],
        ),
    };
    h(format!(
        "{} {} · {} {}",
        t(tr(locale, Msg::LookupFound)),
        strong(display),
        crate::markup::code(code),
        t(place)
    ))
}

/// The concept's display, designations and properties.
fn concept_sections(locale: &Locale, concept: &Concept) -> Html {
    let display = panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::LookupDisplay),
            count: None,
            actions: None,
        }),
        p(t(concept.display.clone().unwrap_or_default())),
    );
    let designations = (!concept.designations.is_empty()).then(|| {
        let columns = [
            Column::text("term", tr(locale, Msg::LookupTerm)).role(Role::Title),
            Column::text("language", tr(locale, Msg::LookupTermLanguage)),
            Column::text("use", tr(locale, Msg::LookupUse)),
        ];
        let [_, language, usage] = &columns;
        let rows = join(concept.designations.iter().map(|row| {
            h(format!(
                "<tr>{}{}{}</tr>",
                title_cell(None, t(&row.value), None),
                cell(language, t(row.language.clone().unwrap_or_default())),
                cell(usage, t(row.usage.clone().unwrap_or_default())),
            ))
        }));
        let caption = tr(locale, Msg::LookupDesignations);
        panel_section(
            locale,
            Some(SectionHead {
                title: caption.clone(),
                count: Some(HeadingCount::All(
                    u64::try_from(concept.designations.len()).unwrap_or(u64::MAX),
                )),
                actions: None,
            }),
            data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            }),
        )
    });
    let properties = (!concept.properties.is_empty()).then(|| {
        let columns = [
            Column::text("property", tr(locale, Msg::LookupProperty)).role(Role::Title),
            Column::text("value", tr(locale, Msg::LookupValue)),
        ];
        let [_, value] = &columns;
        let rows = join(concept.properties.iter().map(|row| {
            h(format!(
                "<tr>{}{}</tr>",
                title_cell(None, code(&row.code), row.description.as_ref().map(t)),
                cell(value, t(row.value.clone().unwrap_or_default())),
            ))
        }));
        let caption = tr(locale, Msg::LookupProperties);
        panel_section(
            locale,
            Some(SectionHead {
                title: caption.clone(),
                count: Some(HeadingCount::All(
                    u64::try_from(concept.properties.len()).unwrap_or(u64::MAX),
                )),
                actions: None,
            }),
            data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            }),
        )
    });
    Html::join([
        display,
        designations.unwrap_or_default(),
        properties.unwrap_or_default(),
    ])
}
