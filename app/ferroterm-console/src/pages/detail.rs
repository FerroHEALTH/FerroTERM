// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the three resource pages share (T2): the heading with the trail, the
//! status, the canonical and the version, the action group, and the binder
//! tabs over one panel, each tab a route.
//!
//! A tab the resource cannot have is left out, never disabled: a resource the
//! deployment loaded has no write path, so it has no History tab and no
//! Edit. No specification governs the page: our own design.

use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::coding::canonical;
use ferrohealth_ui::view::surface::{Crumb, Header};
use ferrohealth_ui::view::tabs::{Tab, binder};
use leptos::prelude::*;

use crate::fhir::version::FhirVersion;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::listing::status_state;
use crate::pages::parts::{Source, headed};
use crate::routes::{CONCEPT_MAPS_PATH, SYSTEMS_PATH, VALUE_SETS_PATH, history_link, ui_link};

/// The query parameter that names a resource page's tab.
pub(crate) const PANE_PARAM: &str = "pane";

/// The three resource types the console has a page for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A `CodeSystem`.
    CodeSystem,
    /// A `ValueSet`.
    ValueSet,
    /// A `ConceptMap`.
    ConceptMap,
}

impl Kind {
    /// The kind of `resource_type`, when the console has a page for it.
    #[must_use]
    pub(crate) fn of(resource_type: &str) -> Option<Self> {
        match resource_type {
            CODE_SYSTEM => Some(Self::CodeSystem),
            VALUE_SET => Some(Self::ValueSet),
            CONCEPT_MAP => Some(Self::ConceptMap),
            _ => None,
        }
    }

    /// The resource type as a request path writes it.
    #[must_use]
    pub(crate) const fn resource_type(self) -> &'static str {
        match self {
            Self::CodeSystem => CODE_SYSTEM,
            Self::ValueSet => VALUE_SET,
            Self::ConceptMap => CONCEPT_MAP,
        }
    }

    /// The listing the trail leads back to: its words and its address.
    pub(crate) fn listing(self, version: FhirVersion) -> Crumb {
        let (words, path) = match self {
            Self::CodeSystem => (Msg::ShellCodeSystems, SYSTEMS_PATH),
            Self::ValueSet => (Msg::ShellValueSets, VALUE_SETS_PATH),
            Self::ConceptMap => (Msg::ShellConceptMaps, CONCEPT_MAPS_PATH),
        };
        Crumb::new(tr(words), ui_link(path, version))
    }

    /// The name of the tab row.
    fn tabs_label(self) -> String {
        tr(match self {
            Self::CodeSystem => Msg::DetailTabsCodeSystem,
            Self::ValueSet => Msg::DetailTabsValueSet,
            Self::ConceptMap => Msg::DetailTabsConceptMap,
        })
    }

    /// The words of its first and second tab.
    fn tab_words(self) -> (Msg, Msg) {
        match self {
            Self::CodeSystem => (Msg::DetailOverview, Msg::DetailVersions),
            Self::ValueSet => (Msg::DetailDefinition, Msg::DetailExpansion),
            Self::ConceptMap => (Msg::DetailMappings, Msg::DetailOverview),
        }
    }

    /// The key of its second tab, as its `pane` parameter names it.
    #[must_use]
    pub(crate) const fn second(self) -> &'static str {
        match self {
            Self::CodeSystem => "versions",
            Self::ValueSet => "expansion",
            Self::ConceptMap => "overview",
        }
    }
}

/// The tab a resource page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pane {
    /// Its first tab, at the page's own address.
    First,
    /// Its second tab.
    Second,
    /// Its revisions, at the history screen.
    History,
    /// The resource as JSON.
    Json,
}

impl Pane {
    /// The tab the address's `pane` names for `kind`, the first for none.
    #[must_use]
    pub(crate) fn of(kind: Kind, pane: Option<&str>) -> Option<Self> {
        match pane {
            None => Some(Self::First),
            Some("json") => Some(Self::Json),
            Some(key) if key == kind.second() => Some(Self::Second),
            Some(_) => None,
        }
    }

    /// The tab's key.
    const fn key(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
            Self::History => "history",
            Self::Json => "json",
        }
    }
}

