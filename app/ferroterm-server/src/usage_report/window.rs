//! The usage report's metrics window: latency histograms per operation group
//! and request counts per FHIR version, recorded by the request middleware.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. A
//! recording carries a matched route TEMPLATE (never a concrete path), the
//! method, a duration, and a status class; nothing of a request's parameters,
//! body, or caller reaches it, so nothing of them can reach a report.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use http::{Method, StatusCode};

use crate::usage_report::payload::{
    BUCKET_BOUNDS_MS, BUCKETS, Counters, Groups, MAX_COUNT, MAX_MS, Metrics, OperationStats,
    RequestsBucket, Window24h,
};

/// An operation group of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// `$lookup`.
    Lookup,
    /// `$validate-code`.
    ValidateCode,
    /// `$expand`.
    Expand,
    /// `$subsumes`.
    Subsumes,
    /// `$translate`.
    Translate,
    /// `$closure`.
    Closure,
    /// `/metadata`.
    Metadata,
    /// A read, a history read, or a version read.
    Read,
    /// A type-level search.
    Search,
    /// A create, update, or delete.
    Write,
    /// Every other request under a FHIR base.
    Other,
}

impl Group {
    /// Every group, in storage order.
    pub const ALL: [Self; 11] = [
        Self::Lookup,
        Self::ValidateCode,
        Self::Expand,
        Self::Subsumes,
        Self::Translate,
        Self::Closure,
        Self::Metadata,
        Self::Read,
        Self::Search,
        Self::Write,
        Self::Other,
    ];

    /// Returns the group's storage index.
    const fn index(self) -> usize {
        match self {
            Self::Lookup => 0,
            Self::ValidateCode => 1,
            Self::Expand => 2,
            Self::Subsumes => 3,
            Self::Translate => 4,
            Self::Closure => 5,
            Self::Metadata => 6,
            Self::Read => 7,
            Self::Search => 8,
            Self::Write => 9,
            Self::Other => 10,
        }
    }

    /// Returns the group a request under a FHIR base falls in, from its method
    /// and the route template it matched below the version prefix.
    ///
    /// `template` is `None` for a request no route matched, which is `other`.
    /// Parameter segments (`{id}`) are recognised by shape and never read.
    #[must_use]
    pub fn of(method: &Method, template: Option<&[&str]>) -> Self {
        let Some(segments) = template else {
            return Self::Other;
        };
        if let Some(operation) = segments.last().and_then(|s| s.strip_prefix('$')) {
            return match operation {
                "lookup" => Self::Lookup,
                "validate-code" => Self::ValidateCode,
                "expand" => Self::Expand,
                "subsumes" => Self::Subsumes,
                "translate" => Self::Translate,
                "closure" => Self::Closure,
                _ => Self::Other,
            };
        }
        let parameter = |segment: &&str| segment.starts_with('{');
        let resource = |segment: &&str| segment.chars().next().is_some_and(char::is_uppercase);
        match segments {
            ["metadata"] => Self::Metadata,
            [kind] if resource(kind) => match *method {
                Method::GET => Self::Search,
                Method::POST => Self::Write,
                _ => Self::Other,
            },
            [kind, id] if resource(kind) && parameter(id) => match *method {
                Method::GET => Self::Read,
                Method::PUT | Method::DELETE => Self::Write,
                _ => Self::Other,
            },
            [kind, rest @ ..]
                if resource(kind) && rest.contains(&"_history") && *method == Method::GET =>
            {
                Self::Read
            }
            _ => Self::Other,
        }
    }
}

/// A served FHIR version, by its path prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FhirVersion {
    /// `/r4`.
    R4,
    /// `/r4b`.
    R4b,
    /// `/r5`.
    R5,
    /// `/r6`.
    R6,
}

impl FhirVersion {
    /// Returns the version a path prefix segment names.
    #[must_use]
    pub fn of_segment(segment: &str) -> Option<Self> {
        match segment {
            "r4" => Some(Self::R4),
            "r4b" => Some(Self::R4b),
            "r5" => Some(Self::R5),
            "r6" => Some(Self::R6),
            _ => None,
        }
    }

