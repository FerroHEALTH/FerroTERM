// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one place the engine reads the wall clock for a FHIR `dateTime`.

/// The current instant.
///
/// # Panics
///
/// `jiff::Timestamp::now` panics when the system clock lies outside the
/// range a `Timestamp` represents, the years -9999 to 9999
/// (<https://docs.rs/jiff/latest/jiff/struct.Timestamp.html#method.now>).
// NOTE: a clock in that state is a machine fault no answer can be right
// under, so the panic stays (`unwind`, the HTTP layer answers 500 per
// `.claude/rules/reliability.md`) and every caller shares this one decision.
#[must_use]
pub fn now() -> jiff::Timestamp {
    jiff::Timestamp::now()
}
