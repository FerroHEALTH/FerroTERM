//! What the command bar was given, and what this root can do with it.
//!
//! The classification is the viewer's own reading of the string's shape, in
//! `crate::find`, and it happens before any request. What gates the offers is
//! the one request this screen makes: the root's `CapabilityStatement`, which
//! is where a server states the operations it answers per resource type
//! (<https://hl7.org/fhir/R5/capabilitystatement.html>). An operation it does
//! not declare is left out rather than offered and refused.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::components::failure::Failure;
use crate::components::reading::Reading;
use crate::components::shell::SelectedVersion;
use crate::fhir::CODE_SYSTEM;
use crate::fhir::CONCEPT_MAP;
use crate::fhir::FhirClient;
use crate::fhir::VALUE_SET;
use crate::fhir::capability::CapabilityStatement;
use crate::find::Declared;
use crate::find::Offer;
use crate::find::QUERY_PARAM;
use crate::find::Subject;
use crate::find::offers;
use crate::locale::{Msg, tr, tr_with};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::empty::empty_state;
use ferrohealth_ui::view::surface::{facts_row, link_card, page_header, panel, stack};

/// The `$expand` operation, as a capability statement names it.
const EXPAND: &str = "expand";

/// The `$validate-code` operation, as a capability statement names it.
const VALIDATE_CODE: &str = "validate-code";

/// The `$translate` operation, as a capability statement names it.
const TRANSLATE: &str = "translate";

/// Shows what the command bar was given and where it can go.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and a binary crate has no reachable public API"
)]
pub(crate) fn FindPage() -> impl IntoView {
    let client = expect_context::<FhirClient>();
    let SelectedVersion(version) = expect_context::<SelectedVersion>();
    let query = use_query_map();
    let typed = Memo::new(move |_| {
        query.with(|map| map.get(QUERY_PARAM).unwrap_or_default().trim().to_owned())
    });
    let subject = Memo::new(move |_| typed.with(|typed| Subject::of(typed)));

    let read_client = client.clone();
    let statement = Resource::new(
        move || version.get(),
        move |version| {
            let client = read_client.clone();
            async move { client.capability_statement(version).await }
        },
    );

    let heading = view! {
        <Title text=tr(Msg::TitleFind) />
        {move || {
            let line = subject
                .with(|subject| match subject {
                    Some(subject) => tr_with(Msg::ReadFromShapeAlone, &[("subject", (subject.label()).clone().into())]),
                    None => tr(Msg::NothingWasTyped).clone(),
                });
            page_header(tr(Msg::WhatDo), Some(line), ())
        }}
    }
    .into_any();

    let subject_view = move || {
        subject.with(|subject| {
            subject.as_ref().map(|subject| {
                panel(facts_row(vec![(
                    tr(Msg::WhatYouTyped),
                    view! { <span class="fui-mono break-all">{subject.typed().to_owned()}</span> }
                        .into_any(),
                )]))
                .into_any()
            })
        })
    };

    let body = move || {
        statement.with(|answered| {
            answered.as_ref().map(|result| match result {
                Ok(statement) => subject.with(|subject| match subject {
                    Some(subject) => {
                        offers_view(&offers(subject, declared_by(statement), version.get()))
                    }
                    None => ().into_any(),
                }),
                Err(error) => {
                    let error = error.clone();
                    view! { <Failure error=Signal::stored(error) /> }.into_any()
                }
            })
        })
    };

    view! {
        {heading}
        {stack(
            view! {
                {subject_view}
                <section class="fui-stack" aria-labelledby="find-offers-heading">
                    <h2 id="find-offers-heading" class="fui-card-title">
                        {tr(Msg::WhereItGoes)}
                    </h2>
                    <Reading label=tr(Msg::ReadingWhatRootDeclares)>{body}</Reading>
                </section>
            },
        )}
    }
}

/// What the root declares, as the offers read it.
fn declared_by(statement: &CapabilityStatement) -> Declared {
    Declared {
        validate: statement.declares_operation(CODE_SYSTEM, VALIDATE_CODE)
            || statement.declares_operation(VALUE_SET, VALIDATE_CODE),
        expand: statement.declares_operation(VALUE_SET, EXPAND),
        translate: statement.declares_operation(CONCEPT_MAP, TRANSLATE),
    }
}

/// The offers, as the list of links they are.
fn offers_view(found: &[Offer]) -> AnyView {
    if found.is_empty() {
        return panel(empty_state(
            Glyph::SEARCH_X,
            tr(Msg::NowhereToGo),
            tr(Msg::RootDeclaresNoneOperationsWouldApply),
            (),
        ))
        .into_any();
    }
    let rows: Vec<AnyView> = found
        .iter()
        .map(|offer| {
            let card = link_card(
                offer.href.clone(),
                offer.label.clone(),
                view! { <p class="fui-muted">{offer.detail.clone()}</p> },
            );
            view! { <li class="grid">{card}</li> }.into_any()
        })
        .collect();
    view! { <ul class="fui-grid sm:grid-cols-2 xl:grid-cols-4">{rows}</ul> }.into_any()
}
