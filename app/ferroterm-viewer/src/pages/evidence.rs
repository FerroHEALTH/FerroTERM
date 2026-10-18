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

use crate::components::NOT_DECLARED;
use crate::evidence::Conformance;
use crate::evidence::Latency;
use crate::evidence::Run;
use crate::evidence::SystemRun;
use crate::evidence::embedded;
use crate::evidence::relative_width;

/// The columns of the conformance table.
const MODE_COLUMNS: [Column; 5] = [
    Column::text("mode", "Mode"),
    Column::text("surface", "Surface"),
    Column::text("passed", "Passed").numeric(),
    Column::text("share", "Share"),
    Column::text("from", "From"),
];

/// The columns of the latency table.
const BAR_COLUMNS: [Column; 5] = [
    Column::text("bench", "Benchmark"),
    Column::text("bar", "Bar").numeric(),
    Column::text("measured", "Measured"),
    Column::text("room", "Room"),
    Column::text("claim", "What the bar claims"),
];

/// The columns of one system's timing table.
const TIMING_COLUMNS: [Column; 7] = [
    Column::text("operation", "Operation"),
    Column::text("status", "Status").numeric(),
    Column::text("cold", "Cold").numeric(),
    Column::text("p50", "Median").numeric(),
    Column::text("p95", "95th").numeric(),
    Column::text("p99", "99th").numeric(),
    Column::text("warm", "Warm requests").numeric(),
];

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
            value: format!("{passed} of {ran}"),
            label: "Ecosystem suite cases pass".into(),
            href: None,
        })}
        {stat_card(Stat {
            glyph: Glyph::GAUGE,
            value: evidence.latency.bars.len().to_string(),
            label: "Latency bars the project claims".into(),
            href: None,
        })}
        {stat_card(Stat {
            glyph: Glyph::DATABASE,
            value: evidence.run.systems.len().to_string(),
            label: "Code systems in the newest run".into(),
            href: None,
        })}
    });
    view! {
        <p class="fui-muted">
            "Read from files this repository committed for FerroTERM "
            <strong>{evidence.release}</strong>
            ", not from the server answering this page. Each figure names its file."
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
    title: &'static str,
    lead: AnyView,
    summary: String,
    caption: &'static str,
    table: AnyView,
    sources: AnyView,
) -> AnyView {
    let body = view! {
        <div class="fui-panel-body fui-stack">
            {lead} <p class="text-title font-medium">{summary}</p> {disclosure(caption, table)}
            <p class="fui-small fui-muted">{sources}</p>
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
                    <td data-label="Surface" class="fui-mono">
                        {mode.surface}
                    </td>
                    <td data-label="Passed" class=NUM>
                        {mode.passed}
                        " of "
                        {mode.ran}
                    </td>
                    <td data-label="Share">
                        <div class="grid gap-tight">
                            <span class="tabular-nums">{share} "%"</span>
                            {proportion(u8::try_from(share.min(100)).unwrap_or(100), None)}
                        </div>
                    </td>
                    <td data-label="From" class="fui-mono fui-small fui-muted break-all">
                        {mode.source}
                    </td>
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: "The cases each mode passes",
        columns: &MODE_COLUMNS,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any();
    let sources = view! {
        <span class="fui-mono">{conformance.table_source}</span>
        " and "
        <span class="fui-mono">{conformance.total_source}</span>
    }
    .into_any();
    section(
        "conformance",
        "The HL7 terminology ecosystem suite",
        ().into_any(),
        conformance.summary(),
        "The cases each mode passes, from the committed pass lists",
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
                || "faster than the run could time".to_owned(),
                |times| format!("{times} times under it"),
            );
            let width = relative_width(bar.measured_us, widest).min(100);
            view! {
                <tr>
                    <th scope="row" class="fui-mono">
                        {bar.bench}
                    </th>
                    <td data-label="Bar" class=NUM>
                        {bar.max_us}
                        " µs"
                    </td>
                    <td data-label="Measured">
                        <div class="grid gap-tight">
                            <span class="tabular-nums whitespace-nowrap">
                                {bar.measured_us} " µs"
                            </span>
                            {proportion(u8::try_from(width).unwrap_or(100), None)}
                        </div>
                    </td>
                    <td data-label="Room" class="whitespace-nowrap">
                        {headroom}
                    </td>
                    <td data-label="What the bar claims">{bar.claim}</td>
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: "Each benchmark, its bar, and the run recorded against it",
        columns: &BAR_COLUMNS,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any();
    let lead =
        view! { <p class="fui-muted">"Recorded on " <strong>{latency.machine}</strong> "."</p> }
            .into_any();
    let sources = view! {
        <span class="fui-mono">{latency.source}</span>
        ". A bar is drawn against the slowest measurement beside it."
    }
    .into_any();
    section(
        "latency",
        "The latency the project claims",
        lead,
        latency.summary(),
        "Each benchmark, its bar, and the run recorded against it",
        table,
        sources,
    )
}

/// The newest committed benchmark run, system by system.
fn run_section(run: Run) -> AnyView {
    let systems: Vec<AnyView> = run.systems.iter().map(system_view).collect();
    let lead =
        view! { <p class="fui-muted">"From " <span class="fui-mono">{run.source}</span> "."</p> }
            .into_any();
    section(
        "run",
        "The newest benchmark run",
        lead,
        run.summary(),
        "One record per code system the run loaded",
        view! { <div class="fui-stack">{systems}</div> }.into_any(),
        ().into_any(),
    )
}

/// One code system's record within the run.
fn system_view(system: &SystemRun) -> AnyView {
    let ingest = system
        .ingest_seconds
        .map_or_else(|| NOT_DECLARED.to_owned(), |seconds| format!("{seconds} s"));
    let release = system.release.unwrap_or("built outside this run");
    let mono =
        |text: &'static str| view! { <span class="fui-mono break-all">{text}</span> }.into_any();
    let facts = definitions(vec![
        ("Concepts", system.concepts.into_any()),
        ("Offline build", ingest.into_any()),
        ("Built from", mono(release)),
        (
            "Ready after",
            format!("{} s", system.ready_seconds).into_any(),
        ),
        ("Resident memory", system.resident_memory.into_any()),
        ("Served as", format!("/{}", system.fhir).into_any()),
        ("Answered by", system.built_by.into_any()),
        ("Machine", system.machine.into_any()),
        ("Taken at", mono(system.taken_at)),
        ("Record", mono(system.source)),
    ]);
    let title = format!("{} {}", system.system, system.system_version);
    let body = view! {
        <div class="fui-panel-body fui-stack">
            <p class="fui-mono fui-small fui-muted break-all">{system.system_uri}</p>
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
            empty_quiet(Glyph::GAUGE, "No timing", "This record timed no operation.").into_any(),
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
                            <td data-label="Status" class=NUM>
                                {timing.status}
                            </td>
                            <td data-label="Cold" class=NUM>
                                {timing.cold_ms}
                            </td>
                            <td data-label="Median" class=NUM>
                                {timing.p50_ms}
                            </td>
                            <td data-label="95th" class=NUM>
                                {timing.p95_ms}
                            </td>
                            <td data-label="99th" class=NUM>
                                {timing.p99_ms}
                            </td>
                            <td data-label="Warm requests" class=NUM>
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
        caption: "What the system answered, in milliseconds",
        columns: &TIMING_COLUMNS,
        sort: None,
        sort_href: &|_| String::new(),
        body,
    })
    .into_any()
}
