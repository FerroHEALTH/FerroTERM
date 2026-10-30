// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One value set (T2): what its definition selects, clause by clause (M4),
//! the first page of its expansion, its revisions (History, for one stored
//! on this server) and the resource itself (JSON).
//!
//! The definition is the value set as its publisher wrote it; expanding it
//! is a separate operation the Expansion tab runs and the Operations
//! workbench pages through (<https://hl7.org/fhir/R4B/valueset.html>).

use std::collections::BTreeMap;

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::clause::{Clause, ClauseCode, ClauseKind, clause, clause_codes, clauses};
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::dialog::{Confirmation, danger_dialog};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, definitions, or_none, panel_section, panel_sections, section_intro,
};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::auth::scopes::Letter;
use crate::fhir::concept::LookupRequest;
use crate::fhir::error::FhirError;
use crate::fhir::expansion::{ExpandRequest, ExpandedValueSet};
use crate::fhir::value_set::{ClauseRow, PublishedValueSet};
use crate::fhir::version::FhirVersion;
use crate::fhir::{Document, FhirClient, VALUE_SET, resources_text};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, link, t};
use crate::pages::detail::{Head, Kind, Links, PANE_PARAM, Pane, header, line, tabbed};
use crate::pages::parts::{Source, failure, heading, name_of, system_names};
use crate::pages::value_sets::DELETED_PARAM;
use crate::posted::INTENT;
use crate::request::Request;
use crate::routes::{
    VALIDATE_PATH, VALUE_SETS_PATH, editing_link, expansion_link, resource_link, system_link,
    ui_link,
};

/// The intent of the delete form.
const DELETE: &str = "delete";

/// The id of the delete confirmation.
const DELETE_DIALOG: &str = "value-set-delete";

/// How many concepts the Expansion tab shows of an expansion.
const PREVIEW: u32 = 10;

/// What the page read.
struct Read {
    /// The value set as the server holds it.
    document: Document<PublishedValueSet>,
    /// The name each code system goes by, by canonical.
    names: BTreeMap<String, String>,
    /// The display of each filter value the clauses name, by system and
    /// code.
    displays: BTreeMap<(String, String), String>,
}

/// One value set, on the tab the address names.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let id = request.segment(1).unwrap_or_default();
    let Some(pane) = Pane::of(Kind::ValueSet, request.param(PANE_PARAM).as_deref()) else {
        return crate::pages::errors::not_found_page(&request);
    };
    let form = request
        .posted
        .clone()
        .filter(|form| form.intent() == DELETE);
    let refused = match form {
        Some(_) => match request.client.delete(request.version, VALUE_SET, &id).await {
            Ok(()) => {
                request.answering.redirect(&format!(
                    "{}&{DELETED_PARAM}={}",
                    ui_link(VALUE_SETS_PATH, request.version),
                    percent(&id)
                ));
                return Html::empty();
            }
            Err(error) => Some(error),
        },
        None => None,
    };
    let document = match request.client.value_set_read(request.version, &id).await {
        Ok(document) => document,
        Err(error) => {
            return Html::join([
                heading(&request, id.clone(), None, Vec::new(), ActionGroup::none()),
                failure(locale, &error),
            ]);
        }
    };
    let names = system_names(&request.client, request.version).await;
    let displays =
        filter_displays(&request.client, request.version, &document.value.clauses()).await;
    let expansion = if pane == Pane::Second {
        Some(expand(&request, &document.value).await)
    } else {
        None
    };
    Html::join([
        refused
            .as_ref()
            .map(|error| failure(locale, error))
            .unwrap_or_default(),
        screen(
            &request,
            &id,
            &Read {
                document,
                names,
                displays,
            },
            pane,
            expansion,
        ),
    ])
}

/// The display of each concept a clause's filter names, read with
/// `$lookup`; a value that does not read keeps its code alone.
async fn filter_displays(
    client: &FhirClient,
    version: FhirVersion,
    clauses: &[ClauseRow],
) -> BTreeMap<(String, String), String> {
    let mut displays = BTreeMap::new();
    for row in clauses {
        let Some(system) = &row.system else { continue };
        for (_, op, value) in &row.filter_parts {
            if !matches!(
                op.as_str(),
                "is-a" | "descendent-of" | "is-not-a" | "=" | "generalizes" | "child-of"
            ) || value.is_empty()
            {
                continue;
            }
            let request = LookupRequest {
                system: system.clone(),
                system_version: row.version.clone(),
                code: value.clone(),
                display_language: None,
            };
            // NOTE: no specification governs this: our own design; a value that does
            // not look up is still stated by its code, so nothing is hidden.
            if let Ok(answer) = client.lookup(version, &request).await
                && let Some(display) = answer.value.concept().display
            {
                displays.insert((system.clone(), value.clone()), display);
            }
        }
    }
    displays
}

