// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's own instruments, on the meter of `ferrohealth-telemetry`.
//!
//! The request histogram is the crate's (`http.server.request.duration`);
//! these are the terminology facts beside it: the code system versions
//! served and the reloads of the served set. Prometheus reads them as
//! `ferroterm_code_system_loaded` and `ferroterm_reloads_total`. No FHIR
//! specification governs this: our own design.

use std::sync::{Arc, Mutex, PoisonError};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Meter, ObservableGauge};

/// The scope of the server's instruments on the shared meter provider.
pub const SCOPE: &str = "ferroterm";

/// How a reload of the served set ended.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Outcome {
    /// The new set was built and swapped in.
    Ok,
    /// The new set did not build, and the old one still answers.
    Failed,
}

impl Outcome {
    /// The attribute value, the lower-case word a dashboard selects on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

/// The instruments of one running server.
#[derive(Debug)]
pub struct Metrics {
    served: Arc<Mutex<Vec<(String, String)>>>,
    reloads: Counter<u64>,
    // NOTE: the gauge reports through its callback; the handle is held so the
    // instrument lives as long as the server (<https://docs.rs/opentelemetry/0.33.1/opentelemetry/metrics/struct.ObservableGauge.html>).
    _loaded: ObservableGauge<u64>,
}

impl Metrics {
    /// The instruments on `meter`, serving nothing yet.
    #[must_use]
    pub fn new(meter: &Meter) -> Self {
        let served: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let read = Arc::clone(&served);
        let loaded = meter
            .u64_observable_gauge("ferroterm.code_system.loaded")
            .with_description("One per code system version the server loaded")
            .with_callback(move |observer| {
                let served = read.lock().unwrap_or_else(PoisonError::into_inner);
                for (system, version) in served.iter() {
                    observer.observe(
                        1,
                        &[
                            KeyValue::new("system", system.clone()),
                            KeyValue::new("version", version.clone()),
                        ],
                    );
                }
            })
            .build();
        let reloads = meter
            .u64_counter("ferroterm.reloads")
            .with_description("The reloads of the served set, by outcome")
            .build();
        // Both series start at zero, so a dashboard reads a rate from the first
        // scrape instead of waiting for a reload to create the series
        // (<https://prometheus.io/docs/practices/instrumentation/#avoid-missing-metrics>).
        for outcome in [Outcome::Ok, Outcome::Failed] {
            reloads.add(0, &[KeyValue::new("outcome", outcome.as_str())]);
        }
        Self {
            served,
            reloads,
            _loaded: loaded,
        }
    }

    /// Declares the code system versions the server serves, replacing every
    /// version it served before.
    ///
    /// A reload swaps the whole set, so a version the new set does not carry
    /// leaves the exposition rather than staying at `1` forever.
    pub fn serving<'a>(&self, systems: impl IntoIterator<Item = (&'a str, &'a str)>) {
        let next = systems
            .into_iter()
            .map(|(system, version)| (system.to_owned(), version.to_owned()))
            .collect();
        *self.served.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }

    /// Counts one reload of the served set.
    pub fn reloaded(&self, outcome: Outcome) {
        self.reloads
            .add(1, &[KeyValue::new("outcome", outcome.as_str())]);
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_telemetry::section::TelemetrySection;
    use ferrohealth_telemetry::telemetry::{Service, Telemetry};

    use super::{Metrics, Outcome, SCOPE};

    fn telemetry() -> Telemetry {
        Telemetry::new(
            &TelemetrySection::default(),
            Service::new("ferroterm", "test"),
        )
        .expect("the default section builds")
    }

    #[test]
    fn the_exposition_carries_the_recorded_series() {
        let telemetry = telemetry();
        let metrics = Metrics::new(&telemetry.meter(SCOPE));
        metrics.serving([(
            "http://snomed.info/sct",
            "http://snomed.info/sct/1/version/2",
        )]);
        let text = telemetry.render_prometheus().expect("encodes");
        assert!(
            text.contains("ferroterm_code_system_loaded{system=\"http://snomed.info/sct\""),
            "{text}"
        );
        assert!(
            text.contains("ferroterm_reloads_total{outcome=\"ok\"} 0"),
            "{text}"
        );
        assert!(
            text.contains("ferroterm_reloads_total{outcome=\"failed\"} 0"),
            "{text}"
        );
    }

    #[test]
    fn a_swapped_set_replaces_the_loaded_series_and_counts_the_reload() {
        let telemetry = telemetry();
        let metrics = Metrics::new(&telemetry.meter(SCOPE));
        metrics.serving([("http://example.org/first", "1")]);
        metrics.serving([("http://example.org/second", "2")]);
        metrics.reloaded(Outcome::Ok);
        metrics.reloaded(Outcome::Failed);
        let text = telemetry.render_prometheus().expect("encodes");
        assert!(
            !text.contains("http://example.org/first"),
            "a dropped system leaves the exposition: {text}"
        );
        assert!(text.contains("http://example.org/second"), "{text}");
        assert!(
            text.contains("ferroterm_reloads_total{outcome=\"ok\"} 1"),
            "{text}"
        );
        assert!(
            text.contains("ferroterm_reloads_total{outcome=\"failed\"} 1"),
            "{text}"
        );
    }
}