/// What a resource page's heading says.
#[derive(Debug)]
pub(crate) struct Head {
    /// The resource's kind, which names the trail.
    pub(crate) kind: Kind,
    /// The name a reader reads it by.
    pub(crate) title: String,
    /// Its publication status, when it states one.
    pub(crate) status: Option<String>,
    /// Its canonical, empty when it carries none.
    pub(crate) canonical: String,
    /// Its version and where it comes from, in words.
    pub(crate) line: String,
    /// The title row's action group (F14).
    pub(crate) actions: ActionGroup,
}

/// The heading of a resource page.
#[must_use]
pub(crate) fn header(head: Head, version: FhirVersion) -> AnyView {
    let (subtitle, subtitle_view) = if head.canonical.is_empty() {
        (Some(head.line.into()), None)
    } else {
        let line = head.line;
        let drawn = view! {
            {canonical(&head.canonical)}
            " · "
            {line}
        }
        .into_any();
        (None, Some(drawn))
    };
    headed(
        Header {
            title: head.title.into(),
            subtitle,
            subtitle_view,
            crumbs: vec![head.kind.listing(version)],
            mono: false,
            badge: Some(status_state(head.status.as_deref())),
        },
        head.actions,
    )
}

/// Its version and where it comes from, as the heading's line says them.
#[must_use]
pub(crate) fn line(version: Option<&str>, source: Source) -> String {
    let source = match source {
        Source::Stored => tr(Msg::DetailStored),
        Source::Index => tr(Msg::DetailIndex),
        Source::Loaded => tr(Msg::DetailLoaded),
    };
    match version {
        Some(version) => tr_with(
            Msg::DetailVersionLine,
            &[
                ("version", version.to_owned().into()),
                ("source", source.into()),
            ],
        ),
        None => source,
    }
}

/// Where each tab of a resource page leads.
#[derive(Debug)]
pub(crate) struct Links {
    /// The page's own address, its first tab.
    pub(crate) page: String,
    /// The resource's logical id, for its history; `None` for a resource
    /// with no write path, which has no History tab.
    pub(crate) history: Option<String>,
}

/// The address of `pane` on the page at `page`.
fn pane_href(page: &str, key: &str) -> String {
    let separator = if page.contains('?') { '&' } else { '?' };
    format!("{page}{separator}{PANE_PARAM}={key}")
}

/// The binder tabs of a `kind` page over `panel`, `current` raised.
#[must_use]
pub(crate) fn tabbed(
    kind: Kind,
    current: Pane,
    links: &Links,
    version: FhirVersion,
    panel: AnyView,
) -> AnyView {
    let (first, second) = kind.tab_words();
    let mut tabs = vec![
        Tab::new(Pane::First.key(), links.page.clone(), tr(first)),
        Tab::new(
            Pane::Second.key(),
            pane_href(&links.page, kind.second()),
            tr(second),
        ),
    ];
    if let Some(id) = &links.history {
        tabs.push(Tab::new(
            Pane::History.key(),
            history_link(kind.resource_type(), id, version),
            tr(Msg::DetailHistory),
        ));
    }
    tabs.push(Tab::new(
        Pane::Json.key(),
        pane_href(&links.page, "json"),
        tr(Msg::DetailJson),
    ));
    binder(kind.tabs_label(), tabs, current.key(), panel)
}

#[cfg(test)]
mod tests {
    use super::{Kind, Pane, pane_href};

    #[test]
    fn each_kind_names_its_second_tab() {
        assert_eq!(
            Some(Pane::Second),
            Pane::of(Kind::CodeSystem, Some("versions"))
        );
        assert_eq!(
            Some(Pane::Second),
            Pane::of(Kind::ValueSet, Some("expansion"))
        );
        assert_eq!(
            Some(Pane::Second),
            Pane::of(Kind::ConceptMap, Some("overview"))
        );
        assert_eq!(None, Pane::of(Kind::ValueSet, Some("versions")));
        assert_eq!(Some(Pane::First), Pane::of(Kind::ConceptMap, None));
        assert_eq!(Some(Pane::Json), Pane::of(Kind::CodeSystem, Some("json")));
    }

    #[test]
    fn a_tab_keeps_the_query_the_page_carries() {
        assert_eq!(
            "/valuesets/a?fhir=r5&pane=json",
            pane_href("/valuesets/a?fhir=r5", "json")
        );
        assert_eq!("/valuesets/a?pane=json", pane_href("/valuesets/a", "json"));
    }
}
