// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Translate: one code followed through the concept maps the server holds,
//! with `ConceptMap/$translate`
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::mapping::{Relationship, relation};
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::fhir::translate::{TranslateRequest, TranslationMatch};
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, p, strong, t};
use crate::pages::concept_map::relationship;
use crate::pages::lookup::CODE_PARAM;
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::{failure, name_of};
use crate::request::Request;
use crate::routes::SYSTEM_PARAM;

/// The address parameter carrying the concept map's canonical.
const URL_PARAM: &str = "url";

/// The address parameter carrying the code system to translate into.
const TARGET_PARAM: &str = "targetSystem";

/// The Translate tab.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
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
    let fields = Html::join([
        text(
            "translate-code",
            CODE_PARAM,
            tr(locale, Msg::LookupCode),
            &code,
            tr(locale, Msg::LookupCodeHint),
        ),
        select(
            "translate-system",
            SYSTEM_PARAM,
            tr(locale, Msg::CheckItsSystem),
            &systems,
            &system,
            Some(tr(locale, Msg::CheckChooseSystem)),
        ),
        select(
            "translate-map",
            URL_PARAM,
            tr(locale, Msg::TranslateMap),
            &maps,
            &map,
            Some(tr(locale, Msg::TranslateAnyMap)),
        ),
        select(
            "translate-target",
            TARGET_PARAM,
            tr(locale, Msg::TranslateTarget),
            &systems,
            &target,
            Some(tr(locale, Msg::TranslateAnyTarget)),
        ),
    ]);
    let form = options(
        &request,
        &[],
        fields,
        run(
            locale,
            "translate-run",
            tr(locale, Msg::OpsTranslate),
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
        before(locale, tr(locale, Msg::TranslateBefore))
    } else if !asked.runnable() {
        before(locale, tr(locale, Msg::CheckNeedsSystem))
    } else {
        answer(&request, &asked, &map_names, &system_names).await
    };
    frame(&request, Op::Translate, form, answer)
}

/// The answer to `asked`, the maps named by `map_names` and the systems by
/// `names`.
async fn answer(
    request: &Request,
    asked: &TranslateRequest,
    map_names: &BTreeMap<String, String>,
    names: &BTreeMap<String, String>,
) -> Html {
    let locale = &request.locale;
    let url = request.client.translate_url(request.version, asked);
    match request.client.translate(request.version, asked).await {
        Err(error) => Html::join([
            panel_section(locale, None, failure(locale, &error)),
            sent(locale, &url, None),
        ]),
        Ok(document) => {
            let answer = &document.value;
            let used = answer.used_maps();
            let through = if !asked.concept_map.is_empty() {
                name_of(map_names, &asked.concept_map)
            } else if used.is_empty() {
                tr(locale, Msg::TranslateTheMaps)
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
            let title = h(format!(
                "{} {} {} {}",
                t(tr(locale, Msg::TranslateTranslated)),
                code(&asked.code),
                t(tr_with(
                    locale,
                    Msg::TranslateThrough,
                    &[("map", through.into())]
                )),
                strong(tr_with(
                    locale,
                    Msg::TranslateMatches,
                    &[("count", found.into())]
                )),
            ));
            let outcome = if answer.result() == Some(true) {
                Outcome::Ok
            } else {
                Outcome::Warn
            };
            let message = answer.message().map(|message| {
                panel_section(
                    locale,
                    Some(SectionHead {
                        title: tr(locale, Msg::CheckServerSays),
                        count: None,
                        actions: None,
                    }),
                    p(t(message)),
                )
            });
            Html::join([
                verdict(locale, outcome, title, None),
                table(locale, request.version, &matches, names).unwrap_or_default(),
                comments(locale, &matches).unwrap_or_default(),
                message.unwrap_or_default(),
                sent(
                    locale,
                    &url,
                    Some((tr(locale, Msg::CheckRaw), document.text)),
                ),
            ])
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
    locale: &Locale,
    version: FhirVersion,
    matches: &[TranslationMatch],
    names: &BTreeMap<String, String>,
) -> Option<Html> {
    if matches.is_empty() {
        return None;
    }
    let columns = [
        Column::text("target", tr(locale, Msg::TranslateTargetColumn)).role(Role::Title),
        Column::text("relationship", tr(locale, Msg::TranslateRelationship)),
    ];
    let [_, related_column] = &columns;
    let rows = join(matches.iter().map(|found| {
        let target = found.target.as_ref().map_or_else(
            || {
                h(format!(
                    r#"<span class="fui-muted">{}</span>"#,
                    t(tr(locale, Msg::TranslateNoTarget))
                ))
            },
            |target| {
                let mut drawn = Coding::new(
                    target.code.clone().unwrap_or_default(),
                    target.display.clone().unwrap_or_default(),
                );
                if let Some(system) = &target.system {
                    drawn = drawn.system(name_of(names, system));
                }
                coding(locale, drawn, Layout::Stacked)
            },
        );
        h(format!(
            "<tr>{}{}</tr>",
            title_cell(None, target, None),
            cell(related_column, relation(locale, related(version, found))),
        ))
    }));
    let caption = tr(locale, Msg::TranslateMatchesTitle);
    Some(panel_section(
        locale,
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
fn comments(locale: &Locale, matches: &[TranslationMatch]) -> Option<Html> {
    let written: Vec<String> = matches
        .iter()
        .flat_map(|found| [found.source_comment.clone(), found.target_comment.clone()])
        .flatten()
        .collect();
    (!written.is_empty()).then(|| {
        let lines = join(written.into_iter().map(|line| p(t(line))));
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::TranslateComment),
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
