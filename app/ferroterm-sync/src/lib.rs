// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroTERM synchronisation service: one way from a terminology
//! syndication feed into a running FerroTERM.
//!
//! The service runs beside the server and never inside it. On a schedule, or
//! on a manual trigger, it lists every configured source, decides what the
//! deployment does not hold yet ([`terminology_syndication::select`]), fetches
//! it with its digest verified, and puts it where the server reads: an RF2
//! archive is built by `ferroterm-build` into a staging directory and renamed
//! into the index root, a FHIR resource is written into the managed resource
//! directory. The server is then asked to reload
//! ([`reload::AdminClient`]). Nothing is ever deleted except by
//! [`retention`], and the server's own write store is never touched.
//!
//! No FHIR or SNOMED CT specification governs the service: our own design. The
//! feed dialect it reads is the one `terminology-syndication` documents.
//!
//! The modules follow one run: [`config`] is the file that describes the
//! deployment, [`schedule`] and [`clock`] say when a run starts, [`state`]
//! remembers what already happened, [`holdings`] reads what is already served,
//! [`source`] is the add-on seam, [`build`] is the RF2 lane, [`activate`] puts
//! a staged release in front of the server, [`retention`] prunes what it
//! replaced, [`record`] writes down what happened, and [`admin`] serves that
//! record, [`metrics`], and the manual triggers behind [`auth`].
#![doc(test(attr(deny(warnings))))]

pub mod activate;
pub mod admin;
pub mod auth;
pub mod build;
pub mod cli;
pub mod clock;
pub mod config;
pub mod holdings;
pub mod metrics;
pub mod naming;
pub mod problem;
pub mod record;
pub mod reload;
pub mod retention;
pub mod revalidate;
pub mod run;
pub mod schedule;
pub mod source;
pub mod state;
pub mod webhook;

/// An error and every cause under it, as one line.
///
/// A run record and a log line both carry the whole chain, so a reader sees
/// the file that did not open rather than only the layer that reported it.
#[must_use]
pub fn reason(error: &dyn core::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::reason;

    #[derive(Debug, thiserror::Error)]
    #[error("the run did not finish")]
    struct Outer(#[source] Inner);

    #[derive(Debug, thiserror::Error)]
    #[error("the feed did not answer")]
    struct Inner;

    #[test]
    fn a_reason_carries_every_cause_under_it() {
        assert_eq!(
            reason(&Outer(Inner)),
            "the run did not finish: the feed did not answer"
        );
    }
}
