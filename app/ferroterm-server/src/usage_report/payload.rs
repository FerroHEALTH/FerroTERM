// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The report payload: `FerroPULSE`'s envelope v2 with FerroTERM's `status`
//! and `metrics` objects.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. The field
//! set, the value sets, and the bounds are `FerroPULSE`'s report v2 contract.
//! Every type here is closed: a field that is not declared cannot be
//! serialized, and the `start` and `daily` variants carry exactly the members
//! their event admits.

use serde::{Serialize, Serializer};
use uuid::Uuid;

use crate::state::Origin;
use crate::usage_report::config::Deployment;

/// The number of latency buckets in a histogram.
pub const BUCKETS: usize = 11;

/// The upper bounds of the finite latency buckets, in milliseconds; the
/// eleventh bucket is `+Inf`.
pub const BUCKET_BOUNDS_MS: [u32; BUCKETS - 1] = [5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000];

/// The largest count a report may carry.
pub const MAX_COUNT: u64 = 10_000_000;

/// The largest latency a report may carry, in milliseconds.
pub const MAX_MS: f64 = 600_000.0;

/// The largest uptime a report may carry: ten years of seconds.
pub const MAX_UPTIME_S: u64 = 315_576_000;

/// The most code system families a report may list.
pub const MAX_FAMILIES: usize = 16;

/// The payload schema version; serializes as the integer `2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaV2;

impl Serialize for SchemaV2 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(2)
    }
}

/// The reporting product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Product {
    /// This terminology server.
    #[serde(rename = "ferroterm")]
    FerroTerm,
}

/// The licence grant in force, as its type alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LicenceGrant {
    /// What the Business Source License grants everyone.
    NonCommercial,
    /// A commercial licence.
    Commercial,
}

/// The CPUs available to the process, bucketed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CpuBucket {
    /// One or two.
    #[serde(rename = "1-2")]
    UpTo2,
    /// Three or four.
    #[serde(rename = "3-4")]
    UpTo4,
    /// Five to eight.
    #[serde(rename = "5-8")]
    UpTo8,
    /// Nine to sixteen.
    #[serde(rename = "9-16")]
    UpTo16,
    /// Seventeen or more.
    #[serde(rename = "17+")]
    Above16,
}

impl CpuBucket {
    /// Returns the bucket holding `cpus`.
    #[must_use]
    pub const fn of(cpus: usize) -> Self {
        match cpus {
            0..=2 => Self::UpTo2,
            3..=4 => Self::UpTo4,
            5..=8 => Self::UpTo8,
            9..=16 => Self::UpTo16,
            _ => Self::Above16,
        }
    }
}

/// The memory available to the process in GiB, bucketed by half-open ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MemoryBucket {
    /// Under 4 GiB.
    #[serde(rename = "<4")]
    Below4,
    /// 4 GiB to under 8.
    #[serde(rename = "4-8")]
    Below8,
    /// 8 GiB to under 16.
    #[serde(rename = "8-16")]
    Below16,
    /// 16 GiB to under 32.
    #[serde(rename = "16-32")]
    Below32,
    /// 32 GiB or more.
    #[serde(rename = "32+")]
    AtLeast32,
}

impl MemoryBucket {
    /// Returns the bucket holding `bytes`.
    #[must_use]
    pub const fn of(bytes: u64) -> Self {
        match bytes >> 30 {
            0..=3 => Self::Below4,
            4..=7 => Self::Below8,
            8..=15 => Self::Below16,
            16..=31 => Self::Below32,
            _ => Self::AtLeast32,
        }
    }
}

/// The requests served in the window, bucketed by half-open ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RequestsBucket {
    /// None.
    #[serde(rename = "0")]
    None,
    /// 1 to 99.
    #[serde(rename = "1-100")]
    Below100,
    /// 100 to 999.
    #[serde(rename = "100-1k")]
    Below1k,
    /// 1,000 to 9,999.
    #[serde(rename = "1k-10k")]
    Below10k,
    /// 10,000 to 99,999.
    #[serde(rename = "10k-100k")]
    Below100k,
    /// 100,000 or more.
    #[serde(rename = "100k+")]
    AtLeast100k,
}

