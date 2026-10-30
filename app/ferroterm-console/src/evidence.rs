// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The conformance and benchmark evidence this build carries.
//!
//! Every figure here was read out of a committed file by `build.rs` and
//! written into the constant this module includes. Nothing is typed in: each
//! record names the file it came from, so a reader who doubts a number can
//! open that file in the repository and check it.
//!
//! These are facts about the build, never about the deployment serving this
//! page. The screen says so, and the release below is the version the binary
//! was built from.
//!
//! The evidence is a constant rather than a document read at run time. It is
//! fixed when the binary is built, so it leaves no failure the screen has to
//! render.

// The generated source binds `EVIDENCE`, or refuses to compile with the reason
// the emitter could not read a figure.
include!(concat!(env!("OUT_DIR"), "/evidence.rs"));

/// The whole evidence this build read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Evidence {
    /// The version of FerroTERM this binary was built from.
    pub(crate) release: &'static str,
    /// The HL7 terminology ecosystem suite, mode by mode.
    pub(crate) conformance: Conformance,
    /// The latency bars the project claims, and what measured against them.
    pub(crate) latency: Latency,
}

/// The suite's committed pass lists, and how many cases each mode ran.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Conformance {
    /// One entry per mode and served surface.
    pub(crate) modes: &'static [Mode],
}

/// One suite mode, run against one served FHIR root.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mode {
    /// The mode the run selected.
    pub(crate) name: &'static str,
    /// The served root the run drove.
    pub(crate) surface: &'static str,
    /// How many cases the run passed.
    pub(crate) passed: u32,
    /// How many cases the run ran.
    pub(crate) ran: u32,
    /// The content it needs beyond what the runner supplies, `nothing` for
    /// none.
    pub(crate) needs: &'static str,
}

/// The latency claims and the machine that measured them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Latency {
    /// The machine the recorded measurements were taken on.
    pub(crate) machine: &'static str,
    /// One entry per benchmark.
    pub(crate) bars: &'static [LatencyBar],
}

/// One latency bar: the claim, and the run recorded against it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LatencyBar {
    /// The benchmark the bar governs.
    pub(crate) bench: &'static str,
    /// The claim in microseconds, which never moves to match a slower run.
    pub(crate) max_us: u32,
    /// What one machine answered, in microseconds.
    pub(crate) measured_us: u32,
}

/// The evidence this build read out of the repository's committed files.
pub(crate) fn embedded() -> Evidence {
    EVIDENCE
}

#[cfg(test)]
mod tests {
    use super::embedded;

    #[test]
    fn the_build_read_every_section() {
        let evidence = embedded();
        assert!(
            !evidence.release.is_empty(),
            "the bundle states which release it was built from"
        );
        assert!(
            !evidence.conformance.modes.is_empty(),
            "the screen would state no conformance figure at all"
        );
        assert!(
            !evidence.latency.bars.is_empty(),
            "the screen would state no latency claim at all"
        );
    }

    #[test]
    fn no_mode_claims_more_passes_than_the_cases_it_ran() {
        for mode in embedded().conformance.modes {
            assert!(
                mode.passed <= mode.ran,
                "{} on {} passed {} of {}",
                mode.name,
                mode.surface,
                mode.passed,
                mode.ran
            );
        }
    }

    #[test]
    fn every_recorded_measurement_is_inside_the_bar_it_is_measured_against() {
        for bar in embedded().latency.bars {
            assert!(
                bar.measured_us <= bar.max_us,
                "{} measured {} against a bar of {}",
                bar.bench,
                bar.measured_us,
                bar.max_us
            );
        }
    }
}
