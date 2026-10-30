// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Find: what the top bar's search was given, the resources whose name or
//! canonical holds it, and the screens that take it.
//!
//! The string is read by its shape alone (`crate::find`); the offers are
//! gated on what the root's `CapabilityStatement` declares
//! (<https://hl7.org/fhir/R5/capabilitystatement.html>), so an operation it
//! does not declare is left out rather than offered and refused.
use axum::Extension;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::search::{Hit, HitGroup, Mark, SearchAnswer, find_results};
use ferrohealth_ui::view::surface::stack;
use serde::Deserialize;

use crate::fhir::capability::CapabilityStatement;
use crate::fhir::named::Listed;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP, VALUE_SET};
use crate::find::{
    Declared, EXPAND, LOOKUP, Offer, QUERY_PARAM, Subject, TRANSLATE, VALIDATE_CODE, offers,
};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, t};
use crate::pages::parts::{failure, heading};
use crate::request::Request;
use crate::routes::{CONCEPT_MAPS_PATH, VALUE_SETS_PATH, resource_link, system_link};

/// The address the panel under the top bar's search asks as you type
/// ([`answer_route`]).
pub(crate) const ANSWER_ROUTE: &str = "/find/answer";

/// The glyphs the panel's hits carry, which the page holds for the `search`
/// module to draw.
pub(crate) const GLYPHS: [Glyph; 4] = [
    Glyph::BOOK_OPEN,
    Glyph::LAYERS,
    Glyph::ARROW_LEFT_RIGHT,
    Glyph::ARROW_RIGHT,
];

/// What the root declares, as the offers read it.
fn declared_by(statement: &CapabilityStatement) -> Declared {
    let mut operations = Vec::new();
    if statement.declares_operation(CODE_SYSTEM, LOOKUP) {
        operations.push(LOOKUP);
    }
    if statement.declares_operation(CODE_SYSTEM, VALIDATE_CODE)
        || statement.declares_operation(VALUE_SET, VALIDATE_CODE)
    {
        operations.push(VALIDATE_CODE);
    }
    if statement.declares_operation(VALUE_SET, EXPAND) {
        operations.push(EXPAND);
    }
    if statement.declares_operation(CONCEPT_MAP, TRANSLATE) {
        operations.push(TRANSLATE);
    }
    Declared::of(operations)
}

/// Whether `resource`'s name or canonical holds `typed`, ignoring case.
fn matches(resource: &Listed, typed: &str) -> bool {
    let typed = typed.to_lowercase();
    resource.label.to_lowercase().contains(&typed) || resource.url.to_lowercase().contains(&typed)
}