impl RequestsBucket {
    /// Returns the bucket holding `requests`.
    #[must_use]
    pub const fn of(requests: u64) -> Self {
        match requests {
            0 => Self::None,
            1..=99 => Self::Below100,
            100..=999 => Self::Below1k,
            1_000..=9_999 => Self::Below10k,
            10_000..=99_999 => Self::Below100k,
            _ => Self::AtLeast100k,
        }
    }
}

/// A code system family, the only way a report names what is loaded.
///
/// The order is the report's: the list is sorted ascending by the serialized
/// name, which is the declaration order here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// The Dutch DHD thesauri.
    Dhd,
    /// FHIR `CodeSystem` resources loaded from `terminology.code_systems` that
    /// no other family names.
    Fhir,
    /// The Dutch G-Standaard product ladder.
    Gstandaard,
    /// WHO ICD-10 and its national editions.
    Icd10,
    /// ICD-10-CM.
    Icd10cm,
    /// ICD-11.
    Icd11,
    /// The Dutch Labcodeset.
    Labcodeset,
    /// LOINC.
    Loinc,
    /// Any loaded system the closed set does not name.
    Other,
    /// `RxNorm`.
    Rxnorm,
    /// SNOMED CT.
    Snomed,
}

impl Family {
    /// Returns the family of a loaded system: by its URL when the closed set
    /// names it, else `fhir` for a `CodeSystem` resource and `other` for an
    /// artifact.
    ///
    /// The URL is read here and never leaves the process.
    #[must_use]
    pub fn of(url: &str, origin: Origin) -> Self {
        // NOTE: the URLs are the canonicals each source publishes under
        // (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
        match url {
            "http://snomed.info/sct" => Self::Snomed,
            "http://loinc.org" => Self::Loinc,
            "http://hl7.org/fhir/sid/icd-10" | "http://hl7.org/fhir/sid/icd-10-nl" => Self::Icd10,
            "http://hl7.org/fhir/sid/icd-10-cm" => Self::Icd10cm,
            "http://www.nlm.nih.gov/research/umls/rxnorm" => Self::Rxnorm,
            "urn:oid:2.16.840.1.113883.2.4.3.120.5.1" => Self::Dhd,
            "urn:oid:2.16.840.1.113883.2.4.4.1"
            | "urn:oid:2.16.840.1.113883.2.4.4.7"
            | "urn:oid:2.16.840.1.113883.2.4.4.8"
            | "urn:oid:2.16.840.1.113883.2.4.4.10" => Self::Gstandaard,
            "https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc" => Self::Labcodeset,
            _ if url.starts_with("http://id.who.int/icd/") => Self::Icd11,
            _ => match origin {
                Origin::CodeSystems => Self::Fhir,
                Origin::Index => Self::Other,
            },
        }
    }
}

/// FerroTERM's `status` object, carried by both events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Whether at least one code system the deployment loaded is serving.
    pub index_ok: bool,
    /// The loaded code system families, unique and sorted ascending.
    pub code_systems: Vec<Family>,
    /// Whether a resources database is configured.
    pub persistence: bool,
}

impl Status {
    /// Returns the status with `families` deduplicated, sorted, and capped at
    /// [`MAX_FAMILIES`].
    #[must_use]
    pub fn new(
        index_ok: bool,
        families: impl IntoIterator<Item = Family>,
        persistence: bool,
    ) -> Self {
        let mut code_systems: Vec<Family> = families.into_iter().collect();
        code_systems.sort_unstable();
        code_systems.dedup();
        code_systems.truncate(MAX_FAMILIES);
        Self {
            index_ok,
            code_systems,
            persistence,
        }
    }
}

