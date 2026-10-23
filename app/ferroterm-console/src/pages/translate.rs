// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Translate: one code followed through the concept maps the server holds,
//! with `ConceptMap/$translate`
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::mapping::{Relationship, relation};
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::template::list::HeadingCount;
use leptos::prelude::*;

use crate::fhir::translate::{TranslateRequest, TranslationMatch};
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::concept_map::relationship;
use crate::pages::lookup::CODE_PARAM;
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::{failure, loaded, name_of};
use crate::request::{Request, request};
use crate::routes::SYSTEM_PARAM;

/// The address parameter carrying the concept map's canonical.
const URL_PARAM: &str = "url";

/// The address parameter carrying the code system to translate into.
const TARGET_PARAM: &str = "targetSystem";

/// The Translate tab.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn TranslatePage() -> impl IntoView {
    let request = request();
    loaded(async move {
        let systems_named = request
            .client
            .published_names(request.version, CODE_SYSTEM)
            .await
            .ok();
        let systems = choices(systems_named.as_ref());
        let system_names = systems_named
            .as_ref()
            .map(crate::fhir::named::NamedSearch::names)
            .unwrap_or_default();
        let maps_named = request
            .client
            .published_names(request.version, CONCEPT_MAP)
            .await
            .ok();
        let maps = choices(maps_named.as_ref());
        let map_names = maps_named
            .as_ref()
            .map(crate::fhir::named::NamedSearch::names)
            .unwrap_or_default();
        let code = request.text(CODE_PARAM);
        let system = request.text(SYSTEM_PARAM);
        let map = request.text(URL_PARAM);
        let target = request.text(TARGET_PARAM);
        let fields = view! {
            {text(
                "translate-code",
                CODE_PARAM,
                tr(Msg::LookupCode),
                &code,
                tr(Msg::LookupCodeHint),
            )}
            {select(
                "translate-system",
                SYSTEM_PARAM,
                tr(Msg::CheckItsSystem),
                &systems,
                &system,
                Some(tr(Msg::CheckChooseSystem)),
            )}
            {select(
                "translate-map",
                URL_PARAM,
                tr(Msg::TranslateMap),
                &maps,
                &map,
                Some(tr(Msg::TranslateAnyMap)),
            )}
            {select(
                "translate-target",
                TARGET_PARAM,
                tr(Msg::TranslateTarget),
                &systems,
                &target,
                Some(tr(Msg::TranslateAnyTarget)),
            )}
        }
        .into_any();
        let form = options(
            &request,
            &[],
            fields,
            run(
                "translate-run",
                tr(Msg::OpsTranslate),
                Glyph::CORNER_DOWN_RIGHT,
                "$translate",
            ),
        );
        let asked = TranslateRequest {
            concept_map: map.clone(),
            system: system.clone(),
            code: code.clone(),
            target_system: target,
            ..TranslateRequest::default()
        };
        let answer = if code.is_empty() {
            before(tr(Msg::TranslateBefore))
        } else if !asked.runnable() {
            before(tr(Msg::CheckNeedsSystem))
        } else {
            answer(&request, &asked, &map_names, &system_names).await
        };
        frame(&request, Op::Translate, form, answer)
    })
}

