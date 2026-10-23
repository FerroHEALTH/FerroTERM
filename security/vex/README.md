<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# VEX statements

Vulnerability Exploitability eXchange documents, in [OpenVEX](https://openvex.dev)
format, asserting the exploitability of specific findings in the artefacts
FerroTERM publishes: the server tarballs, the `ferroterm` image and the
`ferroterm-console` image.

A VEX document is the machine-readable form of an argument, and it carries the
argument with it: each statement names the vulnerability, the product, a
`status`, a controlled-vocabulary `justification`, and an `impact_statement` a
reader can check. An ignore list records the decision without the reasoning,
and nobody can re-evaluate it later.

## Rules

- **`not_affected` needs a justification from the OpenVEX vocabulary**, and the
  `impact_statement` says concretely why the code is unreachable in FerroTERM's
  use. "Low risk" is not a justification.
- **A finding we can fix is fixed, not VEXed.** These documents exist for
  findings in inherited upstream layers and dependencies, where the fix belongs
  to someone else.
- **Re-check on every base-image bump.** When the distroless base rebuilds its
  packages, statements about them can become obsolete, and a statement whose
  finding no longer fires is deleted.
- **An `affected` statement leaves its finding visible.** `trivy --vex`
  removes a finding only for `not_affected` or `fixed`; an `affected` finding
  stays in every scan with its action statement beside it in the release
  record, which is intended: it applies to the reader until the fix ships.
- **Every finding a release ships carries a statement.** The release pipeline
  (`.github/workflows/release.yml`, job `vulnerability-record`) scans both
  images and the server's glibc tarballs with no severity floor and unfixed
  findings included, and `scripts/checks/vex-coverage.sh` refuses the release
  while a finding has no statement here that judges it: `not_affected` with a
  `justification` or `impact_statement`, `affected` with an
  `action_statement`, or `fixed`. Products are named `pkg:oci/ferroterm`,
  `pkg:oci/ferroterm-console` and `pkg:cargo/ferroterm-server`, and a statement
  matches a finding by its `vulnerability.name` or an `aliases` entry. The
  record is a release asset, `ferroterm-<tag>-vulnerability-record.json`.
- **A Security changelog entry may name a statement here** in place of a
  `GHSA-` id, for a dependency bump the statement shows does not affect any
  shipped artefact (`.claude/rules/changelog.md`).

## Where the documents are read

- `scripts/checks/vex-coverage.sh` in the release record joins every finding
  with these documents itself.
- `.github/workflows/image-scan.yml` (weekly) and
  `scripts/security/scan-images.sh` (the same scan, locally) pass every
  document to Trivy as `--vex`. Trivy takes VEX only as that flag, and a
  `pkg:oci/...` product matches only a published reference.
- The release gate (`.github/workflows/scan-and-tag.yml`) passes none: it scans
  the untagged digest a `pkg:oci/...` product cannot match, and fails only on
  HIGH and CRITICAL findings that have a fix, which a rebuild resolves.

## Documents

| File | Subject | Authored |
|---|---|---|
| `rust-advisories.openvex.json` | The Rust dependency advisories the advisory gate accepts. Each statement additionally carries a `ferroterm:reachability` block (our own extension, since OpenVEX defines none) naming the crates the affected package is reached through. | **generated** |

There is no document for the operating-system layer. Both images are static
distroless (`gcr.io/distroless/static-debian13`, musl binaries), and a scan
of the base at the pinned digest reports no finding. A finding the release
record reports in that layer gets a hand-written `ferroterm-os.openvex.json`,
one statement per vulnerability, naming the images it covers.

## The generated document

`rust-advisories.openvex.json` is produced by
`scripts/security/vex-generate.sh` from two inputs, and is never edited by
hand:

- **`deny.toml`** `[advisories].ignore`: the authoritative set of advisory
  ids. It is the gate that decides whether a build passes, so it is the only
  place the id list lives.
- **`security/vex/rust-advisories.toml`**: the reasoning, the OpenVEX
  `status`, the controlled-vocabulary `justification`, and the
  `impact_statement` for each id.

The generator refuses to emit anything unless the two id sets match in both
directions, and `scripts/checks/vex-advisories.sh`, the `vex` CI job,
regenerates the document and fails on any difference. Adding an ignore to
`deny.toml` without publishing its justification is a red build.

Agreement is not truth: an ignore and its justification stay in agreement
while a dependency upgrade resolves the advisory underneath both.
`scripts/checks/advisory-exceptions.sh` (in the `cargo-deny` CI job) promotes
cargo-deny's `advisory-not-detected` diagnostic to an error, so an exception
that has outlived its finding fails the build.

A statement can also agree, describe an advisory that still fires, and argue
from a dependency path that is not the real one. So each statement carries its
path as data (`carriers`, `workspace_roots`), and
`scripts/checks/vex-reachability.sh`, also in the `cargo-deny` job, compares
both sets against the graph cargo resolves, exhaustively in both directions.
They travel into the published document as the `ferroterm:reachability` block,
so a consumer reads the same claim the gate checks.

To change a statement: edit `rust-advisories.toml` (and `deny.toml` if the id
set changes), bump the document's `version` and `timestamp`, then run
`bash scripts/security/vex-generate.sh`. If the change is about where a crate
enters the build, `carriers` and `workspace_roots` come from
`cargo tree -i <crate>[@<version>] --workspace --all-features --target all -e
normal,build,dev`.

Advisories a scanner reading only `Cargo.lock` reports but cargo-deny never
raises (it resolves features) go in the `[[lockfile_only]]` section of
`rust-advisories.toml`, never in `deny.toml`. The section is empty today.