    /// Returns the version's storage index.
    const fn index(self) -> usize {
        match self {
            Self::R4 => 0,
            Self::R4b => 1,
            Self::R5 => 2,
            Self::R6 => 3,
        }
    }
}

/// Classifies one request: its FHIR version and its group, or `None` when it
/// is not under a FHIR base (`/health`, `/metrics`, `/ui`).
///
/// `matched` is the route template axum matched, when one did; `path` is read
/// only for its first segment, so an unmatched request under a FHIR base still
/// counts toward its version and `other`.
#[must_use]
pub fn classify(
    method: &Method,
    matched: Option<&str>,
    path: &str,
) -> Option<(FhirVersion, Group)> {
    if let Some(template) = matched {
        let segments: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();
        let (prefix, rest) = segments.split_first()?;
        let version = FhirVersion::of_segment(prefix)?;
        return Some((version, Group::of(method, Some(rest))));
    }
    let version = FhirVersion::of_segment(path.split('/').find(|s| !s.is_empty())?)?;
    Some((version, Group::Other))
}

/// One group's counts over a window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GroupCounts {
    /// The count per latency bucket.
    pub buckets: [u64; BUCKETS],
    /// Responses with a 5xx status.
    pub errors_5xx: u64,
    /// The largest latency recorded, in milliseconds.
    pub max_ms: f64,
}

impl GroupCounts {
    /// Returns the number of recordings.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.buckets
            .iter()
            .fold(0_u64, |sum, count| sum.saturating_add(*count))
    }

    /// Returns the `q` quantile in milliseconds, interpolated linearly inside
    /// its bucket; the `+Inf` bucket reaches up to the largest recording.
    #[must_use]
    pub fn quantile_ms(&self, q: f64) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let rank = q.clamp(0.0, 1.0) * to_f64(total);
        let mut cumulative = 0_u64;
        for (index, count) in self.buckets.iter().copied().enumerate() {
            if count == 0 {
                continue;
            }
            let before = cumulative;
            cumulative = cumulative.saturating_add(count);
            if to_f64(cumulative) >= rank {
                let lower = lower_bound_ms(index);
                let upper = upper_bound_ms(index, self.max_ms).max(lower);
                let fraction = ((rank - to_f64(before)) / to_f64(count)).clamp(0.0, 1.0);
                return report_ms((upper - lower).mul_add(fraction, lower));
            }
        }
        report_ms(self.max_ms)
    }

    /// Returns the group as the report carries it, or `None` without
    /// requests.
    #[must_use]
    pub fn stats(&self) -> Option<OperationStats> {
        (self.total() > 0).then(|| OperationStats {
            p50_ms: self.quantile_ms(0.50),
            p95_ms: self.quantile_ms(0.95),
            p99_ms: self.quantile_ms(0.99),
            errors_5xx: self.errors_5xx.min(MAX_COUNT),
            histogram: self.buckets.map(|count| count.min(MAX_COUNT)),
        })
    }
}

/// A window's counts: per group, and per FHIR version.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Counts {
    /// The counts of each group, in [`Group::ALL`] order.
    pub groups: [GroupCounts; 11],
    /// The requests per version, in `r4`, `r4b`, `r5`, `r6` order.
    pub versions: [u64; 4],
}

impl Counts {
    /// Returns the counts of `group`.
    #[must_use]
    pub fn group(&self, group: Group) -> GroupCounts {
        self.groups.get(group.index()).copied().unwrap_or_default()
    }

