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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{
    SectionHead, definitions, or_none, panel_section, panel_sections, section_intro,
};
use ferrohealth_ui::view::template::list::HeadingCount;

use crate::auth::scopes::Letter;
use crate::fhir::code_system::{CodeSystemSearch, PublishedCodeSystem};
use crate::fhir::concept::{Hierarchy, chosen_version};
use crate::fhir::terminology::{SystemCard, VersionRow};
use crate::fhir::{CODE_SYSTEM, Document, resources_text};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, t};
use crate::pages::code_systems::content_word;
use crate::pages::detail::{Head, Kind, Links, PANE_PARAM, Pane, header, line, tabbed};
use crate::pages::parts::{Source, failure, heading};
use crate::request::Request;
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
pub(crate) async fn page(request: Request) -> Html {
    let system = request.segment(1).unwrap_or_default();
    let Some(pane) = Pane::of(Kind::CodeSystem, request.param(PANE_PARAM).as_deref()) else {
        return crate::pages::errors::not_found_page(&request);
    };
    let capabilities = match request
        .client
        .terminology_capabilities(request.version)
        .await
    {
        Ok(capabilities) => capabilities,
        Err(error) => return refused(&request, &system, &error),
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
    screen(&request, &read, pane)
}

/// The page of a read that was refused.
fn refused(request: &Request, system: &str, error: &crate::fhir::error::FhirError) -> Html {
    Html::join([
        heading(
            request,
            system.to_owned(),
            None,
            Vec::new(),
            ActionGroup::none(),
        ),
        failure(&request.locale, error),
    ])
}

/// The page of a canonical the root does not declare.
fn undeclared(request: &Request, system: &str) -> Html {
    let locale = &request.locale;
    let title = if system.is_empty() {
        tr(locale, Msg::CodeSystemNoCanonical)
    } else {
        system.to_owned()
    };
    Html::join([
        header(
            request,
            Head {
                kind: Kind::CodeSystem,
                title,
                status: None,
                canonical: String::new(),
                line: tr_with(
                    locale,
                    Msg::CodeSystemOnVersion,
                    &[("version", request.version.label().into())],
                ),
                actions: ActionGroup::none(),
            },
        ),
        notice(
            Tone::Neutral,
            Announce::Status,
            None,
            t(tr(locale, Msg::RootSTerminologyCapabilitiesDoNot)),
        ),
    ])
}

/// The version the root serves by default.
fn served(card: &SystemCard) -> Option<VersionRow> {
    chosen_version(card, None)
}

/// The title row's action group: Edit for a stored system the reader may
/// write, else Browse concepts where there is a tree, the operations in More.
fn actions(request: &Request, read: &Read, title: &str) -> ActionGroup {
    let locale = &request.locale;
    let url = &read.card.url;
    let version = served(&read.card);
    let code = version.as_ref().and_then(|row| row.code.clone());
    let walkable = version
        .as_ref()
        .is_some_and(|row| Hierarchy::of(row).walk().is_some());
    let tool = |path: &str| system_tool_link(path, url, code.as_deref(), request.version);
    let browse = Action::new(
        Some(Glyph::NETWORK),
        tr(locale, Msg::CodeSystemBrowse),
        Act::Link(tool(BROWSE_PATH)),
    );
    let look_up = Action::new(
        Some(Glyph::SEARCH),
        tr(locale, Msg::CodeSystemLookUp),
        Act::Link(tool(LOOKUP_PATH)),
    );
    let check = Action::new(
        Some(Glyph::SHIELD_CHECK),
        tr(locale, Msg::CodeSystemCheck),
        Act::Link(tool(VALIDATE_PATH)),
    );
    let expand = Action::new(
        Some(Glyph::LIST_CHECKS),
        tr(locale, Msg::CodeSystemExpand),
        Act::Link(tool(EXPAND_PATH)),
    );
    let editable = read.source == Source::Stored && request.can(CODE_SYSTEM, Letter::Update);
    let (primary, mut more) = if editable {
        let edit = editing_link(CODE_SYSTEM, url, "", request.version).map(|href| {
            Action::new(
                Some(Glyph::PENCIL),
                tr(locale, Msg::CodeSystemEdit),
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
        tr_with(
            locale,
            Msg::DetailMoreFor,
            &[("name", title.to_owned().into())],
        ),
        more.into_iter().map(Item::new).collect(),
    );
    let group = primary.map_or_else(ActionGroup::none, ActionGroup::primary);
    group.more(menu)
}

/// The page of `read` on `pane`.
fn screen(request: &Request, read: &Read, pane: Pane) -> Html {
    let locale = &request.locale;
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
        request,
        Head {
            kind: Kind::CodeSystem,
            title: title.clone(),
            status: read
                .published
                .as_ref()
                .and_then(PublishedCodeSystem::status)
                .map(str::to_owned),
            canonical: read.card.url.clone(),
            line: line(locale, version.as_deref(), read.source),
            actions: actions(request, read, &title),
        },
    );
    let panel = match pane {
        Pane::First | Pane::History => overview(locale, read),
        Pane::Second => versions(locale, read),
        Pane::Json => json(locale, read),
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
    Html::join([head, tabbed(request, Kind::CodeSystem, pane, &links, panel)])
}

/// A declared boolean as its word.
fn yes_no(locale: &Locale, value: Option<bool>) -> Html {
    match value {
        Some(true) => t(tr(locale, Msg::DetailYes)),
        Some(false) => t(tr(locale, Msg::DetailNo)),
        None => or_none(locale, String::new()),
    }
}

/// A declared text, or the quiet word for none.
fn text(locale: &Locale, value: Option<&str>) -> Html {
    or_none(locale, value.unwrap_or_default().to_owned())
}

/// The quiet note on a system the console cannot edit, saying why.
fn read_only_note(locale: &Locale, source: Source) -> Option<Html> {
    let words = match source {
        Source::Stored => return None,
        Source::Index => tr(locale, Msg::CodeSystemIndexNote),
        Source::Loaded => tr(locale, Msg::CodeSystemLoadedNote),
    };
    Some(panel_section(
        locale,
        None,
        notice(Tone::Neutral, Announce::Status, None, t(words)),
    ))
}

/// Overview: what the code system says it is.
fn overview(locale: &Locale, read: &Read) -> Html {
    let note = read_only_note(locale, read.source).unwrap_or_default();
    let Some(published) = &read.published else {
        return panel_sections(Html::join([
            note,
            panel_section(
                locale,
                None,
                section_intro(tr(locale, Msg::CodeSystemNoResource)),
            ),
        ]));
    };
    let date = published
        .date()
        .map(|raw| ferrohealth_ui::view::time::timestamp(locale, raw));
    let details = definitions(vec![
        (
            tr(locale, Msg::CodeSystemComputerName),
            text(locale, published.name()),
        ),
        (
            tr(locale, Msg::CodeSystemExperimental),
            yes_no(locale, published.experimental()),
        ),
        (
            tr(locale, Msg::CodeSystemPublisher),
            text(locale, published.publisher()),
        ),
        (
            tr(locale, Msg::CodeSystemLastChanged),
            date.unwrap_or_else(|| or_none(locale, String::new())),
        ),
        (
            tr(locale, Msg::CodeSystemDescription),
            text(locale, published.description()),
        ),
    ]);
    let content = match (published.content(), published.count()) {
        (content, Some(count)) => tr_with(
            locale,
            Msg::CodeSystemContentCount,
            &[
                ("content", content_word(locale, content).into()),
                ("count", count.into()),
            ],
        ),
        (content, None) => content_word(locale, content),
    };
    let structure = definitions(vec![
        (tr(locale, Msg::CodeSystemsContent), t(content)),
        (
            tr(locale, Msg::CodeSystemHierarchyMeaning),
            text(locale, published.hierarchy_meaning()),
        ),
        (
            tr(locale, Msg::CodeSystemCaseSensitive),
            yes_no(locale, published.case_sensitive()),
        ),
        (
            tr(locale, Msg::CodeSystemVersionNeeded),
            yes_no(locale, published.version_needed()),
        ),
        (
            tr(locale, Msg::CodeSystemCompositional),
            yes_no(locale, published.compositional()),
        ),
        (
            tr(locale, Msg::CodeSystemSupplements),
            text(locale, published.supplements()),
        ),
    ]);
    let properties = declared_properties(locale, published).unwrap_or_default();
    panel_sections(Html::join([
        note,
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::CodeSystemDetails),
                count: None,
                actions: None,
            }),
            details,
        ),
        panel_section(
            locale,
            Some(SectionHead {
                title: tr(locale, Msg::CodeSystemStructure),
                count: None,
                actions: None,
            }),
            structure,
        ),
        properties,
    ]))
}

