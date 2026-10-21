// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Prometheus metrics the service exposes at `/metrics`.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. The
//! exposition follows the `OpenMetrics` text format the Prometheus client
//! writes (<https://prometheus.io/docs/instrumenting/exposition_formats/>).

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue, LabelValueEncoder};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::{Registry, Unit};

use crate::record::{Outcome, RunRecord};

/// The labels of one finished run.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Run {
    /// How the run ended.
    pub outcome: Outcome,
}

impl EncodeLabelValue for Outcome {
    fn encode(&self, encoder: &mut LabelValueEncoder<'_>) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_str(), encoder)
    }
}

/// The labels of one locally authored resource that was revalidated.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Local {
    /// The resource, as its run record names it.
    pub resource: String,
}

/// The metrics of one running service.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    runs: Family<Run, Counter>,
    last_run: Gauge,
    entries_taken: Counter,
    bytes_staged: Counter,
    index_bytes: Gauge,
    revalidation: Family<Local, Gauge>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// The registry with every metric this service exposes, empty of samples.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = <Registry>::with_prefix("ferroterm_sync");
        let runs = Family::<Run, Counter>::default();
        registry.register("runs", "The runs that finished, by outcome", runs.clone());
        let last_run = Gauge::default();
        registry.register_with_unit(
            "last_run_timestamp",
            "When the last run finished",
            Unit::Seconds,
            last_run.clone(),
        );
        let entries_taken = Counter::default();
        registry.register(
            "entries_taken",
            "The feed entries the service fetched",
            entries_taken.clone(),
        );
        let bytes_staged = Counter::default();
        registry.register(
            "bytes_staged",
            "The bytes the service downloaded into staging",
            bytes_staged.clone(),
        );
        let index_bytes = Gauge::default();
        registry.register(
            "index_bytes",
            "The bytes the index root holds after the last run",
            index_bytes.clone(),
        );
        // Both series start at zero, so a dashboard reads a rate from the first
        // scrape instead of waiting for a run to create the series
        // (<https://prometheus.io/docs/practices/instrumentation/#avoid-missing-metrics>).
        for outcome in [Outcome::Ok, Outcome::Failed] {
            let series = runs.get_or_create(&Run { outcome });
            drop(series);
        }
        let revalidation = Family::<Local, Gauge>::default();
        registry.register(
            "revalidation_findings",
            "The local codes the last run's release changed, per locally authored resource",
            revalidation.clone(),
        );
        Self {
            registry,
            runs,
            last_run,
            entries_taken,
            bytes_staged,
            index_bytes,
            revalidation,
        }
    }

    /// Records one finished run.
    pub fn recorded(&self, record: &RunRecord) {
        self.runs
            .get_or_create(&Run {
                outcome: record.outcome,
            })
            .inc();
        self.last_run.set(record.finished.as_second());
        self.entries_taken
            .inc_by(u64::try_from(record.entries_taken).unwrap_or(u64::MAX));
        self.bytes_staged.inc_by(record.bytes_staged);
        self.index_bytes
            .set(i64::try_from(record.bytes_used).unwrap_or(i64::MAX));
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
        self.revalidation.clear();
        for resource in &record.revalidation.checked_resources {
            let findings = record
                .revalidation
                .findings
                .iter()
                .filter(|finding| &finding.resource == resource)
                .count();
            self.revalidation
                .get_or_create(&Local {
                    resource: resource.clone(),
                })
                .set(i64::try_from(findings).unwrap_or(i64::MAX));
        }
    }

    /// The exposition text a scrape reads.
    ///
    /// # Errors
    ///
    /// Returns the formatting error when the registry cannot be written, which
    /// means a metric name or label is not encodable.
    pub fn exposition(&self) -> Result<String, std::fmt::Error> {
        let mut out = String::new();
        encode(&mut out, &self.registry)?;
        Ok(out)
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::Metrics;
    use crate::record::{Outcome, RunRecord, Trigger};

    #[test]
    fn the_exposition_carries_every_series() -> Result<(), Box<dyn core::error::Error>> {
        let metrics = Metrics::new();
        let started: jiff::Timestamp = "2026-09-22T03:00:00Z".parse()?;
        let mut record = RunRecord::new(String::from("r"), Trigger::Schedule, started);
        record.bytes_used = 42;
        record.close("2026-09-22T03:00:01Z".parse()?);
        metrics.recorded(&record);
        let text = metrics.exposition()?;
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
        let metrics = Metrics::new();
        let started: jiff::Timestamp = "2026-09-22T03:00:00Z".parse()?;
        let mut record = RunRecord::new(String::from("r"), Trigger::Manual, started);
        record.fail(String::from("the build did not finish"));
        record.close(started);
        metrics.recorded(&record);
        assert_eq!(record.outcome, Outcome::Failed, "the run failed");
        let text = metrics.exposition()?;
        assert!(
            text.contains("ferroterm_sync_runs_total{outcome=\"failed\"} 1"),
            "the failure is counted: {text}"
        );
        Ok(())
    }
}
