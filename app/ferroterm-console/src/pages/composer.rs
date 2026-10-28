// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value set composer: a `ValueSet`'s metadata and its includes and
//! excludes (M4), each a code system with its filters or its codes, or a
//! published value set, with a preview of what the draft selects.
//!
//! The criteria of one include intersect and an exclude takes codes back
//! out (<https://hl7.org/fhir/R4B/valueset.html#compositions>). The preview
//! sends the unsaved compose inline to `ValueSet/$expand`
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). It is one
//! form posted to its own address (`crate::pages::editor`).

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::clause::{Clause, ClauseIssue, ClauseKind, clause, clauses};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::form::{FORM, Field, INPUT, SELECT, field_flow};
use ferrohealth_ui::view::layout::main_aside;
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, panel_section, panel_sections_card, section_intro, stack,
};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::transform::transform_quiet;
use leptos::prelude::*;

use crate::auth::scopes::Letter;
use crate::fhir::compose::{Draft, Preview, STATUSES, StoredValueSet};
use crate::fhir::concept::chosen_version;
use crate::fhir::terminology::{FilterRow, SystemCard};
use crate::fhir::{CODE_SYSTEM, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::detail::Kind;
use crate::pages::editor::{
    DISCARD, OPENED, OPENED_AT, SAVE, SAVED_PARAM, Standing, add_button, dock, field, form,
    heading, icon_submit, name, opened_line, percent, refused, rows, text_field,
};
use crate::pages::operations::choices;
use crate::pages::parts::{failure, name_of, redirect};
use crate::posted::Posted;
use crate::request::Request;
use crate::routes::{
    COMPOSE_ROUTE, COMPOSER_ID_PARAM, VALUE_SETS_PATH, resource_link, sign_in_link, ui_link,
};

/// The list of clauses in the form.
const CLAUSE: &str = "clause";

/// The list of filter rows in the form.
const FILTER: &str = "filter";

/// The list of code rows in the form.
const CODE: &str = "code";

/// The list of value set rows in the form.
const VALUE_SET_ROW: &str = "vs";

/// The intent that runs the preview.
const PREVIEW: &str = "preview";

/// How many concepts the preview shows.
const PREVIEW_COUNT: u32 = 10;

/// The filter operators a composer offers
/// (<https://hl7.org/fhir/R4B/valueset-filter-operator.html>).
const OPERATORS: [&str; 10] = [
    "=",
    "is-a",
    "descendent-of",
    "is-not-a",
    "regex",
    "in",
    "not-in",
    "generalizes",
    "child-of",
    "exists",
];

/// The value set composer.
pub(crate) async fn page(request: Request) -> AnyView {
    let posted = request.posted.clone();
    let letter = if request.text(COMPOSER_ID_PARAM).is_empty() {
        Letter::Create
    } else {
        Letter::Update
    };
    if !request.can(VALUE_SET, letter) {
        if request.session.who().is_none() && request.sign_in {
            redirect(&sign_in_link(&request.back()));
        }
        return crate::pages::errors::forbidden_page(
            Msg::EditorForbiddenValueSet,
            "user/ValueSet.cu",
            (
                tr(Msg::ShellValueSets),
                ui_link(VALUE_SETS_PATH, request.version),
            ),
        );
    }
    edited(&request, posted.as_ref()).await
}

/// The composer's address: the value set it edits, on the reader's version.
fn address(request: &Request, id: &str) -> String {
    let base = format!("{COMPOSE_ROUTE}?fhir={}", request.version.segment());
    if id.is_empty() {
        base
    } else {
        format!("{base}&{COMPOSER_ID_PARAM}={}", percent(id))
    }
}

/// The page, after whatever the reader `posted`.
async fn edited(request: &Request, posted: Option<&Posted>) -> AnyView {
    let id = request.text(COMPOSER_ID_PARAM);
    let here = address(request, &id);
    if posted.is_some_and(|form| form.intent() == DISCARD) {
        redirect(&here);
        return ().into_any();
    }
    let opened = match open(request, &id).await {
        Ok(opened) => opened,
        Err(page) => return page,
    };
    if let Some(saved) = request.param(SAVED_PARAM) {
        return saved_page(request, &opened, &saved);
    }
    let systems_named = request
        .client
        .published_names(request.version, CODE_SYSTEM)
        .await
        .ok();
    let systems = choices(systems_named.as_ref());
    let names = systems_named
        .as_ref()
        .map(crate::fhir::named::NamedSearch::names)
        .unwrap_or_default();
    let cards = request
        .client
        .terminology_capabilities(request.version)
        .await
        .map(|capabilities| capabilities.cards())
        .unwrap_or_default();
    let mut draft = opened.clone();
    let mut opened_version = opened.version_id.clone();
    let mut opened_at = jiff::Timestamp::now().to_string();
    let mut answer = None;
    let mut preview = None;
    if let Some(form) = posted {
        opened_version = form.text(OPENED);
        opened_at = form.text(OPENED_AT);
        apply(form, &mut draft);
        if form.intent() == PREVIEW && draft.broken().is_empty() {
            preview = Some(run_preview(request, &draft).await);
        }
        if form.intent() == SAVE && draft.savable() {
            match save(request, &draft, &opened_version, &here).await {
                Ok(()) => return ().into_any(),
                Err(refusal) => answer = Some(*refusal),
            }
        }
    }
    let standing = match &answer {
        Some((_, standing)) => standing.clone(),
        None => standing(&opened, &draft),
    };
    let bar = dock(
        tr(if draft.id.is_empty() {
            Msg::EditorCreateValueSet
        } else {
            Msg::EditorSaveValueSet
        }),
        &standing,
    );
    let resource = crumb(request, &opened);
    let title = tr(if opened.id.is_empty() {
        Msg::EditorNewValueSet
    } else {
        Msg::EditorEdit
    });
    let editor = panel_sections_card(
        title.clone(),
        view! {
            {metadata(&draft)}
            {compose(
                &draft,
                &Offers {
                    systems: &systems,
                    names: &names,
                    cards: &cards,
                },
            )}
        },
    );
    let body = stack(view! {
        {answer.map(|(view, _)| view)}
        {main_aside(editor, preview_card(preview, &names))}
    })
    .into_any();
    view! {
        {heading(
            request,
            Kind::ValueSet,
            resource,
            title,
            opened_line(&opened.version_id, &opened_at),
        )}
        {form(here, &[(OPENED, opened_version), (OPENED_AT, opened_at)], true, body)}
        {bar}
    }
    .into_any()
}

/// The first page of what `draft` selects, expanded by the server.
async fn run_preview(
    request: &Request,
    draft: &Draft,
) -> Result<crate::fhir::expansion::ExpandedValueSet, crate::fhir::error::FhirError> {
    let page = Preview {
        filter: None,
        count: PREVIEW_COUNT,
        offset: 0,
    };
    request
        .client
        .preview_compose(request.version, draft, &page)
        .await
}

/// Saves `draft` over the version `opened` and redirects to what the
/// server holds now, or answers the refusal over the draft at `here`.
async fn save(
    request: &Request,
    draft: &Draft,
    opened: &str,
    here: &str,
) -> Result<(), Box<(AnyView, Standing)>> {
    let body = draft.resource().to_string();
    match crate::pages::editor::send(request, VALUE_SET, &draft.id, &body, opened).await {
        Ok(written) => {
            let saved_id = written
                .resource
                .as_ref()
                .and_then(|resource| resource.id.clone())
                .unwrap_or_else(|| draft.id.clone());
            let version = written.version_id().unwrap_or_default().to_owned();
            redirect(&format!(
                "{}&{SAVED_PARAM}={}",
                address(request, &saved_id),
                percent(&version)
            ));
            Ok(())
        }
        Err(error) => Err(Box::new(refused(request, &error, here))),
    }
}

/// The draft of the value set `id` names, a new one for none, or the page
/// that says why it cannot be edited.
async fn open(request: &Request, id: &str) -> Result<Draft, AnyView> {
    if id.is_empty() {
        return Ok(Draft::new());
    }
    match request.client.value_set_stored(request.version, id).await {
        Ok(resource) => {
            let stored = StoredValueSet::of(&resource);
            if stored.writable() {
                Ok(stored.draft(&resource))
            } else {
                Err(crate::pages::editor::unmanaged(
                    request,
                    Kind::ValueSet,
                    resource_link(VALUE_SETS_PATH, id, request.version),
                ))
            }
        }
        Err(error) => Err(failure(&error)),
    }
}

/// Where `draft` stands against the value set `opened` read.
fn standing(opened: &Draft, draft: &Draft) -> Standing {
    if !draft.savable() {
        Standing::Incomplete(tr(Msg::EditorValueSetNeeds))
    } else if draft.resource() == opened.resource() {
        Standing::Unchanged
    } else {
        Standing::Changed
    }
}

/// The trail's step to the value set `opened` read, once it is stored.
fn crumb(request: &Request, opened: &Draft) -> Option<(String, String)> {
    (!opened.id.is_empty()).then(|| {
        let shown = if opened.title.is_empty() {
            opened.url.clone()
        } else {
            opened.title.clone()
        };
        (
            shown,
            resource_link(VALUE_SETS_PATH, &opened.id, request.version),
        )
    })
}

/// The clause key an intent such as `add-filter:3` names, when it names
/// `action`.
fn target<'a>(intent: &'a str, action: &str) -> Option<&'a str> {
    intent
        .strip_prefix(action)
        .and_then(|rest| rest.strip_prefix(':'))
}

