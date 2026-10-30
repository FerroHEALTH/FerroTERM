// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept browser (M1): search a code system, read a concept and walk
//! its hierarchy, three panes of one panel.
//!
//! Every move is a link or a `GET` form, so the browser walks with no
//! script: a match or a tree item selects its concept, a toggle opens or
//! closes one level, and FerroUI's `tree` module folds a level it drew in
//! place and keeps the folds for the session.
//! The tree walks one level at a time with the direct-child filter the
//! served version declares (`TerminologyCapabilities.codeSystem.version.filter`,
//! <https://hl7.org/fhir/R4B/terminologycapabilities.html>), read with an
//! inline `ValueSet/$expand`; a version that declares none is a flat list.

use std::collections::BTreeSet;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::filter::Find;
use ferrohealth_ui::table::model::StateSource;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::browser::{
    BrowserPane, Heading, Panes, browser, browser_find, match_list, pane_body,
};
use ferrohealth_ui::view::coding::{Coding, Layout, coding};
use ferrohealth_ui::view::surface::{
    SectionHead, card_action, definitions, panel_section, section_intro,
};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::transform::{Density, transform_options};
use ferrohealth_ui::view::tree::{Tree, TreeKind, TreeNode, TreeWords, path_key, tree_view};

use crate::fhir::concept::{
    CHILD_OF_OPERATOR, ChildOf, Concept, ConceptQuery, Hierarchy, LookupRequest, chosen_version,
};
use crate::fhir::expansion::ConceptRow;
use crate::fhir::terminology::SystemCard;
use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, FhirClient};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, join, t};
use crate::pages::lookup::CODE_PARAM;
use crate::pages::operations::{choices, hidden_field, select};
use crate::pages::parts::{failure, heading};
use crate::request::Request;
use crate::routes::{
    BROWSE_PATH, LOOKUP_PATH, SYSTEM_PARAM, SYSTEM_VERSION_PARAM, VERSION_PARAM, system_tool_link,
    ui_link,
};
use crate::tree::{decode_open, encode_open};

/// The address parameter carrying the search text.
const FILTER_PARAM: &str = "filter";

/// The address parameter carrying the display language.
const LANGUAGE_PARAM: &str = "displayLanguage";

/// The address parameter carrying the codes opened beyond the path.
const OPEN_PARAM: &str = "open";

/// How many matches one search reads.
const MATCHES: u32 = 25;

/// How many children one level of the tree reads.
const LEVEL: u32 = 100;

/// How far the path above a concept is walked before it stops.
const DEPTH: usize = 16;

/// The state the address carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    system: String,
    version: String,
    language: String,
    filter: String,
    code: String,
    open: BTreeSet<String>,
}

impl State {
    /// The state the address names.
    fn read(request: &Request) -> Self {
        Self {
            system: request.text(SYSTEM_PARAM),
            version: request.text(SYSTEM_VERSION_PARAM),
            language: request.text(LANGUAGE_PARAM),
            filter: request.text(FILTER_PARAM),
            code: request.text(CODE_PARAM),
            open: decode_open(&request.text(OPEN_PARAM)),
        }
    }

    /// The address of this state, on the FHIR version `fhir`.
    fn href(&self, fhir: FhirVersion) -> String {
        let open = encode_open(&self.open);
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in [
            (SYSTEM_PARAM, &self.system),
            (SYSTEM_VERSION_PARAM, &self.version),
            (LANGUAGE_PARAM, &self.language),
            (FILTER_PARAM, &self.filter),
            (CODE_PARAM, &self.code),
            (OPEN_PARAM, &open),
        ] {
            if !value.is_empty() {
                query.append_pair(name, value);
            }
        }
        format!("{}&{}", ui_link(BROWSE_PATH, fhir), query.finish())
    }

    /// This state reading `code`.
    fn selecting(&self, code: &str) -> Self {
        Self {
            code: code.to_owned(),
            ..self.clone()
        }
    }

    /// This state with the level under `code` opened or closed.
    fn toggling(&self, code: &str, open: bool) -> Self {
        let mut showing = self.open.clone();
        if open {
            showing.insert(code.to_owned());
        } else {
            showing.remove(code);
        }
        Self {
            open: showing,
            ..self.clone()
        }
    }

