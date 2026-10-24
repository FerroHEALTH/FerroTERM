// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routed screens, each drawn on the server alone.
//!
//! A screen reads what it shows on the server while the page renders
//! ([`parts::loaded`]), so the browser receives the whole page as HTML and
//! hydrates the islands in it and nothing else.

// The diagnostic lands on macro output rather than on an item written here,
// so the expectation covers the module tree.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]

pub(crate) mod about;
pub(crate) mod browse;
pub(crate) mod code_system;
pub(crate) mod code_system_editor;
pub(crate) mod code_systems;
pub(crate) mod composer;
pub(crate) mod concept_map;
pub(crate) mod concept_map_editor;
pub(crate) mod concept_maps;
pub(crate) mod configuration;
pub(crate) mod dashboard;
pub(crate) mod detail;
pub(crate) mod editor;
pub(crate) mod errors;
pub(crate) mod expand;
pub(crate) mod find;
pub(crate) mod history;
pub(crate) mod listing;
pub(crate) mod lookup;
pub(crate) mod operations;
pub(crate) mod parts;
pub(crate) mod preferences;
pub(crate) mod server;
pub(crate) mod shortcuts;
pub(crate) mod sign_in;
pub(crate) mod sync;
pub(crate) mod translate;
pub(crate) mod validate;
pub(crate) mod value_set;
pub(crate) mod value_sets;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

    /// Text that would mean a screen knows one code system from another.
    ///
    /// Every screen renders what `TerminologyCapabilities` declares, so a
    /// canonical written into a screen is a defect: a system the server has
    /// never served before has to draw with no change to any of these files.
    const CANONICAL_FRAGMENTS: [&str; 8] = [
        "snomed.info",
        "loinc.org",
        "nlm.nih.gov",
        "unitsofmeasure.org",
        "who.int",
        "hl7.org/fhir/sid",
        "urn:ietf:bcp",
        "urn:iso:std:iso",
    ];

    /// The directories holding the screens and the screen logic behind the
    /// authoring screens.
    ///
    /// Each entry is a directory rather than a list of files, so a screen
    /// added tomorrow is covered the day it lands. The reading layer under
    /// `src/fhir` is out of scope, because its tests quote real canonicals to
    /// prove the reader handles them.
    const SCREEN_DIRECTORIES: [&str; 2] = ["src/pages", "src/authoring"];

    /// Every `.rs` file under `directory`, in a stable order.
    fn sources(directory: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let entries = fs::read_dir(directory).expect("the screen directory is in the crate");
        for entry in entries {
            let path = entry.expect("the directory entry reads").path();
            if path.is_dir() {
                found.extend(sources(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
        found.sort();
        found
    }

    #[test]
    fn no_screen_names_a_code_system() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let guard = crate_root.join("src").join("pages").join("mod.rs");
        assert!(
            guard.is_file(),
            "the guard writes the fragments out in order to forbid them, so it exempts itself"
        );
        let mut exempted = 0_u32;
        let mut offences: Vec<String> = Vec::new();
        for directory in SCREEN_DIRECTORIES {
            for path in sources(&crate_root.join(directory)) {
                if path == guard {
                    exempted += 1;
                    continue;
                }
                let text = fs::read_to_string(&path).expect("the source file reads");
                for fragment in CANONICAL_FRAGMENTS {
                    if text.contains(fragment) {
                        offences.push(format!("{} names {fragment}", path.display()));
                    }
                }
            }
        }
        assert_eq!(exempted, 1, "nothing but the guard itself is exempt");
        assert!(
            offences.is_empty(),
            "a screen renders the capability statement and names no system: {offences:?}"
        );
    }

    #[test]
    fn the_guard_reads_the_screens_it_claims_to_read() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for directory in SCREEN_DIRECTORIES {
            assert!(
                !sources(&crate_root.join(directory)).is_empty(),
                "{directory} holds screens, so a passing guard means something"
            );
        }
    }
}
