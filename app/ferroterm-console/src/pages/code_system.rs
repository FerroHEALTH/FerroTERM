// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One code system (T2): what it says it is (Overview), what the server can
//! do with each version (Versions), its revisions (History, for one stored
//! on this server) and the resource itself (JSON).
//!
//! The page reads the root's `TerminologyCapabilities`, which says what the
//! server serves, and the `CodeSystem` resource, which says what the system
//! is; nothing here names a system, assumes a hierarchy or a language.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{
    SectionHead, definitions, or_none, panel_section, panel_sections, section_intro,
};
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::auth::scopes::Letter;
use crate::fhir::code_system::{CodeSystemSearch, PublishedCodeSystem};
use crate::fhir::concept::{Hierarchy, chosen_version};
use crate::fhir::terminology::{SystemCard, VersionRow};
use crate::fhir::{CODE_SYSTEM, Document, resources_text};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::code_systems::content_word;
use crate::pages::detail::{Head, Kind, Links, PANE_PARAM, Pane, header, line, tabbed};
use crate::pages::parts::{Source, failure, heading, loaded};
use crate::request::{Request, request};
use crate::routes::{
    BROWSE_PATH, EXPAND_PATH, LOOKUP_PATH, VALIDATE_PATH, editing_link, system_link,
    system_tool_link,
};

/// What the page read.
struct Read {
    /// What the root's capabilities declare of the system.
    card: SystemCard,
    /// The published resource, when the root publishes one.
    published: Option<PublishedCodeSystem>,
    /// The resource as the server sent it, pretty.
    text: Option<String>,
    /// Where it comes from.
    source: Source,
}

/// One code system, on the tab the address names.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn CodeSystemPage() -> impl IntoView {
    let request = request();
    let params = use_params_map();
    let system = params
        .with_untracked(|params| params.get("system"))
        .unwrap_or_default();
    let Some(pane) = Pane::of(Kind::CodeSystem, request.param(PANE_PARAM).as_deref()) else {
        return crate::pages::errors::not_found_page();
    };
    loaded(async move {
        let capabilities = match request
            .client
            .terminology_capabilities(request.version)
            .await
        {
            Ok(capabilities) => capabilities,
            Err(error) => return refused(&system, &error),
        };
        let Some(card) = capabilities.card(&system) else {
            return undeclared(&request, &system);
        };
        let index = card
            .versions
            .iter()
            .any(|version| version.artifact.is_some());
        let search: Result<Document<CodeSystemSearch>, _> = if index {
            request
                .client
                .code_system_summary(request.version, &system)
                .await
        } else {
            request
                .client
                .code_system_search(request.version, &system)
                .await
        };
        let (published, text) = match search {
            Ok(document) => (
                document.value.published().into_iter().next(),
                Some(resources_text(&document.text)),
            ),
            Err(_refused) => (None, None),
        };
        let source = if index {
            Source::Index
        } else if published
            .as_ref()
            .and_then(PublishedCodeSystem::stored)
            .is_some()
        {
            Source::Stored
        } else {
            Source::Loaded
        };
        let read = Read {
            card,
            published,
            text,
            source,
        };
        page(&request, &read, pane)
    })
}

/// The page of a read that was refused.
fn refused(system: &str, error: &crate::fhir::error::FhirError) -> AnyView {
    view! {
        {heading(system.to_owned(), None, Vec::new(), ActionGroup::none())}
        {failure(error)}
    }
    .into_any()
}

/// The page of a canonical the root does not declare.
fn undeclared(request: &Request, system: &str) -> AnyView {
    let title = if system.is_empty() {
        tr(Msg::CodeSystemNoCanonical)
    } else {
        system.to_owned()
    };
    view! {
        {header(
            Head {
                kind: Kind::CodeSystem,
                title,
                status: None,
                canonical: String::new(),
                line: tr_with(
                    Msg::CodeSystemOnVersion,
                    &[("version", request.version.label().into())],
                ),
                actions: ActionGroup::none(),
            },
            request.version,
        )}
        {notice(Tone::Neutral, Announce::Status, None, tr(Msg::RootSTerminologyCapabilitiesDoNot))}
    }
    .into_any()
}

/// The version the root serves by default.
fn served(card: &SystemCard) -> Option<VersionRow> {
    chosen_version(card, None)
}