/// The first page of the value set's expansion.
async fn expand(
    request: &Request,
    value_set: &PublishedValueSet,
) -> Result<ExpandedValueSet, FhirError> {
    let expand = ExpandRequest {
        url: value_set.url().unwrap_or_default().to_owned(),
        count: Some(PREVIEW),
        offset: Some(0),
        display_language: Some(request.prefs.display_language.clone())
            .filter(|tag| !tag.is_empty()),
        ..ExpandRequest::default()
    };
    request.client.expand(request.version, &expand).await
}

/// The title row's action group: Edit for a stored value set the reader may
/// write, else Check a code in it.
fn actions(
    request: &Request,
    id: &str,
    value_set: &PublishedValueSet,
    source: Source,
    title: &str,
) -> ActionGroup {
    let locale = &request.locale;
    let url = value_set.url().unwrap_or_default();
    let check_href = format!(
        "{}&url={}",
        ui_link(VALIDATE_PATH, request.version),
        percent(url)
    );
    let check = Action::new(
        Some(Glyph::SHIELD_CHECK),
        tr(locale, Msg::ValueSetCheck),
        Act::Link(check_href),
    );
    let editable = source == Source::Stored && request.can(VALUE_SET, Letter::Update);
    let edit = editing_link(VALUE_SET, url, id, request.version)
        .filter(|_| editable)
        .map(|href| {
            Action::new(
                Some(Glyph::PENCIL),
                tr(locale, Msg::CodeSystemEdit),
                Act::Link(href),
            )
        });
    let expand = Action::new(
        Some(Glyph::LIST_TREE),
        tr(locale, Msg::ValueSetExpandAll),
        Act::Link(expansion_link(url, request.version)),
    );
    let menu_label = tr_with(
        locale,
        Msg::DetailMoreFor,
        &[("name", title.to_owned().into())],
    );
    let destructive = if deletable(request, source) {
        vec![Item::new(Action::new(
            Some(Glyph::TRASH),
            tr(locale, Msg::ValueSetDelete),
            Act::Dialog(String::from(DELETE_DIALOG)),
        ))]
    } else {
        Vec::new()
    };
    match edit {
        Some(edit) => ActionGroup::primary(edit).more(
            Menu::new(
                "value-set-more",
                menu_label,
                vec![Item::new(check), Item::new(expand)],
            )
            .destructive(destructive),
        ),
        None => ActionGroup::primary(check).more(
            Menu::new("value-set-more", menu_label, vec![Item::new(expand)])
                .destructive(destructive),
        ),
    }
}

/// Whether the reader may delete the value set: one stored on this server,
/// with a token that opens delete on value sets
/// (<https://hl7.org/fhir/R4B/http.html#delete>).
fn deletable(request: &Request, source: Source) -> bool {
    source == Source::Stored && request.can(VALUE_SET, Letter::Delete)
}