/// The properties the code system declares, as a table; nothing when it
/// declares none.
fn declared_properties(locale: &Locale, published: &PublishedCodeSystem) -> Option<Html> {
    let declared = published.properties();
    (!declared.is_empty()).then(|| {
        let columns = [
            Column::text("property", tr(locale, Msg::CodeSystemProperty)).role(Role::Title),
            Column::text("type", tr(locale, Msg::CodeSystemPropertyType)).role(Role::Sub),
            Column::text("meaning", tr(locale, Msg::CodeSystemPropertyMeaning)).role(Role::Quiet),
        ];
        let [_, kind, meaning] = &columns;
        let rows = join(declared.iter().map(|property| {
            h(format!(
                "<tr>{}{}{}</tr>",
                title_cell(None, code(property.code.clone().unwrap_or_default()), None),
                cell(kind, t(property.kind.clone().unwrap_or_default())),
                cell(
                    meaning,
                    t(property
                        .description
                        .clone()
                        .or_else(|| property.uri.clone())
                        .unwrap_or_default()),
                ),
            ))
        }));
        let caption = tr(locale, Msg::CodeSystemDeclaredProperties);
        let table = data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        });
        panel_section(
            locale,
            Some(SectionHead {
                title: caption.clone(),
                count: Some(HeadingCount::All(
                    u64::try_from(declared.len()).unwrap_or(u64::MAX),
                )),
                actions: None,
            }),
            table,
        )
    })
}

