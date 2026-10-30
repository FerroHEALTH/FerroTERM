// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Value sets (T1): every value set the root holds, with where it comes
//! from, its version, its status and what it selects in words.

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Role, cell, data_table, title_cell};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::coding::canonical;

use crate::auth::scopes::Letter;
use crate::fhir::VALUE_SET;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::value_set::{ClauseRow, PublishedValueSet};
use crate::locale::{Msg, message, tr, tr_with};
use crate::markup::{h, join, t};
use crate::pages::listing::{Frame, ListState, NAME_COLUMN, counted, status_state};
use crate::pages::parts::{Source, failure, heading, name_of, system_names};
use crate::request::Request;
use crate::routes::{COMPOSE_PATH, VALUE_SETS_PATH, resource_link, ui_link, ui_path};

/// What one clause selects, in words: its filters, its count of codes, the
/// value sets it draws in, or the whole code system.
fn clause_words(locale: &Locale, clause: &ClauseRow, names: &BTreeMap<String, String>) -> String {
    let system = clause
        .system
        .as_deref()
        .map(|system| name_of(names, system));
    if !clause.filters.is_empty() {
        clause.filters.join(", ")
    } else if clause.codes > 0 {
        match system {
            Some(system) => tr_with(
                locale,
                Msg::SelectsCodesOf,
                &[("count", clause.codes.into()), ("system", system.into())],
            ),
            None => tr_with(locale, Msg::SelectsCodes, &[("count", clause.codes.into())]),
        }
    } else if !clause.value_sets.is_empty() {
        tr_with(
            locale,
            Msg::SelectsValueSets,
            &[("count", clause.value_sets.len().into())],
        )
    } else {
        tr(locale, Msg::SelectsWholeSystem)
    }
}

/// What a value set selects, in words: its includes, then less its
/// excludes.
#[must_use]
pub(crate) fn selects(
    locale: &Locale,
    clauses: &[ClauseRow],
    names: &BTreeMap<String, String>,
) -> String {
    let included: Vec<String> = clauses
        .iter()
        .filter(|clause| clause.included)
        .map(|clause| clause_words(locale, clause, names))
        .collect();
    let excluded: Vec<String> = clauses
        .iter()
        .filter(|clause| !clause.included)
        .map(|clause| clause_words(locale, clause, names))
        .collect();
    if included.is_empty() {
        return tr(locale, Msg::SelectsNothingStated);
    }
    let mut words = included.join("; ");
    if !excluded.is_empty() {
        words.push_str(&tr_with(
            locale,
            Msg::SelectsLess,
            &[("excluded", excluded.join("; ").into())],
        ));
    }
    words
}

/// Value sets.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let deleted = request.param(DELETED_PARAM).filter(|id| !id.is_empty());
    let found = request
        .client
        .value_set_search(request.version, &SearchFilter::default())
        .await;
    let names = system_names(&request.client, request.version).await;
    let said = deleted.map(|id| {
        ferrohealth_ui::view::notice::notice(
            ferrohealth_ui::tone::Tone::Ok,
            ferrohealth_ui::view::notice::Announce::Status,
            None,
            t(tr_with(locale, Msg::ValueSetsDeleted, &[("id", id.into())])),
        )
    });
    match found {
        Ok(found) => Html::join([
            said.unwrap_or_default(),
            listing(
                &request,
                found.found().into_iter().cloned().collect(),
                &names,
            ),
        ]),
        Err(error) => Html::join([
            heading(
                &request,
                tr(locale, Msg::ShellValueSets),
                None,
                Vec::new(),
                ActionGroup::none(),
            ),
            failure(locale, &error),
        ]),
    }
}

/// The query parameter naming the value set a delete just removed.
pub(crate) const DELETED_PARAM: &str = "deleted";

/// The name a reader reads a value set by.
fn label_of(value_set: &PublishedValueSet) -> String {
    value_set
        .label()
        .or_else(|| value_set.url())
        .unwrap_or_default()
        .to_owned()
}

/// Where `value_set` comes from.
fn source_of(value_set: &PublishedValueSet) -> Source {
    Source::of_stored(value_set.stored())
}

