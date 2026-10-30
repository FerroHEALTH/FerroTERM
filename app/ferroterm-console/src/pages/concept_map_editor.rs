// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept map editor: a `ConceptMap`'s metadata and its groups, each a
//! pair of systems and the codes mapped between them (M6 in an editor: one
//! block per source code, its targets as rows under it).
//!
//! R4 and R4B state a target's relation in `equivalence`, R5 and R6 in
//! `relationship` (<https://hl7.org/fhir/R5/conceptmap.html>); the editor
//! offers the codes of the version it writes to and checks what that
//! version requires (`crate::authoring::concept_map::MapDraft::problems`).
//! It is one form posted to its own address (`crate::pages::editor`).

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::coding::Coding;
use ferrohealth_ui::view::form::{FORM, Field, INPUT, SELECT, field_flow};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::mapping::{MapElement as Block, Relationship, map_element, map_elements};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, panel_section, panel_sections_card, section_intro, stack,
};

use crate::auth::scopes::Letter;
use crate::authoring::code_system::Key;
use crate::authoring::concept_map::{MapDraft, MapElement, MapGroup, MapTarget, dialect};
use crate::fhir::CONCEPT_MAP;
use crate::fhir::compose::STATUSES;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{h, join, t};
use crate::pages::detail::Kind;
use crate::pages::editor::{
    DISCARD, OPENED, OPENED_AT, SAVE, SAVED_PARAM, Standing, add_button, dock, field, form,
    heading, icon_submit, name, opened_line, percent, refused, rows, text_field,
};
use crate::pages::expand::checkbox;
use crate::pages::operations::{hidden_field, option};
use crate::pages::parts::{failure, name_of};
use crate::posted::Posted;
use crate::request::Request;
use crate::routes::{
    CONCEPT_MAP_EDITOR_ROUTE, CONCEPT_MAPS_PATH, MAP_EDITOR_PARAM, resource_link, sign_in_link,
    ui_link,
};

/// The list of groups in the form.
const GROUP: &str = "group";

/// The list of source codes in the form.
const ELEMENT: &str = "element";

/// The list of targets in the form.
const TARGET: &str = "target";

/// The R4 and R4B `ConceptMapEquivalence` codes
/// (<https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>).
const EQUIVALENCES: [&str; 10] = [
    "relatedto",
    "equivalent",
    "equal",
    "wider",
    "subsumes",
    "narrower",
    "specializes",
    "inexact",
    "unmatched",
    "disjoint",
];

/// The R5 `ConceptMapRelationship` codes
/// (<https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>).
const RELATIONSHIPS: [&str; 5] = [
    "related-to",
    "equivalent",
    "source-is-narrower-than-target",
    "source-is-broader-than-target",
    "not-related-to",
];

/// How a version reads one of its relationship codes.
type ReadRelationship = fn(&str) -> Option<Relationship>;

/// The relationship codes `version` writes, each with its words.
fn relationship_choices(locale: &Locale, version: FhirVersion) -> Vec<(&'static str, String)> {
    let (codes, read): (&[&'static str], ReadRelationship) = match version {
        FhirVersion::R4 | FhirVersion::R4B => (&EQUIVALENCES, Relationship::from_r4),
        FhirVersion::R5 | FhirVersion::R6 => (&RELATIONSHIPS, Relationship::from_r5),
    };
    codes
        .iter()
        .map(|code| {
            let words = read(code).map_or_else(
                || (*code).to_owned(),
                |relation| relation.words(locale).text().to_owned(),
            );
            (*code, format!("{words} ({code})"))
        })
        .collect()
}

/// The concept map editor.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let posted = request.posted.clone();
    let letter = if request.text(MAP_EDITOR_PARAM).is_empty() {
        Letter::Create
    } else {
        Letter::Update
    };
    if !request.can(CONCEPT_MAP, letter) {
        if request.session.who().is_none() && request.sign_in {
            request.answering.redirect(&sign_in_link(&request.back()));
        }
        return crate::pages::errors::forbidden_page(
            &request,
            Msg::EditorForbiddenConceptMap,
            "user/ConceptMap.cu",
            (
                tr(locale, Msg::ShellConceptMaps),
                ui_link(CONCEPT_MAPS_PATH, request.version),
            ),
        );
    }
    edited(&request, posted.as_ref()).await
}

