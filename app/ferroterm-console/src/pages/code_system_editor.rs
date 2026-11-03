// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code system editor: a local `CodeSystem`'s metadata, the properties
//! it declares, and its concepts, each retired rather than removed so a
//! published code keeps meaning what it meant
//! (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
//!
//! It is one form posted to its own address (`crate::pages::editor`). The
//! draft is the stored resource with the posted fields written over it, so
//! an element the form does not draw travels back untouched
//! (`crate::authoring::code_system::Draft::body`).

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Shape, Sorting, cell, data_table,
};
use ferrohealth_ui::view::form::{Field, INPUT, MEDIUM, SELECT, check, field_flow};
use ferrohealth_ui::view::layout::{Gap, stack_at};
use ferrohealth_ui::view::repeat::{Fields, ROW, Repeat, RepeatAdd, RepeatRow, RowEnd, repeat};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, panel_section, panel_sections_card, section_intro,
};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::authoring::code_system::{Concept, Declared, Draft, Lifecycle};
use crate::fhir::CODE_SYSTEM;
use crate::fhir::write::Written;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::code;
use crate::pages::detail::Kind;
use crate::pages::editor::{
    self, Guard, OPENED, OPENED_AT, SAVE, Standing, add_button, apply_metadata, dock, field, form,
    heading, metadata_section, name, opened_line, rows, status_options, text_field,
};
use crate::pages::expand::checkbox;
use crate::pages::operations::{hidden_field, option};
use crate::pages::parts::failure;
use crate::posted::{INTENT, Posted};
use crate::request::Request;
use crate::routes::{CODE_SYSTEM_EDITOR_ROUTE, EDITOR_SYSTEM_PARAM, SYSTEMS_PATH, system_link};

/// The content modes a code system states
/// (<https://hl7.org/fhir/R4B/valueset-codesystem-content-mode.html>).
const CONTENT_MODES: [&str; 5] = [
    "complete",
    "fragment",
    "example",
    "not-present",
    "supplement",
];

/// The property types a code system declares
/// (<https://hl7.org/fhir/R4B/valueset-concept-property-type.html>).
const PROPERTY_TYPES: [&str; 7] = [
    "code", "Coding", "string", "integer", "boolean", "dateTime", "decimal",
];

/// The four lifecycle states, as the lifecycle select offers them.
const LIFECYCLES: [&str; 4] = ["active", "experimental", "deprecated", "retired"];

/// The list of declared properties in the form.
const PROPERTY: &str = "property";

/// The list of concepts in the form.
const CONCEPT: &str = "concept";

/// The intent that declares a property.
const ADD_PROPERTY: &str = "add-property";

/// The intent that adds a concept.
const ADD_CONCEPT: &str = "add-concept";

/// The field a property row's Remove submits.
const REMOVE_PROPERTY: &str = "remove-property";

/// The field a new concept row's Remove submits.
const REMOVE_CONCEPT: &str = "remove-concept";

/// The words of a lifecycle state.
fn lifecycle_words(locale: &Locale, status: &str) -> String {
    tr(
        locale,
        match status {
            "experimental" => Msg::EditorLifecycleExperimental,
            "deprecated" => Msg::EditorLifecycleDeprecated,
            "retired" => Msg::EditorLifecycleRetired,
            _ => Msg::EditorLifecycleActive,
        },
    )
}

/// The words of a content mode.
fn content_words(locale: &Locale, mode: &str) -> String {
    tr(
        locale,
        match mode {
            "complete" => Msg::ContentComplete,
            "not-present" => Msg::ContentNotPresent,
            "example" => Msg::ContentExample,
            "fragment" => Msg::ContentFragment,
            "supplement" => Msg::ContentSupplement,
            _ => Msg::ContentUndeclared,
        },
    )
}

/// The code system editor.
pub(crate) async fn page(request: Request) -> Html {
    editor::guarded(&request, &GUARD, edited).await
}

