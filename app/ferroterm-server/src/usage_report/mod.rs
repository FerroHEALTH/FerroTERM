//! The usage report to `FerroPULSE`.
//!
//! One `start` report at each process start and one `daily` report a day,
//! carrying the installation id, the version, the licence grant type, the
//! deployment kind, coarse host size, the loaded code system families, and
//! latency aggregates per operation group.
//!
//! No FHIR or SNOMED CT specification governs any of this: our own design. The
//! wire contract is `FerroPULSE`'s report envelope v2 ([`payload`]). A report
//! carries no code, no system or value set URL, no request path, and no host
//! name: the payload types have no field that could hold one, and [`window`]
//! classifies a request by its matched route template alone. It is on by
//! default; `FERROTERM_USAGE_REPORT=off` stops it, a CI environment stops it,
//! and so does the lack of a writable `FERROTERM_STATE_DIR`. One boot line says
//! which ([`reporter::announce`]).
//!
//! - [`config`]: the environment the report reads.
//! - [`payload`]: the typed report.
//! - [`window`]: the in-process metrics window and its rendering.
//! - [`state`]: the instance id and the last start time on disk.
//! - [`reporter`]: assembly, sending, scheduling.

pub mod config;
pub mod payload;
pub mod reporter;
pub mod state;
pub mod window;