/// The editor's address: the canonical it edits, on the reader's version.
fn address(request: &Request, canonical: &str) -> String {
    let base = format!(
        "{CONCEPT_MAP_EDITOR_ROUTE}?fhir={}",
        request.version.segment()
    );
    if canonical.is_empty() {
        base
    } else {
        format!("{base}&{MAP_EDITOR_PARAM}={}", percent(canonical))
    }
}

/// The page, after whatever the reader `posted`.
async fn edited(request: &Request, posted: Option<&Posted>) -> Html {
    let locale = &request.locale;
    let canonical = request.text(MAP_EDITOR_PARAM);
    let here = address(request, &canonical);
    if posted.is_some_and(|form| form.intent() == DISCARD) {
        request.answering.redirect(&here);
        return Html::empty();
    }
    let opened = match open(request, &canonical).await {
        Ok(opened) => opened,
        Err(page) => return page,
    };
    if let Some(saved) = request.param(SAVED_PARAM) {
        return saved_page(request, &opened, &saved);
    }
    let (systems, names) = crate::pages::composer::systems_of(request).await;
    let mut draft = opened.clone();
    let mut opened_version = opened.version_id.clone();
    let mut opened_at = jiff::Timestamp::now().to_string();
    let mut answer = None;
    if let Some(form) = posted {
        opened_version = form.text(OPENED);
        opened_at = form.text(OPENED_AT);
        apply(form, &opened, &mut draft);
        if form.intent() == SAVE && draft.problems(request.version).is_empty() {
            match save(request, &draft, &opened_version, &here).await {
                Ok(()) => return Html::empty(),
                Err(refusal) => answer = Some(*refusal),
            }
        }
    }
    let problems = draft.problems(request.version);
    let standing = match (&answer, problems.first()) {
        (Some((_, standing)), _) => standing.clone(),
        (None, Some(problem)) => Standing::Incomplete(problem.text(locale)),
        (None, None) if draft.body(request.version) == opened.body(request.version) => {
            Standing::Unchanged
        }
        (None, None) => Standing::Changed,
    };
    let bar = dock(
        locale,
        tr(
            locale,
            if draft.id.is_empty() {
                Msg::EditorCreateConceptMap
            } else {
                Msg::EditorSaveConceptMap
            },
        ),
        &standing,
    );
    let resource = crumb(request, &opened);
    let title = tr(
        locale,
        if opened.id.is_empty() {
            Msg::EditorNewConceptMap
        } else {
            Msg::EditorEdit
        },
    );
    let issues = (problems.len() > 1 && posted.is_some()).then(|| listed(locale, &problems));
    let body = stack(Html::join([
        answer.map(|(view, _)| view).unwrap_or_default(),
        issues.unwrap_or_default(),
        panel_sections_card(
            title.clone(),
            Html::join([
                metadata(locale, &draft),
                groups(locale, request.version, &draft, &systems, &names),
            ]),
        ),
    ]));
    Html::join([
        heading(
            request,
            Kind::ConceptMap,
            resource,
            title,
            opened_line(locale, &opened.version_id, &opened_at),
        ),
        form(
            here,
            &[(OPENED, opened_version), (OPENED_AT, opened_at)],
            body,
        ),
        bar,
    ])
}

/// Every problem of the draft, listed in one notice.
fn listed(locale: &Locale, problems: &[crate::authoring::concept_map::Problem]) -> Html {
    let lines = join(
        problems
            .iter()
            .map(|problem| h(format!("<li>{}</li>", t(problem.text(locale))))),
    );
    notice(
        Tone::Warn,
        Announce::Status,
        None,
        h(format!("<ul>{lines}</ul>")),
    )
}

/// The draft of the concept map `canonical` names, a new one for none, or
/// the page that says why it cannot be edited.
async fn open(request: &Request, canonical: &str) -> Result<MapDraft, Html> {
    if canonical.is_empty() {
        return Ok(MapDraft::new());
    }
    let opened = match request
        .client
        .authored_map(request.version, canonical)
        .await
    {
        Ok(Some(resource)) => MapDraft::of(&resource),
        Ok(None) => return Err(crate::pages::errors::not_found_page(request)),
        Err(error) => return Err(failure(&request.locale, &error)),
    };
    if opened.managed() {
        Ok(opened)
    } else {
        Err(crate::pages::editor::unmanaged(
            request,
            Kind::ConceptMap,
            resource_link(CONCEPT_MAPS_PATH, &opened.id, request.version),
        ))
    }
}

