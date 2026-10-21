<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Generated code: consumed here, produced elsewhere

FerroTERM no longer generates a FHIR layer. The `fhir-types` crate and its
generator moved to the FerroBRIDGE repository, which vendors the pinned HL7
FHIR packages and publishes the crate to crates.io (#300). This repository
depends on it like any other third-party crate, and nothing here emits Rust.

## Consuming the model

- **The FHIR model is `fhir-types` from crates.io**
  (<https://docs.rs/fhir-types>), required by version in the root
  `Cargo.toml` `[workspace.dependencies]` and taken by a crate with
  `fhir-types.workspace = true`.
- **Consume the generated types directly.** Never re-model or re-serialize
  FHIR by hand, and never shadow a generated shape with a local type, a
  duplicate model, an adapter layer, or a placeholder value. That silently
  forks the FHIR model, which is the whole reason the model is generated.
- **A wrong or missing shape is fixed upstream.** When engine code hits a
  shape that is wrong or insufficient versus the FHIR specification
  (<https://hl7.org/fhir/>), the fix is a generator change in FerroBRIDGE and
  a release consumed here. A local workaround is forbidden while that lands;
  on discovering an existing one, register its removal.
- **Taking a new release moves three things in one change:** the requirement
  in the root `Cargo.toml`, the pin row in `docs/VERSIONS.md`, and
  `Cargo.lock`. `scripts/checks/versions.sh` fails when they disagree, and
  Dependabot opens that pull request on its own (`.github/dependabot.yml`).

## The one generated artefact committed here

`crates/fhir-terminology/data/fhir/**` holds the FHIR core terminology
bundles: generated from the same pinned HL7 packages, committed, and read at
runtime. Never hand-edit a file there; `data/fhir/PROVENANCE.md` records how
it was produced and what it may carry.

Everything else in the tree is hand-written idiomatic Rust of our own design
(`rust-style.md`), with the FHIR and SNOMED CT specifications as the
authority.
