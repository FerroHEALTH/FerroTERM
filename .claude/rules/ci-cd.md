---
paths:
  - ".github/**"
  - "scripts/**"
  - "deny.toml"
---

# CI/CD + supply-chain discipline

No spec governs this: our own design, grounded in the OWASP GitHub Actions
Security Cheat Sheet, SLSA v1.0, OpenSSF Scorecard, and Sigstore. The full
posture (lanes, the release pipeline, how we hit SLSA L3 + signed SBOM +
Scorecard) is in `docs/ci-cd.md`; this file is the enforceable discipline.

## Workflow security (every workflow, no exceptions)

- **Every `uses:` is pinned to a full commit SHA** with a trailing `# vX.Y.Z`
  comment. Dependabot (`github-actions`) bumps them. A tag or branch ref is a
  finding.
- **`permissions: {}` at workflow level**, with the minimum granted per job.
- **`persist-credentials: false`** on every `actions/checkout` that does not push
  with git.
- **No `${{ }}` context interpolation inside `run:`:** pass context through
  `env:`. (Prevents template injection.)
- **Publishing lanes restore no build cache** (a cache an untrusted run could
  poison must not feed a release).
- Enforced by the `ci.yml` workflows lane: `actionlint` + `zizmor
  --min-severity=low` over `.github/workflows/`, and `shellcheck --severity=style`
  over tracked shell scripts. These run even before any Rust exists.

## The merge queue and the pull-request guards

- **`main` merges through the merge queue.** `ci.yml` runs on `merge_group`,
  and `conclusion` is the one required check. A job that reads the
  pull-request payload is gated on `github.event_name == 'pull_request'`, so
  it skips in the queue and `conclusion` counts the skip as a pass. A new
  required job keeps that shape or runs on every event.
- **Tracker helpers and changelog fragments are checked on every event:** the
  `tracker-helpers` job runs the `--self-test` of every `scripts/gh/` helper,
  and the `changelog` job runs `scripts/release/changelog.sh --check` plus
  both self-tests. `changelog-guard` fails a pull request with no fragment
  under `changelog.d/` unless it carries `no-changelog`.

## Rust CI lanes (activate with the Cargo workspace)

`cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features
-- -D warnings`; `cargo nextest run --workspace --locked` + `cargo test --doc
--locked`; `cargo doc` with `RUSTDOCFLAGS=-D warnings`; `cargo deny check`
(advisories/licenses/bans/sources, subsumes cargo-audit); MSRV via `cargo hack
check --rust-version`; `dependency-review-action` on PRs. **Always `--locked`** so
CI fails on lockfile drift, never on registry drift. Commit `Cargo.lock`.

## Supply chain

- **Releases are SLSA Build L3.** The build AND its attestation live in a
  REUSABLE workflow (`release-build.yml`, `on: workflow_call`) so the builder is
  isolated and the signing identity is not reachable by build steps. That
  isolation is what makes the provenance non-falsifiable. Do not inline the
  build+attest back into a normal job.
- **Every release artifact carries provenance + a signed SBOM.**
  `cargo auditable build` embeds the dependency list; `cargo cyclonedx` emits the
  CycloneDX SBOM; `actions/attest-build-provenance` and `actions/attest-sbom`
  sign both keyless (Sigstore/Fulcio, `id-token: write`). Consumers verify with
  `gh attestation verify … --signer-workflow …/release-build.yml`.
- **Releases are immutable** (draft → attach all assets → verify the set →
  publish last; the platform freezes on publish). The fix for a bad cut is a new
  patch version, never a retag.
- **The `crates/*` members publish to crates.io through Trusted Publishing
  (OIDC), per crate, in dependency order, approval-gated by the `crates-io`
  environment** (`crates-publishing.md`): the `crates` leg of `release.yml`
  and the `publish-crates.yml` dispatch lane share
  `scripts/release/publish-crates.sh`; the `crate-version-guard` and
  `publish-dry-run` CI jobs keep the line publishable on every PR.
- **Version pins have a single source of truth** (`docs/VERSIONS.md`);
  `scripts/checks/versions.sh` (the `versions` CI job) fails on cross-file drift
  (`CITATION.cff` ↔ `Cargo.toml`, vendored `PROVENANCE.md` ↔ the pin table).
- **NEVER add AI/Claude attribution** to any commit, PR, or release text.

## Never

- Never unpin a `uses:` to a tag/branch, widen a job's permissions without cause,
  interpolate context into `run:`, or restore a cache in a publishing lane.
- Never weaken a gate to go green (`testing.md`); fix the cause.

## Reference

Official sources and the full design: `docs/ci-cd.md`.
