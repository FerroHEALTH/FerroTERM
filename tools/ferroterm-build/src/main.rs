// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferroterm-build` binary: parse the command line, run the build.
#![expect(clippy::print_stdout, reason = "a command-line tool reports to stdout")]

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = ferroterm_build::Cli::parse();
    let out = ferroterm_build::output::output(&cli)?;
    match ferroterm_build::run_into(&cli, &out)? {
        ferroterm_build::Report::Snomed(report) => {
            println!(
                "{}: {} concepts, {} designations, {} is-a edges, {} words, written to {}",
                report.version_uri,
                report.concepts,
                report.designations,
                report.is_a_edges,
                report.words,
                out.display()
            );
            if report.skipped_relationships > 0 {
                println!(
                    "{} relationship rows left out: outside 900000000000011006 |Inferred relationship|",
                    report.skipped_relationships
                );
            }
        }
        ferroterm_build::Report::Labcodeset(report) => println!(
            "Labcodeset {}: {} active concepts, {} retired, {} ordinal value sets, written to {}",
            report.release,
            report.active,
            report.retired,
            report.ordinals,
            report.dir.display()
        ),
        ferroterm_build::Report::Loinc(report) => println!(
            "LOINC {}: {} terms, {} parts, {} answer lists, {} designations, {} words, written to {}",
            report.version,
            report.terms,
            report.parts,
            report.answer_lists,
            report.designations,
            report.words,
            out.display()
        ),
        ferroterm_build::Report::Icd11(reports) => {
            for report in reports {
                println!(
                    "{} {}: {} concepts, {} designations, {} postcoordination scales, {} words, written to {}",
                    report.system,
                    report.version,
                    report.concepts,
                    report.designations,
                    report.scales,
                    report.words,
                    report.dir.display()
                );
            }
        }
        ferroterm_build::Report::RxNorm(report) => println!(
            "RxNorm {}: {} concepts, {} atoms, {} relationships, {} words, written to {}",
            report.version,
            report.concepts,
            report.atoms,
            report.relationships,
            report.words,
            out.display()
        ),
        ferroterm_build::Report::Classification(report) => println!(
            "{} {}: {} concepts, {} designations, {} words, written to {}",
            report.system,
            report.version,
            report.concepts,
            report.designations,
            report.words,
            out.display()
        ),
        ferroterm_build::Report::Classifications(reports) => {
            for report in reports {
                println!(
                    "{} {}: {} concepts, {} designations, {} words, written to {}",
                    report.system,
                    report.version,
                    report.concepts,
                    report.designations,
                    report.words,
                    report.store.display()
                );
            }
        }
    }
    Ok(())
}