/// What the root holds under what was typed: the resources of each kind
/// whose name or canonical holds it, and the screens that take it.
#[derive(Debug, Clone)]
struct Found {
    /// The resources that match, by resource type, each with its kind's
    /// heading, a kind with none left out.
    groups: Vec<(&'static str, String, Vec<Listed>)>,
    /// The screens that take what was typed, as the root's
    /// `CapabilityStatement` gates them.
    offered: Vec<Offer>,
    /// The resource types whose read failed: the kinds of resource left out
    /// of [`Found::groups`], and `CapabilityStatement` when the offers are.
    failed: Vec<&'static str>,
}

/// The glyph each kind of resource carries in the navigation, which its
/// hits carry too.
fn glyph_of(resource_type: &str) -> Glyph {
    match resource_type {
        CODE_SYSTEM => Glyph::BOOK_OPEN,
        VALUE_SET => Glyph::LAYERS,
        _ => Glyph::ARROW_LEFT_RIGHT,
    }
}

/// Reads what the root holds under `subject` through `request`, offering
/// the screens `declared` gates, or none when the root's
/// `CapabilityStatement` did not answer.
async fn find(request: &Request, subject: &Subject, declared: Option<&Declared>) -> Found {
    let locale = &request.locale;
    let mut failed = Vec::new();
    let offered = if let Some(declared) = declared {
        offers(locale, subject, declared, request.version)
    } else {
        failed.push("CapabilityStatement");
        Vec::new()
    };
    let mut groups = Vec::new();
    for (resource_type, kind) in [
        (CODE_SYSTEM, Msg::ShellCodeSystems),
        (VALUE_SET, Msg::ShellValueSets),
        (CONCEPT_MAP, Msg::ShellConceptMaps),
    ] {
        match request
            .client
            .published_names(request.version, resource_type)
            .await
        {
            Ok(named) => {
                let found: Vec<Listed> = named
                    .listed()
                    .into_iter()
                    .filter(|resource| matches(resource, subject.typed()))
                    .collect();
                if !found.is_empty() {
                    groups.push((resource_type, tr(locale, kind), found));
                }
            }
            Err(error) => {
                tracing::warn!(%error, resource_type, "the find read no names");
                failed.push(resource_type);
            }
        }
    }
    Found {
        groups,
        offered,
        failed,
    }
}

impl Found {
    /// The answer the panel under the search box and the find page draw:
    /// every match grouped by kind, then the screens that take what was
    /// typed.
    fn answer(&self, request: &Request, subject: &Subject) -> SearchAnswer {
        let locale = &request.locale;
        let mut groups: Vec<HitGroup> = self
            .groups
            .iter()
            .map(|(resource_type, kind, found)| HitGroup {
                kind: kind.clone(),
                hits: found
                    .iter()
                    .map(|resource| Hit {
                        href: href_of(request, resource_type, resource),
                        name: resource.label.clone(),
                        line: resource.url.clone(),
                        mark: Some(Mark::Glyph(glyph_of(resource_type))),
                        tag: None,
                    })
                    .collect(),
            })
            .collect();
        if !self.offered.is_empty() {
            groups.push(HitGroup {
                kind: tr(locale, Msg::WhereItGoes),
                hits: self
                    .offered
                    .iter()
                    .map(|offer| Hit {
                        href: offer.href.clone(),
                        name: offer.label.clone(),
                        line: offer.detail.clone(),
                        mark: Some(Mark::Glyph(Glyph::ARROW_RIGHT)),
                        tag: None,
                    })
                    .collect(),
            });
        }
        SearchAnswer {
            query: subject.typed().to_owned(),
            head: None,
            groups,
            none: Some((
                tr_with(
                    locale,
                    Msg::FindNothing,
                    &[("typed", subject.typed().into())],
                ),
                tr(locale, Msg::RootDeclaresNoneOperationsWouldApply),
            )),
            unfinished: (!self.failed.is_empty()).then(|| {
                tr_with(
                    locale,
                    Msg::FindUnfinished,
                    &[("reads", self.failed.join(", ").into())],
                )
            }),
            more: None,
        }
    }
}

/// The address of `resource`, a resource of `resource_type`.
fn href_of(request: &Request, resource_type: &str, resource: &Listed) -> String {
    match resource_type {
        CODE_SYSTEM => system_link(&resource.url, request.version),
        VALUE_SET => resource_link(VALUE_SETS_PATH, &resource.id, request.version),
        _ => resource_link(CONCEPT_MAPS_PATH, &resource.id, request.version),
    }
}

/// What the panel under the top bar's search sends.
#[derive(Debug, Deserialize)]
pub(crate) struct Typed {
    /// The words typed.
    #[serde(default)]
    q: String,
}

/// What the root holds under `typed`, as the panel and the find page read
/// it; a read the server refuses is named in the answer's `unfinished`.
async fn find_answer(request: &Request, typed: &str) -> SearchAnswer {
    let locale = &request.locale;
    let Some(subject) = Subject::of(typed) else {
        return SearchAnswer {
            query: String::new(),
            none: Some((tr(locale, Msg::NothingWasTyped), String::new())),
            ..SearchAnswer::default()
        };
    };
    let declared = match request.client.capability_statement(request.version).await {
        Ok(statement) => Some(declared_by(&statement)),
        Err(error) => {
            tracing::warn!(%error, "the find read no capability statement");
            None
        }
    };
    find(request, &subject, declared.as_ref())
        .await
        .answer(request, &subject)
}

/// Answers the panel under the top bar's search: what the root holds under
/// the words `q`, as JSON, never kept by a cache.
pub(crate) async fn answer_route(
    Extension(state): Extension<crate::server::ConsoleState>,
    Query(typed): Query<Typed>,
    request: axum::extract::Request,
) -> Response {
    let (parts, _body) = request.into_parts();
    let request = Request::read(&state, &parts);
    let answer = find_answer(&request, &typed.q).await;
    (
        [(http::header::CACHE_CONTROL, "no-store")],
        axum::Json(answer),
    )
        .into_response()
}

/// The Find page.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    let typed = request.text(QUERY_PARAM);
    let head = heading(
        &request,
        tr(locale, Msg::TitleFind),
        Some(tr(locale, Msg::FindSubtitle)),
        Vec::new(),
        ActionGroup::none(),
    );
    let Some(subject) = Subject::of(&typed) else {
        return Html::join([
            head,
            notice(
                Tone::Info,
                Announce::Status,
                None,
                t(tr(locale, Msg::NothingWasTyped)),
            ),
        ]);
    };
    let statement = match request.client.capability_statement(request.version).await {
        Ok(statement) => statement,
        Err(error) => return Html::join([head, failure(locale, &error)]),
    };
    let found = find(&request, &subject, Some(&declared_by(&statement))).await;
    let read_as = found.groups.is_empty().then(|| {
        notice(
            Tone::Info,
            Announce::Status,
            None,
            h(format!(
                "{} {}",
                code(subject.typed()),
                t(tr_with(
                    locale,
                    Msg::FindReadAs,
                    &[
                        ("subject", subject.label(locale).into()),
                        ("version", request.version.label().into()),
                    ],
                ))
            )),
        )
    });
    let results = find_results(locale, &found.answer(&request, &subject));
    Html::join([
        head,
        stack(Html::join([read_as.unwrap_or_default(), results])),
    ])
}

#[cfg(test)]
mod tests {
    use super::matches;
    use crate::fhir::named::Listed;

    #[test]
    fn a_resource_matches_by_name_or_canonical_ignoring_case() {
        let resource = Listed {
            url: String::from("https://x/fhir/ValueSet/respiratory"),
            label: String::from("Synthetic respiratory findings"),
            ..Listed::default()
        };
        assert!(matches(&resource, "RESPIR"));
        assert!(matches(&resource, "ValueSet/resp"));
        assert!(!matches(&resource, "cardiac"));
    }
}
