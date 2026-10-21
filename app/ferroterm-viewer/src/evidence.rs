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
//! page. The screen says so, and the release below is the version the bundle
//! was built from.
//!
//! The evidence is a constant rather than a document the browser decodes. It
//! is fixed when the bundle is built, so decoding it in every reader's browser
//! would cost bytes for nothing and leave a failure the screen has to render.

// The generated source binds `EVIDENCE`, or refuses to compile with the reason
// the emitter could not read a figure.
include!(concat!(env!("OUT_DIR"), "/evidence.rs"));

/// The whole evidence this build read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Evidence {
    /// The version of FerroTERM this bundle was built from.
    pub(crate) release: &'static str,
    /// The HL7 terminology ecosystem suite, mode by mode.
    pub(crate) conformance: Conformance,
    /// The latency bars the project claims, and what measured against them.
    pub(crate) latency: Latency,
    /// The newest committed benchmark run.
    pub(crate) run: Run,
}

/// The suite's committed pass lists, and how many cases each mode ran.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Conformance {
    /// How many cases the `general` mode runs, from `total.txt`.
    pub(crate) suite_total: u32,
    /// The file recording that total.
    pub(crate) total_source: &'static str,
    /// The file whose table records what each mode ran.
    pub(crate) table_source: &'static str,
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
    /// The committed pass list this row was counted from.
    pub(crate) source: &'static str,
}

/// The latency claims and the machine that measured them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Latency {
    /// The machine the recorded measurements were taken on.
    pub(crate) machine: &'static str,
    /// The file holding the bars.
    pub(crate) source: &'static str,
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
    /// What the bar claims, in words.
    pub(crate) claim: &'static str,
}

/// The newest committed benchmark run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Run {
    /// The run's own name, which is the date it was taken.
    pub(crate) name: &'static str,
    /// The directory the run's records live in.
    pub(crate) source: &'static str,
    /// One record per code system the run loaded.
    pub(crate) systems: &'static [SystemRun],
}

/// One code system's record within a run.
///
/// The figures arrive already rounded for reading. Formatting one `f64` in
/// the browser pulls the whole floating-point formatter into a bundle that
/// holds no other float, so the emitter rounds on the host instead
/// (`docs/viewer.md` section 13).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SystemRun {
    /// The code system the record is about, as the record names it.
    pub(crate) system: &'static str,
    /// That system's canonical.
    pub(crate) system_uri: &'static str,
    /// The version of it the run loaded.
    pub(crate) system_version: &'static str,
    /// The served FHIR root the run drove.
    pub(crate) fhir: &'static str,
    /// When the record was taken.
    pub(crate) taken_at: &'static str,
    /// The FerroTERM version that answered the run.
    pub(crate) built_by: &'static str,
    /// The machine the run was taken on.
    pub(crate) machine: &'static str,
    /// How many concepts the artifact holds.
    pub(crate) concepts: &'static str,
    /// How long the offline build took, in seconds, or `None` when the run
    /// did not build the artifact itself.
    pub(crate) ingest_seconds: Option<&'static str>,
    /// The release the artifact was built from, when the run built it.
    pub(crate) release: Option<&'static str>,
    /// How long the server took to answer `/health`, in seconds.
    pub(crate) ready_seconds: &'static str,
    /// Resident memory after the warm requests.
    pub(crate) resident_memory: &'static str,
    /// One entry per timed operation.
    pub(crate) operations: &'static [Timing],
    /// The committed record this row was read from.
    pub(crate) source: &'static str,
}

/// One operation's timings within a record.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    /// The operation the run timed.
    pub(crate) operation: &'static str,
    /// The HTTP status every request in the run answered.
    pub(crate) status: u16,
    /// The first request of the operation, in milliseconds.
    pub(crate) cold_ms: &'static str,
    /// The median of the warm requests, in milliseconds.
    pub(crate) p50_ms: &'static str,
    /// The 95th percentile of the warm requests, in milliseconds.
    pub(crate) p95_ms: &'static str,
    /// The 99th percentile of the warm requests, in milliseconds.
    pub(crate) p99_ms: &'static str,
    /// How many warm requests the percentiles were taken over.
    pub(crate) warm_requests: u32,
}

