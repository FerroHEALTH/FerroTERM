// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Find: what the top bar's search was given, the resources whose name or
//! canonical holds it, and the screens that take it.
//!
//! The string is read by its shape alone (`crate::find`); the offers are
//! gated on what the root's `CapabilityStatement` declares
//! (<https://hl7.org/fhir/R5/capabilitystatement.html>), so an operation it
//! does not declare is left out rather than offered and refused.

use ferrohealth_ui::table::view::{Body, Column, DataTable, Role, Sorting, data_table, title_cell};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections_card, stack};
use leptos::prelude::*;

use crate::fhir::capability::CapabilityStatement;
use crate::fhir::named::Listed;
use crate::fhir::{CODE_SYSTEM, CONCEPT_MAP, VALUE_SET};
use crate::find::{
    Declared, EXPAND, LOOKUP, Offer, QUERY_PARAM, Subject, TRANSLATE, VALIDATE_CODE, offers,
};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::parts::{failure, heading, loaded};
use crate::request::{Request, request};
use crate::routes::{CONCEPT_MAPS_PATH, VALUE_SETS_PATH, resource_link, system_link};

/// The most matches one kind lists.
const MATCHES: usize = 10;

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

/// The Find page.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn FindPage() -> impl IntoView {
    let request = request();
    let typed = request.text(QUERY_PARAM);
    loaded(async move {
        let head = heading(
            tr(Msg::TitleFind),
            Some(tr(Msg::FindSubtitle)),
            Vec::new(),
            ActionGroup::none(),
        );
        let Some(subject) = Subject::of(&typed) else {
            return view! {
                {head}
                {notice(Tone::Info, Announce::Status, None, tr(Msg::NothingWasTyped))}
            }
            .into_any();
        };
        let statement = match request.client.capability_statement(request.version).await {
            Ok(statement) => statement,
            Err(error) => {
                return view! {
                    {head}
                    {failure(&error)}
                }
                .into_any();
            }
        };
        let mut groups = Vec::new();
        for (resource_type, kind) in [
            (CODE_SYSTEM, Msg::ShellCodeSystems),
            (VALUE_SET, Msg::ShellValueSets),
            (CONCEPT_MAP, Msg::ShellConceptMaps),
        ] {
            let listed = request
                .client
                .published_names(request.version, resource_type)
                .await
                .map(|named| named.listed())
                .unwrap_or_default();
            let found: Vec<Listed> = listed
                .into_iter()
                .filter(|resource| matches(resource, subject.typed()))
                .take(MATCHES)
                .collect();
            if !found.is_empty() {
                groups.push((resource_type, tr(kind), found));
            }
        }
        let read = if groups.is_empty() {
            notice(
                Tone::Info,
                Announce::Status,
                None,
                view! {
                    <code>{subject.typed().to_owned()}</code>
                    " "
                    {tr_with(
                        Msg::FindReadAs,
                        &[
                            ("subject", subject.label().into()),
                            ("version", request.version.label().into()),
                        ],
                    )}
                },
            )
            .into_any()
        } else {
            found_card(&request, &groups)
        };
        let offered = offers(&subject, &declared_by(&statement), request.version);
        view! {
            {head}
            {stack(
                view! {
                    {read}
                    {offers_card(&offered)}
                },
            )}
        }
        .into_any()
    })
}

/// The resources whose name or canonical holds what was typed, by kind.
fn found_card(request: &Request, groups: &[(&str, String, Vec<Listed>)]) -> AnyView {
    let sections = groups
        .iter()
        .map(|(resource_type, kind, found)| {
            let columns = [Column::text("resource", kind.clone()).role(Role::Title)];
            let rows = found
                .iter()
                .map(|resource| {
                    let href = match *resource_type {
                        CODE_SYSTEM => system_link(&resource.url, request.version),
                        VALUE_SET => resource_link(VALUE_SETS_PATH, &resource.id, request.version),
                        _ => resource_link(CONCEPT_MAPS_PATH, &resource.id, request.version),
                    };
                    let sub = view! { <code>{resource.url.clone()}</code> }.into_any();
                    view! { <tr>{title_cell(Some(&href), resource.label.clone(), Some(sub))}</tr> }
                })
                .collect_view()
                .into_any();
            panel_section(
                Some(SectionHead {
                    title: kind.clone(),
                    count: Some(u64::try_from(found.len()).unwrap_or(u64::MAX)),
                    actions: None,
                }),
                data_table(DataTable {
                    caption: kind,
                    columns: &columns,
                    sorting: Sorting::default(),
                    body: Body::Rows(rows),
                }),
            )
        })
        .collect_view();
    panel_sections_card(tr(Msg::FindMatches), sections)
}

/// Where what was typed can go: one link per screen that takes it.
fn offers_card(offered: &[Offer]) -> AnyView {
    let title = tr(Msg::WhereItGoes);
    let columns = [Column::text("screen", title.clone()).role(Role::Title)];
    let body = if offered.is_empty() {
        Body::Empty(view! { <p>{tr(Msg::RootDeclaresNoneOperationsWouldApply)}</p> }.into_any())
    } else {
        Body::Rows(
            offered
                .iter()
                .map(|offer| {
                    let sub = view! { {offer.detail.clone()} }.into_any();
                    view! { <tr>{title_cell(Some(&offer.href), offer.label.clone(), Some(sub))}</tr> }
                })
                .collect_view()
                .into_any(),
        )
    };
    panel_sections_card(
        title.clone(),
        panel_section(
            Some(SectionHead {
                title: title.clone(),
                count: Some(u64::try_from(offered.len()).unwrap_or(u64::MAX)),
                actions: None,
            }),
            data_table(DataTable {
                caption: &title,
                columns: &columns,
                sorting: Sorting::default(),
                body,
            }),
        ),
    )
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
