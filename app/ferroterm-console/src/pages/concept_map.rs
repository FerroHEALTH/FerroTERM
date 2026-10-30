// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One concept map (T2): every mapping it writes (M6), grouped by the
//! systems each group maps between, what it declares and maps (Overview),
//! its revisions (History, for one stored on this server) and the resource
//! itself (JSON).
//!
//! R4 and R4B read `equivalence` from the target to the source, R5 and R6
//! read `relationship` from the source to the target
//! (<https://hl7.org/fhir/R5/conceptmap.html>); FerroUI says each the way
//! its version reads.

use std::collections::BTreeMap;

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, DataTable, Sorting, data_table, group_row};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::coding::{Coding, canonical};
use ferrohealth_ui::view::mapping::{Mapping, Relationship, mapping_columns, mapping_row};
use ferrohealth_ui::view::surface::{
    SectionHead, definitions, or_none, panel_section, panel_sections, section_intro,
};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::auth::scopes::Letter;
use crate::fhir::concept_map::{MappingRow, PublishedConceptMap};
use crate::fhir::version::FhirVersion;
use crate::fhir::{CONCEPT_MAP, Document, resources_text};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{h, join, link, t};
use crate::pages::detail::{Head, Kind, Links, PANE_PARAM, Pane, header, line, tabbed};
use crate::pages::parts::{Source, failure, heading, system_names};
use crate::request::Request;
use crate::routes::{
    CONCEPT_MAPS_PATH, TRANSLATE_PATH, editing_link, resource_link, system_link, ui_link,
};

