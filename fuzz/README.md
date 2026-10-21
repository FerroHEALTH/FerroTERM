<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The fuzz targets

Every input a client or a release hands the server passes a hand-written
parser, and each one answers a malformed input with a typed error, never a
panic and never a 500 (`.claude/rules/reliability.md`). `proptest` covers the
shapes we thought of; these targets cover the ones we did not.

| Target | Feeds |
|---|---|
| `ecl_parse` | arbitrary text to `sct_ecl::parse` |
| `ecl_roundtrip` | a parsed expression prints and parses back to the same tree |
| `fhir_json` | arbitrary JSON to the generated R4B `Parameters`, `ValueSet`, `CodeSystem`, and `ConceptMap` decoders |
| `fhir_xml` | arbitrary text to the FHIR XML reader over the generated schemas |
| `snomed_uri` | arbitrary text to the implicit `?fhir_vs=` and `?fhir_cm=` resolution |
| `rf2_row` | arbitrary text to the RF2 file-name, effective-time, and identifier parsers |

## Running

The crate is outside the workspace, because `cargo-fuzz` builds with sanitizer
flags that need a nightly toolchain and the product stays on the pinned stable
one (<https://rust-fuzz.github.io/book/cargo-fuzz.html>).

```bash
cargo install cargo-fuzz
mkdir -p fuzz/corpus/ecl_parse && cp fuzz/seeds/ecl_parse/* fuzz/corpus/ecl_parse/
cargo +nightly fuzz run ecl_parse fuzz/corpus/ecl_parse -- -max_total_time=300
```

`seeds/` is committed and holds the shapes each parser admits, written with
identifiers only: no term, no definition, no hierarchy, so it distributes no
SNOMED CT content (`.claude/rules/vendored-inputs.md`). `corpus/` and
`artifacts/` are a run's own output and are not committed.

A crash writes its input under `artifacts/<target>/`. Reproduce it with
`cargo +nightly fuzz run <target> artifacts/<target>/<file>`, shrink it with
`cargo +nightly fuzz tmin <target> <file>`, then fix the defect and pin it with
a test in the crate that owns the parser. The weekly `fuzz.yml` lane runs every
target for a bounded time and uploads what it finds.

## What the first runs found

`ecl_parse` found a stack overflow: the parser is recursive descent, so an
expression of deeply nested brackets descended until the process aborted. A
stack overflow is not a panic, so it takes the server with it rather than
unwinding into a 500. `sct_ecl::NESTING_LIMIT` now bounds the nesting and the
parser refuses past it, which the server renders as an `OperationOutcome`
(`crates/sct-ecl/tests/it/grammar.rs`,
`app/ferroterm-server/tests/it/value_set.rs`).
