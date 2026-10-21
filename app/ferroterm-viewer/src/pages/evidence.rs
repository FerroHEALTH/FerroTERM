// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The evidence screen: what this build passes, and what it measured.
//!
//! Every other screen reads the running server. This one reads nothing: the
//! figures were taken out of the repository's committed files when the bundle
//! was built, so they describe the build a deployment is running rather than
//! the deployment itself. Each figure names the file it came from.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, Column, DataTable, NUM, data_table};
use ferrohealth_ui::view::chart::proportion;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::surface::{
    Stat, definitions, disclosure, stat_card, stat_grid, titled_panel,
};
use leptos::prelude::*;

use crate::components::not_declared;
use crate::evidence::Conformance;
use crate::evidence::Latency;
use crate::evidence::Run;
use crate::evidence::SystemRun;
use crate::evidence::embedded;
use crate::evidence::relative_width;
use crate::locale::{Msg, tr, tr_with};

/// The columns of the conformance table.
fn mode_columns() -> [Column; 5] {
    [
        Column::text("mode", tr(Msg::Mode)),
        Column::text("surface", tr(Msg::Surface)),
        Column::text("passed", tr(Msg::Passed)).numeric(),
        Column::text("share", tr(Msg::Share)),
        Column::text("from", tr(Msg::From)),
    ]
}

/// The columns of the latency table.
fn bar_columns() -> [Column; 5] {
    [
        Column::text("bench", tr(Msg::Benchmark)),
        Column::text("bar", tr(Msg::Bar)).numeric(),
        Column::text("measured", tr(Msg::Measured)),
        Column::text("room", tr(Msg::Room)),
        Column::text("claim", tr(Msg::WhatTheBarClaims)),
    ]
}

/// The columns of one system's timing table.
fn timing_columns() -> [Column; 7] {
    [
        Column::text("operation", tr(Msg::Operation)),
        Column::text("status", tr(Msg::Status)).numeric(),
        Column::text("cold", tr(Msg::Cold)).numeric(),
        Column::text("p50", tr(Msg::Median)).numeric(),
        Column::text("p95", "95th").numeric(),
        Column::text("p99", "99th").numeric(),
        Column::text("warm", tr(Msg::WarmRequests)).numeric(),
    ]
}

/// Draws the conformance and benchmark evidence the bundle carries.
///
/// Nothing here is reactive, and nothing here is fetched. The figures were
/// fixed when the bundle was built, so the screen draws a constant.
pub(crate) fn pane() -> AnyView {
    let evidence = embedded();
    let passed: u32 = evidence
        .conformance
        .modes
        .iter()
        .map(|mode| mode.passed)
        .sum();
    let ran: u32 = evidence.conformance.modes.iter().map(|mode| mode.ran).sum();
    let stats = stat_grid(view! {
        {stat_card(Stat {
            glyph: Glyph::LIST_CHECKS,
            value: tr_with(
                Msg::Of3,
                &[("passed", (passed).to_string().into()), ("ran", (ran).to_string().into())],
            ),
            label: tr(Msg::EcosystemSuiteCasesPass).into(),
            href: None,
        })}
        {stat_card(Stat {
            glyph: Glyph::GAUGE,
            value: evidence.latency.bars.len().to_string(),
            label: tr(Msg::LatencyBarsProjectClaims).into(),
            href: None,
        })}
        {stat_card(Stat {
            glyph: Glyph::DATABASE,
            value: evidence.run.systems.len().to_string(),
            label: tr(Msg::CodeSystemsNewestRun).into(),
            href: None,
        })}
    });
    view! {
        <p class="fui-muted">
            {tr(Msg::ReadFromFilesRepositoryCommittedFerroterm)} <strong>{evidence.release}</strong>
            {tr(Msg::NotFromServerAnsweringPageEach)}
        </p>
        {stats}
        {conformance_section(evidence.conformance)}
        {latency_section(evidence.latency)}
        {run_section(evidence.run)}
    }
    .into_any()
}

