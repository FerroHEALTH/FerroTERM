# Pinned version matrix

The single source of truth for every version pin in FerroTERM. If this file and a
config file (`Cargo.toml`, `rust-toolchain.toml`, a vendored package's
`PROVENANCE.md`, `CITATION.cff`) disagree, that is drift: fix it, do not let
either silently win. `scripts/checks/versions.sh` enforces the cross-file
agreement it can, and skips (loudly) what does not exist yet in this
discovery-phase repo.

## Product and citation version

- The product version is the workspace `version` in the root `Cargo.toml`
  (inherited by the server and the tools), currently **0.1.1**.
- `CITATION.cff` `version` tracks it exactly. The version guard fails if they
  disagree once `Cargo.toml` exists.

## Crate line (crates.io)

- The `crates/*` members carry their own lockstep version in each member's
  `Cargo.toml` and in every internal requirement of the root
  `[workspace.dependencies]` table; `scripts/release/publish-crates.sh version`
  prints the current value. It moves with the crates'
  packaged content (`.claude/rules/crates-publishing.md`), never with the
  product version. `scripts/checks/crate-version-guard.sh` enforces the bump
  and the lockstep; `scripts/checks/versions.sh` checks the members' metadata
  (README, LICENSE, `publish = true`).

## Language and runtime

| Item | Pin |
|---|---|
| Rust toolchain | 1.98.0 (stable, pinned in `rust-toolchain.toml`) |
| Edition | 2024 |
| Cargo resolver | 3 |
| MSRV | 1.98 (`rust-version` in the root `Cargo.toml`, checked by `cargo hack check --rust-version`); the deliverable is a binary, so MSRV tracks the pinned stable |

## openEHR: n/a

FerroTERM is a FHIR/SNOMED project; it has no openEHR pins, and this stays
true now that it serves an archetype's local terminology. It ingests the FHIR
resources a producer derives from an archetype and reads no openEHR artefact,
which is the scope decision recorded on #445 (`docs/terminologies.md`, openEHR
archetype terminology). The one openEHR document cited anywhere is the
Archetype Object Model 2 specification §3.2, for the archetype id a producer
mints a canonical from, and it is cited rather than pinned.

## FHIR

The FHIR model is the `fhir-types` crate: generated from the machine-readable
HL7 FHIR packages and published by the FerroBRIDGE repository
(<https://github.com/rubentalstra/FerroBRIDGE>), which vendors and pins those
packages. FerroTERM consumes the crate from crates.io like any other
dependency, so no HL7 package is vendored here and a change to the model is
requested on that repository's tracker.

| Package | Pin | Notes |
|---|---|---|
| `fhir-types` | 0.1.109 | from crates.io; the requirement lives in the root `Cargo.toml` `[workspace.dependencies]` |

The requirement is a caret, so a later 0.1.x release resolves into `Cargo.lock`
by itself. `scripts/checks/versions.sh` fails when the row above, the root
requirement, and the lock disagree, so a new FHIR model reaches the build only
in a change that moves the pin with it.

## SNOMED CT / ECL

| Item | Pin |
|---|---|
| ECL | 2.2 (the tag of the official grammar repository; `ECL.g4` and the example corpus vendored under `crates/sct-ecl/vendor/` by `scripts/vendor/ecl-grammar.sh`) |
| SCG | 23d3812cf17ac8459fc9f1a0068041b95bacd3e1 (the commit of the official `IHTSDO/SNOMEDCT-Languages` repository, which publishes no tags; the normative Compositional Grammar ABNF and the example corpus vendored under `crates/sct-scg/vendor/` by `scripts/vendor/scg-grammar.sh`) |
| SNOMED CT content | **not pinned in-repo**: licence-gated, bring-your-own RF2 (International edition); the loaded edition+version is a runtime/deployment fact (`.claude/rules/snomed-terminology.md`) |

## Usage report contract

| Item | Pin |
|---|---|
| FerroPULSE report v2 | 095ccb5718a1b5b0adb088f4d8cb143ba3f5f101 (the commit of the private `FerroHEALTH/FerroPULSE` repository; the report envelope v2 JSON Schema and its examples vendored under `app/ferroterm-server/tests/fixtures/ferropulse/` by `scripts/vendor/ferropulse-schemas.sh`) |

## Rust dependency pins

The authoritative, fully-pinned third-party crate set lives in the root
`Cargo.toml` `[workspace.dependencies]`. This file
does not duplicate crate versions; on any discrepancy, the manifest wins. Add a
dependency to a crate with `dep.workspace = true`. The one crate this file also
names is `fhir-types` above, because it carries the served FHIR model and its
version is a conformance fact, not a build detail.

## Console toolchain

The console (`app/ferroterm-viewer`) is a Leptos app rendered on the server
and hydrated in the browser. cargo-leptos builds its WebAssembly half and the
site bundle; the server half is an ordinary `cargo build`. Its crate versions
live in the root `Cargo.toml` like every other dependency, `ferrohealth-ui`
among them; the pins below are the tools.

| Item | Pin |
|---|---|
| Compilation target | `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`) |
| cargo-leptos | 0.3.11, installed by `ci.yml` and `release-viewer.yml` |
| `wasm-bindgen` CLI | 0.2.129, the version of the `wasm-bindgen` crate the root `Cargo.toml` pins |
| Tailwind CSS standalone CLI | 4.3.3, downloaded and checked against its SHA-256 by `scripts/release/viewer-tools.sh`, so there is no Node and no npm |
| Binaryen `wasm-opt` | version_133, downloaded and checked the same way |
| `leptosfmt` | 0.1.33, the `view!` macro formatter |
| Chrome for Testing | 155.0.8059.79 with its chromedriver, installed by `browser-actions/setup-chrome` in the `ui-e2e` job of `ci.yml` |

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`); Dependabot bumps them.

## Conformance suite

| Item | Pin |
|---|---|
| HL7 terminology ecosystem test cases | `HL7/fhir-tx-ecosystem-ig` at `eaec771d82fba4eac596c14963546f39b4ecffe7` (test cases 1.9.3), `tests/` only. The requirements are cited by their unversioned page (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>): the versioned 1.9.3 path does not resolve, and this table is where the version is pinned. |
| FHIR Validator (`txTests` runner) | 6.10.4, `validator_cli.jar` sha256 `1106b9d58f9e363e47bea7c4fc065841e5fc91fe9d062775c3bfdd212bd653cc` |

Both pins live in `scripts/checks/tx-ecosystem.sh`; the pass list is
`conformance/tx-ecosystem/passing.txt`.