    /// Renders the report's `metrics` object.
    #[must_use]
    pub fn to_metrics(&self) -> Metrics {
        let stats = |group: Group| self.group(group).stats();
        let requests = self
            .versions
            .iter()
            .fold(0_u64, |sum, count| sum.saturating_add(*count));
        let version = |index: usize| {
            self.versions
                .get(index)
                .copied()
                .unwrap_or(0)
                .min(MAX_COUNT)
        };
        Metrics {
            window_24h: Window24h {
                requests_bucket: RequestsBucket::of(requests),
                groups: Groups {
                    lookup: stats(Group::Lookup),
                    validate_code: stats(Group::ValidateCode),
                    expand: stats(Group::Expand),
                    subsumes: stats(Group::Subsumes),
                    translate: stats(Group::Translate),
                    closure: stats(Group::Closure),
                    metadata: stats(Group::Metadata),
                    read: stats(Group::Read),
                    search: stats(Group::Search),
                    write: stats(Group::Write),
                    other: stats(Group::Other),
                },
                counters: Counters {
                    fhir_r4: version(FhirVersion::R4.index()),
                    fhir_r4b: version(FhirVersion::R4b.index()),
                    fhir_r5: version(FhirVersion::R5.index()),
                    fhir_r6: version(FhirVersion::R6.index()),
                },
            },
        }
    }
}

/// One group's live counters.
#[derive(Debug, Default)]
struct Cell {
    buckets: [AtomicU64; BUCKETS],
    errors_5xx: AtomicU64,
    /// The largest recording, in microseconds.
    max_us: AtomicU64,
}

impl Cell {
    fn record(&self, us: u64, server_error: bool) {
        if let Some(bucket) = self.buckets.get(bucket_index(us)) {
            bucket.fetch_add(1, Ordering::Relaxed);
        }
        if server_error {
            self.errors_5xx.fetch_add(1, Ordering::Relaxed);
        }
        self.max_us.fetch_max(us, Ordering::Relaxed);
    }

    fn take(&self) -> GroupCounts {
        GroupCounts {
            buckets: std::array::from_fn(|index| {
                self.buckets
                    .get(index)
                    .map_or(0, |bucket| bucket.swap(0, Ordering::Relaxed))
            }),
            errors_5xx: self.errors_5xx.swap(0, Ordering::Relaxed),
            max_ms: to_f64(self.max_us.swap(0, Ordering::Relaxed)) / 1000.0,
        }
    }

    fn restore(&self, counts: &GroupCounts) {
        for (bucket, count) in self.buckets.iter().zip(counts.buckets) {
            bucket.fetch_add(count, Ordering::Relaxed);
        }
        self.errors_5xx
            .fetch_add(counts.errors_5xx, Ordering::Relaxed);
        self.max_us
            .fetch_max(ms_to_us(counts.max_ms), Ordering::Relaxed);
    }
}

/// The live window of one process, shared by the request middleware and the
/// reporter; it survives a reload of the served set.
#[derive(Debug, Default)]
pub struct Window {
    groups: [Cell; 11],
    versions: [AtomicU64; 4],
}