/// Saves `draft` over the version `opened` and redirects to what the
/// server holds now, or answers the refusal over the draft at `here`.
async fn save(
    request: &Request,
    draft: &MapDraft,
    opened: &str,
    here: &str,
) -> Result<(), Box<(Html, Standing)>> {
    let body = draft.body(request.version);
    match crate::pages::editor::send(request, CONCEPT_MAP, &draft.id, &body, opened).await {
        Ok(written) => {
            let version = written.version_id().unwrap_or_default().to_owned();
            request.answering.redirect(&format!(
                "{}&{SAVED_PARAM}={}",
                address(request, draft.url.trim()),
                percent(&version)
            ));
            Ok(())
        }
        Err(error) => Err(Box::new(refused(request, &error, here))),
    }
}

/// The trail's step to the concept map `opened` read, once it is stored.
fn crumb(request: &Request, opened: &MapDraft) -> Option<(String, String)> {
    (!opened.id.is_empty()).then(|| {
        let shown = if opened.title.is_empty() {
            opened.url.clone()
        } else {
            opened.title.clone()
        };
        (
            shown,
            resource_link(CONCEPT_MAPS_PATH, &opened.id, request.version),
        )
    })
}

/// The row key an intent such as `add-target:e3` names, when it names
/// `action`.
fn target<'a>(intent: &'a str, action: &str) -> Option<&'a str> {
    intent
        .strip_prefix(action)
        .and_then(|rest| rest.strip_prefix(':'))
}

/// Writes the posted fields over `draft`, read from `opened`, the row a
/// Remove names left out and the row an Add asks for added.
fn apply(form: &Posted, opened: &MapDraft, draft: &mut MapDraft) {
    let intent = form.intent();
    draft.url = form.text("url");
    draft.title = form.text("title");
    draft.name = form.text("name");
    draft.version = form.text("version");
    draft.status = form.text("status");
    let elements = rows(form, ELEMENT);
    let targets = rows(form, TARGET);
    let mut keys = opened.keys;
    let mut groups = Vec::new();
    for (group_key, fields) in rows(form, GROUP) {
        if target(intent, "remove-group") == Some(group_key.as_str()) {
            continue;
        }
        let mut group = MapGroup {
            key: keys.next(),
            source: field(&fields, "source"),
            target: field(&fields, "target"),
            ..MapGroup::default()
        };
        if let Some(held) = opened
            .groups
            .iter()
            .find(|held| held.source == group.source && held.target == group.target)
        {
            group.source_version.clone_from(&held.source_version);
            group.target_version.clone_from(&held.target_version);
        }
        for (element_key, each) in elements
            .iter()
            .filter(|(_, each)| field(each, "group") == group_key)
        {
            if target(intent, "remove-element") == Some(element_key.as_str()) {
                continue;
            }
            let mut element = MapElement {
                key: keys.next(),
                code: field(each, "code"),
                display: field(each, "display"),
                no_map: each.contains_key("nomap"),
                targets: Vec::new(),
            };
            for (target_key, row) in targets
                .iter()
                .filter(|(_, row)| field(row, "element") == *element_key)
            {
                if target(intent, "remove-target") == Some(target_key.as_str()) {
                    continue;
                }
                element.targets.push(MapTarget {
                    key: keys.next(),
                    code: field(row, "code"),
                    display: field(row, "display"),
                    relationship: field(row, "relationship"),
                    comment: field(row, "comment"),
                });
            }
            if target(intent, "add-target") == Some(element_key.as_str()) {
                element.targets.push(MapTarget {
                    key: keys.next(),
                    ..MapTarget::default()
                });
            }
            group.elements.push(element);
        }
        if target(intent, "add-element") == Some(group_key.as_str()) {
            let mut element = MapElement {
                key: keys.next(),
                ..MapElement::default()
            };
            element.targets.push(MapTarget {
                key: keys.next(),
                ..MapTarget::default()
            });
            group.elements.push(element);
        }
        groups.push(group);
    }
    if intent == "add-group" {
        groups.push(MapGroup {
            key: keys.next(),
            ..MapGroup::default()
        });
    }
    draft.groups = groups;
    draft.keys = keys;
}

