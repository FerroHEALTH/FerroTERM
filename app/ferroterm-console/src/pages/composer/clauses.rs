// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composer's includes and excludes: each clause a code system and its
//! version, its value sets, its filters and its codes, as editable rows
//! (<https://hl7.org/fhir/R4B/valueset.html#compositions>).

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::clause::{Clause, ClauseIssue, ClauseKind, clause, clauses};
use ferrohealth_ui::view::form::{Field, INPUT, SELECT, field_flow};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, section_intro};
use ferrohealth_ui::view::template::list::HeadingCount;

use super::{CLAUSE, CODE, FILTER, OPERATORS, VALUE_SET_ROW};
use crate::fhir::compose::Draft;
use crate::fhir::concept::chosen_version;
use crate::fhir::terminology::{FilterRow, SystemCard};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::editor::{add_button, icon_submit, name};
use crate::pages::operations::{hidden_field, option};
use crate::pages::parts::name_of;

/// The includes and excludes, each an editable clause, and the ways to add
/// one.
pub(super) fn compose(locale: &Locale, draft: &Draft, offers: &Offers<'_>) -> Html {
    let drawn = Html::join(
        draft
            .clauses
            .iter()
            .map(|each| clause_view(locale, each, offers)),
    );
    let count = u64::try_from(draft.clauses.len()).unwrap_or(u64::MAX);
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::EditorIncludesExcludes),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        Html::join([
            section_intro(tr(locale, Msg::EditorIncludesExcludesIntro)),
            clauses(drawn),
            Html::raw(format!(
                r#"<div class="fui-add-row">{}{}{}</div>"#,
                add_button(
                    String::from("add-include"),
                    tr(locale, Msg::EditorAddInclude)
                ),
                add_button(
                    String::from("add-exclude"),
                    tr(locale, Msg::EditorAddExclude)
                ),
                add_button(
                    String::from("add-value-set"),
                    tr(locale, Msg::EditorAddValueSet)
                ),
            )),
        ]),
    )
}

/// What the composer offers a clause: the published code systems, their
/// names, and what each declares it serves.
#[derive(Clone, Copy, Debug)]
pub(super) struct Offers<'a> {
    /// The published code systems, each its canonical and its name.
    pub(super) systems: &'a [(String, String)],
    /// The name each code system goes by, by canonical.
    pub(super) names: &'a BTreeMap<String, String>,
    /// What the server declares about each code system it serves.
    pub(super) cards: &'a [SystemCard],
}

/// A clause's side, its code system and its version, as fields.
fn clause_fields(
    locale: &Locale,
    key: &str,
    each: &crate::fhir::compose::Clause,
    systems: &[(String, String)],
) -> Html {
    let system_options = Html::join([
        option("", &tr(locale, Msg::EditorNoSystem), each.system.is_empty()),
        Html::join(
            systems
                .iter()
                .map(|(value, words)| option(value, words, *value == each.system)),
        ),
    ]);
    let system_id = format!("vs-{key}-system");
    let version_id = format!("vs-{key}-version");
    Html::join([
        hidden_field(
            &name(CLAUSE, key, "side"),
            if each.included { "include" } else { "exclude" },
        ),
        field_flow(Html::join([
            Field::new(system_id.clone(), tr(locale, Msg::LookupSystem)).view(Html::raw(format!(
                r#"<select id="{system_id}" name="{}" class="{SELECT}">{system_options}</select>"#,
                Html::text(name(CLAUSE, key, "system"))
            ))),
            Field::new(version_id.clone(), tr(locale, Msg::LookupVersion)).view(Html::raw(format!(
                r#"<input id="{version_id}" name="{}" class="{INPUT}" value="{}" placeholder="{}">"#,
                Html::text(name(CLAUSE, key, "version")),
                Html::text(&each.system_version),
                Html::text(tr(locale, Msg::LookupServerVersion))
            ))),
        ])),
    ])
}

