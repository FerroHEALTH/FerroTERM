# Cutting a release

A milestone is a delivery promise, and a release is cut when its milestone
reaches zero open issues (`.claude/rules/issue-workflow.md`). The lanes that
build, sign, and publish are in `docs/ci-cd.md`; this page is the checklist the
cut follows, in order.

## Before the tag

1. **The milestone is empty.** `gh issue list --milestone vX.Y.Z --state open`
   answers nothing, or the owner calls the cut and moves the stragglers to the
   next milestone.
2. **The version moves everywhere**, not only in the manifests: the workspace
   `version`, `CITATION.cff`, the README, the landing page, the book.
   `scripts/checks/versions.sh` fails on any file left behind.
   The viewer's recorded capability documents carry the software version, so
   re-record them in the same commit:
   `FERROTERM_RECORD_VIEWER_FIXTURES=1 cargo nextest run -p ferroterm-server
   -E 'test(the_viewer_capability_fixtures)'`.
3. **The changelog names the release.** Every change since the last release
   is a fragment under `changelog.d/` (`changelog.d/README.md`), and entries
   written before the fragments may still sit under `[Unreleased]`. Run

   ```sh
   scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD
   ```

   in the version-bump branch. It writes a `## [X.Y.Z] - YYYY-MM-DD` section
   under a fresh empty `[Unreleased]`, holding the `[Unreleased]` entries and
   then the fragments in Keep a Changelog order; it moves the `[Unreleased]`
   link reference on, adds the version's, and `git rm`s the fragments. It
   refuses a malformed fragment, a version that already has a section, and a
   release with no entry. Commit `CHANGELOG.md` and the removals together, and
   read the section as the release notes before you tag.
4. **The gates pass on the release commit**: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --all-features`, `cargo nextest run
   --workspace --locked`, `RUSTDOCFLAGS="-D warnings" cargo doc`, and the
   scripts under `scripts/checks/`.
5. **The latency bars hold**: `scripts/checks/bench-bars.sh` measures the four
   operations over the generated edition and compares each median to the claim
   in `bench/bars.json`. CI runs it on every pull request too.
6. **The measured figures are refreshed.** Run `ferroterm-bench` over the local
   editions on one machine, copy the records into
   `bench/records/<date>-<machine>/`, and re-render with
   `scripts/checks/bench-table.sh check`. The Dutch edition's row carries the
   two figures a deployment plans capacity with: **the ingest time and the
   index size on disk**, beside the concept count and the peak build memory.
   The rendered footer names the FerroTERM version the records came from, so a
   stale set is visible in the published table.
7. **The crate line is publishable**: the `crate-version-guard` and
   `publish-dry-run` jobs are green (`.claude/rules/crates-publishing.md`).

## The tag

The signed tag is the owner's: `git tag -s vX.Y.Z -m "vX.Y.Z" <commit>` on the
merged release commit, then `git push origin vX.Y.Z`. `release.yml` takes it
from there: the draft release, the four per-architecture builds, the image, the
asset verification, the publish, and the crates.io leg behind the `crates-io`
environment's required reviewer.

## After the tag

1. **Verify as a consumer**: download a binary and the image, and verify the
   provenance and the SBOM attestations with `gh attestation verify` against
   the signer workflow (`docs/ci-cd.md`).
2. **Post the board status update** with what shipped and what the next
   milestone targets (`.claude/rules/project-board.md`).
3. **The FerroEHR pin follows.** FerroEHR's quickstart overlay
   (`docker-compose.terminology.yml` in that repository) and its
   `adl2_vetdf_ferroterm` test pin the FerroTERM image by tag and digest. Open
   the pull request there that moves both to the release just cut, with the
   image digest read from GHCR after the image lane finishes. The owner asked
   for this on 2026-09-22, starting with v0.1.4.
4. **A bad cut is a new patch version**, never a retag: the platform freezes a
   published release.