/// The metadata section.
fn metadata(locale: &Locale, draft: &MapDraft) -> Html {
    let statuses = join(STATUSES.iter().map(|status| {
        let words = tr(
            locale,
            match *status {
                "active" => Msg::StatusActive,
                "retired" => Msg::StatusRetired,
                "unknown" => Msg::StatusUnknown,
                _ => Msg::StatusDraft,
            },
        );
        option(status, &words, *status == draft.status)
    }));
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::EditorMetadata),
            count: None,
            actions: None,
        }),
        h(format!(
            r#"<div class="{FORM}">{}{}</div>"#,
            text_field(
                "cm-url",
                "url",
                tr(locale, Msg::EditorCanonical),
                draft.url.clone(),
                true
            ),
            field_flow(Html::join([
                text_field(
                    "cm-title",
                    "title",
                    tr(locale, Msg::EditorTitle),
                    draft.title.clone(),
                    false,
                ),
                text_field(
                    "cm-name",
                    "name",
                    tr(locale, Msg::EditorName),
                    draft.name.clone(),
                    true,
                ),
                text_field(
                    "cm-version",
                    "version",
                    tr(locale, Msg::EditorBusinessVersion),
                    draft.version.clone(),
                    false,
                ),
                Field::new("cm-status", tr(locale, Msg::EditorStatus)).view(h(format!(
                    r#"<select id="cm-status" name="status" class="{SELECT}">{statuses}</select>"#
                ))),
            ])),
        )),
    )
}

/// A select of `systems` named `field_name`, `picked` chosen.
fn system_select(
    locale: &Locale,
    id: String,
    field_name: String,
    label: String,
    systems: &[(String, String)],
    picked: &str,
) -> Html {
    let offered = Html::join([
        option("", &tr(locale, Msg::BrowseChooseSystem), picked.is_empty()),
        join(
            systems
                .iter()
                .map(|(value, words)| option(value, words, value == picked)),
        ),
    ]);
    Field::new(id.clone(), label).view(h(format!(
        r#"<select id="{}" name="{}" class="{SELECT}">{offered}</select>"#,
        t(id),
        t(field_name)
    )))
}

/// The row key of a draft row.
fn keyed(lead: &str, key: Key) -> String {
    format!("{lead}{}", key.0)
}

/// The groups, each its two systems and its source codes with their
/// targets, and the way to add one.
fn groups(
    locale: &Locale,
    version: FhirVersion,
    draft: &MapDraft,
    systems: &[(String, String)],
    names: &BTreeMap<String, String>,
) -> Html {
    let choices = relationship_choices(locale, version);
    let no_map = dialect(version).no_map;
    let drawn = join(draft.groups.iter().zip(1_usize..).map(|(group, place)| {
        let group_key = keyed("g", group.key);
        let source_name = if group.source.is_empty() {
            tr(locale, Msg::EditorNoSystemYet)
        } else {
            name_of(names, &group.source)
        };
        let target_name = if group.target.is_empty() {
            tr(locale, Msg::EditorNoSystemYet)
        } else {
            name_of(names, &group.target)
        };
        let title = tr_with(
            locale,
            Msg::EditorGroupTitle,
            &[
                ("place", place.into()),
                ("source", source_name.into()),
                ("target", target_name.into()),
            ],
        );
        let blocks = join(
            group
                .elements
                .iter()
                .map(|element| element_block(locale, &group_key, element, &choices, no_map)),
        );
        panel_section(
            locale,
            Some(SectionHead {
                title,
                count: None,
                actions: Some(icon_submit(
                    format!("remove-group:{group_key}"),
                    Glyph::TRASH,
                    tr(locale, Msg::EditorRemoveGroup),
                )),
            }),
            Html::join([
                field_flow(Html::join([
                    system_select(
                        locale,
                        format!("cm-{group_key}-source"),
                        name(GROUP, &group_key, "source"),
                        tr(locale, Msg::EditorSourceSystem),
                        systems,
                        &group.source,
                    ),
                    system_select(
                        locale,
                        format!("cm-{group_key}-target"),
                        name(GROUP, &group_key, "target"),
                        tr(locale, Msg::EditorTargetSystem),
                        systems,
                        &group.target,
                    ),
                ])),
                h(format!(
                    r#"<p class="fui-label">{}</p>"#,
                    t(tr_with(
                        locale,
                        Msg::EditorCodesCount,
                        &[("count", group.elements.len().into())]
                    ))
                )),
                map_elements(blocks),
                h(format!(
                    r#"<div class="fui-add-row">{}</div>"#,
                    add_button(
                        format!("add-element:{group_key}"),
                        tr(locale, Msg::EditorAddMapCode)
                    )
                )),
            ]),
        )
    }));
    Html::join([
        drawn,
        panel_section(
            locale,
            None,
            Html::join([
                h(format!(
                    r#"<div class="fui-add-row">{}</div>"#,
                    add_button(String::from("add-group"), tr(locale, Msg::EditorAddGroup))
                )),
                section_intro(tr(locale, Msg::EditorGroupIntro)),
            ]),
        ),
    ])
}

