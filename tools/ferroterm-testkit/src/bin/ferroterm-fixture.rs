// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writes a synthetic fixture to a directory, for a job that serves it from a
//! built binary rather than from a test.
//!
//! `ferroterm-fixture snomed-scaled <DIR> <CONCEPTS>` writes the scaled
//! edition of [`ferroterm_testkit::scaled`] under `DIR`, which the server
//! reads as an artifact directory (`FERROTERM__TERMINOLOGY__INDEX`). The
//! performance gate serves it (`scripts/perf/gate.sh`). It exits 0 when the
//! edition is written, 1 when a writer fails and 2 on a usage error.
#![expect(
    clippy::print_stderr,
    reason = "a binary reports its usage and its failure on stderr"
)]

use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

use ferroterm_testkit::scaled;

const USAGE: &str = "usage: ferroterm-fixture snomed-scaled <DIR> <CONCEPTS>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [kind, dir, concepts] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let (true, Ok(concepts)) = (kind == "snomed-scaled", concepts.parse::<u32>()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let dir = Path::new(dir);
    let written: Result<(), Box<dyn Error>> = match std::fs::create_dir_all(dir) {
        Ok(()) => scaled::write(dir, concepts).map_err(Into::into),
        Err(error) => Err(error.into()),
    };
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut text = format!("ferroterm-fixture: {error}");
            let mut cause = error.source();
            while let Some(inner) = cause {
                text.push_str(": ");
                text.push_str(&inner.to_string());
                cause = inner.source();
            }
            eprintln!("{text}");
            ExitCode::FAILURE
        }
    }
}