/// A label paragraph of a clause's part.
fn part_label(words: String) -> Html {
    Html::raw(format!(r#"<p class="fui-label">{}</p>"#, Html::text(words)))
}

/// One Add row of a clause, asking for `intent`.
fn add_row(intent: String, words: String) -> Html {
    Html::raw(format!(
        r#"<div class="fui-add-row">{}</div>"#,
        add_button(intent, words)
    ))
}

/// One clause, editable: its code system and version, its value sets, its
/// filters and its codes, with the ways to add and remove each.
fn clause_view(locale: &Locale, each: &crate::fhir::compose::Clause, offers: &Offers<'_>) -> Html {
    let Offers {
        systems,
        names,
        cards,
    } = *offers;
    let key = format!("c{}", each.key);
    let system_name = if each.system.is_empty() {
        tr(locale, Msg::EditorPublishedValueSets)
    } else {
        name_of(names, &each.system)
    };
    let source = match each.system_version.as_str() {
        "" => system_name.clone(),
        version => tr_with(
            locale,
            Msg::CodeSystemOnVersion,
            &[
                ("system", system_name.clone().into()),
                ("version", version.to_owned().into()),
            ],
        ),
    };
    let body = Html::join([
        clause_fields(locale, &key, each, systems),
        set_rows(locale, &key, each),
        part_label(tr(locale, Msg::EditorFilters)),
        filter_rows(locale, &key, each, cards),
        add_row(
            format!("add-filter:{key}"),
            tr(locale, Msg::EditorAddFilter),
        ),
        part_label(tr(locale, Msg::EditorCodes)),
        code_rows(locale, &key, each),
        add_row(format!("add-code:{key}"), tr(locale, Msg::EditorAddCode)),
    ]);
    let issues = each
        .defect()
        .map(|defect| ClauseIssue {
            key: defect.constraint().to_owned(),
            text: tr(locale, defect.sentence()),
        })
        .into_iter()
        .collect();
    clause(
        locale,
        Clause {
            kind: if each.included {
                ClauseKind::Include
            } else {
                ClauseKind::Exclude
            },
            title: Html::text(&source),
            source,
            sentence: Html::text(each.rule(locale)),
            body: Some(body),
            actions: Some(icon_submit(
                format!("remove-clause:{key}"),
                Glyph::TRASH,
                tr(locale, Msg::EditorRemoveClause),
            )),
            issues,
        },
    )
}

/// The filter rows of the clause `each`, keyed by `key` in the form: the
/// property a select of the filters its code system's served version
/// declares, the operator one of the operators that filter declares
/// (`TerminologyCapabilities.codeSystem.version.filter`,
/// <https://hl7.org/fhir/R4B/terminologycapabilities.html>); a system that
/// declares none takes any property and the operators FHIR defines.
fn filter_rows(
    locale: &Locale,
    key: &str,
    each: &crate::fhir::compose::Clause,
    cards: &[SystemCard],
) -> Html {
    let declared: Vec<FilterRow> = cards
        .iter()
        .find(|card| card.url == each.system)
        .and_then(|card| chosen_version(card, Some(each.system_version.as_str())))
        .map(|row| row.filters)
        .unwrap_or_default();
    Html::join(
        each.filters
            .iter()
            .map(|row| filter_row(locale, key, row, &declared)),
    )
}

/// One filter row of a clause, keyed by `key`, offering the `declared`
/// filters.
fn filter_row(
    locale: &Locale,
    key: &str,
    row: &crate::fhir::compose::ClauseFilter,
    declared: &[FilterRow],
) -> Html {
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
    let operators = Html::join(operators.iter().map(|op| option(op, op, *op == row.op)));
    let property = if declared.is_empty() {
        Html::raw(format!(
            r#"<input id="{property_id}" name="{}" class="{INPUT} fui-mono" value="{}">"#,
            Html::text(name(FILTER, &row_key, "property")),
            Html::text(&row.property)
        ))
    } else {
        let kept = (!declared.iter().any(|filter| filter.code == row.property))
            .then(|| option(&row.property, &row.property, true));
        let offered = Html::join(
            declared
                .iter()
                .map(|filter| option(&filter.code, &filter.code, filter.code == row.property)),
        );
        Html::raw(format!(
            r#"<select id="{property_id}" name="{}" class="{SELECT}">{}{offered}</select>"#,
            Html::text(name(FILTER, &row_key, "property")),
            kept.unwrap_or_default()
        ))
    };
    Html::raw(format!(
        r#"<div class="fui-clause-filter">{}{}{}{}{}</div>"#,
        hidden_field(&name(FILTER, &row_key, "clause"), key),
        Field::new(property_id, tr(locale, Msg::EditorFilterProperty)).view(property),
        Field::new(op_id.clone(), tr(locale, Msg::EditorFilterOperator)).view(Html::raw(format!(
            r#"<select id="{op_id}" name="{}" class="{SELECT}">{operators}</select>"#,
            Html::text(name(FILTER, &row_key, "op"))
        ))),
        Field::new(value_id.clone(), tr(locale, Msg::LookupValue)).view(Html::raw(format!(
            r#"<input id="{value_id}" name="{}" class="{INPUT} fui-mono" value="{}">"#,
            Html::text(name(FILTER, &row_key, "value")),
            Html::text(&row.value)
        ))),
        icon_submit(
            format!("remove-filter:{row_key}"),
            Glyph::X,
            tr(locale, Msg::EditorRemoveFilter)
        ),
    ))
}

/// The codes rows of the clause `each`, keyed by `key` in the form.
fn code_rows(locale: &Locale, key: &str, each: &crate::fhir::compose::Clause) -> Html {
    Html::join(each.concepts.iter().map(|row| {
        let row_key = format!("p{}", row.key);
        let code_id = format!("vs-{row_key}-code");
        let display_id = format!("vs-{row_key}-display");
        Html::raw(format!(
            r#"<div class="fui-clause-filter">{}{}{}{}{}</div>"#,
            hidden_field(&name(CODE, &row_key, "clause"), key),
            hidden_field(&name(CODE, &row_key, "served"), &row.served_display),
            Field::new(code_id.clone(), tr(locale, Msg::LookupCode)).view(Html::raw(format!(
                r#"<input id="{code_id}" name="{}" class="{INPUT} fui-mono" value="{}">"#,
                Html::text(name(CODE, &row_key, "code")),
                Html::text(&row.code)
            ))),
            Field::new(display_id.clone(), tr(locale, Msg::EditorConceptDisplay)).view(Html::raw(
                format!(
                    r#"<input id="{display_id}" name="{}" class="{INPUT}" value="{}" placeholder="{}">"#,
                    Html::text(name(CODE, &row_key, "display")),
                    Html::text(&row.display),
                    Html::text(&row.served_display)
                )
            )),
            icon_submit(
                format!("remove-code:{row_key}"),
                Glyph::X,
                tr(locale, Msg::EditorRemoveCode),
            ),
        ))
    }))
}

/// The sets rows of the clause `each`, keyed by `key` in the form.
fn set_rows(locale: &Locale, key: &str, each: &crate::fhir::compose::Clause) -> Html {
    Html::join(each.value_sets.iter().map(|row| {
        let row_key = format!("v{}", row.key);
        let url_id = format!("vs-{row_key}-url");
        Html::raw(format!(
            r#"<div class="fui-repeater-row">{}{}{}</div>"#,
            hidden_field(&name(VALUE_SET_ROW, &row_key, "clause"), key),
            Field::new(url_id.clone(), tr(locale, Msg::EditorValueSetCanonical)).view(Html::raw(
                format!(
                    r#"<input id="{url_id}" name="{}" class="{INPUT} fui-mono" value="{}">"#,
                    Html::text(name(VALUE_SET_ROW, &row_key, "url")),
                    Html::text(&row.canonical)
                )
            )),
            icon_submit(
                format!("remove-vs:{row_key}"),
                Glyph::X,
                tr(locale, Msg::EditorRemoveValueSet),
            ),
        ))
    }))
}