/// One concept map, on the tab the address names.
pub(crate) async fn page(request: Request) -> Html {
    let id = request.segment(1).unwrap_or_default();
    let Some(pane) = Pane::of(Kind::ConceptMap, request.param(PANE_PARAM).as_deref()) else {
        return crate::pages::errors::not_found_page(&request);
    };
    match request.client.concept_map_read(request.version, &id).await {
        Ok(document) => {
            let names = system_names(&request.client, request.version).await;
            screen(&request, &id, &document, &names, pane)
        }
        Err(error) => Html::join([
            heading(&request, id.clone(), None, Vec::new(), ActionGroup::none()),
            failure(&request.locale, &error),
        ]),
    }
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The title row's action group: Edit for a stored map the reader may
/// write, else Translate a code through it.
fn actions(
    request: &Request,
    id: &str,
    map: &PublishedConceptMap,
    source: Source,
    title: &str,
) -> ActionGroup {
    let locale = &request.locale;
    let url = map.url().unwrap_or_default();
    let mut translate_href = format!(
        "{}&url={}",
        ui_link(TRANSLATE_PATH, request.version),
        percent(url)
    );
    if let Some(system) = map.only_source_system() {
        translate_href.push_str("&system=");
        translate_href.push_str(&percent(&system));
    }
    let translate = Action::new(
        Some(Glyph::CORNER_DOWN_RIGHT),
        tr(locale, Msg::ConceptMapTranslate),
        Act::Link(translate_href),
    );
    let editable = source == Source::Stored && request.can(CONCEPT_MAP, Letter::Update);
    let edit = editing_link(CONCEPT_MAP, url, id, request.version)
        .filter(|_| editable)
        .map(|href| {
            Action::new(
                Some(Glyph::PENCIL),
                tr(locale, Msg::CodeSystemEdit),
                Act::Link(href),
            )
        });
    let menu_label = tr_with(
        locale,
        Msg::DetailMoreFor,
        &[("name", title.to_owned().into())],
    );
    match edit {
        Some(edit) => ActionGroup::primary(edit).more(Menu::new(
            "concept-map-more",
            menu_label,
            vec![Item::new(translate)],
        )),
        None => ActionGroup::primary(translate),
    }
}

/// The page of `document` on `pane`.
fn screen(
    request: &Request,
    id: &str,
    document: &Document<PublishedConceptMap>,
    names: &BTreeMap<String, String>,
    pane: Pane,
) -> Html {
    let locale = &request.locale;
    let map = &document.value;
    let source = if map.stored().is_some() {
        Source::Stored
    } else {
        Source::Loaded
    };
    let title = map.label().or_else(|| map.url()).unwrap_or(id).to_owned();
    let head = header(
        request,
        Head {
            kind: Kind::ConceptMap,
            title: title.clone(),
            status: map.status().map(str::to_owned),
            canonical: map.url().unwrap_or_default().to_owned(),
            line: line(locale, map.version(), source),
            actions: actions(request, id, map, source, &title),
        },
    );
    let panel = match pane {
        Pane::Second => overview(request, map, names, source),
        Pane::Json => json(locale, id, document),
        Pane::First | Pane::History => mappings(locale, request.version, map, names),
    };
    let links = Links {
        page: resource_link(CONCEPT_MAPS_PATH, id, request.version),
        history: (source == Source::Stored).then(|| id.to_owned()),
    };
    Html::join([head, tabbed(request, Kind::ConceptMap, pane, &links, panel)])
}

/// How `code` relates a target to its source, read the way `version`
/// writes it; a code neither version defines draws as related.
pub(crate) fn relationship(version: FhirVersion, code: Option<&str>) -> Relationship {
    let read = match version {
        FhirVersion::R4 | FhirVersion::R4B => Relationship::from_r4,
        FhirVersion::R5 | FhirVersion::R6 => Relationship::from_r5,
    };
    // NOTE: no specification governs this: our own design; a mapping whose code
    // the version does not define is still drawn, as related, never dropped.
    code.and_then(read).unwrap_or(Relationship::RelatedTo)
}

/// The mapping `row` as FerroUI draws one.
fn mapping(version: FhirVersion, row: &MappingRow) -> Mapping {
    Mapping {
        source: Coding::new(
            row.source_code.clone(),
            row.source_display.clone().unwrap_or_default(),
        ),
        relationship: relationship(version, row.relationship.as_deref()),
        target: row
            .target_code
            .as_ref()
            .map(|code| Coding::new(code.clone(), row.target_display.clone().unwrap_or_default())),
        comment: row.comment.clone(),
    }
}

/// A group's system: its name, a link to its page, the canonical in its
/// tooltip.
fn system(
    locale: &Locale,
    version: FhirVersion,
    names: &BTreeMap<String, String>,
    canonical: Option<&str>,
) -> Html {
    match canonical {
        Some(canonical) => {
            let href = system_link(canonical, version);
            // NOTE: #775: a system this server names nothing reads as its canonical,
            // which breaks after each slash so the table keeps inside its card.
            let shown = names
                .get(canonical)
                .map_or_else(|| ferrohealth_ui::view::coding::canonical(canonical), t);
            h(format!(
                r#"<a href="{}" title="{}">{shown}</a>"#,
                t(href),
                t(canonical)
            ))
        }
        None => t(tr(locale, Msg::ConceptMapNoSystem)),
    }
}

/// The source and the target system a group maps between.
type Systems = (Option<String>, Option<String>);

/// Mappings: every mapping the map writes, a group row over each group's.
fn mappings(
    locale: &Locale,
    version: FhirVersion,
    map: &PublishedConceptMap,
    names: &BTreeMap<String, String>,
) -> Html {
    let rows = map.mappings();
    let comments = rows.iter().any(|row| row.comment.is_some());
    let columns = mapping_columns(locale, comments);
    let mut groups: Vec<(Systems, Vec<&MappingRow>)> = Vec::new();
    for row in &rows {
        let key = (row.source_system.clone(), row.target_system.clone());
        match groups.last_mut() {
            Some((last, members)) if *last == key => members.push(row),
            _ => groups.push((key, vec![row])),
        }
    }
    let bodies = join(groups.into_iter().map(|((from, to), members)| {
        let heading = h(format!(
            "{} {} {} {}",
            t(tr(locale, Msg::ClauseFrom)),
            system(locale, version, names, from.as_deref()),
            t(tr(locale, Msg::ConceptMapTo)),
            system(locale, version, names, to.as_deref()),
        ));
        let drawn = join(
            members
                .into_iter()
                .map(|row| mapping_row(locale, &columns, mapping(version, row))),
        );
        h(format!(
            "<tbody>{}{drawn}</tbody>",
            group_row(&columns, heading)
        ))
    }));
    let caption = tr(locale, Msg::ConceptMapEvery);
    let body = if rows.is_empty() {
        Body::Empty(section_intro(tr(locale, Msg::ConceptMapNoMappings)))
    } else {
        Body::Bodies(bodies)
    };
    let count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    panel_sections(panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body,
        }),
    ))
}