    /// The fields the forms carry, besides their own.
    fn fields(&self, fhir: FhirVersion, without: &[&str]) -> Vec<(String, String)> {
        [
            (VERSION_PARAM, fhir.segment().to_owned()),
            (SYSTEM_PARAM, self.system.clone()),
            (SYSTEM_VERSION_PARAM, self.version.clone()),
            (LANGUAGE_PARAM, self.language.clone()),
            (FILTER_PARAM, self.filter.clone()),
            (CODE_PARAM, self.code.clone()),
        ]
        .into_iter()
        .filter(|(name, value)| !value.is_empty() && !without.contains(name))
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
    }

    /// The display language to ask for, when the address names one.
    fn language(&self) -> Option<String> {
        Some(self.language.clone()).filter(|tag| !tag.is_empty())
    }

    /// The version to pin the operations to, when the address names one.
    fn pinned(&self) -> Option<String> {
        Some(self.version.clone()).filter(|version| !version.is_empty())
    }
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The concept browser.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let state = State::read(&request);
    let named = request
        .client
        .published_names(request.version, CODE_SYSTEM)
        .await
        .ok();
    let systems = choices(named.as_ref());
    let card = request
        .client
        .terminology_capabilities(request.version)
        .await
        .ok()
        .and_then(|capabilities| capabilities.card(&state.system));
    let row = card
        .as_ref()
        .and_then(|card| chosen_version(card, state.pinned().as_deref()));
    let hierarchy = row.as_ref().map(Hierarchy::of).unwrap_or_default();
    let languages: Vec<(String, String)> = row
        .as_ref()
        .map(|row| {
            row.languages
                .iter()
                .map(|tag| (tag.clone(), tag.clone()))
                .collect()
        })
        .unwrap_or_default();
    let options = picker(
        &request,
        &state,
        &systems,
        &versions(locale, card.as_ref()),
        &languages,
    );
    let body = panes(&request, &state, &hierarchy).await;
    Html::join([
        heading(
            &request,
            tr(locale, Msg::BrowseTitle),
            Some(tr(locale, Msg::BrowseSubtitle)),
            Vec::new(),
            ActionGroup::none(),
        ),
        h(format!(
            r#"<section class="fui-card fui-panel" aria-label="{}">{options}{body}</section>"#,
            t(tr(locale, Msg::BrowseTitle))
        )),
    ])
}

