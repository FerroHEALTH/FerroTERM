// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Prometheus metrics a deployment scrapes, and the registry behind them.
//!
//! No FHIR specification governs this: our own design, kept off the FHIR base
//! path so a scrape is never a terminology request. The exposition follows the
//! `OpenMetrics` text format the Prometheus client writes
//! (<https://prometheus.io/docs/instrumenting/exposition_formats/>).

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue, LabelValueEncoder};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
use prometheus_client::registry::Registry;

/// The labels of one request: the matched route rather than the URI, so a
/// scrape has one series per operation instead of one per code.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Request {
    /// The HTTP method.
    pub method: Method,
    /// The matched route, `/r4b/CodeSystem/$lookup` rather than the URI.
    pub route: String,
    /// The status the server answered.
    pub status: u16,
}

/// The HTTP methods the server routes.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `DELETE`.
    Delete,
    /// Anything else, so an unrouted method still counts.
    Other,
}

impl From<&http::Method> for Method {
    fn from(method: &http::Method) -> Self {
        match *method {
            http::Method::GET => Self::Get,
            http::Method::POST => Self::Post,
            http::Method::PUT => Self::Put,
            http::Method::DELETE => Self::Delete,
            _ => Self::Other,
        }
    }
}

/// The labels of one loaded code system version.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct System {
    /// The system URI.
    pub system: String,
    /// The version served.
    pub version: String,
}

/// The labels of one reload of the served set.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Reload {
    /// How the reload ended.
    pub outcome: Outcome,
}

/// How a reload of the served set ended.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Outcome {
    /// The new set was built and swapped in.
    Ok,
    /// The new set did not build, and the old one still answers.
    Failed,
}

impl Outcome {
    /// The label value, the lower-case word a dashboard selects on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

impl EncodeLabelValue for Outcome {
    fn encode(&self, encoder: &mut LabelValueEncoder<'_>) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_str(), encoder)
    }
}

/// The metrics of one running server.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    requests: Family<Request, Counter>,
    durations: Family<Request, Histogram>,
    systems: Family<System, Gauge>,
    reloads: Family<Reload, Counter>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// The registry with every metric this server exposes, empty of samples.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = <Registry>::with_prefix("ferroterm");
        let requests = Family::<Request, Counter>::default();
        registry.register(
            "http_requests",
            "The requests answered, by route and status",
            requests.clone(),
        );
        // A point read is held to a millisecond and an expansion page to ten
        // (`docs/architecture.md`), so the buckets start below both and reach a
        // slow request without a long tail of empty ones.
        let durations = Family::<Request, Histogram>::new_with_constructor(|| {
            Histogram::new(exponential_buckets(0.000_5, 2.0, 12))
        });
        registry.register_with_unit(
            "http_request_duration",
            "How long the server took to answer, by route and status",
            prometheus_client::registry::Unit::Seconds,
            durations.clone(),
        );
        let systems = Family::<System, Gauge>::default();
        registry.register(
            "code_system_loaded",
            "One per code system version the server loaded",
            systems.clone(),
        );
        let reloads = Family::<Reload, Counter>::default();
        registry.register(
            "reloads",
            "The reloads of the served set, by outcome",
            reloads.clone(),
        );
        // Both series start at zero, so a dashboard reads a rate from the first
        // scrape instead of waiting for a reload to create the series
        // (<https://prometheus.io/docs/practices/instrumentation/#avoid-missing-metrics>).
        for outcome in [Outcome::Ok, Outcome::Failed] {
            let series = reloads.get_or_create(&Reload { outcome });
            drop(series);
        }
        Self {
            registry,
            requests,
            durations,
            systems,
            reloads,
        }
    }

    /// Records one answered request.
    pub fn record(&self, request: &Request, seconds: f64) {
        self.requests.get_or_create(request).inc();
        self.durations.get_or_create(request).observe(seconds);
    }

    /// Declares the code system versions the server serves, dropping the
    /// series of every version it served before.
    ///
    /// A reload swaps the whole set, so a version the new set does not carry
    /// leaves the exposition rather than staying at `1` forever.
    pub fn serving<'a>(&self, systems: impl IntoIterator<Item = (&'a str, &'a str)>) {
        self.systems.clear();
        for (system, version) in systems {
            self.systems
                .get_or_create(&System {
                    system: system.to_owned(),
                    version: version.to_owned(),
                })
                .set(1);
        }
    }

    /// Counts one reload of the served set.
    pub fn reloaded(&self, outcome: Outcome) {
        self.reloads.get_or_create(&Reload { outcome }).inc();
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
mod tests {
    use super::{Method, Metrics, Outcome, Request};

    #[test]
    fn the_exposition_carries_the_recorded_series() {
        let metrics = Metrics::new();
        metrics.serving([(
            "http://snomed.info/sct",
            "http://snomed.info/sct/1/version/2",
        )]);
        metrics.record(
            &Request {
                method: Method::Get,
                route: String::from("/r4b/CodeSystem/$lookup"),
                status: 200,
            },
            0.001_5,
        );
        let text = metrics.exposition().expect("encodes");
        assert!(
            text.contains(
                "ferroterm_http_requests_total{method=\"Get\",route=\"/r4b/CodeSystem/$lookup\",status=\"200\"} 1"
            ),
            "{text}"
        );
        assert!(
            text.contains("ferroterm_http_request_duration_seconds_count{"),
            "{text}"
        );
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
        let metrics = Metrics::new();
        metrics.serving([("http://example.org/first", "1")]);
        metrics.serving([("http://example.org/second", "2")]);
        metrics.reloaded(Outcome::Ok);
        metrics.reloaded(Outcome::Failed);
        let text = metrics.exposition().expect("encodes");
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