/// One operation group's latency and errors over the window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationStats {
    /// The median latency, in milliseconds.
    pub p50_ms: f64,
    /// The 95th percentile latency, in milliseconds.
    pub p95_ms: f64,
    /// The 99th percentile latency, in milliseconds.
    pub p99_ms: f64,
    /// Responses with a 5xx status.
    pub errors_5xx: u64,
    /// The request count per latency bucket.
    pub histogram: [u64; BUCKETS],
}

/// The operation groups with requests in the window; an absent group had
/// none.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Groups {
    /// `$lookup`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup: Option<OperationStats>,
    /// `$validate-code`, on `CodeSystem` and `ValueSet`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_code: Option<OperationStats>,
    /// `$expand`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand: Option<OperationStats>,
    /// `$subsumes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsumes: Option<OperationStats>,
    /// `$translate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<OperationStats>,
    /// `$closure`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closure: Option<OperationStats>,
    /// `/metadata`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<OperationStats>,
    /// A read of one resource, its history, or one of its versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<OperationStats>,
    /// A search of one resource type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<OperationStats>,
    /// A create, update, or delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write: Option<OperationStats>,
    /// Every other request under a FHIR base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<OperationStats>,
}

/// The counters of the window: the requests per served FHIR version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[expect(
    clippy::struct_field_names,
    reason = "the field names are the counter keys the report v2 contract names"
)]
pub struct Counters {
    /// Requests under `/r4`.
    pub fhir_r4: u64,
    /// Requests under `/r4b`.
    pub fhir_r4b: u64,
    /// Requests under `/r5`.
    pub fhir_r5: u64,
    /// Requests under `/r6`.
    pub fhir_r6: u64,
}

/// The window a daily report covers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Window24h {
    /// All requests under a FHIR base, bucketed.
    pub requests_bucket: RequestsBucket,
    /// Per operation group.
    pub groups: Groups,
    /// The requests per FHIR version.
    pub counters: Counters,
}

/// FerroTERM's `metrics` object, carried by the daily report only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metrics {
    /// The window since the previous daily report of this process.
    pub window_24h: Window24h,
}

/// The event a report records, with the members only that event carries.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    /// A process start.
    Start {
        /// Whether this start created the instance id.
        first_start: bool,
    },
    /// The daily report.
    Daily {
        /// The window's metrics.
        metrics: Box<Metrics>,
    },
}

/// One report, exactly as it is sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Always `2`.
    pub schema: SchemaV2,
    /// The event and its event-specific members.
    #[serde(flatten)]
    pub event: Event,
    /// Always `ferroterm`.
    pub product: Product,
    /// The random installation id.
    pub instance_id: Uuid,
    /// The server version.
    pub version: String,
    /// The commit the binary was built from.
    pub git_sha: String,
    /// The licence grant type.
    pub licence: LicenceGrant,
    /// How the server was deployed.
    pub deployment: Deployment,
    /// Seconds since this process started.
    pub uptime_s: u64,
    /// The CPUs available, bucketed.
    pub cpu_bucket: CpuBucket,
    /// The memory available, bucketed.
    pub memory_bucket: MemoryBucket,
    /// The server's state facts.
    pub status: Status,
}

