//! The four served FHIR roots, side by side.
//!
//! One process answers `/r4`, `/r4b`, `/r5`, and `/r6`, each with its own
//! generated operation set, so the same request is answered differently per
//! root. This screen reads the four `CapabilityStatement` documents and shows
//! the differences it finds in them. It names no version's behaviour of its
//! own: a change in a vendored package changes what the roots declare, and the
//! screen follows with no edit here.

use leptos::prelude::*;

use crate::comparison::Cell;
use crate::comparison::Comparison;
use crate::comparison::RootAnswer;
use crate::comparison::RootColumn;
use crate::comparison::Row;
use crate::comparison::compare;
use crate::comparison::notes;
use crate::comparison::state_sentence;
use crate::components::failure::Failure;
use crate::components::icon;
use crate::components::not_declared;
use crate::components::reading::Reading;
use crate::fhir::FhirClient;
use crate::fhir::capability::CapabilityStatement;
use crate::fhir::error::FhirError;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table};
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::surface::{card, disclosure, panel_body, titled_panel};

/// What one root answered, before the screen has read it.
type RootRead = LocalResource<Result<CapabilityStatement, FhirError>>;

/// Reads the four roots and draws what they declare, side by side.
///
/// The four reads are issued together: a `LocalResource` begins running its
/// future when it is created (`leptos_server` 0.8.7 `local_resource.rs`, which
/// documents `ArcLocalResource::new` as loading on the client), so creating
/// them in one setup body puts the four requests in flight at once. They are
/// then read inside one `<Reading>`, so the comparison appears complete rather
/// than filling in column by column.
///
/// Nothing on this screen depends on the version switcher: it reads every
/// root, so no read refetches and none goes stale.
pub(crate) fn pane() -> AnyView {
    let client = expect_context::<FhirClient>();
    let roots: [RootRead; 4] = FhirVersion::ALL.map(|version| {
        let client = client.clone();
        LocalResource::new(move || {
            let client = client.clone();
            async move { client.capability_statement(version).await }
        })
    });

    let comparison = comparison_section(roots);

    view! { {comparison} }.into_any()
}

/// The four documents, read together and drawn as one comparison.
fn comparison_section(roots: [RootRead; 4]) -> AnyView {
    // The live region is built outside the boundary, so it settles with the
    // read rather than with the fallback it replaces.
    let announcement = Memo::new(move |_| state_sentence(&answers(roots)));

    view! {
        <section class="fui-stack" aria-labelledby="comparison-heading">
            <div>
                <h3 id="comparison-heading" class="fui-card-title">
                    {tr(Msg::WhatEachRootDeclares)}
                </h3>
                <p aria-live="polite" class="fui-muted fui-small">
                    {announcement}
                </p>
            </div>
            <Reading label=&{tr(Msg::ReadingFourCapabilityStatements)}>
                {move || drawn(&answers(roots))}
            </Reading>
        </section>
    }
    .into_any()
}

/// What the four roots have answered so far, in release order.
fn answers(roots: [RootRead; 4]) -> Vec<(FhirVersion, RootAnswer)> {
    FhirVersion::ALL
        .into_iter()
        .zip(roots)
        .map(|(version, read)| {
            let answer = read
                .with(|answered| {
                    answered
                        .as_ref()
                        .map(|result| RootAnswer::read(result.clone()))
                })
                .unwrap_or(RootAnswer::Unread);
            (version, answer)
        })
        .collect()
}

/// The comparison itself: the two tables, the refusals, and the notes.
fn drawn(answers: &[(FhirVersion, RootAnswer)]) -> AnyView {
    let comparison = compare(answers);
    let refused = refusals_view(answers);
    let facts = table_view(
        &tr(Msg::WhatEachRootSaysAboutItself),
        &tr(Msg::Declaration),
        &comparison.columns,
        &comparison.facts,
    );
    let operations = operations_view(&comparison);
    let added = notes_view(answers);
    view! {
        {refused}
        {titled_panel(tr(Msg::WhatEachRootSaysAboutItself), (), facts)}
        {operations}
        {added}
    }
    .into_any()
}

/// The roots that refused, each in the server's own words.
///
/// A refusal is drawn beside the comparison rather than in place of it, so one
/// root that will not answer never blanks the other three.
fn refusals_view(answers: &[(FhirVersion, RootAnswer)]) -> AnyView {
    let drawn: Vec<AnyView> = answers
        .iter()
        .filter_map(|(version, answer)| {
            let error = answer.failure()?.clone();
            Some(
                view! {
                    <div>
                        <p class="fui-label">{version.label()} {tr(Msg::DidNotAnswer)}</p>
                        <Failure error=Signal::stored(error) />
                    </div>
                }
                .into_any(),
            )
        })
        .collect();
    if drawn.is_empty() {
        return ().into_any();
    }
    view! { <div class="fui-stack">{drawn}</div> }.into_any()
}

