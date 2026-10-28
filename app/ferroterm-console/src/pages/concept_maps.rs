// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Concept maps (T1): every concept map the root holds, with where it comes
//! from, what it maps from and to, how many mappings it carries and its
//! status.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Role, cell, data_table, title_cell};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::coding::canonical;
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;

use crate::auth::scopes::Letter;
use crate::fhir::CONCEPT_MAP;
use crate::fhir::concept_map::PublishedConceptMap;
use crate::fhir::searchset::SearchFilter;
use crate::locale::{Msg, message, tr, tr_with};
use crate::pages::listing::{Frame, ListState, NAME_COLUMN, counted, status_state};
use crate::pages::parts::{Source, failure, heading, name_of, system_names};
use crate::request::Request;
use crate::routes::{CONCEPT_MAP_EDITOR_PATH, CONCEPT_MAPS_PATH, resource_link, ui_link, ui_path};

/// The name a reader reads a concept map by.
fn label_of(map: &PublishedConceptMap) -> String {
    map.label()
        .or_else(|| map.url())
        .unwrap_or_default()
        .to_owned()
}

/// What a concept map maps from and to, in words: the systems of its first
/// group, named, else its scopes.
#[must_use]
pub(crate) fn from_to(
    map: &PublishedConceptMap,
    names: &BTreeMap<String, String>,
) -> Option<String> {
    let group = map.groups().into_iter().next();
    let source = group
        .as_ref()
        .and_then(|group| group.source.clone())
        .or_else(|| map.source_scope().map(str::to_owned))?;
    let target = group
        .and_then(|group| group.target)
        .or_else(|| map.target_scope().map(str::to_owned))?;
    Some(tr_with(
        Msg::ConceptMapsFromTo,
        &[
            ("source", name_of(names, &source).into()),
            ("target", name_of(names, &target).into()),
        ],
    ))
}

/// Concept maps.
pub(crate) async fn page(request: Request) -> AnyView {
    let found = request
        .client
        .concept_map_search(request.version, &SearchFilter::default())
        .await;
    let names = system_names(&request.client, request.version).await;
    match found {
        Ok(found) => listing(
            &request,
            found.found().into_iter().cloned().collect(),
            &names,
        ),
        Err(error) => view! {
            {heading(tr(Msg::ShellConceptMaps), None, Vec::new(), ActionGroup::none())}
            {failure(&error)}
        }
        .into_any(),
    }
}

/// Where a listed map comes from: stored on the server, or loaded at start.
fn source_of(map: &PublishedConceptMap) -> Source {
    if map.stored().is_some() {
        Source::Stored
    } else {
        Source::Loaded
    }
}

/// The listing of `all` the root holds, as the address filters it.
fn listing(
    request: &Request,
    all: Vec<PublishedConceptMap>,
    names: &BTreeMap<String, String>,
) -> AnyView {
    let state = ListState::read(request);
    let path = ui_path(CONCEPT_MAPS_PATH);
    let counts = counted(all.iter().map(source_of));
    let total = all.len();
    let kept = state.kept(
        all,
        label_of,
        |map| map.url().unwrap_or_default().to_owned(),
        source_of,
    );
    let columns = [
        Column::text(NAME_COLUMN, tr(Msg::ConceptMapsMap))
            .role(Role::Title)
            .sortable(),
        Column::text("source", tr(Msg::CodeSystemsSource)).role(Role::Meta),
        Column::text("mappings", tr(Msg::ConceptMapsMappings)).numeric(),
        Column::text("status", tr(Msg::ValueSetsStatus)).role(Role::Meta),
    ];
    let [_, source, mappings, status] = &columns;
    let title = tr(Msg::ShellConceptMaps);
    let body = if kept.is_empty() && total == 0 {
        let first = request.can(CONCEPT_MAP, Letter::Create).then(|| {
            (
                tr(Msg::ConceptMapsFirst),
                ui_link(CONCEPT_MAP_EDITOR_PATH, request.version),
            )
        });
        ferrohealth_ui::view::template::list::list_empty(tr(Msg::ConceptMapsNone), first)
    } else if kept.is_empty() {
        state.emptied(
            request,
            &path,
            tr_with(Msg::ConceptMapsNoMatch, &[("count", total.into())]),
        )
    } else {
        let drawn = state
            .window(kept.len())
            .slice(&kept)
            .iter()
            .map(|map| {
                let href = resource_link(
                    CONCEPT_MAPS_PATH,
                    map.id().unwrap_or_default(),
                    request.version,
                );
                let url = map.url().unwrap_or_default();
                let sub = view! {
                    {canonical(url)}
                    {from_to(map, names)
                        .map(|words| view! { <span class="fui-row-sub">{words}</span> })}
                }
                .into_any();
                let count = u32::try_from(map.mappings().len()).unwrap_or(u32::MAX);
                view! {
                    <tr>
                        {title_cell(Some(&href), label_of(map), Some(sub))}
                        {cell(source, source_of(map).tag())}
                        {cell(
                            mappings,
                            tr_with(Msg::ConceptMapsMappingCount, &[("count", count.into())]),
                        )} {cell(status, status_state(map.status()))}
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
    let actions = create_action(request);
    state.frame(
        request,
        Frame {
            title,
            subtitle: tr_with(
                Msg::ConceptMapsSubtitle,
                &[("version", request.version.label().into())],
            ),
            actions,
            path,
            total,
            kept: kept.len(),
            counts,
            find: tr(Msg::ValueSetsFind),
            body,
            pages: (
                tr(Msg::ConceptMapsPages),
                Noun::new(message(Msg::NounConceptMaps)),
            ),
        },
    )
}

/// The title row's action: a new resource, for a reader who may create one.
fn create_action(request: &Request) -> ActionGroup {
    if request.can(CONCEPT_MAP, Letter::Create) {
        ActionGroup::primary(Action::new(
            Some(Glyph::FILE_PLUS),
            tr(Msg::ConceptMapsNew),
            Act::Link(ui_link(CONCEPT_MAP_EDITOR_PATH, request.version)),
        ))
    } else {
        ActionGroup::none()
    }
}
