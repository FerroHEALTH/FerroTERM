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

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Role, Sorting, cell, data_table};
use ferrohealth_ui::view::form::{FORM, Field, INPUT, MEDIUM, SELECT, check, field_flow};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, panel_section, panel_sections_card, section_intro, stack,
};
use ferrohealth_ui::widget::repeat::{Fields, Repeat, RepeatAdd, RepeatRow, RowEnd, repeat};
use leptos::prelude::*;

use crate::auth::scopes::Letter;
use crate::authoring::code_system::{Concept, Declared, Draft, Lifecycle};
use crate::fhir::CODE_SYSTEM;
use crate::fhir::compose::STATUSES;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::detail::Kind;
use crate::pages::editor::{
    DISCARD, OPENED, OPENED_AT, SAVE, SAVED_PARAM, Standing, add_button, dock, field, form,
    heading, name, opened_line, percent, refused, rows, text_field,
};
use crate::pages::parts::{failure, loaded};
use crate::posted::{INTENT, Posted, posted};
use crate::request::{Request, request};
use crate::routes::{
    CODE_SYSTEM_EDITOR_ROUTE, EDITOR_SYSTEM_PARAM, SYSTEMS_PATH, sign_in_link, system_link, ui_link,
};

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
fn lifecycle_words(status: &str) -> String {
    tr(match status {
        "experimental" => Msg::EditorLifecycleExperimental,
        "deprecated" => Msg::EditorLifecycleDeprecated,
        "retired" => Msg::EditorLifecycleRetired,
        _ => Msg::EditorLifecycleActive,
    })
}

/// The words of a publication status.
fn status_words(status: &str) -> String {
    tr(match status {
        "active" => Msg::StatusActive,
        "retired" => Msg::StatusRetired,
        "unknown" => Msg::StatusUnknown,
        _ => Msg::StatusDraft,
    })
}

/// The words of a content mode.
fn content_words(mode: &str) -> String {
    tr(match mode {
        "complete" => Msg::ContentComplete,
        "not-present" => Msg::ContentNotPresent,
        "example" => Msg::ContentExample,
        "fragment" => Msg::ContentFragment,
        "supplement" => Msg::ContentSupplement,
        _ => Msg::ContentUndeclared,
    })
}

/// The code system editor.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn CodeSystemEditorPage() -> impl IntoView {
    let request = request();
    let posted = posted();
    let letter = if request.text(EDITOR_SYSTEM_PARAM).is_empty() {
        Letter::Create
    } else {
        Letter::Update
    };
    if !request.can(CODE_SYSTEM, letter) {
        if request.session.who().is_none() && request.sign_in {
            leptos_axum::redirect(&sign_in_link(&request.back()));
        }
        return crate::pages::errors::forbidden_page(
            Msg::EditorForbiddenCodeSystem,
            "user/CodeSystem.cu",
            (
                tr(Msg::ShellCodeSystems),
                ui_link(SYSTEMS_PATH, request.version),
            ),
        );
    }
    loaded(async move { page(&request, posted.as_ref()).await })
}

/// The editor's address: the canonical it edits, on the reader's version.
fn address(request: &Request, canonical: &str) -> String {
    let base = format!(
        "{CODE_SYSTEM_EDITOR_ROUTE}?fhir={}",
        request.version.segment()
    );
    if canonical.is_empty() {
        base
    } else {
        format!("{base}&{EDITOR_SYSTEM_PARAM}={}", percent(canonical))
    }
}