/// One target of a mapped code: its code, the relationship and a comment,
/// with its Remove.
fn target_row(
    locale: &Locale,
    key: &str,
    each: &MapTarget,
    choices: &[(&'static str, String)],
) -> Html {
    let target_key = keyed("t", each.key);
    let relation = Html::join([
        option(
            "",
            &tr(locale, Msg::EditorChooseRelationship),
            each.relationship.is_empty(),
        ),
        join(
            choices
                .iter()
                .map(|(code, words)| option(code, words, *code == each.relationship)),
        ),
    ]);
    let code_id = format!("cm-{target_key}-code");
    let relation_id = format!("cm-{target_key}-relationship");
    let comment_id = format!("cm-{target_key}-comment");
    h(format!(
        r#"<div class="fui-clause-filter">{}{}{}{}{}{}</div>"#,
        hidden_field(&name(TARGET, &target_key, "element"), key),
        hidden_field(&name(TARGET, &target_key, "display"), &each.display),
        Field::new(code_id.clone(), tr(locale, Msg::EditorTargetCode)).view(h(format!(
            r#"<input id="{code_id}" name="{}" class="{INPUT} fui-mono" value="{}" placeholder="{}">"#,
            t(name(TARGET, &target_key, "code")),
            t(&each.code),
            t(tr(locale, Msg::EditorNoTarget))
        ))),
        Field::new(relation_id.clone(), tr(locale, Msg::TranslateRelationship)).view(h(
            format!(
                r#"<select id="{relation_id}" name="{}" class="{SELECT}">{relation}</select>"#,
                t(name(TARGET, &target_key, "relationship"))
            )
        )),
        Field::new(comment_id.clone(), tr(locale, Msg::TranslateComment)).view(h(format!(
            r#"<input id="{comment_id}" name="{}" class="{INPUT}" value="{}" placeholder="{}">"#,
            t(name(TARGET, &target_key, "comment")),
            t(&each.comment),
            t(tr(locale, Msg::EditorOptional))
        ))),
        icon_submit(
            format!("remove-target:{target_key}"),
            Glyph::X,
            tr(locale, Msg::EditorRemoveTarget),
        ),
    ))
}

/// One source code's block: its code and display, and its targets as rows.
fn element_block(
    locale: &Locale,
    group_key: &str,
    element: &MapElement,
    choices: &[(&'static str, String)],
    no_map: bool,
) -> Html {
    let key = keyed("e", element.key);
    let code_id = format!("cm-{key}-code");
    let display_id = format!("cm-{key}-display");
    let head = Html::join([
        hidden_field(&name(ELEMENT, &key, "group"), group_key),
        field_flow(Html::join([
            Field::new(code_id.clone(), tr(locale, Msg::EditorSourceCode)).view(h(format!(
                r#"<input id="{code_id}" name="{}" class="{INPUT} fui-mono" value="{}">"#,
                t(name(ELEMENT, &key, "code")),
                t(&element.code)
            ))),
            Field::new(display_id.clone(), tr(locale, Msg::EditorConceptDisplay)).view(h(format!(
                r#"<input id="{display_id}" name="{}" class="{INPUT}" value="{}">"#,
                t(name(ELEMENT, &key, "display")),
                t(&element.display)
            ))),
        ])),
        if no_map {
            let id = format!("cm-{key}-nomap");
            check_keyed(
                &id,
                tr(locale, Msg::EditorNoMap),
                &name(ELEMENT, &key, "nomap"),
                element.no_map,
            )
        } else {
            Html::empty()
        },
    ]);
    let rows = join(
        element
            .targets
            .iter()
            .map(|each| target_row(locale, &key, each, choices)),
    );
    let actions = Html::join([
        icon_submit(
            format!("add-target:{key}"),
            Glyph::PLUS,
            tr(locale, Msg::EditorAddTarget),
        ),
        icon_submit(
            format!("remove-element:{key}"),
            Glyph::TRASH,
            tr(locale, Msg::EditorRemoveMapCode),
        ),
    ]);
    let source = Coding::new(element.code.clone(), element.display.clone());
    map_element(
        locale,
        Block {
            source,
            actions: Some(actions),
            targets: Html::join([head, rows]),
            after: None,
        },
    )
}

/// A check box named `field_name` with the id `id`, labelled `label`.
fn check_keyed(id: &str, label: String, field_name: &str, on: bool) -> Html {
    ferrohealth_ui::view::form::keyed_check(id, label, checkbox(id, field_name, on))
}

/// The page after a save: the version the server now holds, and the way to
/// the concept map.
fn saved_page(request: &Request, opened: &MapDraft, saved: &str) -> Html {
    let locale = &request.locale;
    let href = resource_link(CONCEPT_MAPS_PATH, &opened.id, request.version);
    let open = card_action(href.clone(), tr(locale, Msg::EditorOpenConceptMap));
    let shown = if opened.title.is_empty() {
        opened.url.clone()
    } else {
        opened.title.clone()
    };
    Html::join([
        heading(
            request,
            Kind::ConceptMap,
            Some((shown, href)),
            tr(locale, Msg::EditorSavedTitle),
            tr_with(
                locale,
                Msg::EditorVersion,
                &[("version", saved.to_owned().into())],
            ),
        ),
        load_report(LoadReport {
            outcome: Outcome::Ok,
            arrival: Arrival::AfterAction,
            title: t(tr_with(
                locale,
                Msg::EditorSaved,
                &[("version", saved.to_owned().into())],
            )),
            counts: Vec::new(),
            action: Some(open),
        }),
    ])
}

#[cfg(test)]
mod tests {
    use super::{apply, relationship_choices};
    use crate::authoring::concept_map::MapDraft;
    use crate::fhir::version::FhirVersion;
    use crate::posted::Posted;

    #[test]
    fn each_version_offers_its_own_relationship_codes() {
        assert!(
            relationship_choices(&ferrohealth_ui::i18n::Locale::english(), FhirVersion::R4)
                .iter()
                .any(|(code, _)| *code == "narrower")
        );
        assert!(
            relationship_choices(&ferrohealth_ui::i18n::Locale::english(), FhirVersion::R5)
                .iter()
                .all(|(code, _)| *code != "narrower")
        );
    }

    #[test]
    fn a_post_rebuilds_the_groups_and_adds_the_target_asked_for() {
        let posted = Posted::parse(
            b"url=http%3A%2F%2Fx%2Fcm&status=draft&group.g0.source=http%3A%2F%2Fx%2Fa&group.g0.target=http%3A%2F%2Fx%2Fb\
              &element.e1.group=g0&element.e1.code=A&target.t2.element=e1&target.t2.code=B&target.t2.relationship=equivalent\
              &intent=add-target%3Ae1",
        );
        let mut draft = MapDraft::new();
        apply(&posted, &MapDraft::new(), &mut draft);
        assert_eq!(1, draft.groups.len());
        assert_eq!("A", draft.groups[0].elements[0].code);
        assert_eq!(2, draft.groups[0].elements[0].targets.len());
        assert_eq!(
            "equivalent",
            draft.groups[0].elements[0].targets[0].relationship
        );
    }
}