/// The operations table, with the sentence that reads its shape.
fn operations_view(comparison: &Comparison) -> AnyView {
    let table = if comparison.operations.is_empty() {
        panel_body(empty_quiet(
            icon::VERSION,
            tr(Msg::NoOperation),
            tr(Msg::NoRootAnsweredDeclaresOperation),
        ))
        .into_any()
    } else {
        table_view(
            &tr(Msg::OperationsLevelsEachAnswers),
            &tr(Msg::Operation),
            &comparison.columns,
            &comparison.operations,
        )
    };
    titled_panel(tr(Msg::OperationsLevelsEachAnswers), (), table).into_any()
}

/// One comparison table: a row heading, then one cell per root.
///
/// The rows are a whole-value replacement with no per-row state, so they are a
/// plain `Vec`, which rebuilds every position when a read settles again. A
/// `<For>` key it retained would be moved rather than re-rendered, and a row
/// would keep the old answer under its new heading.
fn table_view(caption: &str, corner: &str, columns: &[RootColumn], rows: &[Row]) -> AnyView {
    let mut heads = vec![Column::owned("corner", corner)];
    heads.extend(columns.iter().map(|column| {
        let release = column
            .fhir_version
            .clone()
            .unwrap_or_else(|| not_declared().clone());
        Column::owned(
            column.version.segment(),
            tr_with(
                Msg::Fhir4,
                &[
                    ("column", (column.version.label()).to_string().into()),
                    ("release", (release).clone().into()),
                    ("state", (column.state).clone().into()),
                ],
            ),
        )
    }));
    let labels: Vec<&'static str> = columns
        .iter()
        .map(|column| column.version.label())
        .collect();
    data_table(DataTable {
        caption,
        columns: &heads,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(
            rows.iter()
                .map(|row| row_view(row, &labels))
                .collect_view()
                .into_any(),
        ),
    })
    .into_any()
}

/// One row: what it describes, then what each root says about it, each cell
/// named by its root's version for a stacked card.
fn row_view(row: &Row, labels: &[&'static str]) -> AnyView {
    let cells: Vec<AnyView> = row
        .cells
        .iter()
        .zip(labels.iter().copied())
        .map(|(cell, label)| view! { <td data-label=label>{cell_view(cell)}</td> }.into_any())
        .collect();
    // An operation is named by its code, which reads in the code face;
    // a declaration is named in words.
    let class = if row.label.contains('$') {
        "fui-mono wrap-break-word"
    } else {
        "wrap-break-word"
    };
    view! {
        <tr>
            <th scope="row" class=class>
                {row.label.clone()}
            </th>
            {cells}
        </tr>
    }
    .into_any()
}

/// One cell, in words, because a tint carries no meaning.
fn cell_view(cell: &Cell) -> AnyView {
    match cell {
        Cell::NotDeclared => view! { <span class="fui-muted">{cell_text(cell)}</span> }.into_any(),
        Cell::Stated(_) | Cell::Unread => cell_text(cell).into_any(),
    }
}

/// One cell, in words, because a tint carries no meaning.
fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::Stated(text) => text.clone(),
        Cell::NotDeclared => not_declared().clone(),
        Cell::Unread => tr(Msg::NoAnswer).clone(),
    }
}

/// What each root says it answers beyond the definition its release publishes.
fn notes_view(answers: &[(FhirVersion, RootAnswer)]) -> AnyView {
    let drawn: Vec<AnyView> = answers
        .iter()
        .map(|(version, answer)| root_notes_view(version.label(), &notes(answer)))
        .collect();
    let body = view! {
        <p class="fui-muted">{tr(Msg::OneRootSOwnOperationDocumentation)}</p>
        <div class="fui-stack">{drawn}</div>
    };
    card(tr(Msg::WhatEachRootAddsOwnDefinition), body).into_any()
}

/// One root's notes, closed until a reader opens them.
fn root_notes_view(label: &str, notes: &[(String, String)]) -> AnyView {
    let count = notes.len();
    let lines: Vec<AnyView> = notes
        .iter()
        .map(|(operation, note)| {
            view! {
                <dt class="fui-mono wrap-break-word">{operation.clone()}</dt>
                <dd>{note.clone()}</dd>
            }
            .into_any()
        })
        .collect();
    let body = if count == 0 {
        view! {
            <p class="fui-muted">
                {tr(Msg::RootAddedNothingDefinitionsReleasePublishes)}
            </p>
        }
        .into_any()
    } else {
        view! { <dl class="fui-facts">{lines}</dl> }.into_any()
    };
    disclosure(format!("{label} · {}", note_count(count)), body).into_any()
}

/// How many operations one root wrote a note on, as a phrase.
fn note_count(count: usize) -> String {
    if count == 1 {
        tr(Msg::N1OperationNote).clone()
    } else {
        tr_with(
            Msg::OperationsWithANote,
            &[("count", (count).to_string().into())],
        )
    }
}