editor::metadata!(Draft);

/// Who may open the code system editor.
const GUARD: Guard = Guard {
    route: CODE_SYSTEM_EDITOR_ROUTE,
    resource_type: CODE_SYSTEM,
    param: EDITOR_SYSTEM_PARAM,
    forbidden: Msg::EditorForbiddenCodeSystem,
    scope: "user/CodeSystem.cu",
    back: (Msg::ShellCodeSystems, SYSTEMS_PATH),
};

/// The page, after whatever the reader `posted`.
async fn edited(request: &Request, posted: Option<&Posted>) -> Html {
    let system = request.text(EDITOR_SYSTEM_PARAM);
    let here = editor::address(request, &GUARD, &system);
    if let Some(page) = editor::discarded(request, posted, &here) {
        return page;
    }
    let stored = if system.is_empty() {
        None
    } else {
        match request.client.authored(request.version, &system).await {
            Ok(Some(resource)) => Some(resource),
            Ok(None) => return crate::pages::errors::not_found_page(request),
            Err(error) => return failure(&request.locale, &error),
        }
    };
    let held = stored.is_some();
    let opened = stored.unwrap_or_else(Draft::new);
    if held && !opened.managed() {
        return editor::unmanaged(
            request,
            Kind::CodeSystem,
            system_link(&opened.url, request.version),
        );
    }
    let opened = match editor::unless_saved(request, opened, saved_page) {
        Ok(opened) => opened,
        Err(page) => return page,
    };
    let (mut draft, opened_version, opened_at) = editor::reopened(posted, &opened);
    let mut answer = None;
    if let Some(form) = posted {
        apply(form, &opened, &mut draft);
        if form.intent() == SAVE && draft.savable() {
            let sent = (CODE_SYSTEM, draft.id.as_str(), draft.body(request.version));
            let at = |_written: &Written| editor::address(request, &GUARD, draft.url.trim());
            if editor::saved(request, sent, (&opened_version, &here), at, &mut answer).await {
                return Html::empty();
            }
        }
    }
    drawn(
        request,
        &opened,
        &draft,
        answer,
        here,
        (opened_version, opened_at),
    )
}

