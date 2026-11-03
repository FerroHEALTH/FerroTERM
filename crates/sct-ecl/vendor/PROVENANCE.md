# Provenance: the SNOMED CT Expression Constraint Language grammar

- Source: <https://github.com/IHTSDO/snomed-expression-constraint-language>
- Tag: 2.3
- Commit: b0e07105ae395821bcc953f3d6084b57dc7bef2c
- Fetched: 2026-11-02 by `scripts/vendor/ecl-grammar.sh`
- Licence: Apache License 2.0 (`LICENSE.md`, vendored verbatim)
- Contents: `syntax/` (the ANTLR grammar `ECL.g4` and the ABNF forms) and
  `examples/` (the valid example corpus), copied verbatim; `README.md`.

The parser in `crates/sct-ecl` mirrors `syntax/ECL.g4` rule for
rule; the corpus is the parse-conformance fixture. Never hand-edit these
files; change the pin in `docs/VERSIONS.md` and re-run the script.
