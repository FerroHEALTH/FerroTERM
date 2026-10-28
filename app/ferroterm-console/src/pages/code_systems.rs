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

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Role, cell, data_table, title_cell};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::coding::canonical;
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;

use crate::auth::scopes::Letter;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::concept::Hierarchy;
use crate::fhir::named::{Listed, NamedSearch};
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, message, tr, tr_with};
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
pub(crate) fn content_word(content: Option<&str>) -> String {
    match content {
        Some("complete") => tr(Msg::ContentComplete),
        Some("not-present") => tr(Msg::ContentNotPresent),
        Some("example") => tr(Msg::ContentExample),
        Some("fragment") => tr(Msg::ContentFragment),
        Some("supplement") => tr(Msg::ContentSupplement),
        Some(other) => other.to_owned(),
        None => tr(Msg::ContentUndeclared),
    }
}

/// Whether a code system has a hierarchy the browser can walk, in words.
#[must_use]
pub(crate) fn hierarchy_word(card: &SystemCard) -> String {
    let default = card
        .versions
        .iter()
        .find(|version| version.is_default)
        .or_else(|| card.versions.first());
    match default.map(Hierarchy::of) {
        Some(Hierarchy::Walkable { .. }) => tr(Msg::HierarchyTree),
        Some(Hierarchy::WithoutChildren { .. }) => tr(Msg::HierarchyNoTree),
        Some(Hierarchy::Absent) | None => tr(Msg::HierarchyFlat),
    }
}

/// Code systems.
pub(crate) async fn page(request: Request) -> AnyView {
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
        Err(error) => view! {
            {heading(tr(Msg::ShellCodeSystems), None, Vec::new(), ActionGroup::none())}
            {failure(&error)}
        }
        .into_any(),
    }
}

/// The listing of `all` the root serves, as the address filters it.
fn listing(request: &Request, all: Vec<SystemRow>) -> AnyView {
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
        Column::text(NAME_COLUMN, tr(Msg::CodeSystemsSystem))
            .role(Role::Title)
            .sortable(),
        Column::text("source", tr(Msg::CodeSystemsSource)).role(Role::Meta),
        Column::text("versions", tr(Msg::CodeSystemsVersions)).role(Role::Meta),
        Column::text("content", tr(Msg::CodeSystemsContent)).role(Role::Quiet),
        Column::text("hierarchy", tr(Msg::CodeSystemsHierarchy)).role(Role::Quiet),
    ];
    let [_, source, versions, content, hierarchy] = &columns;
    let title = tr(Msg::ShellCodeSystems);
    let body = if kept.is_empty() {
        state.emptied(
            request,
            &path,
            tr_with(Msg::CodeSystemsNoMatch, &[("count", total.into())]),
        )
    } else {
        let drawn = state
            .window(kept.len())
            .slice(&kept)
            .iter()
            .map(|row| {
                let href = system_link(&row.card.url, request.version);
                let codes: Vec<String> = row
                    .card
                    .versions
                    .iter()
                    .filter_map(|version| version.code.clone())
                    .collect();
                view! {
                    <tr>
                        {title_cell(Some(&href), row.name.clone(), Some(canonical(&row.card.url)))}
                        {cell(source, row.source.tag())} {cell(versions, versions_word(&codes))}
                        {cell(content, content_word(row.card.content.as_deref()))}
                        {cell(hierarchy, hierarchy_word(&row.card))}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        let fields = state.fields(request, &[]);
        let sort = state.sort();
        data_table(DataTable {
            caption: &title,
            columns: &columns,
            sorting: ListState::sorting(&sort, &fields),
            body: Body::Rows(drawn),
        })
        .into_any()
    };
    let actions = if request.can(CODE_SYSTEM, Letter::Create) {
        ActionGroup::primary(Action::new(
            Some(Glyph::FILE_PLUS),
            tr(Msg::CodeSystemsNew),
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
                Msg::CodeSystemsSubtitle,
                &[("version", request.version.label().into())],
            ),
            actions,
            path,
            total,
            kept: kept.len(),
            counts,
            find: tr(Msg::CodeSystemsFind),
            body,
            pages: (
                tr(Msg::CodeSystemsPages),
                Noun::new(message(Msg::NounCodeSystems)),
            ),
        },
    )
}