/// The title row's action group: Edit for a stored system the reader may
/// write, else Browse concepts where there is a tree, the operations in More.
fn actions(request: &Request, read: &Read, title: &str) -> ActionGroup {
    let url = &read.card.url;
    let version = served(&read.card);
    let code = version.as_ref().and_then(|row| row.code.clone());
    let walkable = version
        .as_ref()
        .is_some_and(|row| Hierarchy::of(row).walk().is_some());
    let tool = |path: &str| system_tool_link(path, url, code.as_deref(), request.version);
    let browse = Action::new(
        Some(Glyph::NETWORK),
        tr(Msg::CodeSystemBrowse),
        Act::Link(tool(BROWSE_PATH)),
    );
    let look_up = Action::new(
        Some(Glyph::SEARCH),
        tr(Msg::CodeSystemLookUp),
        Act::Link(tool(LOOKUP_PATH)),
    );
    let check = Action::new(
        Some(Glyph::SHIELD_CHECK),
        tr(Msg::CodeSystemCheck),
        Act::Link(tool(VALIDATE_PATH)),
    );
    let expand = Action::new(
        Some(Glyph::LIST_CHECKS),
        tr(Msg::CodeSystemExpand),
        Act::Link(tool(EXPAND_PATH)),
    );
    let editable = read.source == Source::Stored && request.can(CODE_SYSTEM, Letter::Update);
    let (primary, mut more) = if editable {
        let edit = editing_link(CODE_SYSTEM, url, "", request.version).map(|href| {
            Action::new(
                Some(Glyph::PENCIL),
                tr(Msg::CodeSystemEdit),
                Act::Link(href),
            )
        });
        let mut more = Vec::new();
        if walkable {
            more.push(browse);
        }
        (edit, more)
    } else if walkable {
        (Some(browse), Vec::new())
    } else {
        (Some(look_up.clone()), Vec::new())
    };
    if primary
        .as_ref()
        .is_none_or(|action| action.label != look_up.label)
    {
        more.push(look_up);
    }
    more.extend([check, expand]);
    let menu = Menu::new(
        "code-system-more",
        tr_with(Msg::DetailMoreFor, &[("name", title.to_owned().into())]),
        more.into_iter().map(Item::from).collect(),
    );
    let group = primary.map_or_else(ActionGroup::none, ActionGroup::primary);
    group.more(menu)
}

/// The page of `read` on `pane`.
fn page(request: &Request, read: &Read, pane: Pane) -> AnyView {
    let title = read
        .published
        .as_ref()
        .and_then(PublishedCodeSystem::title)
        .map_or_else(|| read.card.url.clone(), str::to_owned);
    let version = read
        .published
        .as_ref()
        .and_then(PublishedCodeSystem::version)
        .map(str::to_owned)
        .or_else(|| served(&read.card).and_then(|row| row.code));
    let head = header(
        Head {
            kind: Kind::CodeSystem,
            title: title.clone(),
            status: read
                .published
                .as_ref()
                .and_then(PublishedCodeSystem::status)
                .map(str::to_owned),
            canonical: read.card.url.clone(),
            line: line(version.as_deref(), read.source),
            actions: actions(request, read, &title),
        },
        request.version,
    );
    let panel = match pane {
        Pane::First | Pane::History => overview(read),
        Pane::Second => versions(read),
        Pane::Json => json(read),
    };
    let links = Links {
        page: system_link(&read.card.url, request.version),
        history: read
            .published
            .as_ref()
            .filter(|_| read.source == Source::Stored)
            .and_then(PublishedCodeSystem::id)
            .map(str::to_owned),
    };
    view! {
        {head}
        {tabbed(Kind::CodeSystem, pane, &links, request.version, panel)}
    }
    .into_any()
}

/// A declared boolean as its word.
fn yes_no(value: Option<bool>) -> AnyView {
    match value {
        Some(true) => tr(Msg::DetailYes).into_any(),
        Some(false) => tr(Msg::DetailNo).into_any(),
        None => or_none(String::new()),
    }
}

/// A declared text, or the quiet word for none.
fn text(value: Option<&str>) -> AnyView {
    or_none(value.unwrap_or_default().to_owned())
}

/// The quiet note on a system the console cannot edit, saying why.
fn read_only_note(source: Source) -> Option<AnyView> {
    let words = match source {
        Source::Stored => return None,
        Source::Index => tr(Msg::CodeSystemIndexNote),
        Source::Loaded => tr(Msg::CodeSystemLoadedNote),
    };
    Some(panel_section(
        None,
        notice(Tone::Neutral, Announce::Status, None, words),
    ))
}

