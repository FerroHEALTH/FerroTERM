// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's own instruments, on the meter of `ferrohealth-telemetry`,
//! which `GET /metrics` exposes for Prometheus.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. The
//! names read as `ferroterm_sync_runs_total`,
//! `ferroterm_sync_last_run_timestamp_seconds`,
//! `ferroterm_sync_entries_taken_total`, `ferroterm_sync_bytes_staged_total`,
//! `ferroterm_sync_index_bytes` and `ferroterm_sync_revalidation_findings`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, Meter, ObservableGauge};

use crate::record::{Outcome, RunRecord};

/// The scope of the service's instruments on the shared meter provider.
pub const SCOPE: &str = "ferroterm-sync";

/// The instruments of one running service.
#[derive(Debug)]
pub struct Metrics {
    runs: Counter<u64>,
    last_run: Gauge<i64>,
    entries_taken: Counter<u64>,
    bytes_staged: Counter<u64>,
    index_bytes: Gauge<u64>,
    findings: Arc<Mutex<BTreeMap<String, u64>>>,
    // NOTE: the gauge reports through its callback; the handle is held so the
    // instrument lives as long as the service (<https://docs.rs/opentelemetry/0.33.1/opentelemetry/metrics/struct.ObservableGauge.html>).
    _revalidation: ObservableGauge<u64>,
}

impl Metrics {
    /// The instruments on `meter`, empty of samples but the two run outcomes.
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        let runs = meter
            .u64_counter("ferroterm_sync.runs")
            .with_description("The runs that finished, by outcome")
            .build();
        let last_run = meter
            .i64_gauge("ferroterm_sync.last_run_timestamp")
            .with_description("When the last run finished")
            .with_unit("s")
            .build();
        let entries_taken = meter
            .u64_counter("ferroterm_sync.entries_taken")
            .with_description("The feed entries the service fetched")
            .build();
        let bytes_staged = meter
            .u64_counter("ferroterm_sync.bytes_staged")
            .with_description("The bytes the service downloaded into staging")
            .build();
        let index_bytes = meter
            .u64_gauge("ferroterm_sync.index_bytes")
            .with_description("The bytes the index root holds after the last run")
            .build();
        // Both series start at zero, so a dashboard reads a rate from the first
        // scrape instead of waiting for a run to create the series
        // (<https://prometheus.io/docs/practices/instrumentation/#avoid-missing-metrics>).
        for outcome in [Outcome::Ok, Outcome::Failed] {
            runs.add(0, &[KeyValue::new("outcome", outcome.as_str())]);
        }
        let findings: Arc<Mutex<BTreeMap<String, u64>>> = Arc::default();
        let read = Arc::clone(&findings);
        let revalidation = meter
            .u64_observable_gauge("ferroterm_sync.revalidation_findings")
            .with_description(
                "The local codes the last run's release changed, per locally authored resource",
            )
            .with_callback(move |observer| {
                let findings = read.lock().unwrap_or_else(PoisonError::into_inner);
                for (resource, count) in findings.iter() {
                    observer.observe(*count, &[KeyValue::new("resource", resource.clone())]);
                }
            })
            .build();
        Self {
            runs,
            last_run,
            entries_taken,
            bytes_staged,
            index_bytes,
            findings,
            _revalidation: revalidation,
        }
    }

    /// Records one finished run.
    pub fn recorded(&self, record: &RunRecord) {
        self.runs
            .add(1, &[KeyValue::new("outcome", record.outcome.as_str())]);
        self.last_run.record(record.finished.as_second(), &[]);
        self.entries_taken
            .add(u64::try_from(record.entries_taken).unwrap_or(u64::MAX), &[]);
        self.bytes_staged.add(record.bytes_staged, &[]);
        self.index_bytes.record(record.bytes_used, &[]);
        self.revalidated(record);
    }

    /// Declares the findings of the last revalidation, one series per resource.
    ///
    /// A run that revalidated nothing leaves the series of the run before it
    /// in place, because the last run that did check is what an operator wants
    /// to see; a run that did check replaces them all.
    fn revalidated(&self, record: &RunRecord) {
        if record.revalidation.resources == 0 {
            return;
        }
        let next = record
            .revalidation
            .checked_resources
            .iter()
            .map(|resource| {
                let findings = record
                    .revalidation
                    .findings
                    .iter()
                    .filter(|finding| &finding.resource == resource)
                    .count();
                (
                    resource.clone(),
                    u64::try_from(findings).unwrap_or(u64::MAX),
                )
            })
            .collect();
        *self.findings.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use ferrohealth_telemetry::section::TelemetrySection;
    use ferrohealth_telemetry::telemetry::{Service, Telemetry};

    use super::{Metrics, SCOPE};
    use crate::record::{Outcome, RunRecord, Trigger};

    fn telemetry() -> Result<Telemetry, Box<dyn core::error::Error>> {
        Ok(Telemetry::new(
            &TelemetrySection::default(),
            Service::new("ferroterm-sync", "test"),
        )?)
    }

    #[test]
    fn the_exposition_carries_every_series() -> Result<(), Box<dyn core::error::Error>> {
        let telemetry = telemetry()?;
        let metrics = Metrics::new(&telemetry.meter(SCOPE));
        let started: jiff::Timestamp = "2026-09-22T03:00:00Z".parse()?;
        let mut record = RunRecord::new(String::from("r"), Trigger::Schedule, started);
        record.bytes_used = 42;
        record.close("2026-09-22T03:00:01Z".parse()?);
        metrics.recorded(&record);
        let text = telemetry.render_prometheus()?;
        assert!(
            text.contains("ferroterm_sync_runs_total{outcome=\"ok\"} 1"),
            "a finished run counts under its outcome: {text}"
        );
        assert!(
            text.contains("ferroterm_sync_runs_total{outcome=\"failed\"} 0"),
            "the failed series exists before the first failure: {text}"
        );
        assert!(
            text.contains("ferroterm_sync_index_bytes 42"),
            "the index size is exposed: {text}"
        );
        assert!(
            text.contains("ferroterm_sync_last_run_timestamp_seconds"),
            "the last run is exposed: {text}"
        );
        Ok(())
    }

    #[test]
    fn a_failed_run_counts_as_failed() -> Result<(), Box<dyn core::error::Error>> {
        let telemetry = telemetry()?;
        let metrics = Metrics::new(&telemetry.meter(SCOPE));
        let started: jiff::Timestamp = "2026-09-22T03:00:00Z".parse()?;
        let mut record = RunRecord::new(String::from("r"), Trigger::Manual, started);
        record.fail(String::from("the build did not finish"));
        record.close(started);
        metrics.recorded(&record);
        assert_eq!(record.outcome, Outcome::Failed, "the run failed");
        let text = telemetry.render_prometheus()?;
        assert!(
            text.contains("ferroterm_sync_runs_total{outcome=\"failed\"} 1"),
            "the failure is counted: {text}"
        );
        Ok(())
    }
}