/// The page, after whatever the reader `posted`.
async fn page(request: &Request, posted: Option<&Posted>) -> AnyView {
    let system = request.text(EDITOR_SYSTEM_PARAM);
    let here = address(request, &system);
    if posted.is_some_and(|form| form.intent() == DISCARD) {
        leptos_axum::redirect(&here);
        return ().into_any();
    }
    let stored = if system.is_empty() {
        None
    } else {
        match request.client.authored(request.version, &system).await {
            Ok(Some(resource)) => Some(resource),
            Ok(None) => return crate::pages::errors::not_found_page(),
            Err(error) => return failure(&error),
        }
    };
    let opened = stored.as_ref().map_or_else(Draft::new, Draft::of);
    if stored.is_some() && !opened.managed() {
        return crate::pages::editor::unmanaged(
            request,
            Kind::CodeSystem,
            system_link(&opened.url, request.version),
        );
    }
    if let Some(saved) = request.param(SAVED_PARAM) {
        return saved_page(request, &opened, &saved);
    }
    let mut draft = opened.clone();
    let mut opened_version = opened.version_id.clone();
    let mut opened_at = jiff::Timestamp::now().to_string();
    let mut answer = None;
    if let Some(form) = posted {
        opened_version = form.text(OPENED);
        opened_at = form.text(OPENED_AT);
        apply(form, &opened, &mut draft);
        if form.intent() == SAVE && draft.savable() {
            let body = draft.body();
            let sent =
                crate::pages::editor::send(request, CODE_SYSTEM, &draft.id, &body, &opened_version)
                    .await;
            match sent {
                Ok(written) => {
                    let version = written.version_id().unwrap_or_default().to_owned();
                    leptos_axum::redirect(&format!(
                        "{}&{SAVED_PARAM}={}",
                        address(request, draft.url.trim()),
                        percent(&version)
                    ));
                    return ().into_any();
                }
                Err(error) => answer = Some(refused(request, &error, &here)),
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
/// `opened`, the server's `answer` to a save, and the docked bar.
fn drawn(
    request: &Request,
    opened: &Draft,
    draft: &Draft,
    answer: Option<(AnyView, Standing)>,
    here: String,
    (opened_version, opened_at): (String, String),
) -> AnyView {
    let standing = match &answer {
        Some((_, standing)) => standing.clone(),
        None if !draft.savable() => Standing::Incomplete(tr(Msg::EditorCodeSystemNeeds)),
        None if draft.body() == opened.body() => Standing::Unchanged,
        None => Standing::Changed,
    };
    let bar = dock(
        tr(if draft.id.is_empty() {
            Msg::EditorCreateCodeSystem
        } else {
            Msg::EditorSaveCodeSystem
        }),
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
    let title = tr(if opened.id.is_empty() {
        Msg::EditorNewCodeSystem
    } else {
        Msg::EditorEdit
    });
    let body = stack(view! {
        {answer.map(|(view, _)| view)}
        {panel_sections_card(
            title.clone(),
            view! {
                {metadata(draft)}
                {properties(draft)}
                {concepts(draft, opened)}
            },
        )}
    })
    .into_any();
    view! {
        {heading(
            request,
            Kind::CodeSystem,
            resource,
            title,
            opened_line(&opened.version_id, &opened_at),
        )}
        {form(here, &[(OPENED, opened_version), (OPENED_AT, opened_at)], false, body)}
        {bar}
    }
    .into_any()
}

/// Writes the posted fields over `draft`, read from `opened`, with the row
/// a Remove names left out and the row an Add asks for added.
fn apply(form: &Posted, opened: &Draft, draft: &mut Draft) {
    draft.url = form.text("url");
    draft.title = form.text("title");
    draft.name = form.text("name");
    draft.version = form.text("version");
    draft.status = form.text("status");
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
fn metadata(draft: &Draft) -> AnyView {
    let option = |value: &str, picked: &str, words: String| {
        view! {
            <option value=value.to_owned() selected=value == picked>
                {words}
            </option>
        }
    };
    let statuses = STATUSES
        .iter()
        .map(|status| option(status, &draft.status, status_words(status)))
        .collect_view();
    let modes = CONTENT_MODES
        .iter()
        .map(|mode| option(mode, &draft.content, content_words(mode)))
        .collect_view();
    panel_section(
        Some(SectionHead {
            title: tr(Msg::EditorMetadata),
            count: None,
            actions: None,
        }),
        view! {
            <div class=FORM>
                {Field::new("cs-url", tr(Msg::EditorCanonical))
                    .view(
                        view! {
                            <input
                                id="cs-url"
                                name="url"
                                class=format!("{INPUT} fui-mono")
                                value=draft.url.clone()
                                required=true
                            />
                        },
                    )}
                {field_flow(
                    view! {
                        {text_field(
                            "cs-title",
                            "title",
                            tr(Msg::EditorTitle),
                            draft.title.clone(),
                            false,
                        )}
                        {text_field(
                            "cs-name",
                            "name",
                            tr(Msg::EditorName),
                            draft.name.clone(),
                            true,
                        )}
                        {text_field(
                            "cs-version",
                            "version",
                            tr(Msg::EditorBusinessVersion),
                            draft.version.clone(),
                            false,
                        )}
                        {Field::new("cs-status", tr(Msg::EditorStatus))
                            .view(
                                view! {
                                    <select id="cs-status" name="status" class=SELECT>
                                        {statuses}
                                    </select>
                                },
                            )}
                    },
                )}
                {field_flow(
                    view! {
                        {Field::new("cs-content", tr(Msg::ContentMode))
                            .view(
                                view! {
                                    <select
                                        id="cs-content"
                                        name="content"
                                        class=format!("{SELECT} {MEDIUM}")
                                    >
                                        {modes}
                                    </select>
                                },
                            )}
                    },
                )}
                {check(
                    "cs-case",
                    tr(Msg::EditorCaseSensitive),
                    view! {
                        <input
                            type="checkbox"
                            id="cs-case"
                            name="caseSensitive"
                            value="true"
                            checked=draft.case_sensitive
                        />
                    },
                )}
            </div>
        },
    )
}

/// One declared property's row: its code and its type.
fn property_row(key: &str, code: &str, kind: &str, end: RowEnd) -> RepeatRow {
    let locked = matches!(end, RowEnd::Kept { .. });
    let code_id = format!("cs-property-{key}-code");
    let type_id = format!("cs-property-{key}-type");
    let types = PROPERTY_TYPES
        .iter()
        .map(|each| {
            view! {
                <option value=*each selected=*each == kind>
                    {*each}
                </option>
            }
        })
        .collect_view();
    let fields = view! {
        {Field::new(code_id.clone(), tr(Msg::EditorPropertyCode))
            .view(
                view! {
                    <input
                        id=code_id
                        name=name(PROPERTY, key, "code")
                        class=format!("{INPUT} fui-mono")
                        value=code.to_owned()
                        disabled=locked
                    />
                },
            )}
        {Field::new(type_id.clone(), tr(Msg::EditorPropertyType))
            .view(
                view! {
                    <select
                        id=type_id
                        name=name(PROPERTY, key, "type")
                        class=SELECT
                        disabled=locked
                    >
                        {types}
                    </select>
                },
            )}
    }
    .into_any();
    RepeatRow {
        key: key.to_owned(),
        fields,
        end,
        after: None,
    }
}

/// The declared properties: each one the reader may change or remove, and
/// the lifecycle's own, which the editor writes itself.
fn properties(draft: &Draft) -> AnyView {
    let mut drawn: Vec<RepeatRow> = draft
        .properties
        .iter()
        .map(|property| {
            let key = format!("k{}", property.key.0);
            let end = RowEnd::Remove {
                label: tr_with(
                    Msg::EditorRemoveProperty,
                    &[("code", property.code.clone().into())],
                ),
                name: String::from(REMOVE_PROPERTY),
                value: key.clone(),
            };
            property_row(&key, &property.code, &property.kind, end)
        })
        .collect();
    drawn.extend(["inactive"].into_iter().map(|code| {
        property_row(
            code,
            code,
            "boolean",
            RowEnd::Kept {
                reason: tr(Msg::EditorPropertyKept),
            },
        )
    }));
    let count = u64::try_from(draft.properties.len().saturating_add(1)).unwrap_or(u64::MAX);
    panel_section(
        Some(SectionHead {
            title: tr(Msg::EditorDeclaredProperties),
            count: Some(count),
            actions: None,
        }),
        repeat(Repeat {
            id: String::from("cs-properties"),
            label: tr(Msg::EditorDeclaredProperties),
            fields: Fields::Two,
            head: Vec::new(),
            rows: drawn,
            blank: None,
            add: Some(RepeatAdd {
                label: tr(Msg::EditorDeclareProperty),
                name: String::from(INTENT),
                value: String::from(ADD_PROPERTY),
            }),
            empty: tr(Msg::EditorNoProperties),
            form: None,
        }),
    )
}

/// A concept row's code: read-only once the server holds it, typed with a
/// Remove beside it while it is new.
fn code_cell(concept: &Concept, key: &str, held: bool) -> AnyView {
    if held {
        view! {
            <code>{concept.code.clone()}</code>
            <input type="hidden" name=name(CONCEPT, key, "code") value=concept.code.clone() />
        }
        .into_any()
    } else {
        let label = tr(Msg::EditorConceptCode);
        view! {
            <input
                name=name(CONCEPT, key, "code")
                class=format!("{INPUT} fui-mono")
                value=concept.code.clone()
                aria-label=label
            />
            <button
                type="submit"
                class="fui-icon-button"
                name=REMOVE_CONCEPT
                value=key.to_owned()
                formnovalidate=true
                aria-label=tr(Msg::EditorRemoveConcept)
                title=tr(Msg::EditorRemoveConcept)
            >
                {ferrohealth_ui::icon::icon(Glyph::X, "16")}
            </button>
        }
        .into_any()
    }
}

/// The concepts: each code with its display, its parent and its
/// lifecycle; a code the server holds is fixed, a new one is typed.
fn concepts(draft: &Draft, opened: &Draft) -> AnyView {
    let columns = [
        Column::text("code", tr(Msg::EditorConceptCode)).role(Role::Title),
        Column::text("display", tr(Msg::EditorConceptDisplay)),
        Column::text("parent", tr(Msg::EditorConceptParent)).role(Role::Quiet),
        Column::text("lifecycle", tr(Msg::EditorConceptLifecycle)),
    ];
    let [code_column, display_column, parent_column, lifecycle_column] = &columns;
    let rows = draft
        .concepts
        .iter()
        .map(|concept| {
            let key = format!("k{}", concept.key.0);
            let held = opened
                .concepts
                .iter()
                .any(|stored| stored.code == concept.code && !concept.code.is_empty());
            let code = code_cell(concept, &key, held);
            let parent = concept
                .values
                .iter()
                .find(|value| value.code == "parent")
                .map_or_else(
                    || tr(Msg::EditorTopLevel).into_any(),
                    |value| view! { <code>{value.text.clone()}</code> }.into_any(),
                );
            let status = concept.lifecycle.status();
            let choices = LIFECYCLES
                .iter()
                .map(|each| {
                    view! {
                        <option value=*each selected=*each == status>
                            {lifecycle_words(each)}
                        </option>
                    }
                })
                .collect_view();
            let display_label = tr_with(
                Msg::EditorDisplayOf,
                &[("code", concept.code.clone().into())],
            );
            let lifecycle_label = tr_with(
                Msg::EditorLifecycleOf,
                &[("code", concept.code.clone().into())],
            );
            view! {
                <tr>
                    {cell(code_column, code)}
                    {cell(
                        display_column,
                        view! {
                            <input
                                name=name(CONCEPT, &key, "display")
                                class=INPUT
                                value=concept.display.clone()
                                aria-label=display_label
                            />
                        },
                    )} {cell(parent_column, parent)}
                    {cell(
                        lifecycle_column,
                        view! {
                            <select
                                name=name(CONCEPT, &key, "lifecycle")
                                class=SELECT
                                aria-label=lifecycle_label
                            >
                                {choices}
                            </select>
                        },
                    )}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::EditorConcepts);
    let count = u64::try_from(draft.concepts.len()).unwrap_or(u64::MAX);
    let add = view! {
        <div class="fui-add-row">
            {add_button(ADD_CONCEPT.to_owned(), tr(Msg::EditorAddConcept))}
        </div>
    };
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(count),
            actions: None,
        }),
        view! {
            {section_intro(tr(Msg::EditorRetiredNotRemoved))}
            {data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            })}
            {add}
        },
    )
}

/// The page after a save: the version the server now holds, and the way to
/// the code system.
fn saved_page(request: &Request, opened: &Draft, saved: &str) -> AnyView {
    let open = card_action(
        system_link(&opened.url, request.version),
        tr(Msg::EditorOpenCodeSystem),
    );
    let resource = (!opened.url.is_empty()).then(|| {
        (
            opened.title.clone(),
            system_link(&opened.url, request.version),
        )
    });
    view! {
        {heading(
            request,
            Kind::CodeSystem,
            resource,
            tr(Msg::EditorSavedTitle),
            tr_with(Msg::EditorVersion, &[("version", saved.to_owned().into())]),
        )}
        {load_report(LoadReport {
            outcome: Outcome::Ok,
            arrival: Arrival::AfterAction,
            title: tr_with(Msg::EditorSaved, &[("version", saved.to_owned().into())]).into_any(),
            counts: Vec::new(),
            action: Some(open),
        })}
    }
    .into_any()
}