/// The evidence this build read out of the repository's committed files.
pub(crate) fn embedded() -> Evidence {
    EVIDENCE
}

impl Mode {
    /// The share of the cases it ran that this mode passed, as a percentage.
    ///
    /// A mode that ran nothing has no share, and answers zero.
    pub(crate) fn share(&self) -> u32 {
        self.passed
            .checked_mul(100)
            .and_then(|scaled| scaled.checked_div(self.ran))
            .unwrap_or_default()
    }
}

impl Conformance {
    /// What the suite table says, in one line a reader can scan.
    ///
    /// The modes are counted as well as the cases, because a mode that runs
    /// nothing and a mode that passes nothing read the same from a case count
    /// alone.
    pub(crate) fn summary(&self) -> String {
        let passed: u32 = self.modes.iter().map(|mode| mode.passed).sum();
        let ran: u32 = self.modes.iter().map(|mode| mode.ran).sum();
        let modes = self.modes.len();
        format!(
            "{passed} of {ran} cases pass, across {modes} mode runs. The general mode alone runs {} of them.",
            self.suite_total,
        )
    }
}

impl Latency {
    /// What the bars say, in one line a reader can scan.
    ///
    /// The tightest bar is the one worth naming: it is the claim a slower
    /// machine breaks first, and the number a reader is deciding against.
    pub(crate) fn summary(&self) -> String {
        let bars = self.bars.len();
        let Some(tightest) = self
            .bars
            .iter()
            .min_by_key(|bar| bar.headroom().unwrap_or(u32::MAX))
        else {
            return "No latency bar is recorded.".to_owned();
        };
        match tightest.headroom() {
            Some(times) => format!(
                "{bars} bars, every one met. The tightest is {}, {times} times under its bar.",
                tightest.bench,
            ),
            None => format!("{bars} bars, every one met faster than the run could time."),
        }
    }
}

impl Run {
    /// What the run says, in one line a reader can scan.
    pub(crate) fn summary(&self) -> String {
        let systems = self.systems.len();
        let noun = if systems == 1 { "system" } else { "systems" };
        format!(
            "{systems} code {noun} loaded, in the run named {}.",
            self.name
        )
    }
}

impl LatencyBar {
    /// How many times under its bar the recorded measurement came in.
    ///
    /// A measurement of zero microseconds has no ratio, and answers `None`.
    pub(crate) fn headroom(&self) -> Option<u32> {
        self.max_us.checked_div(self.measured_us)
    }
}