/// The confirmation Delete opens, posting the delete to this screen.
fn delete_dialog(request: &Request, id: &str, title: &str) -> Html {
    let locale = &request.locale;
    let confirm = action_button(
        locale,
        Action::new(
            Some(Glyph::TRASH),
            tr(locale, Msg::ValueSetDeleteConfirm),
            Act::Post {
                action: format!(
                    "{}?{}",
                    resource_link(VALUE_SETS_PATH, id, request.version)
                        .split('?')
                        .next()
                        .unwrap_or_default(),
                    request.query
                ),
                fields: vec![(String::from(INTENT), String::from(DELETE))],
            },
        ),
        Variant::Destructive,
    );
    danger_dialog(
        locale,
        Confirmation {
            id: DELETE_DIALOG.into(),
            title: tr_with(
                locale,
                Msg::ValueSetDeleteQuestion,
                &[("name", title.to_owned().into())],
            )
            .into(),
            message: tr(locale, Msg::ValueSetDeleteMessage).into(),
        },
        confirm,
    )
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The page of `read` on `pane`.
fn screen(
    request: &Request,
    id: &str,
    read: &Read,
    pane: Pane,
    expansion: Option<Result<ExpandedValueSet, FhirError>>,
) -> Html {
    let locale = &request.locale;
    let value_set = &read.document.value;
    let source = if value_set.stored().is_some() {
        Source::Stored
    } else {
        Source::Loaded
    };
    let title = value_set
        .label()
        .or_else(|| value_set.url())
        .unwrap_or(id)
        .to_owned();
    let head = header(
        request,
        Head {
            kind: Kind::ValueSet,
            title: title.clone(),
            status: value_set.status().map(str::to_owned),
            canonical: value_set.url().unwrap_or_default().to_owned(),
            line: line(locale, value_set.version(), source),
            actions: actions(request, id, value_set, source, &title),
        },
    );
    let panel = match (pane, expansion) {
        (Pane::Second, Some(expansion)) => expansion_panel(request, value_set, &expansion),
        (Pane::Json, _) => json(locale, id, &read.document),
        _ => definition(request, read, source),
    };
    let links = Links {
        page: resource_link(VALUE_SETS_PATH, id, request.version),
        history: (source == Source::Stored).then(|| id.to_owned()),
    };
    Html::join([
        head,
        if deletable(request, source) {
            delete_dialog(request, id, &title)
        } else {
            Html::empty()
        },
        tabbed(request, Kind::ValueSet, pane, &links, panel),
    ])
}

/// The sentence a clause's filter reads as.
fn filter_sentence(
    locale: &Locale,
    part: &(String, String, String),
    display: Option<&String>,
) -> Html {
    let (property, op, value) = part;
    let named = match display {
        Some(display) => h(format!("{} {}", code(value), t(display))),
        None => code(value),
    };
    let msg = match op.as_str() {
        "is-a" => Msg::FilterIsA,
        "descendent-of" => Msg::FilterDescendentOf,
        "is-not-a" => Msg::FilterIsNotA,
        "child-of" => Msg::FilterChildOf,
        "generalizes" => Msg::FilterGeneralizes,
        "=" if property == "concept" => Msg::FilterEquals,
        _ => {
            return t(tr_with(
                locale,
                Msg::FilterOther,
                &[
                    ("property", property.clone().into()),
                    ("op", op.clone().into()),
                    ("value", value.clone().into()),
                ],
            ));
        }
    };
    let (before, after) = split_sentence(&tr(locale, msg));
    h(format!("{}{named}{}", t(before), t(after)))
}

/// The words of a filter sentence before and after the concept it names,
/// which the catalog marks with `{ $concept }` written as `CONCEPT`.
fn split_sentence(sentence: &str) -> (String, String) {
    sentence.split_once("CONCEPT").map_or_else(
        || (sentence.to_owned(), String::new()),
        |(before, after)| (before.to_owned(), after.to_owned()),
    )
}

/// The sentence under a clause: its filters, how many codes or value sets it
/// names, or the whole code system.
fn clause_sentence(locale: &Locale, row: &ClauseRow, system: &str, read: &Read) -> Html {
    if !row.filter_parts.is_empty() {
        join(row.filter_parts.iter().map(|part| {
            filter_sentence(
                locale,
                part,
                read.displays.get(&(system.to_owned(), part.2.clone())),
            )
        }))
    } else if !row.concepts.is_empty() {
        t(tr_with(
            locale,
            if row.included {
                Msg::ClauseIncludesCodes
            } else {
                Msg::ClauseExcludesCodes
            },
            &[("count", row.concepts.len().into())],
        ))
    } else if !row.value_sets.is_empty() {
        t(tr_with(
            locale,
            Msg::ClauseValueSets,
            &[("count", row.value_sets.len().into())],
        ))
    } else {
        t(tr(locale, Msg::ClauseWholeSystem))
    }
}

/// One clause of the definition, read-only.
fn clause_card(request: &Request, read: &Read, row: &ClauseRow) -> Html {
    let locale = &request.locale;
    let system = row.system.clone().unwrap_or_default();
    let named = name_of(&read.names, &system);
    let title = if system.is_empty() {
        t(tr(locale, Msg::ClauseNoSystem))
    } else {
        let href = system_link(&system, request.version);
        let version = row.version.as_ref().map(|version| {
            tr_with(
                locale,
                Msg::ClauseVersion,
                &[("version", version.clone().into())],
            )
        });
        h(format!(
            "{} {}{}",
            t(tr(locale, Msg::ClauseFrom)),
            link(&href, t(&named)),
            t(version.unwrap_or_default())
        ))
    };
    let sentence = clause_sentence(locale, row, &system, read);
    let body = (!row.concepts.is_empty()).then(|| {
        clause_codes(
            locale,
            row.concepts
                .iter()
                .map(|(code, display)| ClauseCode {
                    coding: Coding {
                        display: display.clone(),
                        code: code.clone(),
                        system: None,
                        inactive: false,
                        href: None,
                        current: false,
                    },
                    remove: None,
                })
                .collect(),
            None,
        )
    });
    clause(
        locale,
        Clause {
            kind: if row.included {
                ClauseKind::Include
            } else {
                ClauseKind::Exclude
            },
            title,
            source: named,
            sentence,
            body,
            actions: None,
            issues: Vec::new(),
        },
    )
}

/// Definition: the clauses (M4), read-only, and what the value set declares.
fn definition(request: &Request, read: &Read, source: Source) -> Html {
    let locale = &request.locale;
    let value_set = &read.document.value;
    let rows = value_set.clauses();
    let drawn = join(rows.iter().map(|row| clause_card(request, read, row)));
    let count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    let selects = if rows.is_empty() {
        section_intro(tr(locale, Msg::ValueSetNoCompose))
    } else {
        clauses(drawn)
    };
    let facts = value_set
        .facts()
        .into_iter()
        .filter(|fact| !matches!(fact.label, Msg::Title | Msg::Version | Msg::Status))
        .filter_map(|fact| {
            fact.value
                .map(|value| (tr(locale, fact.label), or_none(locale, value)))
        })
        .collect::<Vec<_>>();
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
                title: tr(locale, Msg::ValueSetSelects),
                count: Some(HeadingCount::All(count)),
                actions: None,
            }),
            selects,
        ),
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::CodeSystemDetails),
                count: None,
                actions: None,
            }),
            definitions(facts),
        ),
    ]))
}

