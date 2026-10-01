# Build and test

This page is a short pointer for contributors. The deeper design and the
working discipline live in the repository, and this book keeps to what you need
to get building.

<!-- toc -->

## Read these first

- [`docs/architecture.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/docs/architecture.md):
  the design authority, with citations.
- [`docs/terminologies.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/docs/terminologies.md):
  every code system, its FHIR page, its licence, and what the provider serves.
- [`CONTRIBUTING.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/CONTRIBUTING.md):
  the contribution rules, branches, commit signing, and pull-request checklist.

## The workspace

One Cargo workspace, Rust 1.98, edition 2024: `crates/*` are the libraries
(`rf2`, `loinc`, `classification`, `rxnorm-rrf`, `icd11` the release readers;
`concept-graph`, `concept-store`, `designation-index` the substrates;
`fhir-terminology` the engine and providers), `app/ferroterm-server` is the
axum server, and `tools/*` are the offline build and the synthetic test
fixtures. Every crate carries a `CLAUDE.md` with its
local discipline.

## The local gates

The local gates mirror CI. Run them before you open a pull request:

```console
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
$ cargo nextest run --workspace --locked
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
$ cargo deny check
$ scripts/checks/comment-style.sh --all
```

Scope a command to a crate with `-p <crate>` while you iterate, then run the
full `--workspace` gates before you push. `Cargo.lock` is committed and CI
builds with `--locked`. Shell scripts stay clean at `shellcheck`; workflows at
`actionlint` and `zizmor`.

## Conformance in CI

`scripts/checks/tx-ecosystem.sh` runs the HL7 terminology ecosystem suite
against a release build of the server with the FHIR Validator's `txTests`
mode (the one place a JVM runs, in CI, never in the server) and compares the
result with the committed pass list under `conformance/tx-ecosystem/`. A
listed test that fails is a regression; a test that starts passing is added in
the same change. `--mode icd-11 --index <dirs>` runs a code system mode by
hand over local artifacts.

## The two layers

- The FHIR model is the generated `fhir-types` crate, taken from crates.io and
  generated in another repository. Never re-model a FHIR shape locally. See
  [The FHIR model](codegen.md).
- Everything else is hand-written, idiomatic Rust, with the FHIR and SNOMED
  specifications as the authority: a conformance-facing test cites the spec
  clause it asserts, and a decision the specs leave open is marked as the
  project's own.

## No code system content in the repository

Tests use shaped, synthetic content only: the testkit writes a small
SNOMED-shaped edition, a LOINC-shaped release, ICD-shaped ClaML and CMS files,
an RxNorm-shaped RRF release, and an ICD-11-shaped API cache, all with
invented identifiers. Never commit a real release or content extracted from
one. See [Loading code systems](../operate/loading-snomed.md) for the
licensing rule; a licensed release for local runs lives under the gitignored
`data/` directory.

## Correctness oracles

The specifications are the oracle. Reference servers (Snowstorm, the Dutch
national server, tx.fhir.org) settle edge cases the specifications leave
silent, and a divergence from them is investigated, never adopted blindly.