/// A scope the map translates from or to: the system's name, linked when
/// this server has a page for it, and its canonical.
fn scope(
    locale: &Locale,
    version: FhirVersion,
    names: &BTreeMap<String, String>,
    url: Option<&str>,
) -> Html {
    match url {
        None => or_none(locale, String::new()),
        Some(url) => {
            let name = names.get(url).map(|name| {
                let href = system_link(url, version);
                h(format!("{} ", link(&href, t(name))))
            });
            h(format!("{}{}", name.unwrap_or_default(), canonical(url)))
        }
    }
}

/// Overview: what the map declares and what it maps.
fn overview(
    request: &Request,
    map: &PublishedConceptMap,
    names: &BTreeMap<String, String>,
    source: Source,
) -> Html {
    let locale = &request.locale;
    let details = map
        .facts()
        .into_iter()
        .filter(|fact| {
            !matches!(
                fact.label,
                Msg::Title
                    | Msg::Version
                    | Msg::Status
                    | Msg::TranslatesFrom
                    | Msg::TranslatesTo
                    | Msg::Groups
            )
        })
        .filter_map(|fact| {
            fact.value
                .map(|value| (tr(locale, fact.label), or_none(locale, value)))
        })
        .collect::<Vec<_>>();
    let rows = map.mappings();
    let total = rows.len();
    let unmapped = rows.iter().filter(|row| row.target_code.is_none()).count();
    let codes = if unmapped == 0 {
        total.to_string()
    } else {
        tr_with(
            locale,
            Msg::ConceptMapCodesUnmapped,
            &[("count", total.into()), ("unmapped", unmapped.into())],
        )
    };
    let groups = map.groups();
    let from = map
        .source_scope()
        .map(str::to_owned)
        .or_else(|| map.only_source_system());
    let to = map.target_scope().map(str::to_owned).or_else(|| {
        let mut targets = groups.iter().filter_map(|group| group.target.clone());
        let first = targets.next()?;
        targets.all(|other| other == first).then_some(first)
    });
    let maps = vec![
        (
            tr(locale, Msg::TranslatesFrom),
            scope(locale, request.version, names, from.as_deref()),
        ),
        (
            tr(locale, Msg::TranslatesTo),
            scope(locale, request.version, names, to.as_deref()),
        ),
        (tr(locale, Msg::Groups), t(groups.len().to_string())),
        (tr(locale, Msg::ConceptMapCodesMapped), t(codes)),
    ];
    let note = (source == Source::Loaded).then(|| {
        panel_section(
            locale,
            None,
            section_intro(tr(locale, Msg::ValueSetLoadedNote)),
        )
    });
    panel_sections(Html::join([
        note.unwrap_or_default(),
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::CodeSystemDetails),
                count: None,
                actions: None,
            }),
            definitions(details),
        ),
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::ConceptMapWhatItMaps),
                count: None,
                actions: None,
            }),
            definitions(maps),
        ),
    ]))
}

/// JSON: the concept map as the server sent it.
fn json(locale: &Locale, id: &str, document: &Document<PublishedConceptMap>) -> Html {
    let code = Code {
        label: tr(locale, Msg::ConceptMapJsonLabel),
        language: Language::Json,
        text: resources_text(&document.text),
        ids: None,
        current: None,
        diagnostics: Vec::new(),
    };
    panel_sections(code_section(
        locale,
        CodeHead {
            title: format!("ConceptMap/{id}"),
            tags: vec![String::from("FHIR")],
            digest: None,
            copy: Some(String::from("concept-map-copy")),
            actions: None,
        },
        &code,
    ))
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::mapping::Relationship;

    use super::relationship;
    use crate::fhir::version::FhirVersion;

    #[test]
    fn each_version_reads_its_own_relationship_codes() {
        assert_eq!(
            Relationship::Narrower,
            relationship(FhirVersion::R4, Some("narrower"))
        );
        assert_eq!(
            Relationship::SourceBroader,
            relationship(FhirVersion::R5, Some("source-is-broader-than-target"))
        );
        assert_eq!(
            Relationship::RelatedTo,
            relationship(FhirVersion::R5, Some("narrower")),
            "an R4 code in an R5 map is drawn, as related"
        );
        assert_eq!(
            Relationship::RelatedTo,
            relationship(FhirVersion::R4B, None)
        );
    }
}