/// The answer to `asked`, the maps named by `map_names` and the systems by
/// `names`.
async fn answer(
    request: &Request,
    asked: &TranslateRequest,
    map_names: &BTreeMap<String, String>,
    names: &BTreeMap<String, String>,
) -> AnyView {
    let url = request.client.translate_url(request.version, asked);
    match request.client.translate(request.version, asked).await {
        Err(error) => view! {
            {panel_section(None, failure(&error))}
            {sent(&url, None)}
        }
        .into_any(),
        Ok(document) => {
            let answer = &document.value;
            let used = answer.used_maps();
            let through = if !asked.concept_map.is_empty() {
                name_of(map_names, &asked.concept_map)
            } else if used.is_empty() {
                tr(Msg::TranslateTheMaps)
            } else {
                used.iter()
                    .map(|map| name_of(map_names, map))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let matches = answer.matches();
            let found = matches
                .iter()
                .filter(|each| !each.no_map && each.target.is_some())
                .count();
            let title = view! {
                {tr(Msg::TranslateTranslated)}
                " "
                <code>{asked.code.clone()}</code>
                " "
                {tr_with(Msg::TranslateThrough, &[("map", through.into())])}
                " "
                <strong>{tr_with(Msg::TranslateMatches, &[("count", found.into())])}</strong>
            }
            .into_any();
            let outcome = if answer.result() == Some(true) {
                Outcome::Ok
            } else {
                Outcome::Warn
            };
            let message = answer.message().map(|message| {
                panel_section(
                    Some(SectionHead {
                        title: tr(Msg::CheckServerSays),
                        count: None,
                        actions: None,
                    }),
                    view! { <p>{message}</p> },
                )
            });
            view! {
                {verdict(outcome, title, None)}
                {table(request.version, &matches, names)}
                {comments(&matches)}
                {message}
                {sent(&url, Some((tr(Msg::CheckRaw), document.text)))}
            }
            .into_any()
        }
    }
}

/// How a match relates its target to the source, read the way `version`
/// writes it.
fn related(version: FhirVersion, found: &TranslationMatch) -> Relationship {
    if found.no_map || found.target.is_none() {
        return Relationship::Unmatched;
    }
    relationship(
        version,
        found
            .relationship
            .as_deref()
            .or(found.equivalence.as_deref()),
    )
}

/// The matches: each target coding and how it relates.
fn table(
    version: FhirVersion,
    matches: &[TranslationMatch],
    names: &BTreeMap<String, String>,
) -> Option<AnyView> {
    if matches.is_empty() {
        return None;
    }
    let columns = [
        Column::text("target", tr(Msg::TranslateTargetColumn)).role(Role::Title),
        Column::text("relationship", tr(Msg::TranslateRelationship)),
    ];
    let [_, related_column] = &columns;
    let rows = matches
        .iter()
        .map(|found| {
            let target = found.target.as_ref().map_or_else(
                || view! { <span class="fui-muted">{tr(Msg::TranslateNoTarget)}</span> }.into_any(),
                |target| {
                    let mut drawn = Coding::new(
                        target.code.clone().unwrap_or_default(),
                        target.display.clone().unwrap_or_default(),
                    );
                    if let Some(system) = &target.system {
                        drawn = drawn.system(name_of(names, system));
                    }
                    coding(drawn, Layout::Stacked)
                },
            );
            view! {
                <tr>
                    {title_cell(None, target, None)}
                    {cell(related_column, relation(related(version, found)))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::TranslateMatchesTitle);
    Some(panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(matches.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }),
    ))
}

/// The comments the map writes on its matches.
fn comments(matches: &[TranslationMatch]) -> Option<AnyView> {
    let written: Vec<String> = matches
        .iter()
        .flat_map(|found| [found.source_comment.clone(), found.target_comment.clone()])
        .flatten()
        .collect();
    (!written.is_empty()).then(|| {
        let lines = written
            .into_iter()
            .map(|line| view! { <p>{line}</p> })
            .collect_view();
        panel_section(
            Some(SectionHead {
                title: tr(Msg::TranslateComment),
                count: None,
                actions: None,
            }),
            lines,
        )
    })
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::mapping::Relationship;

    use super::related;
    use crate::fhir::translate::{Coding, TranslationMatch};
    use crate::fhir::version::FhirVersion;

    #[test]
    fn a_match_with_no_target_maps_to_nothing() {
        let none = TranslationMatch {
            no_map: true,
            ..TranslationMatch::default()
        };
        assert_eq!(Relationship::Unmatched, related(FhirVersion::R5, &none));
        let narrower = TranslationMatch {
            equivalence: Some(String::from("narrower")),
            target: Some(Coding::default()),
            ..TranslationMatch::default()
        };
        assert_eq!(Relationship::Narrower, related(FhirVersion::R4, &narrower));
    }
}
