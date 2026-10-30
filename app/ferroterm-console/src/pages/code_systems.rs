// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Code systems (T1): every code system the root serves, one row each, with
//! where it comes from, its versions, what content it holds and whether it
//! has a hierarchy, the find field and the Source filter in the address.
//!
//! Nothing here names a system, assumes a hierarchy or a language: every
//! cell is the capability statement rendered, so a system the server has
//! never served draws with no change to this file.

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
use crate::fhir::CODE_SYSTEM;
use crate::fhir::concept::Hierarchy;
use crate::fhir::named::{Listed, NamedSearch};
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, message, tr, tr_with};
use crate::markup::{h, join, t};
use crate::pages::listing::{Frame, ListState, NAME_COLUMN, counted};
use crate::pages::parts::{Source, failure, heading, versions_word};
use crate::request::Request;
use crate::routes::{CODE_SYSTEM_EDITOR_PATH, SYSTEMS_PATH, system_link, ui_link, ui_path};

/// One served code system as the listing draws it.
#[derive(Clone, Debug)]
pub(crate) struct SystemRow {
    /// What the capability statement declares of it.
    pub(crate) card: SystemCard,
    /// The name a reader reads.
    pub(crate) name: String,
    /// Where it comes from.
    pub(crate) source: Source,
}

/// The rows of `cards`, named from the published `listed`.
#[must_use]
pub(crate) fn rows(cards: Vec<SystemCard>, listed: &[Listed]) -> Vec<SystemRow> {
    let names: BTreeMap<&str, &str> = listed
        .iter()
        .map(|resource| (resource.url.as_str(), resource.label.as_str()))
        .collect();
    cards
        .into_iter()
        .map(|card| SystemRow {
            name: names
                .get(card.url.as_str())
                .map_or_else(|| card.url.clone(), |name| (*name).to_owned()),
            source: Source::of_system(&card, listed),
            card,
        })
        .collect()
}

/// The content a code system holds, in words: `codeSystem.content`, which
/// R5 and R6 declare and R4 and R4B do not.
#[must_use]
pub(crate) fn content_word(locale: &Locale, content: Option<&str>) -> String {
    match content {
        Some("complete") => tr(locale, Msg::ContentComplete),
        Some("not-present") => tr(locale, Msg::ContentNotPresent),
        Some("example") => tr(locale, Msg::ContentExample),
        Some("fragment") => tr(locale, Msg::ContentFragment),
        Some("supplement") => tr(locale, Msg::ContentSupplement),
        Some(other) => other.to_owned(),
        None => tr(locale, Msg::ContentUndeclared),
    }
}

/// Whether a code system has a hierarchy the browser can walk, in words.
#[must_use]
pub(crate) fn hierarchy_word(locale: &Locale, card: &SystemCard) -> String {
    let default = card
        .versions
        .iter()
        .find(|version| version.is_default)
        .or_else(|| card.versions.first());
    match default.map(Hierarchy::of) {
        Some(Hierarchy::Walkable { .. }) => tr(locale, Msg::HierarchyTree),
        Some(Hierarchy::WithoutChildren { .. }) => tr(locale, Msg::HierarchyNoTree),
        Some(Hierarchy::Absent) | None => tr(locale, Msg::HierarchyFlat),
    }
}

/// Code systems.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let capabilities = request
        .client
        .terminology_capabilities(request.version)
        .await;
    let listed = request
        .client
        .published_names(request.version, CODE_SYSTEM)
        .await
        .map(|named| NamedSearch::listed(&named))
        .unwrap_or_default();
    match capabilities {
        Ok(capabilities) => listing(&request, rows(capabilities.cards(), &listed)),
        Err(error) => Html::join([
            heading(
                &request,
                tr(locale, Msg::ShellCodeSystems),
                None,
                Vec::new(),
                ActionGroup::none(),
            ),
            failure(locale, &error),
        ]),
    }
}

/// The listing of `all` the root serves, as the address filters it.
fn listing(request: &Request, all: Vec<SystemRow>) -> Html {
    let locale = &request.locale;
    let state = ListState::read(request);
    let path = ui_path(SYSTEMS_PATH);
    let counts = counted(all.iter().map(|row| row.source));
    let total = all.len();
    let kept = state.kept(
        all,
        |row| row.name.clone(),
        |row| row.card.url.clone(),
        |row| row.source,
    );
    let columns = [
        Column::text(NAME_COLUMN, tr(locale, Msg::CodeSystemsSystem))
            .role(Role::Title)
            .sortable(),
        Column::text("source", tr(locale, Msg::CodeSystemsSource)).role(Role::Meta),
        Column::text("versions", tr(locale, Msg::CodeSystemsVersions)).role(Role::Meta),
        Column::text("content", tr(locale, Msg::CodeSystemsContent)).role(Role::Quiet),
        Column::text("hierarchy", tr(locale, Msg::CodeSystemsHierarchy)).role(Role::Quiet),
    ];
    let [_, source, versions, content, hierarchy] = &columns;
    let title = tr(locale, Msg::ShellCodeSystems);
    let body = if kept.is_empty() {
        state.emptied(
            request,
            &path,
            tr_with(locale, Msg::CodeSystemsNoMatch, &[("count", total.into())]),
        )
    } else {
        let drawn = join(state.window(kept.len()).slice(&kept).iter().map(|row| {
            let href = system_link(&row.card.url, request.version);
            let codes: Vec<String> = row
                .card
                .versions
                .iter()
                .filter_map(|version| version.code.clone())
                .collect();
            h(format!(
                "<tr>{}{}{}{}{}</tr>",
                title_cell(Some(&href), t(&row.name), Some(canonical(&row.card.url))),
                cell(source, row.source.tag(locale)),
                cell(versions, t(versions_word(locale, &codes))),
                cell(
                    content,
                    t(content_word(locale, row.card.content.as_deref()))
                ),
                cell(hierarchy, t(hierarchy_word(locale, &row.card))),
            ))
        }));
        let fields = state.fields(request, &[]);
        let sort = state.sort();
        data_table(DataTable {
            caption: &title,
            columns: &columns,
            sorting: ListState::sorting(&sort, &fields),
            body: Body::Rows(drawn),
        })
    };
    let actions = if request.can(CODE_SYSTEM, Letter::Create) {
        ActionGroup::primary(Action::new(
            Some(Glyph::FILE_PLUS),
            tr(locale, Msg::CodeSystemsNew),
            Act::Link(ui_link(CODE_SYSTEM_EDITOR_PATH, request.version)),
        ))
    } else {
        ActionGroup::none()
    };
    state.frame(
        request,
        Frame {
            title,
            subtitle: tr_with(
                locale,
                Msg::CodeSystemsSubtitle,
                &[("version", request.version.label().into())],
            ),
            actions,
            path,
            total,
            kept: kept.len(),
            counts,
            find: tr(locale, Msg::CodeSystemsFind),
            body,
            pages: (
                tr(locale, Msg::CodeSystemsPages),
                Noun::new(message(Msg::NounCodeSystems)),
            ),
        },
    )
}