/// The versions the picker offers, the one an unversioned request resolves
/// to marked as the default.
fn versions(locale: &Locale, card: Option<&SystemCard>) -> Vec<(String, String)> {
    card.map(|card| {
        card.versions
            .iter()
            .filter_map(|row| {
                let code = row.code.clone()?;
                let words = if row.is_default {
                    tr_with(
                        locale,
                        Msg::LookupDefaultVersion,
                        &[("version", code.clone().into())],
                    )
                } else {
                    code.clone()
                };
                Some((code, words))
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The panes: the search, the hierarchy where the version declares a walk,
/// and the concept, or what to choose first.
async fn panes(request: &Request, state: &State, hierarchy: &Hierarchy) -> Html {
    let locale = &request.locale;
    if state.system.is_empty() {
        return browser(
            Panes::Three,
            Heading::Two,
            vec![
                search_pane(request, state, None, None),
                BrowserPane::new(
                    tr(locale, Msg::BrowseHierarchy),
                    pane_body(section_intro(tr(locale, Msg::BrowseChooseSystemTree))),
                ),
                BrowserPane::new(
                    tr(locale, Msg::BrowseConcept),
                    pane_body(section_intro(tr(locale, Msg::BrowseChooseConcept))),
                ),
            ],
        );
    }
    let found = search(&request.client, request.version, state).await;
    let concept = read(&request.client, request.version, state).await;
    let concept_pane = concept_pane(request, state, concept.as_ref());
    if let Some(property) = hierarchy.walk() {
        let read_concept = concept.as_ref().and_then(|read| read.as_ref().ok());
        let tree = tree_pane(request, state, property, read_concept).await;
        browser(
            Panes::Three,
            Heading::Two,
            vec![
                search_pane(request, state, Some(found), None),
                tree,
                concept_pane,
            ],
        )
    } else {
        let note = flat_note(locale, hierarchy);
        browser(
            Panes::Flat,
            Heading::Two,
            vec![
                search_pane(request, state, Some(found), Some(note)),
                concept_pane,
            ],
        )
    }
}

/// The pickers over the panes, a `GET` form a new system resets.
fn picker(
    request: &Request,
    state: &State,
    systems: &[(String, String)],
    versions: &[(String, String)],
    languages: &[(String, String)],
) -> Html {
    let locale = &request.locale;
    let fields = Html::join([
        hidden_field(VERSION_PARAM, request.version.segment()),
        select(
            "browse-system",
            SYSTEM_PARAM,
            tr(locale, Msg::LookupSystem),
            systems,
            &state.system,
            Some(tr(locale, Msg::BrowseChooseSystem)),
        ),
        select(
            "browse-version",
            SYSTEM_VERSION_PARAM,
            tr(locale, Msg::LookupVersion),
            versions,
            &state.version,
            Some(tr(locale, Msg::LookupServerVersion)),
        ),
        select(
            "browse-language",
            LANGUAGE_PARAM,
            tr(locale, Msg::LookupLanguage),
            languages,
            &state.language,
            Some(tr(locale, Msg::LookupServerLanguage)),
        ),
    ]);
    let open = h(format!(
        r#"<button type="submit" class="{}">{}</button>"#,
        ferrohealth_ui::view::button::Variant::Secondary.class(),
        t(tr(locale, Msg::BrowseOpen))
    ));
    h(format!(
        r#"<form method="get" action="/{BROWSE_PATH}">{}</form>"#,
        transform_options(
            Density::Plain,
            fields,
            h(format!(r#"<div class="fui-transform-run">{open}</div>"#)),
        )
    ))
}

/// The concepts the search finds, or the first ones of the system when it
/// names no text.
async fn search(
    client: &FhirClient,
    fhir: FhirVersion,
    state: &State,
) -> Result<Vec<ConceptRow>, crate::fhir::error::FhirError> {
    let query = ConceptQuery {
        system: state.system.clone(),
        system_version: state.pinned(),
        filter: Some(state.filter.clone()).filter(|text| !text.is_empty()),
        child_of: None,
        display_language: state.language(),
        count: MATCHES,
    };
    let expanded = client.expand_inline(fhir, &query).await?;
    Ok(expanded
        .expansion()
        .map(|expansion| expansion.concepts)
        .unwrap_or_default())
}

/// The concept the address reads, when it names one.
async fn read(
    client: &FhirClient,
    fhir: FhirVersion,
    state: &State,
) -> Option<Result<Concept, crate::fhir::error::FhirError>> {
    if state.code.is_empty() {
        return None;
    }
    let asked = LookupRequest {
        system: state.system.clone(),
        system_version: state.pinned(),
        code: state.code.clone(),
        display_language: state.language(),
    };
    Some(
        client
            .lookup(fhir, &asked)
            .await
            .map(|document| document.value.concept()),
    )
}

/// The search pane: the find field, the `note` on a flat version under it,
/// and the matches, each a link that reads its concept.
fn search_pane(
    request: &Request,
    state: &State,
    found: Option<Result<Vec<ConceptRow>, crate::fhir::error::FhirError>>,
    note: Option<Html>,
) -> BrowserPane {
    let locale = &request.locale;
    let fields = state.fields(request.version, &[FILTER_PARAM]);
    let label = tr(locale, Msg::BrowseSearchHint);
    let find = browser_find(Find {
        id: "browse-find",
        name: FILTER_PARAM,
        label: &label,
        value: &state.filter,
        source: StateSource::Url,
        action: Some(&format!("/{BROWSE_PATH}")),
        fields: &fields,
    });
    let results = match found {
        None => section_intro(tr(locale, Msg::BrowseChooseSystemFirst)),
        Some(Err(error)) => failure(locale, &error),
        Some(Ok(concepts)) => {
            let count = section_intro(tr_with(
                locale,
                Msg::BrowseCount,
                &[("count", concepts.len().into())],
            ));
            let rows = concepts
                .iter()
                .map(|concept| {
                    let drawn = Coding::new(
                        concept.code.clone(),
                        concept.display.clone().unwrap_or_default(),
                    )
                    .href(state.selecting(&concept.code).href(request.version))
                    .current(concept.code == state.code);
                    let drawn = if concept.inactive {
                        drawn.inactive()
                    } else {
                        drawn
                    };
                    coding(locale, drawn, Layout::Stacked)
                })
                .collect();
            Html::join([
                pane_body(count),
                match_list(&tr(locale, Msg::BrowseMatches), rows),
            ])
        }
    };
    BrowserPane::new(
        tr(locale, Msg::BrowseSearch),
        Html::join([
            pane_body(Html::join([find, note.unwrap_or_default()])),
            results,
        ]),
    )
}

/// The sentence a flat version's search pane says about its hierarchy.
fn flat_note(locale: &Locale, hierarchy: &Hierarchy) -> Html {
    let said = match hierarchy {
        Hierarchy::WithoutChildren { operators } => tr_with(
            locale,
            Msg::VersionDeclaresHierarchyOperatorsNoneThem,
            &[("operators", operators.join(", ").into())],
        ),
        Hierarchy::Absent | Hierarchy::Walkable { .. } => {
            tr(locale, Msg::VersionDeclaresNoHierarchyFilterOperator)
        }
    };
    section_intro(said)
}

/// The direct children of `code`, one level of the tree.
async fn children(
    client: &FhirClient,
    fhir: FhirVersion,
    state: &State,
    property: &str,
    code: &str,
) -> Option<Vec<ConceptRow>> {
    let query = ConceptQuery {
        system: state.system.clone(),
        system_version: state.pinned(),
        filter: None,
        child_of: Some(ChildOf {
            property: property.to_owned(),
            code: code.to_owned(),
        }),
        display_language: state.language(),
        count: LEVEL,
    };
    // NOTE: no specification governs this: our own design; a level that does not
    // read stays closed as unread, and its toggle asks again.
    client
        .expand_inline(fhir, &query)
        .await
        .ok()
        .map(|expanded| {
            expanded
                .expansion()
                .map(|expansion| expansion.concepts)
                .unwrap_or_default()
        })
}

/// The codes above `concept`, from its top ancestor down to its parent,
/// following the first parent each concept declares.
async fn ancestors(
    client: &FhirClient,
    fhir: FhirVersion,
    state: &State,
    concept: &Concept,
) -> Vec<(String, Option<String>)> {
    let mut path: Vec<(String, Option<String>)> = Vec::new();
    let mut parent = concept.parents().into_iter().next();
    while let Some(code) = parent.take() {
        if path.len() >= DEPTH || path.iter().any(|(seen, _)| *seen == code) || code == state.code {
            break;
        }
        let asked = LookupRequest {
            system: state.system.clone(),
            system_version: state.pinned(),
            code: code.clone(),
            display_language: state.language(),
        };
        let read = client
            .lookup(fhir, &asked)
            .await
            .ok()
            .map(|document| document.value.concept());
        parent = read
            .as_ref()
            .and_then(|read| read.parents().into_iter().next());
        path.push((code, read.and_then(|read| read.display)));
    }
    path.reverse();
    path
}

/// The tree pane: the path from the top ancestor down to the concept read,
/// each open level read, every item a link and every toggle a link.
async fn tree_pane(
    request: &Request,
    state: &State,
    property: &str,
    concept: Option<&Concept>,
) -> BrowserPane {
    let locale = &request.locale;
    let title = tr(locale, Msg::BrowseHierarchy);
    let Some(concept) = concept else {
        return BrowserPane::new(
            title,
            pane_body(section_intro(tr(locale, Msg::BrowseChooseConceptTree))),
        );
    };
    let path = ancestors(&request.client, request.version, state, concept).await;
    let top = path
        .first()
        .cloned()
        .unwrap_or_else(|| (state.code.clone(), concept.display.clone()));
    let mut opened: BTreeSet<String> = path.iter().map(|(code, _)| code.clone()).collect();
    opened.extend(state.open.iter().cloned());
    opened.insert(state.code.clone());
    let mut keys: Vec<String> = Vec::new();
    let walk = Walk {
        request,
        state,
        property,
        opened: &opened,
        leaf: concept.children().is_empty(),
    };
    let node = Box::pin(level(&walk, &top, &mut keys, &[])).await;
    let mut chain: Vec<&str> = path.iter().map(|(code, _)| code.as_str()).collect();
    if path.is_empty() {
        chain.clear();
    }
    chain.push(&state.code);
    let selected = Some(path_key(&chain));
    let tree = Tree {
        id: String::from("browse-tree"),
        label: title.clone(),
        kind: TreeKind::Concept,
        editing: false,
        nodes: vec![node],
        open: keys.into_iter().collect(),
        selected,
        open_field: None,
        words: TreeWords::of(locale),
    };
    let declared = tr_with(
        locale,
        Msg::VersionDeclaresOperatorFilterSoTree,
        &[
            ("child_of_operator", CHILD_OF_OPERATOR.into()),
            ("property", property.to_owned().into()),
        ],
    );
    BrowserPane::new(
        title,
        Html::join([
            h(format!(
                r#"<div class="fui-browser-body">{}</div>"#,
                tree_view(&tree)
            )),
            pane_body(section_intro(declared)),
        ]),
    )
}

/// What one drawing of the tree walks with.
struct Walk<'a> {
    /// The request the page is drawn for.
    request: &'a Request,
    /// The state the address carries.
    state: &'a State,
    /// The filter property the direct-child operator is declared on.
    property: &'a str,
    /// The codes whose level is open: the path and the ones opened.
    opened: &'a BTreeSet<String>,
    /// Whether the concept the page reads has no children.
    leaf: bool,
}

/// The item of `concept` (its code and display) at `above` its path, its
/// level read when it is open, the path keys of open items gathered in
/// `keys`.
async fn level(
    walk: &Walk<'_>,
    concept: &(String, Option<String>),
    keys: &mut Vec<String>,
    above: &[String],
) -> TreeNode {
    let Walk {
        request,
        state,
        property,
        opened,
        leaf,
    } = *walk;
    let (code, display) = concept;
    let mut chain: Vec<String> = above.to_vec();
    chain.push(code.clone());
    let open = opened.contains(code) && chain.len() <= DEPTH;
    let mut node = TreeNode::new(
        code.clone(),
        display.clone().unwrap_or_else(|| code.clone()),
    )
    .code(code.clone())
    .href(state.selecting(code).href(request.version))
    .toggle_href(state.toggling(code, !open).href(request.version));
    if code == &state.code && leaf {
        return node;
    }
    if !open {
        return node.unread();
    }
    let Some(read) = children(&request.client, request.version, state, property, code).await else {
        return node.unread();
    };
    keys.push(path_key(&chain));
    let mut drawn = Vec::with_capacity(read.len());
    for child in &read {
        if chain.contains(&child.code) {
            continue;
        }
        let item = (child.code.clone(), child.display.clone());
        drawn.push(Box::pin(level(walk, &item, keys, &chain)).await);
    }
    node = node.children(drawn);
    node
}

/// The concept pane: the concept read, its parents and children, its
/// designations and its properties.
fn concept_pane(
    request: &Request,
    state: &State,
    concept: Option<&Result<Concept, crate::fhir::error::FhirError>>,
) -> BrowserPane {
    let locale = &request.locale;
    let title = tr(locale, Msg::BrowseConcept);
    let all = card_action(
        format!(
            "{}&{CODE_PARAM}={}",
            system_tool_link(
                LOOKUP_PATH,
                &state.system,
                state.pinned().as_deref(),
                request.version
            ),
            percent(&state.code)
        ),
        tr(locale, Msg::BrowseAllProperties),
    );
    match concept {
        None => BrowserPane::new(
            title,
            pane_body(section_intro(tr(locale, Msg::BrowseChooseConcept))),
        ),
        Some(Err(error)) => BrowserPane::new(title, pane_body(failure(locale, error))),
        Some(Ok(concept)) => {
            let link = |each: &String| {
                let href = state.selecting(each).href(request.version);
                h(format!(
                    r#"<span class="fui-browser-rel-item"><a href="{}">{}</a></span>"#,
                    t(href),
                    code(each)
                ))
            };
            let parents: Vec<Html> = concept.parents().iter().map(link).collect();
            let children: Vec<Html> = concept.children().iter().map(link).collect();
            let mut related = Vec::new();
            if !parents.is_empty() {
                related.push((tr(locale, Msg::BrowseParent), join(parents)));
            }
            if !children.is_empty() {
                related.push((tr(locale, Msg::BrowseChildren), join(children)));
            }
            let read_from = concept
                .version
                .as_ref()
                .map_or_else(String::new, |version| {
                    tr_with(
                        locale,
                        Msg::BrowseReadFrom,
                        &[("version", version.clone().into())],
                    )
                });
            let head = h(format!(
                r#"<h3 class="fui-concept-title">{}</h3><p class="fui-small fui-muted">{} {}</p><div class="fui-browser-rel">{}</div>"#,
                t(concept
                    .display
                    .clone()
                    .unwrap_or_else(|| state.code.clone())),
                code(&state.code),
                t(read_from),
                definitions(related),
            ));
            BrowserPane::new(
                title,
                Html::join([
                    pane_body(head),
                    designations(locale, concept).unwrap_or_default(),
                    properties(locale, concept).unwrap_or_default(),
                ]),
            )
            .actions(all)
        }
    }
}

/// The concept's designations.
fn designations(locale: &Locale, concept: &Concept) -> Option<Html> {
    if concept.designations.is_empty() {
        return None;
    }
    let columns = [
        Column::text("term", tr(locale, Msg::LookupTerm)).role(Role::Title),
        Column::text("language", tr(locale, Msg::LookupTermLanguage)),
        Column::text("use", tr(locale, Msg::LookupUse)),
    ];
    let [_, language, usage] = &columns;
    let rows = join(concept.designations.iter().map(|row| {
        h(format!(
            "<tr>{}{}{}</tr>",
            title_cell(None, t(&row.value), None),
            cell(language, t(row.language.clone().unwrap_or_default())),
            cell(usage, t(row.usage.clone().unwrap_or_default())),
        ))
    }));
    let caption = tr(locale, Msg::LookupDesignations);
    Some(panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(concept.designations.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }),
    ))
}

/// The concept's properties.
fn properties(locale: &Locale, concept: &Concept) -> Option<Html> {
    if concept.properties.is_empty() {
        return None;
    }
    let columns = [
        Column::text("property", tr(locale, Msg::LookupProperty)).role(Role::Title),
        Column::text("value", tr(locale, Msg::LookupValue)),
    ];
    let [_, value] = &columns;
    let rows = join(concept.properties.iter().map(|row| {
        h(format!(
            "<tr>{}{}</tr>",
            title_cell(None, code(&row.code), row.description.as_ref().map(t)),
            cell(value, t(row.value.clone().unwrap_or_default())),
        ))
    }));
    let caption = tr(locale, Msg::LookupProperties);
    Some(panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(
                u64::try_from(concept.properties.len()).unwrap_or(u64::MAX),
            )),
            actions: None,
        }),
        data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::fhir::version::FhirVersion;

    #[test]
    fn a_toggle_moves_only_its_level_and_keeps_the_concept() {
        let state = State {
            system: String::from("http://x/cs"),
            code: String::from("a"),
            ..State::default()
        };
        let opened = state.toggling("b", true);
        assert!(opened.open.contains("b"));
        assert_eq!("a", opened.code);
        assert!(!opened.toggling("b", false).open.contains("b"));
        let href = opened.href(FhirVersion::R4);
        assert!(href.contains("system=http%3A%2F%2Fx%2Fcs"), "{href}");
        assert!(href.contains("code=a"), "{href}");
        assert!(href.contains("open=b"), "{href}");
    }
}
