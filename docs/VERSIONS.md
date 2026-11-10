<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Pinned version matrix

The single source of truth for every version pin in FerroTERM. If this file and a
config file (`Cargo.toml`, `rust-toolchain.toml`, a vendored package's
`PROVENANCE.md`, `CITATION.cff`) disagree, that is drift: fix it, do not let
either silently win. `scripts/checks/versions.sh` enforces the cross-file
agreement it can, and skips (loudly) what does not exist yet in this
discovery-phase repo. `scripts/checks/pin-freshness.sh`, run weekly by
`.github/workflows/pin-freshness.yml`, compares every pin no Dependabot
ecosystem reads with its newest upstream release and opens one issue when a
pin is behind; a pin cell that ends in `held: <reason>` stays behind on
purpose.

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
  product version, by the step The Cargo Book's SemVer chapter asks for: the
  patch for a compatible change, the minor at patch 0 for a breaking one.
  `scripts/checks/crate-version-guard.sh` enforces the bump, its SemVer class
  and the lockstep, `scripts/checks/semver.sh` runs `cargo-semver-checks`
  (pinned under CI tools below), and `scripts/checks/versions.sh` checks the
  members' metadata (README, LICENSE, `publish = true`).

## Language and runtime

| Item | Pin |
|---|---|
| Rust toolchain | 1.99.1 (stable, pinned in `rust-toolchain.toml`) |
| Edition | 2024 |
| Cargo resolver | 3 |
| MSRV | 1.98 (`rust-version` in the root `Cargo.toml`, checked by `cargo hack check --rust-version`); the toolchain is the newest stable Rust release and the MSRV the toolchain minus one minor release, the rule every FerroHEALTH repository follows; an MSRV is only ever raised |
| Container base image | `gcr.io/distroless/static-debian13:nonroot@sha256:d62067879de890a1def9da0e7766b7d20810a5e5555cef8506b752437d7a3e88` (`ca-certificates` 20250419+deb13u1), the `FROM` of `docker/Dockerfile`, `docker/Dockerfile.sync` and `docker/console/Dockerfile`, and each one's `org.opencontainers.image.base.name` label without the digest |

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
(<https://github.com/FerroHEALTH/FerroBRIDGE>), which vendors and pins those
packages. FerroTERM consumes the crate from crates.io like any other
dependency, so no HL7 package is vendored here and a change to the model is
requested on that repository's tracker.

| Package | Pin | Notes |
|---|---|---|
| `fhir-types` | 0.1.130 | from crates.io; the requirement lives in the root `Cargo.toml` `[workspace.dependencies]` |

The requirement is a caret, so a later 0.1.x release resolves into `Cargo.lock`
by itself. `scripts/checks/versions.sh` fails when the row above, the root
requirement, and the lock disagree, so a new FHIR model reaches the build only
in a change that moves the pin with it.

| Item | Pin |
|---|---|
| FHIR R6 terminology bundles | 6.0.0-ballot5 (the `hl7.fhir.r6.core` package `crates/fhir-terminology/data/fhir/r6/` is generated from, the R6 package FerroBRIDGE pins), held: snapshot builds are not releases |

## SNOMED CT / ECL

| Item | Pin |
|---|---|
| ECL | 2.3 (the tag of the official grammar repository; `ECL.g4` and the example corpus vendored under `crates/sct-ecl/vendor/` by `scripts/vendor/ecl-grammar.sh`) |
| SCG | 23d3812cf17ac8459fc9f1a0068041b95bacd3e1 (the commit of the official `IHTSDO/SNOMEDCT-Languages` repository, which publishes no tags; the normative Compositional Grammar ABNF and the example corpus vendored under `crates/sct-scg/vendor/` by `scripts/vendor/scg-grammar.sh`) |
| SNOMED CT content | **not pinned in-repo**: licence-gated, bring-your-own RF2 (International edition); the loaded edition+version is a runtime/deployment fact (`.claude/rules/snomed-terminology.md`) |

## Registry data

`scripts/vendor/registries.sh` reads these two pins and vendors the data
under `crates/fhir-terminology/data/`. The IANA Language Subtag and Media
Types registries have no release to pin: the script reads them as
published, and `PROVENANCE.md` records the registry File-Date and the fetch
date.

| Item | Pin |
|---|---|
| Unicode CLDR | 49.0.0 (the release tag of `unicode-org/cldr-json`; the English territory names and the ISO 3166-1 code mappings behind the ISO 3166 code system) |
| UCUM | v2.2 (the release tag of `ucum-org/ucum`; `ucum-essence.xml` and its licence behind the UCUM code system, the bytes FerroBRIDGE pins) |

## Usage report contract

| Item | Pin |
|---|---|
| FerroPULSE report v2 | f83df34cc968f5e0f916834508336c8eeb4c9e41 (the commit of the private `FerroHEALTH/FerroPULSE` repository; the report envelope v2 JSON Schema and its examples vendored under `app/ferroterm-server/tests/fixtures/ferropulse/` by `scripts/vendor/ferropulse-schemas.sh`) |

## Rust dependency pins

The authoritative, fully-pinned third-party crate set lives in the root
`Cargo.toml` `[workspace.dependencies]`. This file
does not duplicate crate versions; on any discrepancy, the manifest wins. Add a
dependency to a crate with `dep.workspace = true`. The one crate this file also
names is `fhir-types` above, because it carries the served FHIR model and its
version is a conformance fact, not a build detail.

## Console toolchain

The console (`app/ferroterm-console`) is an axum service that draws every page
on its server with FerroUI (`ferrohealth-ui`, pinned in the root `Cargo.toml`
like every other dependency), whose stylesheet, themes and script modules the
binary carries and serves. It builds with an ordinary `cargo build`: no
WebAssembly target, no site bundle and no Node. The one tool pinned here is
the browser its journeys drive.

| Item | Pin |
|---|---|
| Chrome for Testing | 157.0.8103.38 with its chromedriver, installed by `browser-actions/setup-chrome` in the `ui-e2e` job of `ci.yml`, and on a developer machine by `scripts/release/journey-tools.sh`, which checks each zip against its SHA-256 |

## CI tools

The CI tools a workflow installs (zizmor, actionlint, shellcheck and the
cargo tools) are pinned in the workflow step that runs them. The clone
detector of `scripts/checks/duplication.sh` (#950) also runs on a developer
machine, so its pins live here and the guard reads them from this table,
and so does the SemVer judge of the published crates (`scripts/checks/semver.sh`):
the release, the official image of that release by digest, and the sha256 of
each release archive `scripts/checks/duplication.sh --install` fetches into
`target/tools/` for a machine with neither the binary on `PATH` nor docker
or podman. The guard refuses a binary that answers another version. Bumping
jscpd moves every row together, plus a fresh `--update` of the count when
the new release counts differently.

| Tool | Pin | Read by |
|---|---|---|
| `jscpd` | `5.4.2` (release v5.4.2 of github.com/kucherenko/jscpd, 2026-10-20) | `scripts/checks/duplication.sh` |
| `jscpd image` | `ghcr.io/kucherenko/jscpd:5.4.2@sha256:112bc771493f25ec412ebf61bef738dc1fbe80d3793090e6a5eeac827ef0ee79` | `scripts/checks/duplication.sh` (docker, podman) |
| `jscpd darwin-arm64` | `b51d22d7c7610d27d8fd022577021ff3266a10f13295a2e545a8b8e126133a88` (`jscpd-darwin-arm64.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd darwin-x64` | `ae8d24aba2719df7682b1bb2c5021ab096116b8240c45fdae88654fc1ff36b17` (`jscpd-darwin-x64.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd linux-x64` | `5f18a6b70b4210782fc3c401c780ba994a2777249cd44d97d6fa460f6fff4570` (`jscpd-linux-x64-musl.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd linux-arm64` | `3635d652ae08ae10bfbc6821bfa8ac14080101f317fb5f45f4b590d5d32b9339` (`jscpd-linux-arm64-musl.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `cargo-semver-checks` | `0.51.0` | `scripts/checks/semver.sh --pin`, which the `crate-version-guard` job of `ci.yml` and the `plan` job of `release.yml` install through `taiki-e/install-action`; the version FerroBRIDGE, FerroSYS, FerroAUTH and FerroAUDIT pin |

Trivy is the one CI tool pinned outside its workflow step: every workflow
that scans (`release.yml`, `scan-and-tag.yml`, `image-scan.yml`) sets
`TRIVY_VERSION`, and `scripts/release/install-trivy.sh` installs that
version from the GitHub release tarball `trivy_<version>_Linux-64bit.tar.gz`,
checked against the sha256 the script pins for it (#1089). The guard fails a
`TRIVY_VERSION` the workflows disagree on, or one the script pins no sha256
for; a bump adds the version's row to the script, read from the release's
`trivy_<version>_checksums.txt`.

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`); Dependabot bumps them.

## Conformance suite

| Item | Pin |
|---|---|
| HL7 terminology ecosystem test cases | `HL7/fhir-tx-ecosystem-ig` at `f158adedf67b280e2a596f805194489205bfe9bd` (test cases 1.10.0), `tests/` only. The requirements are cited by their unversioned page (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>): the versioned path does not resolve, and this table is where the version is pinned. |
| FHIR Validator (`txTests` runner) | 7.0.4, `validator_cli.jar` sha256 `43e29fa8822d670740a909c395ca5b70a3d07fbaa8936f8dedc6a98b007f555b` |

Both pins live in `scripts/checks/tx-ecosystem.sh`; the pass list is
`conformance/tx-ecosystem/passing.txt`.