/// The fields of each row of `rows` that belongs to the clause `key` and
/// that `intent` does not remove with the action `remove`.
fn kept<'a>(
    rows: &'a [(String, BTreeMap<String, String>)],
    key: &'a str,
    intent: &'a str,
    remove: &'a str,
) -> impl Iterator<Item = &'a BTreeMap<String, String>> + 'a {
    rows.iter()
        .filter(move |(row, each)| {
            field(each, "clause") == key && target(intent, remove) != Some(row.as_str())
        })
        .map(|(_, each)| each)
}

/// Writes the posted fields over `draft`, the row a Remove names left out
/// and the row an Add asks for added.
fn apply(form: &Posted, draft: &mut Draft) {
    let intent = form.intent();
    draft.url = form.text("url");
    draft.title = form.text("title");
    draft.name = form.text("name");
    draft.version = form.text("version");
    draft.status = form.text("status");
    draft.inactive = match form.text("inactive").as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    };
    let filters = rows(form, FILTER);
    let codes = rows(form, CODE);
    let sets = rows(form, VALUE_SET_ROW);
    draft.clauses.clear();
    for (key, fields) in rows(form, CLAUSE) {
        if target(intent, "remove-clause") == Some(key.as_str()) {
            continue;
        }
        let made = draft.add_clause(field(&fields, "side") != "exclude");
        if let Some(clause) = draft.clause_mut(made) {
            clause.system = field(&fields, "system");
            clause.system_version = field(&fields, "version");
        }
        for each in kept(&filters, &key, intent, "remove-filter") {
            draft.add_filter(made, &field(each, "property"), &field(each, "op"));
            if let Some(last) = draft
                .clause_mut(made)
                .and_then(|clause| clause.filters.last_mut())
            {
                last.value = field(each, "value");
            }
        }
        for each in kept(&codes, &key, intent, "remove-code") {
            draft.add_concept(made, &field(each, "code"), &field(each, "served"));
            if let Some(last) = draft
                .clause_mut(made)
                .and_then(|clause| clause.concepts.last_mut())
            {
                last.display = field(each, "display");
            }
        }
        for each in kept(&sets, &key, intent, "remove-vs") {
            draft.add_value_set(made, &field(each, "url"));
        }
        if target(intent, "add-filter") == Some(key.as_str()) {
            draft.add_filter(made, "concept", "is-a");
        }
        if target(intent, "add-code") == Some(key.as_str()) {
            draft.add_concept(made, "", "");
        }
    }
    match intent {
        "add-include" => {
            draft.add_clause(true);
        }
        "add-exclude" => {
            draft.add_clause(false);
        }
        "add-value-set" => {
            let made = draft.add_clause(true);
            draft.add_value_set(made, "");
        }
        _ => {}
    }
}

