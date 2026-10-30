// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Concept maps (T1): every concept map the root holds, with where it comes
//! from, what it maps from and to, how many mappings it carries and its
//! status.

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
use crate::fhir::CONCEPT_MAP;
use crate::fhir::concept_map::PublishedConceptMap;
use crate::fhir::searchset::SearchFilter;
use crate::locale::{Msg, message, tr, tr_with};
use crate::markup::{h, join, t};
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
    locale: &Locale,
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
        locale,
        Msg::ConceptMapsFromTo,
        &[
            ("source", name_of(names, &source).into()),
            ("target", name_of(names, &target).into()),
        ],
    ))
}

/// Concept maps.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
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
        Err(error) => Html::join([
            heading(
                &request,
                tr(locale, Msg::ShellConceptMaps),
                None,
                Vec::new(),
                ActionGroup::none(),
            ),
            failure(locale, &error),
        ]),
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
) -> Html {
    let locale = &request.locale;
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
        Column::text(NAME_COLUMN, tr(locale, Msg::ConceptMapsMap))
            .role(Role::Title)
            .sortable(),
        Column::text("source", tr(locale, Msg::CodeSystemsSource)).role(Role::Meta),
        Column::text("mappings", tr(locale, Msg::ConceptMapsMappings)).numeric(),
        Column::text("status", tr(locale, Msg::ValueSetsStatus)).role(Role::Meta),
    ];
    let [_, source, mappings, status] = &columns;
    let title = tr(locale, Msg::ShellConceptMaps);
    let body = if kept.is_empty() && total == 0 {
        let first = request.can(CONCEPT_MAP, Letter::Create).then(|| {
            (
                tr(locale, Msg::ConceptMapsFirst),
                ui_link(CONCEPT_MAP_EDITOR_PATH, request.version),
            )
        });
        ferrohealth_ui::view::template::list::list_empty(tr(locale, Msg::ConceptMapsNone), first)
    } else if kept.is_empty() {
        state.emptied(
            request,
            &path,
            tr_with(locale, Msg::ConceptMapsNoMatch, &[("count", total.into())]),
        )
    } else {
        let drawn = join(state.window(kept.len()).slice(&kept).iter().map(|map| {
            let href = resource_link(
                CONCEPT_MAPS_PATH,
                map.id().unwrap_or_default(),
                request.version,
            );
            let url = map.url().unwrap_or_default();
            let from_to = from_to(locale, map, names)
                .map(|words| h(format!(r#"<span class="fui-row-sub">{}</span>"#, t(words))));
            let sub = h(format!("{}{}", canonical(url), from_to.unwrap_or_default()));
            let count = u32::try_from(map.mappings().len()).unwrap_or(u32::MAX);
            h(format!(
                "<tr>{}{}{}{}</tr>",
                title_cell(Some(&href), t(label_of(map)), Some(sub)),
                cell(source, source_of(map).tag(locale)),
                cell(
                    mappings,
                    t(tr_with(
                        locale,
                        Msg::ConceptMapsMappingCount,
                        &[("count", count.into())]
                    ))
                ),
                cell(status, status_state(locale, map.status())),
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
    let actions = create_action(request);
    state.frame(
        request,
        Frame {
            title,
            subtitle: tr_with(
                locale,
                Msg::ConceptMapsSubtitle,
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
                tr(locale, Msg::ConceptMapsPages),
                Noun::new(message(Msg::NounConceptMaps)),
            ),
        },
    )
}

/// The title row's action: a new resource, for a reader who may create one.
fn create_action(request: &Request) -> ActionGroup {
    let locale = &request.locale;
    if request.can(CONCEPT_MAP, Letter::Create) {
        ActionGroup::primary(Action::new(
            Some(Glyph::FILE_PLUS),
            tr(locale, Msg::ConceptMapsNew),
            Act::Link(ui_link(CONCEPT_MAP_EDITOR_PATH, request.version)),
        ))
    } else {
        ActionGroup::none()
    }
}