/// The listing of `all` the root holds, as the address filters it.
fn listing(
    request: &Request,
    all: Vec<PublishedValueSet>,
    names: &BTreeMap<String, String>,
) -> Html {
    let locale = &request.locale;
    let state = ListState::read(request);
    let path = ui_path(VALUE_SETS_PATH);
    let counts = counted(all.iter().map(source_of));
    let total = all.len();
    let kept = state.kept(
        all,
        label_of,
        |value_set| value_set.url().unwrap_or_default().to_owned(),
        source_of,
    );
    let columns = [
        Column::text(NAME_COLUMN, tr(locale, Msg::ValueSetsValueSet))
            .role(Role::Title)
            .sortable(),
        Column::text("source", tr(locale, Msg::CodeSystemsSource)).role(Role::Meta),
        Column::text("version", tr(locale, Msg::ValueSetsVersion)).role(Role::Meta),
        Column::text("status", tr(locale, Msg::ValueSetsStatus)).role(Role::Meta),
        Column::text("selects", tr(locale, Msg::ValueSetsSelects)).role(Role::Quiet),
    ];
    let title = tr(locale, Msg::ShellValueSets);
    let body = if kept.is_empty() && total == 0 {
        let line = tr(locale, Msg::ValueSetsNone);
        let first = request.can(VALUE_SET, Letter::Create).then(|| {
            (
                tr(locale, Msg::ValueSetsFirst),
                ui_link(COMPOSE_PATH, request.version),
            )
        });
        ferrohealth_ui::view::template::list::list_empty(line, first)
    } else if kept.is_empty() {
        state.emptied(
            request,
            &path,
            tr_with(locale, Msg::ValueSetsNoMatch, &[("count", total.into())]),
        )
    } else {
        let drawn = join(
            state
                .window(kept.len())
                .slice(&kept)
                .iter()
                .map(|value_set| row(request, &columns, value_set, names)),
        );
        let fields = state.fields(request, &[]);
        let sort = state.sort();
        data_table(DataTable {
            caption: &title,
            columns: &columns,
            sorting: ListState::sorting(&sort, &fields),
            body: Body::Rows(drawn),
        })
    };
    let actions = create_action(request);
    state.frame(
        request,
        Frame {
            title,
            subtitle: tr_with(
                locale,
                Msg::ValueSetsSubtitle,
                &[("version", request.version.label().into())],
            ),
            actions,
            path,
            total,
            kept: kept.len(),
            counts,
            find: tr(locale, Msg::ValueSetsFind),
            body,
            pages: (
                tr(locale, Msg::ValueSetsPages),
                Noun::new(message(Msg::NounValueSets)),
            ),
        },
    )
}

/// One value set's row of the listing, its cells under `columns`.
fn row(
    request: &Request,
    columns: &[Column; 5],
    value_set: &PublishedValueSet,
    names: &BTreeMap<String, String>,
) -> Html {
    let locale = &request.locale;
    let [_, source, version, status, selection] = columns;
    let href = resource_link(
        VALUE_SETS_PATH,
        value_set.id().unwrap_or_default(),
        request.version,
    );
    let url = value_set.url().unwrap_or_default();
    let version_words = value_set.version().map(|code| {
        tr_with(
            locale,
            Msg::VersionsOne,
            &[("version", code.to_owned().into())],
        )
    });
    h(format!(
        "<tr>{}{}{}{}{}</tr>",
        title_cell(Some(&href), t(label_of(value_set)), Some(canonical(url))),
        cell(source, source_of(value_set).tag(locale)),
        cell(version, t(version_words.unwrap_or_default())),
        cell(status, status_state(locale, value_set.status())),
        cell(selection, t(selects(locale, &value_set.clauses(), names))),
    ))
}

/// The title row's action: a new resource, for a reader who may create one.
fn create_action(request: &Request) -> ActionGroup {
    let locale = &request.locale;
    if request.can(VALUE_SET, Letter::Create) {
        ActionGroup::primary(Action::new(
            Some(Glyph::FILE_PLUS),
            tr(locale, Msg::ValueSetsNew),
            Act::Link(ui_link(COMPOSE_PATH, request.version)),
        ))
    } else {
        ActionGroup::none()
    }
}