/// The editor as it is drawn: the heading, the form over `draft` read from
/// `opened`, the server's `answer` to a and the docked bar.
fn drawn(
    request: &Request,
    opened: &Draft,
    draft: &Draft,
    answer: Option<(Html, Standing)>,
    here: String,
    (opened_version, opened_at): (String, String),
) -> Html {
    let locale = &request.locale;
    let standing = match &answer {
        Some((_, standing)) => standing.clone(),
        None if !draft.savable() => Standing::Incomplete(tr(locale, Msg::EditorCodeSystemNeeds)),
        None if draft.body(request.version).ok() == opened.body(request.version).ok() => {
            Standing::Unchanged
        }
        None => Standing::Changed,
    };
    let bar = dock(
        locale,
        tr(
            locale,
            if draft.id.is_empty() {
                Msg::EditorCreateCodeSystem
            } else {
                Msg::EditorSaveCodeSystem
            },
        ),
        &standing,
    );
    let resource = (!opened.url.is_empty()).then(|| {
        let name = if opened.title.is_empty() {
            opened.url.clone()
        } else {
            opened.title.clone()
        };
        (name, system_link(&opened.url, request.version))
    });
    let title = tr(
        locale,
        if opened.id.is_empty() {
            Msg::EditorNewCodeSystem
        } else {
            Msg::EditorEdit
        },
    );
    let body = stack_at(
        Gap::Loose,
        Html::join([
            answer.map(|(view, _)| view).unwrap_or_default(),
            panel_sections_card(
                title.clone(),
                Html::join([
                    metadata(locale, draft),
                    properties(locale, draft),
                    concepts(locale, draft, opened),
                ]),
            ),
        ]),
    );
    Html::join([
        heading(
            request,
            Kind::CodeSystem,
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

/// Writes the posted fields over `draft`, read from `opened`, with the row
/// a Remove names left out and the row an Add asks for added.
fn apply(form: &Posted, opened: &Draft, draft: &mut Draft) {
    apply_metadata(form, draft);
    draft.content = form.text("content");
    draft.case_sensitive = form.get("caseSensitive").is_some();
    let removed_property = form.get(REMOVE_PROPERTY).unwrap_or_default();
    let mut keys = opened.keys;
    draft.properties = rows(form, PROPERTY)
        .into_iter()
        .filter(|(key, _)| key != removed_property)
        .map(|(_, fields)| {
            let code = field(&fields, "code");
            let uri = opened
                .properties
                .iter()
                .find(|held| held.code == code)
                .map(|held| held.uri.clone())
                .unwrap_or_default();
            Declared {
                key: keys.next(),
                code,
                uri,
                kind: field(&fields, "type"),
            }
        })
        .collect();
    if form.intent() == ADD_PROPERTY {
        draft.properties.push(Declared {
            key: keys.next(),
            kind: String::from("code"),
            ..Declared::default()
        });
    }
    let removed_concept = form.get(REMOVE_CONCEPT).unwrap_or_default();
    draft.concepts = rows(form, CONCEPT)
        .into_iter()
        .filter(|(key, _)| key != removed_concept)
        .map(|(_, fields)| {
            let code = field(&fields, "code");
            let held = opened.concepts.iter().find(|held| held.code == code);
            let mut concept = held.cloned().unwrap_or_else(|| Concept {
                key: keys.next(),
                code: code.clone(),
                ..Concept::default()
            });
            concept.display = field(&fields, "display");
            let status = field(&fields, "lifecycle");
            let on = match held {
                Some(held) if held.lifecycle.status() == status => held.lifecycle.on().to_owned(),
                _ => jiff::Zoned::now().date().to_string(),
            };
            concept.lifecycle = Lifecycle::of(&status, &on);
            concept
        })
        .collect();
    if form.intent() == ADD_CONCEPT {
        draft.concepts.push(Concept {
            key: keys.next(),
            ..Concept::default()
        });
    }
    draft.keys = keys;
}

/// The metadata section: the canonical, the names, the version, the status,
/// the content mode and case sensitivity.
fn metadata(locale: &Locale, draft: &Draft) -> Html {
    let statuses = status_options(locale, &draft.status);
    let modes = Html::join(
        CONTENT_MODES
            .iter()
            .map(|mode| option(mode, &content_words(locale, mode), *mode == draft.content)),
    );
    metadata_section(
        locale,
        &Html::join([
            Field::new("cs-url", tr(locale, Msg::EditorCanonical)).view(Html::raw(format!(
                r#"<input id="cs-url" name="url" class="{INPUT} fui-mono" value="{}" required>"#,
                Html::text(&draft.url)
            ))),
            field_flow(Html::join([
                text_field(
                    "cs-title",
                    "title",
                    tr(locale, Msg::EditorTitle),
                    draft.title.clone(),
                    false,
                ),
                text_field(
                    "cs-name",
                    "name",
                    tr(locale, Msg::EditorName),
                    draft.name.clone(),
                    true,
                ),
                text_field(
                    "cs-version",
                    "version",
                    tr(locale, Msg::EditorBusinessVersion),
                    draft.version.clone(),
                    false,
                ),
                Field::new("cs-status", tr(locale, Msg::EditorStatus)).view(Html::raw(format!(
                    r#"<select id="cs-status" name="status" class="{SELECT}">{statuses}</select>"#
                ))),
            ])),
            field_flow(
                Field::new("cs-content", tr(locale, Msg::ContentMode)).view(Html::raw(format!(
                    r#"<select id="cs-content" name="content" class="{SELECT} {MEDIUM}">{modes}</select>"#
                )))
            ),
            check(
                "cs-case",
                tr(locale, Msg::EditorCaseSensitive),
                checkbox("cs-case", "caseSensitive", draft.case_sensitive),
            ),
        ]),
    )
}

/// ` disabled` on a control that is `locked`, nothing on one that is not.
const fn disabled(locked: bool) -> &'static str {
    if locked { " disabled" } else { "" }
}

/// One declared property's row: its code and its type.
fn property_row(locale: &Locale, key: &str, code: &str, kind: &str, end: RowEnd) -> RepeatRow {
    let locked = matches!(end, RowEnd::Kept { .. });
    let code_id = format!("cs-property-{key}-code");
    let type_id = format!("cs-property-{key}-type");
    let types = Html::join(
        PROPERTY_TYPES
            .iter()
            .map(|each| option(each, each, *each == kind)),
    );
    let fields = Html::join([
        Field::new(code_id.clone(), tr(locale, Msg::EditorPropertyCode)).view(Html::raw(format!(
            r#"<input id="{code_id}" name="{}" class="{INPUT} fui-mono" value="{}"{}>"#,
            Html::text(name(PROPERTY, key, "code")),
            Html::text(code),
            disabled(locked)
        ))),
        Field::new(type_id.clone(), tr(locale, Msg::EditorPropertyType)).view(Html::raw(format!(
            r#"<select id="{type_id}" name="{}" class="{SELECT}"{}>{types}</select>"#,
            Html::text(name(PROPERTY, key, "type")),
            disabled(locked)
        ))),
    ]);
    RepeatRow {
        key: key.to_owned(),
        heading: None,
        fields,
        end,
        after: None,
    }
}

/// The declared properties: each one the reader may change or remove, and
/// the lifecycle's own, which the editor writes itself.
fn properties(locale: &Locale, draft: &Draft) -> Html {
    let mut drawn: Vec<RepeatRow> = draft
        .properties
        .iter()
        .map(|property| {
            let key = format!("k{}", property.key.0);
            let end = RowEnd::Remove {
                label: tr_with(
                    locale,
                    Msg::EditorRemoveProperty,
                    &[("code", property.code.clone().into())],
                ),
                name: String::from(REMOVE_PROPERTY),
                value: key.clone(),
            };
            property_row(locale, &key, &property.code, &property.kind, end)
        })
        .collect();
    drawn.extend(["inactive"].into_iter().map(|code| {
        property_row(
            locale,
            code,
            code,
            "boolean",
            RowEnd::Kept {
                reason: tr(locale, Msg::EditorPropertyKept),
            },
        )
    }));
    let count = u64::try_from(draft.properties.len().saturating_add(1)).unwrap_or(u64::MAX);
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::EditorDeclaredProperties),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        repeat(Repeat {
            id: String::from("cs-properties"),
            label: tr(locale, Msg::EditorDeclaredProperties),
            fields: Fields::Two,
            head: Vec::new(),
            rows: drawn,
            token: ROW,
            blank: None,
            add: Some(RepeatAdd {
                label: tr(locale, Msg::EditorDeclareProperty),
                name: String::from(INTENT),
                value: String::from(ADD_PROPERTY),
            }),
            empty: tr(locale, Msg::EditorNoProperties),
            form: None,
        }),
    )
}

/// A concept row's code: read-only once the server holds it, typed with a
/// Remove beside it while it is new.
fn code_cell(locale: &Locale, concept: &Concept, key: &str, held: bool) -> Html {
    if held {
        Html::join([
            code(&concept.code),
            hidden_field(&name(CONCEPT, key, "code"), &concept.code),
        ])
    } else {
        let remove = Html::text(tr(locale, Msg::EditorRemoveConcept));
        Html::raw(format!(
            r#"<input name="{}" class="{INPUT} fui-mono" value="{}" aria-label="{}"><button type="submit" class="fui-icon-button" name="{REMOVE_CONCEPT}" value="{}" formnovalidate aria-label="{remove}" title="{remove}">{}</button>"#,
            Html::text(name(CONCEPT, key, "code")),
            Html::text(&concept.code),
            Html::text(tr(locale, Msg::EditorConceptCode)),
            Html::text(key),
            ferrohealth_ui::icon::icon(Glyph::X, "16"),
        ))
    }
}

/// The concepts: each code with its display, its parent and its
/// lifecycle; a code the server holds is fixed, a new one is typed.
fn concepts(locale: &Locale, draft: &Draft, opened: &Draft) -> Html {
    let columns = [
        Column::text("code", tr(locale, Msg::EditorConceptCode)).role(Role::Title),
        Column::text("display", tr(locale, Msg::EditorConceptDisplay)),
        Column::text("parent", tr(locale, Msg::EditorConceptParent)).role(Role::Quiet),
        Column::text("lifecycle", tr(locale, Msg::EditorConceptLifecycle)),
    ];
    let [code_column, display_column, parent_column, lifecycle_column] = &columns;
    let rows = Html::join(draft.concepts.iter().map(|concept| {
        let key = format!("k{}", concept.key.0);
        let held = opened
            .concepts
            .iter()
            .any(|stored| stored.code == concept.code && !concept.code.is_empty());
        let drawn_code = code_cell(locale, concept, &key, held);
        let parent = concept
            .values
            .iter()
            .find(|value| value.code == "parent")
            .map_or_else(
                || Html::text(tr(locale, Msg::EditorTopLevel)),
                |value| code(&value.text),
            );
        let status = concept.lifecycle.status();
        let choices = Html::join(
            LIFECYCLES
                .iter()
                .map(|each| option(each, &lifecycle_words(locale, each), *each == status)),
        );
        let display_label = tr_with(
            locale,
            Msg::EditorDisplayOf,
            &[("code", concept.code.clone().into())],
        );
        let lifecycle_label = tr_with(
            locale,
            Msg::EditorLifecycleOf,
            &[("code", concept.code.clone().into())],
        );
        Html::raw(format!(
            "<tr>{}{}{}{}</tr>",
            cell(code_column, drawn_code),
            cell(
                display_column,
                Html::raw(format!(
                    r#"<input name="{}" class="{INPUT}" value="{}" aria-label="{}">"#,
                    Html::text(name(CONCEPT, &key, "display")),
                    Html::text(&concept.display),
                    Html::text(display_label)
                )),
            ),
            cell(parent_column, parent),
            cell(
                lifecycle_column,
                Html::raw(format!(
                    r#"<select name="{}" class="{SELECT}" aria-label="{}">{choices}</select>"#,
                    Html::text(name(CONCEPT, &key, "lifecycle")),
                    Html::text(lifecycle_label)
                )),
            ),
        ))
    }));
    let caption = tr(locale, Msg::EditorConcepts);
    let count = u64::try_from(draft.concepts.len()).unwrap_or(u64::MAX);
    let add = Html::raw(format!(
        r#"<div class="fui-add-row">{}</div>"#,
        add_button(ADD_CONCEPT.to_owned(), tr(locale, Msg::EditorAddConcept))
    ));
    panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        Html::join([
            section_intro(tr(locale, Msg::EditorRetiredNotRemoved)),
            data_table(DataTable {
                shape: Shape::Stacked,
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            }),
            add,
        ]),
    )
}

/// The page after a save: the version the server now holds, and the way to
/// the code system.
fn saved_page(request: &Request, opened: &Draft, saved: &str) -> Html {
    let href = system_link(&opened.url, request.version);
    let open = card_action(href.clone(), tr(&request.locale, Msg::EditorOpenCodeSystem));
    let resource = (!opened.url.is_empty()).then(|| (opened.title.clone(), href));
    editor::saved_page(request, Kind::CodeSystem, resource, open, saved)
}
