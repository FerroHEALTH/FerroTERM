// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dialect aliases of the ECL specification.
//!
//! Appendix C, "Dialect Aliases"
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language/appendices/appendix-c-dialect-aliases>):
//! each alias names one language reference set.

/// `(alias, language reference set SCTID)`, as the appendix lists them.
pub const ALIASES: [(&str, u64); 29] = [
    ("da-dk", 554_461_000_005_103),
    ("de", 722_130_004),
    ("en-au", 32_570_271_000_036_106),
    ("en-ca", 19_491_000_087_109),
    ("en-gb", 900_000_000_000_508_004),
    ("en-gb-x-drug", 999_000_681_000_001_101),
    ("en-gb-x-ext", 999_001_251_000_000_103),
    ("en-ie", 21_000_220_103),
    ("en-nz", 271_000_210_107),
    ("en-nz-x-pat", 281_000_210_109),
    ("en-us", 900_000_000_000_509_007),
    ("en-x-gmdn", 608_771_002),
    ("en-x-nhs-clinical", 999_001_261_000_000_100),
    ("en-x-nhs-dmd", 999_000_671_000_001_103),
    ("en-x-nhs-pharmacy", 999_000_691_000_001_104),
    ("es", 450_828_004),
    ("es-uy", 5_641_000_179_103),
    ("et-ee", 71_000_181_105),
    ("fr", 722_131_000),
    ("fr-be", 21_000_172_104),
    ("fr-ca", 20_581_000_087_109),
    ("ja", 722_129_009),
    ("mi", 291_000_210_106),
    ("nb-no", 61_000_202_103),
    ("nl-be", 31_000_172_101),
    ("nl-nl", 31_000_146_106),
    ("nn-no", 91_000_202_106),
    ("sv-se", 46_011_000_052_107),
    ("zh", 722_128_001),
];

/// The language reference set an alias names, compared case-insensitively.
#[must_use]
pub fn refset(alias: &str) -> Option<u64> {
    ALIASES
        .iter()
        .find(|(a, _)| a.eq_ignore_ascii_case(alias))
        .map(|(_, refset)| *refset)
}

#[cfg(test)]
mod tests {
    use super::refset;

    #[test]
    fn aliases_resolve_without_case() {
        assert_eq!(refset("en-GB"), Some(900_000_000_000_508_004));
        assert_eq!(refset("nl-nl"), Some(31_000_146_106));
        assert_eq!(refset("xx"), None);
    }
}