/// The metadata section.
fn metadata(draft: &Draft) -> AnyView {
    let statuses = STATUSES
        .iter()
        .map(|status| {
            view! {
                <option value=*status selected=*status == draft.status>
                    {status_words(status)}
                </option>
            }
        })
        .collect_view();
    let inactive = [
        ("", Msg::EditorInactiveServer),
        ("true", Msg::EditorInactiveInclude),
        ("false", Msg::EditorInactiveExclude),
    ]
    .into_iter()
    .map(|(value, words)| {
        let picked = match draft.inactive {
            Some(true) => "true",
            Some(false) => "false",
            None => "",
        };
        view! {
            <option value=value selected=value == picked>
                {tr(words)}
            </option>
        }
    })
    .collect_view();
    panel_section(
        Some(SectionHead {
            title: tr(Msg::EditorMetadata),
            count: None,
            actions: None,
        }),
        view! {
            <div class=FORM>
                {text_field("vs-url", "url", tr(Msg::EditorCanonical), draft.url.clone(), true)}
                {field_flow(
                    view! {
                        {text_field(
                            "vs-title",
                            "title",
                            tr(Msg::EditorTitle),
                            draft.title.clone(),
                            false,
                        )}
                        {Field::new("vs-inactive", tr(Msg::EditorInactiveCodes))
                            .view(
                                view! {
                                    <select id="vs-inactive" name="inactive" class=SELECT>
                                        {inactive}
                                    </select>
                                },
                            )}
                    },
                )}
                {field_flow(
                    view! {
                        {text_field(
                            "vs-name",
                            "name",
                            tr(Msg::EditorName),
                            draft.name.clone(),
                            true,
                        )}
                        {text_field(
                            "vs-version",
                            "version",
                            tr(Msg::EditorBusinessVersion),
                            draft.version.clone(),
                            false,
                        )}
                        {Field::new("vs-status", tr(Msg::EditorStatus))
                            .view(
                                view! {
                                    <select id="vs-status" name="status" class=SELECT>
                                        {statuses}
                                    </select>
                                },
                            )}
                    },
                )}
            </div>
        },
    )
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

/// The includes and excludes, each an editable clause, and the ways to add
/// one.
fn compose(draft: &Draft, offers: &Offers<'_>) -> AnyView {
    let drawn = draft
        .clauses
        .iter()
        .map(|each| clause_view(each, offers))
        .collect_view();
    let count = u64::try_from(draft.clauses.len()).unwrap_or(u64::MAX);
    panel_section(
        Some(SectionHead {
            title: tr(Msg::EditorIncludesExcludes),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        view! {
            {section_intro(tr(Msg::EditorIncludesExcludesIntro))}
            {clauses(drawn)}
            <div class="fui-add-row">
                {add_button(String::from("add-include"), tr(Msg::EditorAddInclude))}
                {add_button(String::from("add-exclude"), tr(Msg::EditorAddExclude))}
                {add_button(String::from("add-value-set"), tr(Msg::EditorAddValueSet))}
            </div>
        },
    )
}

/// What the composer offers a clause: the published code systems, their
/// names, and what each declares it serves.
#[derive(Clone, Copy, Debug)]
struct Offers<'a> {
    /// The published code systems, each its canonical and its name.
    systems: &'a [(String, String)],
    /// The name each code system goes by, by canonical.
    names: &'a BTreeMap<String, String>,
    /// What the server declares about each code system it serves.
    cards: &'a [SystemCard],
}

/// A clause's side, its code system and its version, as fields.
fn clause_fields(
    key: &str,
    each: &crate::fhir::compose::Clause,
    systems: &[(String, String)],
) -> AnyView {
    let blank = view! {
        <option value="" selected=each.system.is_empty()>
            {tr(Msg::EditorNoSystem)}
        </option>
    };
    let offered = systems
        .iter()
        .map(|(value, words)| {
            view! {
                <option value=value.clone() selected=*value == each.system>
                    {words.clone()}
                </option>
            }
        })
        .collect_view();
    let system_options = view! {
        {blank}
        {offered}
    };
    let system_id = format!("vs-{key}-system");
    let version_id = format!("vs-{key}-version");
    view! {
        <input
            type="hidden"
            name=name(CLAUSE, key, "side")
            value=if each.included { "include" } else { "exclude" }
        />
        {field_flow(
            view! {
                {Field::new(system_id.clone(), tr(Msg::LookupSystem))
                    .view(
                        view! {
                            <select id=system_id name=name(CLAUSE, key, "system") class=SELECT>
                                {system_options}
                            </select>
                        },
                    )}
                {Field::new(version_id.clone(), tr(Msg::LookupVersion))
                    .view(
                        view! {
                            <input
                                id=version_id
                                name=name(CLAUSE, key, "version")
                                class=INPUT
                                value=each.system_version.clone()
                                placeholder=tr(Msg::LookupServerVersion)
                            />
                        },
                    )}
            },
        )}
    }
    .into_any()
}

/// One clause, editable: its code system and version, its value sets, its
/// filters and its codes, with the ways to add and remove each.
fn clause_view(each: &crate::fhir::compose::Clause, offers: &Offers<'_>) -> AnyView {
    let Offers {
        systems,
        names,
        cards,
    } = *offers;
    let key = format!("c{}", each.key);
    let system_name = if each.system.is_empty() {
        tr(Msg::EditorPublishedValueSets)
    } else {
        name_of(names, &each.system)
    };
    let source = match each.system_version.as_str() {
        "" => system_name.clone(),
        version => tr_with(
            Msg::CodeSystemOnVersion,
            &[
                ("system", system_name.clone().into()),
                ("version", version.to_owned().into()),
            ],
        ),
    };
    let fields = clause_fields(&key, each, systems);
    let filters = filter_rows(&key, each, cards);
    let codes = code_rows(&key, each);
    let sets = set_rows(&key, each);
    let body = view! {
        {fields}
        {sets}
        <p class="fui-label">{tr(Msg::EditorFilters)}</p>
        {filters}
        <div class="fui-add-row">
            {add_button(format!("add-filter:{key}"), tr(Msg::EditorAddFilter))}
        </div>
        <p class="fui-label">{tr(Msg::EditorCodes)}</p>
        {codes}
        <div class="fui-add-row">
            {add_button(format!("add-code:{key}"), tr(Msg::EditorAddCode))}
        </div>
    }
    .into_any();
    let issues = each
        .defect()
        .map(|defect| ClauseIssue {
            key: defect.constraint().to_owned(),
            text: tr(defect.sentence()),
        })
        .into_iter()
        .collect();
    clause(Clause {
        kind: if each.included {
            ClauseKind::Include
        } else {
            ClauseKind::Exclude
        },
        title: source.clone().into_any(),
        source,
        sentence: each.rule().into_any(),
        body: Some(body),
        actions: Some(icon_submit(
            format!("remove-clause:{key}"),
            Glyph::TRASH,
            tr(Msg::EditorRemoveClause),
        )),
        issues,
    })
}

/// The filter rows of the clause `each`, keyed by `key` in the form: the
/// property a select of the filters its code system's served version
/// declares, the operator one of the operators that filter declares
/// (`TerminologyCapabilities.codeSystem.version.filter`,
/// <https://hl7.org/fhir/R4B/terminologycapabilities.html>); a system that
/// declares none takes any property and the operators FHIR defines.
fn filter_rows(key: &str, each: &crate::fhir::compose::Clause, cards: &[SystemCard]) -> AnyView {
    let declared: Vec<FilterRow> = cards
        .iter()
        .find(|card| card.url == each.system)
        .and_then(|card| chosen_version(card, Some(each.system_version.as_str())))
        .map(|row| row.filters)
        .unwrap_or_default();
    each.filters
        .iter()
        .map(|row| filter_row(key, row, &declared))
        .collect_view()
        .into_any()
}

/// One filter row of a clause, keyed by `key`, offering the `declared`
/// filters.
fn filter_row(
    key: &str,
    row: &crate::fhir::compose::ClauseFilter,
    declared: &[FilterRow],
) -> AnyView {
    let row_key = format!("f{}", row.key);
    let property_id = format!("vs-{row_key}-property");
    let op_id = format!("vs-{row_key}-op");
    let value_id = format!("vs-{row_key}-value");
    let operators: Vec<String> = declared
        .iter()
        .find(|filter| filter.code == row.property)
        .map_or_else(
            || OPERATORS.iter().map(|op| (*op).to_owned()).collect(),
            |filter| filter.operators.clone(),
        );
    let operators = operators
        .iter()
        .map(|op| {
            view! {
                <option value=op.clone() selected=*op == row.op>
                    {op.clone()}
                </option>
            }
        })
        .collect_view();
    let property = if declared.is_empty() {
        view! {
            <input
                id=property_id.clone()
                name=name(FILTER, &row_key, "property")
                class=format!("{INPUT} fui-mono")
                value=row.property.clone()
            />
        }
        .into_any()
    } else {
        let kept = (!declared.iter().any(|filter| filter.code == row.property)).then(|| {
            view! {
                <option value=row.property.clone() selected=true>
                    {row.property.clone()}
                </option>
            }
        });
        let offered = declared
            .iter()
            .map(|filter| {
                view! {
                    <option value=filter.code.clone() selected=filter.code == row.property>
                        {filter.code.clone()}
                    </option>
                }
            })
            .collect_view();
        view! {
            <select id=property_id.clone() name=name(FILTER, &row_key, "property") class=SELECT>
                {kept}
                {offered}
            </select>
        }
        .into_any()
    };
    view! {
        <div class="fui-clause-filter">
            <input type="hidden" name=name(FILTER, &row_key, "clause") value=key.to_owned() />
            {Field::new(property_id, tr(Msg::EditorFilterProperty)).view(property)}
            {Field::new(op_id.clone(), tr(Msg::EditorFilterOperator))
                .view(
                    view! {
                        <select id=op_id name=name(FILTER, &row_key, "op") class=SELECT>
                            {operators}
                        </select>
                    },
                )}
            {Field::new(value_id.clone(), tr(Msg::LookupValue))
                .view(
                    view! {
                        <input
                            id=value_id
                            name=name(FILTER, &row_key, "value")
                            class=format!("{INPUT} fui-mono")
                            value=row.value.clone()
                        />
                    },
                )}
            {icon_submit(format!("remove-filter:{row_key}"), Glyph::X, tr(Msg::EditorRemoveFilter))}
        </div>
    }
    .into_any()
}

/// The codes rows of the clause `each`, keyed by `key` in the form.
fn code_rows(key: &str, each: &crate::fhir::compose::Clause) -> AnyView {
    each.concepts
        .iter()
        .map(|row| {
            let row_key = format!("p{}", row.key);
            let code_id = format!("vs-{row_key}-code");
            view! {
                <div class="fui-clause-filter">
                    <input type="hidden" name=name(CODE, &row_key, "clause") value=key.to_owned() />
                    <input
                        type="hidden"
                        name=name(CODE, &row_key, "served")
                        value=row.served_display.clone()
                    />
                    {Field::new(code_id.clone(), tr(Msg::LookupCode))
                        .view(
                            view! {
                                <input
                                    id=code_id
                                    name=name(CODE, &row_key, "code")
                                    class=format!("{INPUT} fui-mono")
                                    value=row.code.clone()
                                />
                            },
                        )}
                    {Field::new(format!("vs-{row_key}-display"), tr(Msg::EditorConceptDisplay))
                        .view(
                            view! {
                                <input
                                    id=format!("vs-{row_key}-display")
                                    name=name(CODE, &row_key, "display")
                                    class=INPUT
                                    value=row.display.clone()
                                    placeholder=row.served_display.clone()
                                />
                            },
                        )}
                    {icon_submit(
                        format!("remove-code:{row_key}"),
                        Glyph::X,
                        tr(Msg::EditorRemoveCode),
                    )}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

/// The sets rows of the clause `each`, keyed by `key` in the form.
fn set_rows(key: &str, each: &crate::fhir::compose::Clause) -> AnyView {
    each.value_sets
        .iter()
        .map(|row| {
            let row_key = format!("v{}", row.key);
            let url_id = format!("vs-{row_key}-url");
            view! {
                <div class="fui-repeater-row">
                    <input
                        type="hidden"
                        name=name(VALUE_SET_ROW, &row_key, "clause")
                        value=key.to_owned()
                    />
                    {Field::new(url_id.clone(), tr(Msg::EditorValueSetCanonical))
                        .view(
                            view! {
                                <input
                                    id=url_id
                                    name=name(VALUE_SET_ROW, &row_key, "url")
                                    class=format!("{INPUT} fui-mono")
                                    value=row.canonical.clone()
                                />
                            },
                        )}
                    {icon_submit(
                        format!("remove-vs:{row_key}"),
                        Glyph::X,
                        tr(Msg::EditorRemoveValueSet),
                    )}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

/// The preview card: Run the preview, and the first page of what the draft
/// selects once it ran.
fn preview_card(
    preview: Option<
        Result<crate::fhir::expansion::ExpandedValueSet, crate::fhir::error::FhirError>,
    >,
    names: &BTreeMap<String, String>,
) -> AnyView {
    let run = view! {
        <button
            type="submit"
            class=ferrohealth_ui::view::button::Variant::Secondary.small()
            name=crate::posted::INTENT
            value=PREVIEW
        >
            {ferrohealth_ui::icon::icon(Glyph::PLAY, "14")}
            {tr(Msg::EditorRunPreview)}
        </button>
    }
    .into_any();
    let body = match preview {
        None => transform_quiet(tr(Msg::EditorPreviewBefore)),
        Some(Err(error)) => failure(&error),
        Some(Ok(expanded)) => match expanded.expansion() {
            None => transform_quiet(tr(Msg::ValueSetNoExpansion)),
            Some(expansion) => {
                let total = expansion.total.map_or_else(
                    || tr_with(Msg::ValueSetShown, &[("shown", expansion.listed().into())]),
                    |total| {
                        tr_with(
                            Msg::ValueSetShownOf,
                            &[
                                ("shown", expansion.listed().into()),
                                ("total", total.into()),
                            ],
                        )
                    },
                );
                let rows = expansion
                    .concepts
                    .iter()
                    .map(|concept| {
                        let drawn = Coding::new(
                            concept.code.clone(),
                            concept.display.clone().unwrap_or_default(),
                        )
                        .system(name_of(names, &concept.system));
                        view! { <li>{coding(drawn, Layout::Stacked)}</li> }
                    })
                    .collect_view();
                view! {
                    {section_intro(total)}
                    <ul class="fui-match-list">{rows}</ul>
                }
                .into_any()
            }
        },
    };
    panel_sections_card(
        tr(Msg::EditorPreview),
        panel_section(
            Some(SectionHead {
                title: tr(Msg::EditorPreview),
                count: None,
                actions: Some(run),
            }),
            body,
        ),
    )
}

/// The page after a save: the version the server now holds, and the way to
/// the value set.
fn saved_page(request: &Request, opened: &Draft, saved: &str) -> AnyView {
    let href = resource_link(VALUE_SETS_PATH, &opened.id, request.version);
    let open = card_action(href.clone(), tr(Msg::EditorOpenValueSet));
    let shown = if opened.title.is_empty() {
        opened.url.clone()
    } else {
        opened.title.clone()
    };
    view! {
        {heading(
            request,
            Kind::ValueSet,
            Some((shown, href)),
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

#[cfg(test)]
mod tests {
    use super::{apply, target};
    use crate::fhir::compose::Draft;
    use crate::posted::Posted;

    #[test]
    fn an_intent_names_the_row_it_acts_on() {
        assert_eq!(Some("c3"), target("add-filter:c3", "add-filter"));
        assert_eq!(None, target("add-filter:c3", "add-code"));
        assert_eq!(None, target("save", "add-filter"));
    }

    #[test]
    fn a_post_rebuilds_the_clauses_and_adds_the_filter_asked_for() {
        let posted = Posted::parse(
            b"url=http%3A%2F%2Fx%2Fvs&status=active&clause.c0.side=include&clause.c0.system=http%3A%2F%2Fx%2Fcs\
              &code.p1.clause=c0&code.p1.code=A&clause.c2.side=exclude&clause.c2.system=http%3A%2F%2Fx%2Fcs\
              &intent=add-filter%3Ac2",
        );
        let mut draft = Draft::new();
        apply(&posted, &mut draft);
        assert_eq!(2, draft.clauses.len());
        assert_eq!("A", draft.clauses[0].concepts[0].code);
        assert!(!draft.clauses[1].included);
        assert_eq!(1, draft.clauses[1].filters.len());
        assert_eq!("http://x/vs", draft.url);
    }
}
