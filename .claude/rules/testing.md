---
paths: ["crates/**", "addons/**", "app/**", "tools/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Testing discipline

Test discipline is non-negotiable (a standing hard rule; see `CLAUDE.md`). It
applies to every crate, generated and hand-written alike.

## The hard rule

- **Never** silently weaken, skip, or delete an existing test to make a build
  pass.
- **Never** edit a test to route around a runtime bug it exposes. If a test
  fails and the fix is unclear, leave it failing and record a
  `// TODO(#NNNN):` naming its issue; do not touch the test to make it green.
- Conformance tests assert the **FHIR / SNOMED CT / ECL specifications**:
  cite the spec clause a test encodes; never adjust an expectation to match an
  implementation bug. A fixture defect is ADJUDICATED with a first-hand spec
  citation (an expected-rejection entry in the owning test), never routed
  around by editing the case.

## Tooling

- **Runner:** `cargo-nextest` (`cargo nextest run --workspace`), not
  `cargo test`.
- **Snapshots:** `insta` pins FHIR JSON output (per version) against golden
  vectors, the key tool for serialization parity across R4/R4B/R5/R6.
  Redact volatile fields before snapshotting. Review intentional changes with
  `cargo insta review`; never accept a snapshot change you have not read.
- **Properties:** `proptest` for FHIR round-trips (serialize → parse → equal),
  ECL parse/print round-trips, and set-algebra invariants (e.g. `<<X`
  contains `X`; ancestors and descendants are inverse relations).
- **HTTP mocking:** `wiremock` for the reference-server comparison client and
  any external integration test.
- **Benches:** `criterion` + `divan`, kept separate from correctness tests:
  subsumption and expansion latency are the numbers the design targets
  (`docs/architecture.md`).

## Oracles and the acceptance instrument

The acceptance oracles for this project are the FHIR terminology conformance
expectations and the reference servers, NOT a bespoke conformance runner.

- **The HL7 `fhir-tx-ecosystem-ig` test cases are the per-version conformance
  gate**, run by the FHIR Validator's `txTests` mode against a running server.
  Note: the Validator is a Java tool, so this gate runs a JVM IN CI only,
  never in the server runtime (the "no JVM" property is a runtime property).
  <https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/>
- **Snowstorm and Hermes are the reference servers** for differential testing.
  `$lookup`, `$subsumes`, `$validate-code`, `$expand`, and `$translate` results
  are compared against them over the SAME SNOMED edition. Where a reference
  server and the spec disagree, the spec wins and the divergence is recorded
  (the reference server is prior art, never the oracle; a bug in Snowstorm is
  not a requirement). This is a PERIODIC/manual gate, not per-PR: it needs a
  licensed SNOMED edition (uncommittable) and a heavy reference stack, so it
  runs on a schedule or before a release, with the edition + oracle versions
  pinned in the test artifacts.
  - Snowstorm: <https://github.com/IHTSDO/snowstorm>
  - Hermes: <https://github.com/wardle/hermes>
- **ECL is tested against its published grammar** (the ECL ANTLR grammar,
  <https://github.com/IHTSDO/snomed-expression-constraint-language>): the
  evaluator is built and tested as its own layer, against the grammar and a
  set of spec-cited expression/result pairs, BEFORE the value-set surface
  leans on it.
- **The FHIR terminology operations are tested against the operation
  definitions** in the vendored, pinned FHIR packages (per version): the set
  of parameters a version admits is exactly what the `OperationDefinition`
  declares plus the terminology ecosystem overlay, each overlaid parameter
  tested per version (`spec-adherence.md`).
- **Classification parity** (when a reasoner is added): the inferred
  hierarchy is checked against SNOMED's shipped inferred-relationship and
  transitive-closure files.
- Prefer an existing spec-published example over a hand-written fixture. A
  test that encodes a spec rule cites the spec section it asserts
  (`spec-adherence.md`).
- **SNOMED fixtures are shaped/synthetic only:** never real concepts
  extracted from a licensed RF2 release (`vendored-inputs.md`). Build a small
  synthetic hierarchy in-test to exercise subsumption and ECL; the
  reference-server comparison runs against a locally-provisioned licensed
  edition that is never committed.

## Fuzzing (the parsers a client can reach)

Every hand-written parser an untrusted input reaches has a `cargo-fuzz` target
under `fuzz/`: ECL, the FHIR JSON and XML bodies, the SNOMED implicit URI
forms, and the RF2 fields. The crate sits outside the workspace because
cargo-fuzz needs nightly, and the weekly `fuzz.yml` lane runs each target for a
bounded time (`fuzz/README.md`).

- **A crash is a defect, and it is fixed, never tolerated.** The reproducing
  input is shrunk (`cargo fuzz tmin`) and pinned by a test in the crate that
  owns the parser, so the same input never regresses.
- **A hang and a stack overflow count as crashes.** A panic unwinds into a
  clean 500; a stack overflow aborts the process, so recursive descent over
  client input carries a depth limit (`sct_ecl::NESTING_LIMIT`).
- **Seeds are committed, corpora are not.** `fuzz/seeds/` holds the shapes each
  parser admits, written with identifiers only, so it distributes no SNOMED CT
  content; a run's own corpus and artifacts stay out of the repository.

## Where tests live

Unit tests live beside the code they test (`#[cfg(test)] mod tests` in the
same file), and ONLY there: **dedicated test FILES under `src/` are banned**.
A test that drives the public API belongs in the owning crate's `tests/`
directory (`crates/*/tests/`, `app/*/tests/`, `tools/*/tests/`); a test of
private internals stays a small inline module. If an internals test grows
large, that is a design signal to test through the public seam, not to split
into a src file.

**One integration-test binary per crate**: the `tests/` directory is
`tests/it/main.rs` + one `mod` per topic file, NOT one top-level `.rs` per
topic. Cargo compiles and links every top-level `tests/*.rs` as its own crate
("each integration test results in a separate executable binary … this can be
inefficient", https://doc.rust-lang.org/cargo/reference/cargo-targets.html);
nextest still runs each test as its own process, so isolation is unchanged.
Shared helpers live in a plain module under `tests/it/`.

**A binary-only crate is untestable by construction** (Book ch11.3): its
`main.rs` cannot be imported from `tests/`. The server (`app/ferroterm-server`)
therefore keeps a thin `main.rs` over a testable `lib.rs` run path (Book
ch12.3), and its integration tests import the lib.

## Test shapes (the Book ch11 doctrine)

- **`Result`-returning tests are the preferred shape**: `fn t() ->
  Result<(), E>` with `?` instead of unwrap chains
  (https://doc.rust-lang.org/book/ch11-01-writing-tests.html). Plumbing
  failures propagate with `?`, not `.unwrap()`. `clippy::panic_in_result_fn`
  fires on a Result-returning test that also asserts and clippy offers no
  `allow-…-in-tests` knob for it, so such a test carries the lint in the same
  scoped relaxation its file uses for `panic`/`unwrap`/`expect`
  (`#![allow(…, reason = "test assertions")]` at the test-file root, or a
  `#[expect(…, reason)]` on the single test). Never relaxed at the workspace
  level and never in a non-test module.
- **`#[should_panic]` always carries `expected = "…"`:** bare `should_panic`
  passes when the code panics for the WRONG reason (Book ch11.1).
  `should_panic` is illegal on Result-returning tests; assert `value.is_err()`
  there instead.
- **Assertions**: `assert_eq!`/`assert_ne!` over bare `assert!` for
  comparisons (they print both values); production-code asserts carry a
  message.
- **Doctests are copy-paste templates**: `?` via a hidden `# Ok::<(), E>(())`
  tail or hidden `fn main`, never `unwrap` (C-QUESTION-MARK; enforced by
  `#![doc(test(attr(deny(warnings))))]` on library roots). `no_run` for
  examples that would open a `redb` store or touch HTTP, `text` for non-code,
  never `ignore`.

## Coverage is a mandate, not just pass rate

A green suite over a thin set of cases proves almost nothing. The bar is
COVERAGE of what the specs define on the wire:

- Every terminology operation, per FHIR version, with each parameter branch
  the version's `OperationDefinition` admits, each as its own small,
  ISOLATED case so a failure localizes to one behaviour.
- Every ECL operator (`<`, `<<`, `>`, `>>`, `^`, `.`, `:` refinement, `AND`,
  `OR`, `MINUS`, cardinality, dotted attribute traversal, nested
  constraints), each with a spec-cited expected set.
- Every `$expand` paging and filter branch; every error family (unknown code,
  unknown system, invalid ECL, unsupported parameter for the version).
- **A spec-defined behaviour with no case is a COVERAGE GAP, never an
  acceptable omission.** Close it (a new spec-cited case) or record the
  honest boundary. Silence is not coverage.
- **Coverage only ratchets up.** Cases are added, never removed to go green.
- **One behaviour per case:** many small isolated cases beat one broad case.

## Target

Compiling, clippy-clean, tested increments at all times; a green suite is the
standing bar and every change preserves it. Green comes ONLY from fixing the
defect after spec-adjudicated attribution (`spec-adherence.md`), never from
bending a test or a fixture to match the implementation.
