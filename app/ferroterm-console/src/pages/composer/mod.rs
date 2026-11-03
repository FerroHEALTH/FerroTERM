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

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::form::{FORM, Field, SELECT, field_flow};
use ferrohealth_ui::view::kbd::{aria_keys, kbd};
use ferrohealth_ui::view::layout::main_aside;
use ferrohealth_ui::view::layout::{Gap, stack_at};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, panel_section, panel_sections_card, section_intro,
};
use ferrohealth_ui::view::transform::transform_quiet;

use crate::auth::scopes::Letter;
use crate::fhir::compose::{Draft, Preview, STATUSES};
use crate::fhir::{CODE_SYSTEM, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::detail::Kind;
use crate::pages::editor::{
    DISCARD, OPENED, OPENED_AT, SAVE, SAVED_PARAM, Standing, dock, field, form, heading,
    opened_line, percent, refused, rows, text_field,
};
use crate::pages::operations::{choices, option};
use crate::pages::parts::{failure, name_of};
use crate::pages::shortcuts::RUN_KEYS;
use crate::posted::Posted;
use crate::request::Request;
use crate::routes::{
    COMPOSE_ROUTE, COMPOSER_ID_PARAM, VALUE_SETS_PATH, resource_link, sign_in_link, ui_link,
};

mod clauses;

use clauses::{Offers, compose};

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
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let posted = request.posted.clone();
    let letter = if request.text(COMPOSER_ID_PARAM).is_empty() {
        Letter::Create
    } else {
        Letter::Update
    };
    if !request.can(VALUE_SET, letter) {
        if request.session.who().is_none() && request.sign_in {
            request.answering.redirect(&sign_in_link(&request.back()));
        }
        return crate::pages::errors::forbidden_page(
            &request,
            Msg::EditorForbiddenValueSet,
            "user/ValueSet.cu",
            (
                tr(locale, Msg::ShellValueSets),
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
async fn edited(request: &Request, posted: Option<&Posted>) -> Html {
    let locale = &request.locale;
    let id = request.text(COMPOSER_ID_PARAM);
    let here = address(request, &id);
    if posted.is_some_and(|form| form.intent() == DISCARD) {
        request.answering.redirect(&here);
        return Html::empty();
    }
    let opened = match open(request, &id).await {
        Ok(opened) => opened,
        Err(page) => return page,
    };
    if let Some(saved) = request.param(SAVED_PARAM) {
        return saved_page(request, &opened, &saved);
    }
    let (systems, names) = systems_of(request).await;
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
                Ok(()) => return Html::empty(),
                Err(refusal) => answer = Some(*refusal),
            }
        }
    }
    let standing = match &answer {
        Some((_, standing)) => standing.clone(),
        None => standing(locale, &opened, &draft),
    };
    let bar = dock(
        locale,
        tr(
            locale,
            if draft.id.is_empty() {
                Msg::EditorCreateValueSet
            } else {
                Msg::EditorSaveValueSet
            },
        ),
        &standing,
    );
    let resource = crumb(request, &opened);
    let title = tr(
        locale,
        if opened.id.is_empty() {
            Msg::EditorNewValueSet
        } else {
            Msg::EditorEdit
        },
    );
    let offers = Offers {
        systems: &systems,
        names: &names,
        cards: &cards,
    };
    let editor = panel_sections_card(
        title.clone(),
        Html::join([metadata(locale, &draft), compose(locale, &draft, &offers)]),
    );
    let body = stack_at(
        Gap::Loose,
        Html::join([
            answer.map(|(view, _)| view).unwrap_or_default(),
            main_aside(editor, preview_card(locale, preview, &names)),
        ]),
    );
    Html::join([
        heading(
            request,
            Kind::ValueSet,
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

/// The code systems the root publishes, each its canonical and its name,
/// and the name each goes by.
pub(crate) async fn systems_of(
    request: &Request,
) -> (Vec<(String, String)>, BTreeMap<String, String>) {
    let published = request
        .client
        .published_names(request.version, CODE_SYSTEM)
        .await
        .ok();
    let names = published
        .as_ref()
        .map(crate::fhir::named::NamedSearch::names)
        .unwrap_or_default();
    (choices(published.as_ref()), names)
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
) -> Result<(), Box<(Html, Standing)>> {
    let body = draft.resource().to_string();
    match crate::pages::editor::send(request, VALUE_SET, &draft.id, &body, opened).await {
        Ok(written) => {
            let saved_id = written
                .resource
                .as_ref()
                .and_then(|resource| resource.id.clone())
                .unwrap_or_else(|| draft.id.clone());
            let version = written.version_id().unwrap_or_default().to_owned();
            request.answering.redirect(&format!(
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
async fn open(request: &Request, id: &str) -> Result<Draft, Html> {
    if id.is_empty() {
        return Ok(Draft::new());
    }
    match request.client.value_set_stored(request.version, id).await {
        Ok((stored, resource)) => {
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
        Err(error) => Err(failure(&request.locale, &error)),
    }
}

/// Where `draft` stands against the value set `opened` read.
fn standing(locale: &Locale, opened: &Draft, draft: &Draft) -> Standing {
    if !draft.savable() {
        Standing::Incomplete(tr(locale, Msg::EditorValueSetNeeds))
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
fn metadata(locale: &Locale, draft: &Draft) -> Html {
    let statuses = Html::join(STATUSES.iter().map(|status| {
        option(
            status,
            &status_words(locale, status),
            *status == draft.status,
        )
    }));
    let picked = match draft.inactive {
        Some(true) => "true",
        Some(false) => "false",
        None => "",
    };
    let inactive = Html::join(
        [
            ("", Msg::EditorInactiveServer),
            ("true", Msg::EditorInactiveInclude),
            ("false", Msg::EditorInactiveExclude),
        ]
        .into_iter()
        .map(|(value, words)| option(value, &tr(locale, words), value == picked)),
    );
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::EditorMetadata),
            count: None,
            actions: None,
        }),
        Html::raw(format!(
            r#"<div class="{FORM}">{}{}{}</div>"#,
            text_field(
                "vs-url",
                "url",
                tr(locale, Msg::EditorCanonical),
                draft.url.clone(),
                true
            ),
            field_flow(Html::join([
                text_field(
                    "vs-title",
                    "title",
                    tr(locale, Msg::EditorTitle),
                    draft.title.clone(),
                    false,
                ),
                Field::new("vs-inactive", tr(locale, Msg::EditorInactiveCodes)).view(Html::raw(format!(
                    r#"<select id="vs-inactive" name="inactive" class="{SELECT}">{inactive}</select>"#
                ))),
            ])),
            field_flow(Html::join([
                text_field(
                    "vs-name",
                    "name",
                    tr(locale, Msg::EditorName),
                    draft.name.clone(),
                    true,
                ),
                text_field(
                    "vs-version",
                    "version",
                    tr(locale, Msg::EditorBusinessVersion),
                    draft.version.clone(),
                    false,
                ),
                Field::new("vs-status", tr(locale, Msg::EditorStatus)).view(Html::raw(format!(
                    r#"<select id="vs-status" name="status" class="{SELECT}">{statuses}</select>"#
                ))),
            ])),
        )),
    )
}

/// The words of a publication status.
fn status_words(locale: &Locale, status: &str) -> String {
    tr(
        locale,
        match status {
            "active" => Msg::StatusActive,
            "retired" => Msg::StatusRetired,
            "unknown" => Msg::StatusUnknown,
            _ => Msg::StatusDraft,
        },
    )
}

/// The preview card: Run the preview, announcing [`RUN_KEYS`] for FerroUI's
/// `keys` module to press (FerroUI `docs/consoles.md`, "Keyboard
/// shortcuts"), and the first page of what the draft selects once it ran.
fn preview_card(
    locale: &Locale,
    preview: Option<
        Result<crate::fhir::expansion::ExpandedValueSet, crate::fhir::error::FhirError>,
    >,
    names: &BTreeMap<String, String>,
) -> Html {
    let keys = aria_keys(RUN_KEYS)
        .map(|keys| format!(r#" aria-keyshortcuts="{}""#, Html::text(keys)))
        .unwrap_or_default();
    let run = Html::raw(format!(
        r#"<button type="submit" class="{}" name="{}" value="{PREVIEW}"{keys}>{}{}{}</button>"#,
        ferrohealth_ui::view::button::Variant::Secondary.small(),
        crate::posted::INTENT,
        ferrohealth_ui::icon::icon(Glyph::PLAY, "14"),
        Html::text(tr(locale, Msg::EditorRunPreview)),
        kbd(RUN_KEYS),
    ));
    let body = match preview {
        None => transform_quiet(tr(locale, Msg::EditorPreviewBefore)),
        Some(Err(error)) => failure(locale, &error),
        Some(Ok(expanded)) => match expanded.expansion() {
            None => transform_quiet(tr(locale, Msg::ValueSetNoExpansion)),
            Some(expansion) => {
                let total = expansion.total.map_or_else(
                    || {
                        tr_with(
                            locale,
                            Msg::ValueSetShown,
                            &[("shown", expansion.listed().into())],
                        )
                    },
                    |total| {
                        tr_with(
                            locale,
                            Msg::ValueSetShownOf,
                            &[
                                ("shown", expansion.listed().into()),
                                ("total", total.into()),
                            ],
                        )
                    },
                );
                let rows = Html::join(expansion.concepts.iter().map(|concept| {
                    let drawn = Coding::new(
                        concept.code.clone(),
                        concept.display.clone().unwrap_or_default(),
                    )
                    .system(name_of(names, &concept.system));
                    Html::raw(format!(
                        "<li>{}</li>",
                        coding(locale, drawn, Layout::Stacked)
                    ))
                }));
                Html::join([
                    section_intro(total),
                    Html::raw(format!(r#"<ul class="fui-match-list">{rows}</ul>"#)),
                ])
            }
        },
    };
    panel_sections_card(
        tr(locale, Msg::EditorPreview),
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::EditorPreview),
                count: None,
                actions: Some(run),
            }),
            body,
        ),
    )
}

/// The page after a save: the version the server now holds, and the way to
/// the value set.
fn saved_page(request: &Request, opened: &Draft, saved: &str) -> Html {
    let locale = &request.locale;
    let href = resource_link(VALUE_SETS_PATH, &opened.id, request.version);
    let open = card_action(href.clone(), tr(locale, Msg::EditorOpenValueSet));
    let shown = if opened.title.is_empty() {
        opened.url.clone()
    } else {
        opened.title.clone()
    };
    Html::join([
        heading(
            request,
            Kind::ValueSet,
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
            title: Html::text(tr_with(
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
    use std::collections::BTreeMap;

    use ferrohealth_ui::i18n::Locale;

    use super::{apply, preview_card, target};
    use crate::fhir::compose::Draft;
    use crate::posted::Posted;

    #[test]
    fn run_the_preview_announces_ctrl_enter_for_the_keys_module() {
        let html = preview_card(&Locale::default(), None, &BTreeMap::new());
        let html = html.as_str();
        assert!(
            html.contains("value=\"preview\" aria-keyshortcuts=\"Control+Enter\""),
            "{html}"
        );
        assert!(html.contains("<kbd class=\"fui-kbd\">Ctrl</kbd>"), "{html}");
    }

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