/// Overview: what the code system says it is.
fn overview(read: &Read) -> AnyView {
    let note = read_only_note(read.source);
    let Some(published) = &read.published else {
        return panel_sections(view! {
            {note}
            {panel_section(None, section_intro(tr(Msg::CodeSystemNoResource)))}
        });
    };
    let date = published
        .date()
        .map(|raw| ferrohealth_ui::view::time::timestamp(raw).into_any());
    let details = definitions(vec![
        (tr(Msg::CodeSystemComputerName), text(published.name())),
        (
            tr(Msg::CodeSystemExperimental),
            yes_no(published.experimental()),
        ),
        (tr(Msg::CodeSystemPublisher), text(published.publisher())),
        (
            tr(Msg::CodeSystemLastChanged),
            date.unwrap_or_else(|| or_none(String::new())),
        ),
        (
            tr(Msg::CodeSystemDescription),
            text(published.description()),
        ),
    ]);
    let content = match (published.content(), published.count()) {
        (content, Some(count)) => tr_with(
            Msg::CodeSystemContentCount,
            &[
                ("content", content_word(content).into()),
                ("count", count.into()),
            ],
        ),
        (content, None) => content_word(content),
    };
    let structure = definitions(vec![
        (tr(Msg::CodeSystemsContent), content.into_any()),
        (
            tr(Msg::CodeSystemHierarchyMeaning),
            text(published.hierarchy_meaning()),
        ),
        (
            tr(Msg::CodeSystemCaseSensitive),
            yes_no(published.case_sensitive()),
        ),
        (
            tr(Msg::CodeSystemVersionNeeded),
            yes_no(published.version_needed()),
        ),
        (
            tr(Msg::CodeSystemCompositional),
            yes_no(published.compositional()),
        ),
        (
            tr(Msg::CodeSystemSupplements),
            text(published.supplements()),
        ),
    ]);
    let properties = declared_properties(published);
    panel_sections(view! {
        {note}
        {panel_section(
            Some(SectionHead {
                title: tr(Msg::CodeSystemDetails),
                count: None,
                actions: None,
            }),
            details,
        )}
        {panel_section(
            Some(SectionHead {
                title: tr(Msg::CodeSystemStructure),
                count: None,
                actions: None,
            }),
            structure,
        )}
        {properties}
    })
}