/// The width of a row's bar, as a percentage of the widest value beside it.
///
/// The bar is drawn against the largest value in its own table rather than
/// against an absolute scale, so a column whose values differ by two orders of
/// magnitude still reads. A zero largest draws nothing.
pub(crate) fn relative_width(value: u32, largest: u32) -> u32 {
    value
        .checked_mul(100)
        .and_then(|scaled| scaled.checked_div(largest))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    /// Two modes, one of which passes every case it ran.
    const MODES: [Mode; 2] = [
        Mode {
            name: "general",
            surface: "r4b",
            passed: 600,
            ran: 670,
            source: "conformance/tx-ecosystem/README.md",
        },
        Mode {
            name: "bugs",
            surface: "r5",
            passed: 30,
            ran: 30,
            source: "conformance/tx-ecosystem/README.md",
        },
    ];

    /// Two bars, the second of which has less room than the first.
    const BARS: [LatencyBar; 2] = [
        LatencyBar {
            bench: "operations/lookup",
            max_us: 1000,
            measured_us: 10,
            claim: "a point read answers in under a millisecond",
        },
        LatencyBar {
            bench: "http/expand_page_10",
            max_us: 1000,
            measured_us: 500,
            claim: "one page of an expansion answers in under a millisecond",
        },
    ];

    #[test]
    fn the_suite_summary_counts_the_cases_and_the_mode_runs() {
        let conformance = Conformance {
            suite_total: 670,
            total_source: "conformance/tx-ecosystem/total.txt",
            table_source: "conformance/tx-ecosystem/README.md",
            modes: &MODES,
        };
        assert_eq!(
            conformance.summary(),
            "630 of 700 cases pass, across 2 mode runs. The general mode alone runs 670 of them.",
            "a mode that runs nothing and one that passes nothing read the same from a case count alone"
        );
    }

    #[test]
    fn the_latency_summary_names_the_bar_with_the_least_room() {
        let latency = Latency {
            machine: "Apple M2 Pro",
            source: "bench/bars.json",
            bars: &BARS,
        };
        assert_eq!(
            latency.summary(),
            "2 bars, every one met. The tightest is http/expand_page_10, 2 times under its bar.",
            "the tightest bar is the claim a slower machine breaks first"
        );
    }

    #[test]
    fn a_latency_summary_with_no_bar_says_so_rather_than_naming_one() {
        let latency = Latency {
            machine: "Apple M2 Pro",
            source: "bench/bars.json",
            bars: &[],
        };
        assert_eq!(latency.summary(), "No latency bar is recorded.");
    }

    #[test]
    fn the_run_summary_counts_the_systems_and_names_the_run() {
        let run = Run {
            name: "2026-09-06-apple-m2",
            source: "bench/records/2026-09-06-apple-m2",
            systems: &[],
        };
        assert_eq!(
            run.summary(),
            "0 code systems loaded, in the run named 2026-09-06-apple-m2."
        );
    }

    use super::*;

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
        assert!(
            !evidence.run.systems.is_empty(),
            "the screen would state no benchmark record at all"
        );
    }

    #[test]
    fn every_figure_names_the_file_it_came_from() {
        let evidence = embedded();
        for mode in evidence.conformance.modes {
            assert!(
                mode.source.starts_with("conformance/"),
                "a pass count names its committed list: {mode:?}"
            );
        }
        assert!(
            evidence.latency.source.starts_with("bench/"),
            "the latency bars name the file that holds them"
        );
        for system in evidence.run.systems {
            assert!(
                system.source.starts_with("bench/records/"),
                "a benchmark row names its committed record: {}",
                system.system
            );
        }
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

    #[test]
    fn every_record_states_the_machine_and_the_version_that_answered_it() {
        for system in embedded().run.systems {
            assert!(
                !system.machine.is_empty(),
                "a timing without its machine says nothing: {}",
                system.system
            );
            assert!(
                !system.built_by.is_empty(),
                "a timing without the version that answered it says nothing: {}",
                system.system
            );
        }
    }

    /// One mode row, for the arithmetic below.
    fn mode(passed: u32, ran: u32) -> Mode {
        Mode {
            name: "general",
            surface: "/r4b",
            passed,
            ran,
            source: "conformance/tx-ecosystem/passing.txt",
        }
    }

    #[test]
    fn a_share_is_the_percentage_of_the_cases_a_mode_ran() {
        assert_eq!(mode(610, 670).share(), 91);
        assert_eq!(mode(0, 670).share(), 0);
        assert_eq!(mode(670, 670).share(), 100);
    }

    #[test]
    fn a_mode_that_ran_nothing_has_no_share_and_claims_none() {
        assert_eq!(
            mode(0, 0).share(),
            0,
            "a share over no cases is stated as none rather than as a division"
        );
    }

    #[test]
    fn headroom_counts_how_many_times_under_the_bar_a_run_came_in() {
        let bar = |max_us, measured_us| LatencyBar {
            bench: "operations/lookup",
            max_us,
            measured_us,
            claim: "a point read answers in under a millisecond",
        };
        assert_eq!(bar(1000, 9).headroom(), Some(111));
        assert_eq!(bar(1000, 1000).headroom(), Some(1));
        assert_eq!(
            bar(1000, 0).headroom(),
            None,
            "a measurement of zero has no ratio to state"
        );
    }

    #[test]
    fn a_bar_is_drawn_against_the_largest_value_beside_it() {
        assert_eq!(relative_width(118, 118), 100);
        assert_eq!(relative_width(59, 118), 50);
        assert_eq!(relative_width(0, 118), 0);
        assert_eq!(
            relative_width(9, 0),
            0,
            "a column of zeroes draws nothing rather than dividing by none"
        );
    }
}