/// A section: the sentence a reader scans, the table behind a disclosure,
/// and the files the figures came from.
fn section(
    id: &'static str,
    title: String,
    lead: AnyView,
    summary: String,
    caption: String,
    table: AnyView,
    sources: AnyView,
) -> AnyView {
    let body = view! {
        <div class="fui-panel-body fui-stack">
            {lead} <p>
                <strong>{summary}</strong>
            </p> {disclosure(caption, table)} <p class="fui-small fui-muted">{sources}</p>
        </div>
    };
    view! { <div id=id>{titled_panel(title, (), body)}</div> }.into_any()
}

/// The HL7 terminology ecosystem suite, mode by mode.
fn conformance_section(conformance: Conformance) -> AnyView {
    let rows = conformance
        .modes
        .iter()
        .map(|mode| {
            let share = mode.share();
            view! {
                <tr>
                    <th scope="row" class="fui-mono">
                        {mode.name}
                    </th>
                    <td data-label=tr(Msg::Surface) class="fui-mono">
                        {mode.surface}
                    </td>
                    <td data-label=tr(Msg::Passed) class=NUM>
                        {mode.passed}
                        {tr(Msg::Of)}
                        {mode.ran}
                    </td>
                    <td data-label=tr(Msg::Share)>
                        <div class="fui-stack fui-gap-tight">
                            <span>{share} "%"</span>
                            {proportion(u8::try_from(share.min(100)).unwrap_or(100), None)}
                        </div>
                    </td>
                    <td data-label=tr(Msg::From) class="fui-mono fui-small fui-muted fui-break">
                        {mode.source}
                    </td>
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: &tr(Msg::CasesEachModePasses),
        columns: &mode_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any();
    let sources = view! {
        <span class="fui-mono">{conformance.table_source}</span>
        {tr(Msg::And2)}
        <span class="fui-mono">{conformance.total_source}</span>
    }
    .into_any();
    section(
        "conformance",
        tr(Msg::Hl7TerminologyEcosystemSuite),
        ().into_any(),
        conformance.summary(),
        tr(Msg::CasesEachModePassesFromCommitted),
        table,
        sources,
    )
}

/// The latency bars, and the run recorded against each.
fn latency_section(latency: Latency) -> AnyView {
    let widest = latency
        .bars
        .iter()
        .map(|bar| bar.measured_us)
        .max()
        .unwrap_or_default();
    let rows = latency
        .bars
        .iter()
        .map(|bar| {
            let headroom = bar.headroom().map_or_else(
                || tr(Msg::FasterThanRunCouldTime).clone(),
                |times| tr_with(Msg::TimesUnderIt, &[("times", (times).to_string().into())]),
            );
            let width = relative_width(bar.measured_us, widest).min(100);
            view! {
                <tr>
                    <th scope="row" class="fui-mono">
                        {bar.bench}
                    </th>
                    <td data-label=tr(Msg::Bar) class=NUM>
                        {bar.max_us}
                        {tr(Msg::S)}
                    </td>
                    <td data-label=tr(Msg::Measured)>
                        <div class="fui-stack fui-gap-tight">
                            <span>{bar.measured_us} "\u{a0}µs"</span>
                            {proportion(u8::try_from(width).unwrap_or(100), None)}
                        </div>
                    </td>
                    <td data-label=tr(Msg::Room)>{headroom}</td>
                    <td data-label=tr(Msg::WhatTheBarClaims)>{bar.claim}</td>
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: &tr(Msg::EachBenchmarkBarRunRecordedAgainst),
        columns: &bar_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any();
    let lead =
        view! { <p class="fui-muted">{tr(Msg::RecordedOn)} <strong>{latency.machine}</strong> "."</p> }
            .into_any();
    let sources = view! {
        <span class="fui-mono">{latency.source}</span>
        {tr(Msg::BarDrawnAgainstSlowestMeasurementBeside)}
    }
    .into_any();
    section(
        "latency",
        tr(Msg::LatencyProjectClaims),
        lead,
        latency.summary(),
        tr(Msg::EachBenchmarkBarRunRecordedAgainst),
        table,
        sources,
    )
}

/// The newest committed benchmark run, system by system.
fn run_section(run: Run) -> AnyView {
    let systems: Vec<AnyView> = run.systems.iter().map(system_view).collect();
    let lead =
        view! { <p class="fui-muted">{tr(Msg::From2)} <span class="fui-mono">{run.source}</span> "."</p> }
            .into_any();
    section(
        "run",
        tr(Msg::TheNewestBenchmarkRun),
        lead,
        run.summary(),
        tr(Msg::OneRecordPerCodeSystemRun),
        view! { <div class="fui-stack">{systems}</div> }.into_any(),
        ().into_any(),
    )
}

/// One code system's record within the run.
fn system_view(system: &SystemRun) -> AnyView {
    let ingest = system.ingest_seconds.map_or_else(
        || not_declared().clone(),
        |seconds| tr_with(Msg::S2, &[("seconds", (seconds).to_string().into())]),
    );
    let release = system
        .release
        .map_or_else(|| tr(Msg::BuiltOutsideThisRun), str::to_owned);
    let mono = |text: String| view! { <span class="fui-mono fui-break">{text}</span> }.into_any();
    let facts = definitions(vec![
        (tr(Msg::Concepts), system.concepts.into_any()),
        (tr(Msg::OfflineBuild), ingest.into_any()),
        (tr(Msg::BuiltFrom), mono(release.clone())),
        (
            tr(Msg::ReadyAfter),
            tr_with(
                Msg::S3,
                &[("ready_seconds", (system.ready_seconds).to_string().into())],
            )
            .into_any(),
        ),
        (tr(Msg::ResidentMemory), system.resident_memory.into_any()),
        (tr(Msg::ServedAs), format!("/{}", system.fhir).into_any()),
        (tr(Msg::AnsweredBy), system.built_by.into_any()),
        (tr(Msg::Machine), system.machine.into_any()),
        (
            tr(Msg::TakenAt),
            ferrohealth_ui::view::time::timestamp(system.taken_at).into_any(),
        ),
        (tr(Msg::Record), mono(system.source.to_owned())),
    ]);
    let title = format!("{} {}", system.system, system.system_version);
    let body = view! {
        <div class="fui-panel-body fui-stack">
            <p class="fui-mono fui-small fui-muted fui-break">{system.system_uri}</p>
            {facts}
        </div>
        {timings_view(system)}
    };
    titled_panel(title, (), body).into_any()
}

/// The operations the run timed, one row each.
fn timings_view(system: &SystemRun) -> AnyView {
    let body = if system.operations.is_empty() {
        Body::Empty(
            empty_quiet(
                Glyph::GAUGE,
                tr(Msg::NoTiming),
                tr(Msg::RecordTimedNoOperation),
            )
            .into_any(),
        )
    } else {
        Body::Rows(
            system
                .operations
                .iter()
                .map(|timing| {
                    view! {
                        <tr>
                            <th scope="row" class="fui-mono">
                                {timing.operation}
                            </th>
                            <td data-label=tr(Msg::Status) class=NUM>
                                {timing.status}
                            </td>
                            <td data-label=tr(Msg::Cold) class=NUM>
                                {timing.cold_ms}
                            </td>
                            <td data-label=tr(Msg::Median) class=NUM>
                                {timing.p50_ms}
                            </td>
                            <td data-label=tr(Msg::N95th) class=NUM>
                                {timing.p95_ms}
                            </td>
                            <td data-label=tr(Msg::N99th) class=NUM>
                                {timing.p99_ms}
                            </td>
                            <td data-label=tr(Msg::WarmRequests) class=NUM>
                                {timing.warm_requests}
                            </td>
                        </tr>
                    }
                })
                .collect_view()
                .into_any(),
        )
    };
    data_table(DataTable {
        caption: &tr(Msg::WhatSystemAnsweredMilliseconds),
        columns: &timing_columns(),
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}