/// The title of one served version: Release for one read from an index.
fn version_title(locale: &Locale, row: &VersionRow) -> String {
    let code = row
        .code
        .clone()
        .unwrap_or_else(|| tr(locale, Msg::CodeSystemUnnamedVersion));
    if row.artifact.is_some() {
        tr_with(locale, Msg::CodeSystemRelease, &[("version", code.into())])
    } else {
        tr_with(locale, Msg::VersionsOne, &[("version", code.into())])
    }
}

/// Codes in the code face, a space after each.
fn codes(each: &[String]) -> Html {
    join(each.iter().map(|one| h(format!("{} ", code(one)))))
}

/// The filters one served version declares, as a table.
fn version_filters(locale: &Locale, row: &VersionRow, filters_title: &str) -> Html {
    if row.filters.is_empty() {
        section_intro(tr(locale, Msg::CodeSystemNoFilters))
    } else {
        let columns = [
            Column::text("property", tr(locale, Msg::CodeSystemProperty)).role(Role::Title),
            Column::text("operators", tr(locale, Msg::CodeSystemOperators)),
        ];
        let [_, operators] = &columns;
        let rows = join(row.filters.iter().map(|filter| {
            h(format!(
                "<tr>{}{}</tr>",
                title_cell(None, code(&filter.code), None),
                cell(operators, codes(&filter.operators)),
            ))
        }));
        data_table(DataTable {
            caption: filters_title,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
    }
}

/// Versions: what the server declares it can do with each version.
fn versions(locale: &Locale, read: &Read) -> Html {
    let sections = join(read.card.versions.iter().map(|row| {
        let title = version_title(locale, row);
        let answers = if row.is_default {
            tr_with(
                locale,
                Msg::CodeSystemAnswersDefault,
                &[("version", title.clone().into())],
            )
        } else {
            tr_with(
                locale,
                Msg::CodeSystemAnswersOnly,
                &[("version", title.clone().into())],
            )
        };
        let subsumption = if read.card.subsumption == Some(true) {
            tr(locale, Msg::CodeSystemSubsumes)
        } else {
            tr(locale, Msg::CodeSystemNoSubsumption)
        };
        let languages = if row.languages.is_empty() {
            or_none(locale, String::new())
        } else {
            t(row.languages.join(", "))
        };
        let properties = if row.properties.is_empty() {
            or_none(locale, String::new())
        } else {
            codes(&row.properties)
        };
        let mut facts = vec![
            (tr(locale, Msg::CodeSystemAnswersRequests), t(answers)),
            (tr(locale, Msg::CodeSystemSubsumption), t(subsumption)),
            (tr(locale, Msg::CodeSystemLanguages), languages),
            (tr(locale, Msg::CodeSystemLookUpReturns), properties),
        ];
        if let Some(artifact) = &row.artifact {
            let name = artifact
                .name
                .clone()
                .or_else(|| artifact.release.clone())
                .unwrap_or_default();
            facts.push((tr(locale, Msg::CodeSystemReadFrom), code(name)));
        }
        let filters_title = tr_with(
            locale,
            Msg::CodeSystemFiltersOf,
            &[("version", title.clone().into())],
        );
        let filters = version_filters(locale, row, &filters_title);
        Html::join([
            panel_section(
                locale,
                Some(SectionHead {
                    title,
                    count: None,
                    actions: None,
                }),
                definitions(facts),
            ),
            panel_section(
                locale,
                Some(SectionHead {
                    title: filters_title.clone(),
                    count: None,
                    actions: None,
                }),
                filters,
            ),
        ])
    }));
    panel_sections(Html::join([
        panel_section(
            locale,
            None,
            section_intro(tr(locale, Msg::CodeSystemVersionsIntro)),
        ),
        sections,
    ]))
}

/// JSON: the resource as the server sent it, read with `_summary=true` for
/// a system served from an index.
fn json(locale: &Locale, read: &Read) -> Html {
    let Some(text) = &read.text else {
        return panel_sections(panel_section(
            locale,
            None,
            section_intro(tr(locale, Msg::CodeSystemNoResource)),
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
            locale,
            Msg::DetailJsonSummaryTitle,
            &[("path", format!("CodeSystem/{id}").into())],
        )
    } else {
        format!("CodeSystem/{id}")
    };
    let code = Code {
        label: tr(locale, Msg::CodeSystemJsonLabel),
        language: Language::Json,
        text: text.clone(),
        ids: None,
        current: None,
        diagnostics: Vec::new(),
    };
    let note = summary.then(|| {
        panel_section(
            locale,
            None,
            section_intro(tr(locale, Msg::CodeSystemSummaryNote)),
        )
    });
    panel_sections(Html::join([
        note.unwrap_or_default(),
        code_section(
            locale,
            CodeHead {
                title,
                tags: vec![String::from("FHIR")],
                digest: None,
                copy: Some(String::from("code-system-copy")),
                actions: None,
            },
            &code,
        ),
    ]))
}