/// The properties the code system declares, as a table; nothing when it
/// declares none.
fn declared_properties(published: &PublishedCodeSystem) -> Option<AnyView> {
    let declared = published.properties();
    (!declared.is_empty()).then(|| {
        let columns = [
            Column::text("property", tr(Msg::CodeSystemProperty)).role(Role::Title),
            Column::text("type", tr(Msg::CodeSystemPropertyType)).role(Role::Sub),
            Column::text("meaning", tr(Msg::CodeSystemPropertyMeaning)).role(Role::Quiet),
        ];
        let [_, kind, meaning] = &columns;
        let rows = declared
            .iter()
            .map(|property| {
                view! {
                    <tr>
                        {title_cell(
                            None,
                            view! { <code>{property.code.clone().unwrap_or_default()}</code> },
                            None,
                        )} {cell(kind, property.kind.clone().unwrap_or_default())}
                        {cell(
                            meaning,
                            property
                                .description
                                .clone()
                                .or_else(|| property.uri.clone())
                                .unwrap_or_default(),
                        )}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        let caption = tr(Msg::CodeSystemDeclaredProperties);
        let table = data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        });
        panel_section(
            Some(SectionHead {
                title: caption.clone(),
                count: Some(u64::try_from(declared.len()).unwrap_or(u64::MAX)),
                actions: None,
            }),
            table,
        )
    })
}

/// The title of one served version: Release for one read from an index.
fn version_title(row: &VersionRow) -> String {
    let code = row
        .code
        .clone()
        .unwrap_or_else(|| tr(Msg::CodeSystemUnnamedVersion));
    if row.artifact.is_some() {
        tr_with(Msg::CodeSystemRelease, &[("version", code.into())])
    } else {
        tr_with(Msg::VersionsOne, &[("version", code.into())])
    }
}

/// The filters one served version declares, as a table.
fn version_filters(row: &VersionRow, filters_title: &str) -> AnyView {
    if row.filters.is_empty() {
        section_intro(tr(Msg::CodeSystemNoFilters))
    } else {
        let columns = [
            Column::text("property", tr(Msg::CodeSystemProperty)).role(Role::Title),
            Column::text("operators", tr(Msg::CodeSystemOperators)),
        ];
        let [_, operators] = &columns;
        let rows = row
            .filters
            .iter()
            .map(|filter| {
                let ops = filter
                    .operators
                    .iter()
                    .map(|op| {
                        view! {
                            <code>{op.clone()}</code>
                            " "
                        }
                    })
                    .collect_view();
                view! {
                    <tr>
                        {title_cell(None, view! { <code>{filter.code.clone()}</code> }, None)}
                        {cell(operators, ops)}
                    </tr>
                }
            })
            .collect_view()
            .into_any();
        data_table(DataTable {
            caption: filters_title,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
        .into_any()
    }
}

/// Versions: what the server declares it can do with each version.
fn versions(read: &Read) -> AnyView {
    let sections = read
        .card
        .versions
        .iter()
        .map(|row| {
            let title = version_title(row);
            let answers = if row.is_default {
                tr_with(
                    Msg::CodeSystemAnswersDefault,
                    &[("version", title.clone().into())],
                )
            } else {
                tr_with(
                    Msg::CodeSystemAnswersOnly,
                    &[("version", title.clone().into())],
                )
            };
            let subsumption = if read.card.subsumption == Some(true) {
                tr(Msg::CodeSystemSubsumes)
            } else {
                tr(Msg::CodeSystemNoSubsumption)
            };
            let languages = if row.languages.is_empty() {
                or_none(String::new())
            } else {
                row.languages.join(", ").into_any()
            };
            let properties = if row.properties.is_empty() {
                or_none(String::new())
            } else {
                row.properties
                    .iter()
                    .map(|property| {
                        view! {
                            <code>{property.clone()}</code>
                            " "
                        }
                    })
                    .collect_view()
                    .into_any()
            };
            let mut facts = vec![
                (tr(Msg::CodeSystemAnswersRequests), answers.into_any()),
                (tr(Msg::CodeSystemSubsumption), subsumption.into_any()),
                (tr(Msg::CodeSystemLanguages), languages),
                (tr(Msg::CodeSystemLookUpReturns), properties),
            ];
            if let Some(artifact) = &row.artifact {
                let name = artifact
                    .name
                    .clone()
                    .or_else(|| artifact.release.clone())
                    .unwrap_or_default();
                facts.push((
                    tr(Msg::CodeSystemReadFrom),
                    view! { <code>{name}</code> }.into_any(),
                ));
            }
            let filters_title = tr_with(
                Msg::CodeSystemFiltersOf,
                &[("version", title.clone().into())],
            );
            let filters = version_filters(row, &filters_title);
            view! {
                {panel_section(
                    Some(SectionHead {
                        title,
                        count: None,
                        actions: None,
                    }),
                    definitions(facts),
                )}
                {panel_section(
                    Some(SectionHead {
                        title: filters_title.clone(),
                        count: None,
                        actions: None,
                    }),
                    filters,
                )}
            }
            .into_any()
        })
        .collect_view();
    panel_sections(view! {
        {panel_section(None, section_intro(tr(Msg::CodeSystemVersionsIntro)))}
        {sections}
    })
}

/// JSON: the resource as the server sent it, read with `_summary=true` for
/// a system served from an index.
fn json(read: &Read) -> AnyView {
    let Some(text) = &read.text else {
        return panel_sections(panel_section(
            None,
            section_intro(tr(Msg::CodeSystemNoResource)),
        ));
    };
    let id = read
        .published
        .as_ref()
        .and_then(PublishedCodeSystem::id)
        .unwrap_or_default();
    let summary = read.source == Source::Index;
    let title = if summary {
        tr_with(
            Msg::DetailJsonSummaryTitle,
            &[("path", format!("CodeSystem/{id}").into())],
        )
    } else {
        format!("CodeSystem/{id}")
    };
    let code = Code {
        label: tr(Msg::CodeSystemJsonLabel),
        language: Language::Json,
        text: text.clone(),
        ids: None,
        current: None,
        diagnostics: Vec::new(),
    };
    let note = summary.then(|| panel_section(None, section_intro(tr(Msg::CodeSystemSummaryNote))));
    panel_sections(view! {
        {note}
        {code_section(
            CodeHead {
                title,
                tags: vec![String::from("FHIR")],
                digest: None,
                copy: Some(String::from("code-system-copy")),
                actions: None,
            },
            &code,
        )}
    })
}