/// The `git_sha` a build reports: the commit when the build recorded one in
/// the contract's form (7 to 40 lower-case hex digits), else seven zeros.
///
/// The report has no "unknown" spelling for a commit, so a build that records
/// none reports the all-zero commit.
#[must_use]
pub fn report_git_sha(revision: &str) -> String {
    let valid = (7..=40).contains(&revision.len())
        && revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if valid {
        revision.to_owned()
    } else {
        "0000000".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CpuBucket, Family, MAX_FAMILIES, MemoryBucket, RequestsBucket, Status, report_git_sha,
    };
    use crate::state::Origin;

    #[test]
    fn buckets_are_half_open() {
        assert_eq!(RequestsBucket::of(0), RequestsBucket::None);
        assert_eq!(RequestsBucket::of(99), RequestsBucket::Below100);
        assert_eq!(RequestsBucket::of(100), RequestsBucket::Below1k);
        assert_eq!(RequestsBucket::of(100_000), RequestsBucket::AtLeast100k);
        assert_eq!(CpuBucket::of(1), CpuBucket::UpTo2);
        assert_eq!(CpuBucket::of(16), CpuBucket::UpTo16);
        assert_eq!(CpuBucket::of(17), CpuBucket::Above16);
        let gib = 1_u64 << 30;
        assert_eq!(MemoryBucket::of(4 * gib - 1), MemoryBucket::Below4);
        assert_eq!(MemoryBucket::of(4 * gib), MemoryBucket::Below8);
        assert_eq!(MemoryBucket::of(32 * gib), MemoryBucket::AtLeast32);
    }

    #[test]
    fn a_git_sha_outside_the_contract_reports_zeros() {
        assert_eq!(report_git_sha("1a2b3c4d5e6f"), "1a2b3c4d5e6f");
        assert_eq!(report_git_sha(""), "0000000");
        assert_eq!(report_git_sha("1A2B3C4"), "0000000");
        assert_eq!(report_git_sha("abc"), "0000000");
    }

    #[test]
    fn a_loaded_system_maps_to_its_family_and_the_rest_to_fhir_or_other() {
        let index = |url| Family::of(url, Origin::Index);
        assert_eq!(index("http://snomed.info/sct"), Family::Snomed);
        assert_eq!(index("http://loinc.org"), Family::Loinc);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10"), Family::Icd10);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10-nl"), Family::Icd10);
        assert_eq!(index("http://hl7.org/fhir/sid/icd-10-cm"), Family::Icd10cm);
        assert_eq!(index("http://id.who.int/icd/release/11/mms"), Family::Icd11);
        assert_eq!(
            index("http://www.nlm.nih.gov/research/umls/rxnorm"),
            Family::Rxnorm
        );
        assert_eq!(
            index("urn:oid:2.16.840.1.113883.2.4.3.120.5.1"),
            Family::Dhd
        );
        assert_eq!(
            index("urn:oid:2.16.840.1.113883.2.4.4.10"),
            Family::Gstandaard
        );
        assert_eq!(index("http://www.whocc.no/atc"), Family::Other);
        assert_eq!(
            Family::of(
                "https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc",
                Origin::CodeSystems
            ),
            Family::Labcodeset
        );
        assert_eq!(
            Family::of("http://example.org/cs", Origin::CodeSystems),
            Family::Fhir
        );
        assert_eq!(
            Family::of("http://snomed.info/sct", Origin::CodeSystems),
            Family::Snomed,
            "a named family wins over the source"
        );
    }

    #[test]
    fn families_serialize_sorted_by_their_name_and_unique() {
        let status = Status::new(
            true,
            [Family::Snomed, Family::Loinc, Family::Snomed, Family::Fhir],
            false,
        );
        let json = serde_json::to_value(&status).expect("serializes");
        assert_eq!(
            json["code_systems"],
            serde_json::json!(["fhir", "loinc", "snomed"])
        );
        let all = [
            Family::Dhd,
            Family::Fhir,
            Family::Gstandaard,
            Family::Icd10,
            Family::Icd10cm,
            Family::Icd11,
            Family::Labcodeset,
            Family::Loinc,
            Family::Other,
            Family::Rxnorm,
            Family::Snomed,
        ];
        let names: Vec<String> = all
            .iter()
            .map(|f| {
                serde_json::to_value(f)
                    .expect("serializes")
                    .as_str()
                    .expect("a string")
                    .to_owned()
            })
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "the declaration order is the name order");
        assert!(all.len() <= MAX_FAMILIES);
    }
}