/// Expansion: the first page of `$expand`, with its total, and the way to
/// every page in Operations.
fn expansion_panel(
    request: &Request,
    value_set: &PublishedValueSet,
    expansion: &Result<ExpandedValueSet, FhirError>,
) -> Html {
    let locale = &request.locale;
    let url = value_set.url().unwrap_or_default();
    let open = card_action(
        expansion_link(url, request.version),
        tr(locale, Msg::ValueSetOpenInOperations),
    );
    let body = match expansion.as_ref().map(ExpandedValueSet::expansion) {
        Err(error) => failure(locale, error),
        Ok(None) => section_intro(tr(locale, Msg::ValueSetNoExpansion)),
        Ok(Some(expanded)) => {
            let columns = [
                Column::text("concept", tr(locale, Msg::ValueSetConcept)).role(Role::Title),
                Column::text("system", tr(locale, Msg::ValueSetSystem)).role(Role::Quiet),
            ];
            let [_, system] = &columns;
            let rows = join(expanded.concepts.iter().map(|concept| {
                let mut drawn = Coding::new(
                    concept.code.clone(),
                    concept.display.clone().unwrap_or_default(),
                );
                if concept.inactive {
                    drawn = drawn.inactive();
                }
                h(format!(
                    "<tr>{}{}</tr>",
                    title_cell(None, coding(locale, drawn, Layout::Inline), None),
                    cell(
                        system,
                        ferrohealth_ui::view::coding::canonical(&concept.system)
                    ),
                ))
            }));
            let caption = tr(locale, Msg::ValueSetFirstPage);
            let table = data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            });
            let total = expanded.total.map_or_else(
                || {
                    tr_with(
                        locale,
                        Msg::ValueSetShown,
                        &[("shown", expanded.listed().into())],
                    )
                },
                |total| {
                    tr_with(
                        locale,
                        Msg::ValueSetShownOf,
                        &[("shown", expanded.listed().into()), ("total", total.into())],
                    )
                },
            );
            Html::join([section_intro(total), table])
        }
    };
    panel_sections(panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::ValueSetExpansionTitle),
            count: None,
            actions: Some(open),
        }),
        body,
    ))
}

/// JSON: the value set as the server sent it.
fn json(locale: &Locale, id: &str, document: &Document<PublishedValueSet>) -> Html {
    let code = Code {
        label: tr(locale, Msg::ValueSetJsonLabel),
        language: Language::Json,
        text: resources_text(&document.text),
        ids: None,
        current: None,
        diagnostics: Vec::new(),
    };
    panel_sections(code_section(
        locale,
        CodeHead {
            title: format!("ValueSet/{id}"),
            tags: vec![String::from("FHIR")],
            digest: None,
            copy: Some(String::from("value-set-copy")),
            actions: None,
        },
        &code,
    ))
}

#[cfg(test)]
mod tests {
    use super::split_sentence;

    #[test]
    fn a_filter_sentence_splits_around_the_concept_it_names() {
        assert_eq!(
            (
                String::from("Includes "),
                String::from(" and everything below it.")
            ),
            split_sentence("Includes CONCEPT and everything below it.")
        );
        assert_eq!(
            (String::from("No concept"), String::new()),
            split_sentence("No concept")
        );
    }
}