impl Window {
    /// Records one answered request; a request outside every FHIR base is not
    /// counted.
    ///
    /// `matched` is the route template axum matched, never the request path,
    /// which is read for its version prefix alone.
    pub fn record(
        &self,
        method: &Method,
        matched: Option<&str>,
        path: &str,
        elapsed: Duration,
        status: StatusCode,
    ) {
        let Some((version, group)) = classify(method, matched, path) else {
            return;
        };
        if let Some(count) = self.versions.get(version.index()) {
            count.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(cell) = self.groups.get(group.index()) {
            cell.record(micros(elapsed), status.is_server_error());
        }
    }

    /// Takes the counts since the last take, leaving zeros behind.
    #[must_use]
    pub fn take(&self) -> Counts {
        Counts {
            groups: std::array::from_fn(|index| {
                self.groups.get(index).map(Cell::take).unwrap_or_default()
            }),
            versions: std::array::from_fn(|index| {
                self.versions
                    .get(index)
                    .map_or(0, |count| count.swap(0, Ordering::Relaxed))
            }),
        }
    }

    /// Puts counts a failed send took back, so the next attempt carries them.
    pub fn restore(&self, counts: &Counts) {
        for (cell, taken) in self.groups.iter().zip(&counts.groups) {
            cell.restore(taken);
        }
        for (count, taken) in self.versions.iter().zip(counts.versions) {
            count.fetch_add(taken, Ordering::Relaxed);
        }
    }
}

/// A duration in whole microseconds, saturating.
fn micros(elapsed: Duration) -> u64 {
    elapsed
        .as_secs()
        .saturating_mul(1_000_000)
        .saturating_add(u64::from(elapsed.subsec_micros()))
}

/// A latency in milliseconds back in whole microseconds, saturating, for a
/// restored maximum.
fn ms_to_us(ms: f64) -> u64 {
    Duration::try_from_secs_f64(ms / 1000.0).map_or(0, micros)
}

/// The bucket a recording of `us` microseconds falls in: the first whose upper
/// bound it does not exceed, else the `+Inf` bucket.
fn bucket_index(us: u64) -> usize {
    BUCKET_BOUNDS_MS
        .iter()
        .position(|bound| us <= u64::from(*bound).saturating_mul(1000))
        .unwrap_or(BUCKETS - 1)
}

/// The lower bound of bucket `index`, in milliseconds.
fn lower_bound_ms(index: usize) -> f64 {
    index
        .checked_sub(1)
        .and_then(|previous| BUCKET_BOUNDS_MS.get(previous))
        .map_or(0.0, |bound| f64::from(*bound))
}

/// The upper bound of bucket `index`, in milliseconds; the `+Inf` bucket ends
/// at the largest recording.
fn upper_bound_ms(index: usize, max_ms: f64) -> f64 {
    BUCKET_BOUNDS_MS
        .get(index)
        .map_or(max_ms, |bound| f64::from(*bound))
}

/// A latency as the report carries it: within the contract's bounds, to a
/// tenth of a millisecond.
fn report_ms(ms: f64) -> f64 {
    if ms.is_finite() {
        (ms.clamp(0.0, MAX_MS) * 10.0).round() / 10.0
    } else {
        0.0
    }
}

/// A count as a float, exact up to 2^53 and without an `as` cast.
fn to_f64(value: u64) -> f64 {
    let [b0, b1, b2, b3, b4, b5, b6, b7] = value.to_le_bytes();
    let low = u32::from_le_bytes([b0, b1, b2, b3]);
    let high = u32::from_le_bytes([b4, b5, b6, b7]);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::{Method, StatusCode};

    use super::{FhirVersion, Group, GroupCounts, Window, bucket_index, classify};
    use crate::usage_report::payload::MAX_MS;

    #[test]
    fn templates_map_to_their_group() {
        let get = Method::GET;
        let post = Method::POST;
        let cases = [
            (&get, "/r4b/CodeSystem/$lookup", Group::Lookup),
            (&post, "/r4b/CodeSystem/$lookup", Group::Lookup),
            (&get, "/r5/CodeSystem/{id}/$lookup", Group::Lookup),
            (&get, "/r4b/CodeSystem/$validate-code", Group::ValidateCode),
            (
                &post,
                "/r4b/ValueSet/{id}/$validate-code",
                Group::ValidateCode,
            ),
            (&get, "/r4/ValueSet/$expand", Group::Expand),
            (&get, "/r4/ValueSet/{id}/$expand", Group::Expand),
            (&get, "/r4b/CodeSystem/$subsumes", Group::Subsumes),
            (&get, "/r4b/ConceptMap/$translate", Group::Translate),
            (&post, "/r4b/$closure", Group::Closure),
            (&get, "/r4b/metadata", Group::Metadata),
            (&get, "/r4b/CodeSystem/{id}", Group::Read),
            (&get, "/r4b/CodeSystem/{id}/_history", Group::Read),
            (&get, "/r4b/CodeSystem/{id}/_history/{version}", Group::Read),
            (&get, "/r4b/ValueSet/_history", Group::Read),
            (&get, "/r4b/ConceptMap", Group::Search),
            (&post, "/r4b/ConceptMap", Group::Write),
            (&Method::PUT, "/r4b/ValueSet/{id}", Group::Write),
            (&Method::DELETE, "/r4b/ValueSet/{id}", Group::Write),
            (&post, "/r4b/ValueSet/$batch-validate-code", Group::Other),
            (&get, "/r4b/$versions", Group::Other),
            (&post, "/r4b/$cache-control", Group::Other),
            (&get, "/r4b/.well-known/smart-configuration", Group::Other),
            (&post, "/r4b", Group::Other),
            (&post, "/r4b/", Group::Other),
        ];
        for (method, template, group) in cases {
            assert_eq!(
                classify(method, Some(template), "/ignored").map(|(_, g)| g),
                Some(group),
                "{method} {template}"
            );
        }
    }

    #[test]
    fn the_version_prefix_decides_whether_a_request_counts() {
        assert_eq!(
            classify(&Method::GET, Some("/r6/metadata"), "/r6/metadata"),
            Some((FhirVersion::R6, Group::Metadata))
        );
        for outside in ["/health", "/metrics", "/ui/", "/", "/reload"] {
            assert_eq!(
                classify(&Method::GET, Some(outside), outside),
                None,
                "{outside}"
            );
            assert_eq!(classify(&Method::GET, None, outside), None, "{outside}");
        }
        assert_eq!(
            classify(&Method::GET, None, "/r4b/Nope/123"),
            Some((FhirVersion::R4b, Group::Other)),
            "an unmatched request under a base is other, and its path is not read further"
        );
        assert_eq!(classify(&Method::GET, None, "/r7/metadata"), None);
    }

    #[test]
    fn a_parameter_segment_never_decides_the_group() {
        assert_eq!(
            classify(&Method::GET, Some("/r4b/{metadata}"), "/"),
            Some((FhirVersion::R4b, Group::Other))
        );
    }

    #[test]
    fn recordings_land_in_inclusive_upper_buckets() {
        assert_eq!(bucket_index(0), 0);
        assert_eq!(bucket_index(5_000), 0);
        assert_eq!(bucket_index(5_001), 1);
        assert_eq!(bucket_index(5_000_000), 9);
        assert_eq!(bucket_index(5_000_001), 10);
        assert_eq!(bucket_index(u64::MAX), 10);
    }

    #[test]
    fn quantiles_interpolate_inside_their_bucket() {
        let mut counts = GroupCounts::default();
        if let Some(bucket) = counts.buckets.get_mut(2) {
            *bucket = 100;
        }
        counts.max_ms = 24.0;
        assert!((counts.quantile_ms(0.5) - 17.5).abs() < 1e-9);
        assert!((counts.quantile_ms(1.0) - 25.0).abs() < 1e-9);
        assert!(GroupCounts::default().quantile_ms(0.95).abs() < f64::EPSILON);
        let mut slow = GroupCounts::default();
        if let Some(bucket) = slow.buckets.get_mut(10) {
            *bucket = 2;
        }
        slow.max_ms = 10_000_000.0;
        assert!((slow.quantile_ms(1.0) - MAX_MS).abs() < 1e-9);
    }

    #[test]
    fn a_take_empties_the_window_and_a_restore_refills_it() {
        let window = Window::default();
        window.record(
            &Method::GET,
            Some("/r4b/CodeSystem/$lookup"),
            "/r4b/CodeSystem/$lookup",
            Duration::from_millis(3),
            StatusCode::OK,
        );
        window.record(
            &Method::GET,
            Some("/r4b/CodeSystem/$lookup"),
            "/r4b/CodeSystem/$lookup",
            Duration::from_millis(30),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        window.record(
            &Method::GET,
            Some("/health"),
            "/health",
            Duration::ZERO,
            StatusCode::OK,
        );
        let taken = window.take();
        assert_eq!(taken.group(Group::Lookup).total(), 2);
        assert_eq!(taken.group(Group::Lookup).errors_5xx, 1);
        assert_eq!(taken.versions, [0, 2, 0, 0]);
        assert_eq!(window.take().versions, [0, 0, 0, 0], "a take leaves zeros");
        window.restore(&taken);
        let again = window.take();
        assert_eq!(again.versions, taken.versions);
        assert_eq!(
            again.group(Group::Lookup).buckets,
            taken.group(Group::Lookup).buckets
        );
        let metrics = again.to_metrics();
        let lookup = metrics
            .window_24h
            .groups
            .lookup
            .expect("lookup is rendered");
        assert_eq!(lookup.errors_5xx, 1);
        assert!(metrics.window_24h.groups.expand.is_none());
    }
}
