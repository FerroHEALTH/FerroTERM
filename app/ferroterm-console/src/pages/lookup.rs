// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Look up: one code read in the code system it belongs to, with
//! `CodeSystem/$lookup` (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>).

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::small_action_button;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;

use crate::fhir::CODE_SYSTEM;
use crate::fhir::concept::{Concept, LookupRequest};
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, tr, tr_with};
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
pub(crate) async fn page(request: Request) -> AnyView {
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
        named
            .as_ref()
            .map(|named| named.versions(&system))
            .unwrap_or_default(),
        card.as_ref(),
    );
    let version = request.text(SYSTEM_VERSION_PARAM);
    let code = request.text(CODE_PARAM);
    let language = request.text(LANGUAGE_PARAM);
    let languages = card
        .as_ref()
        .map(|card| {
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
        .unwrap_or_default();
    let fields = view! {
        {select("lookup-system", SYSTEM_PARAM, tr(Msg::LookupSystem), &systems, &system, None)}
        {select(
            "lookup-version",
            SYSTEM_VERSION_PARAM,
            tr(Msg::LookupVersion),
            &versions,
            &version,
            Some(tr(Msg::LookupServerVersion)),
        )}
        {text("lookup-code", CODE_PARAM, tr(Msg::LookupCode), &code, tr(Msg::LookupCodeHint))}
        {select(
            "lookup-language",
            LANGUAGE_PARAM,
            tr(Msg::LookupLanguage),
            &languages,
            &language,
            Some(tr(Msg::LookupServerLanguage)),
        )}
    }
    .into_any();
    let form = options(
        &request,
        &[],
        fields,
        run("lookup-run", tr(Msg::OpsLookUp), Glyph::SEARCH, "$lookup"),
    );
    let answer = if system.is_empty() || code.is_empty() {
        before(tr(Msg::LookupBefore))
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

/// The versions a picker offers: each published version, the one an
/// unversioned request resolves to marked as the default.
fn versions(
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
async fn answer(request: &Request, asked: &LookupRequest, name: Option<String>) -> AnyView {
    let url = request.client.lookup_url(request.version, asked);
    match request.client.lookup(request.version, asked).await {
        Err(error) => view! {
            {panel_section(None, failure(&error))}
            {sent(&url, None)}
        }
        .into_any(),
        Ok(document) => {
            let concept = document.value.concept();
            let system = name
                .or_else(|| concept.name.clone())
                .unwrap_or_else(|| asked.system.clone());
            let open = small_action_button(
                Action::new(
                    Some(Glyph::NETWORK),
                    tr(Msg::LookupOpenInBrowser),
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
            let found = found(&concept, &asked.code, &system);
            view! {
                {verdict(Outcome::Ok, found, Some(open))}
                {concept_sections(&concept)}
                {sent(&url, Some((tr(Msg::LookupRaw), document.text)))}
            }
            .into_any()
        }
    }
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The verdict's sentence: the display and code found in the named system.
fn found(concept: &Concept, code: &str, system: &str) -> AnyView {
    let display = concept.display.clone().unwrap_or_default();
    let place = match &concept.version {
        Some(version) => tr_with(
            Msg::LookupInVersion,
            &[
                ("system", system.to_owned().into()),
                ("version", version.clone().into()),
            ],
        ),
        None => tr_with(Msg::LookupIn, &[("system", system.to_owned().into())]),
    };
    view! {
        {tr(Msg::LookupFound)}
        " "
        <strong>{display}</strong>
        " · "
        <code>{code.to_owned()}</code>
        " "
        {place}
    }
    .into_any()
}

/// The concept's display, designations and properties.
fn concept_sections(concept: &Concept) -> AnyView {
    let display = panel_section(
        Some(SectionHead {
            title: tr(Msg::LookupDisplay),
            count: None,
            actions: None,
        }),
        view! { <p>{concept.display.clone().unwrap_or_default()}</p> },
    );
    let designations = (!concept.designations.is_empty()).then(|| {
        let columns = [
            Column::text("term", tr(Msg::LookupTerm)).role(Role::Title),
            Column::text("language", tr(Msg::LookupTermLanguage)),
            Column::text("use", tr(Msg::LookupUse)),
        ];
        let [_, language, usage] = &columns;
        let rows = concept
            .designations
            .iter()
            .map(|row| {
                view! {
                    <tr>
                        {title_cell(None, row.value.clone(), None)}
                        {cell(language, row.language.clone().unwrap_or_default())}
                        {cell(usage, row.usage.clone().unwrap_or_default())}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        let caption = tr(Msg::LookupDesignations);
        panel_section(
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
            Column::text("property", tr(Msg::LookupProperty)).role(Role::Title),
            Column::text("value", tr(Msg::LookupValue)),
        ];
        let [_, value] = &columns;
        let rows = concept
            .properties
            .iter()
            .map(|row| {
                let sub = row.description.clone();
                view! {
                    <tr>
                        {title_cell(
                            None,
                            view! { <code>{row.code.clone()}</code> },
                            sub.map(IntoAny::into_any),
                        )} {cell(value, row.value.clone().unwrap_or_default())}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        let caption = tr(Msg::LookupProperties);
        panel_section(
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
    view! {
        {display}
        {designations}
        {properties}
    }
    .into_any()
}
