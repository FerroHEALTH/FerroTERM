<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Cutting a release

A milestone is a delivery promise, and a release is cut when its milestone
reaches zero open issues (`.claude/rules/issue-workflow.md`). The lanes that
build, sign, and publish are in `docs/ci-cd.md`; this page is the checklist the
cut follows, in order.

## Before the tag

1. **The milestone is empty.** `gh issue list --milestone vX.Y.Z --state open`
   answers nothing, or the owner calls the cut and moves the stragglers to the
   next milestone. The `plan` job of `release.yml` refuses the tag otherwise,
   naming each open issue or pull request, and
   `GITHUB_REPOSITORY=FerroHEALTH/FerroTERM bash scripts/release/milestone.sh
   check vX.Y.Z` asks the same question before you tag.
2. **The version moves everywhere**, not only in the manifests: the workspace
   `version`, `CITATION.cff`, the README, the landing page, the book.
   `scripts/checks/versions.sh` fails on any file left behind.
3. **The changelog names the release.** Every change since the last release
   is a fragment under `changelog.d/` (`changelog.d/README.md`), and entries
   written before the fragments may still sit under `[Unreleased]`. Run

   ```sh
   scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD
   ```

   in the version-bump branch (`scripts/release/changelog.sh --preview` shows
   the section before you cut it). It writes a `## [X.Y.Z] - YYYY-MM-DD` section
   under a fresh empty `[Unreleased]`, holding the `[Unreleased]` entries and
   then the fragments in Keep a Changelog order; it moves the `[Unreleased]`
   link reference on, adds the version's, and `git rm`s the fragments. It
   refuses a malformed fragment, a version that already has a section, and a
   release with no entry. Commit `CHANGELOG.md` and the removals together, and
   read the section as the release notes before you tag.

   Then re-record the console's capability fixtures in the same commit:
   `FERROTERM_RECORD_VIEWER_FIXTURES=1 cargo nextest run -p ferroterm-server
   -E 'test(the_viewer_capability_fixtures)'`. The recorded documents carry
   the software version and its release date, and the server reads
   `software.releaseDate` from the changelog's release heading when it is
   built, so fixtures recorded before the assembly carry no release date.

   A release with `### Security` entries beside other sections opens that
   section with the reason line of the `changelog.d/*.security-reason.md`
   fragment, and every Security entry names its `GHSA-` advisory or a
   `security/vex/` statement (`.claude/rules/changelog.md`). CI holds every
   pull request to both; the plan job checks the section again.
4. **The gates pass on the release commit**: `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets --all-features`, `cargo nextest run
   --workspace --locked`, `RUSTDOCFLAGS="-D warnings" cargo doc`, and the
   guards under `scripts/checks/`, run as CI runs them. The two that compare a
   change against its base take `origin/main HEAD` on the release branch:

   ```sh
   bash scripts/checks/versions.sh
   bash scripts/checks/comment-style.sh --all
   bash scripts/checks/stray-literals.sh app/ferroterm-console/src
   bash scripts/checks/spdx-headers.sh
   bash scripts/checks/copyright-holder.sh
   bash scripts/checks/vendor-dests.sh
   bash scripts/checks/repo-map.sh
   bash scripts/checks/claims.sh
   bash scripts/checks/extension-definitions.sh
   bash scripts/checks/usage-report-off.sh
   bash scripts/checks/brand-contrast.sh
   bash scripts/checks/code-systems.sh
   bash scripts/checks/bench-table.sh check
   bash scripts/checks/console-boundary.sh
   bash scripts/checks/console-tokens.sh
   bash scripts/checks/addon-boundary.sh
   bash scripts/checks/no-client-in-server.sh
   bash scripts/checks/changelog-guard.sh origin/main HEAD
   bash scripts/checks/crate-version-guard.sh origin/main HEAD
   ```

   `contribution-licence.sh` reads the pull request body, so it runs in CI
   only. The guards that need a running server or the licensed editions run
   in the steps below or in their own lanes: `bench-bars.sh` in step 5,
   `served-bars.sh` over the record set of step 6, `served-artifacts.sh
   --server URL --index DIRS` against a server over the local editions, and
   `tx-ecosystem.sh` and `differential.sh --index DIR` in their CI lanes.
   `conformance-badges.sh <out-dir>` writes the badge files the Docs workflow
   publishes and checks nothing.
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
8. **The advisories are drafted.** Every `GHSA-` id the section names is a
   draft advisory on this repository, ready to publish with the release
   (`docs/post-market.md`).
9. **The preflight passes** at the commit you are about to tag (below).

## Preflight: the plan job, before the tag

The `plan` job verifies every release fact at the tagged commit before anything
is built or published. Its checks are one list, `scripts/release/plan.sh`, and
`scripts/release/preflight.sh` runs the same list on your machine:

```sh
bash scripts/release/preflight.sh v0.2.0
bash scripts/release/preflight.sh v0.2.0 --notes /tmp/notes.md   # keep the notes
```

Run it on the release branch once the release PR is ready, and again on `main`
after the merge, at the commit you are about to tag. It exits 0 when the plan
job would pass, names every check that refuses, and prints the path of the
release notes it composed so you can read them before they are published.

The checks, in the order the plan job runs them:

| Check | Script |
|---|---|
| The milestone titled as the tag holds no open item | `scripts/release/milestone.sh check` |
| The tag equals the workspace version | `plan.sh` |
| The changelog has only Keep a Changelog types, each once per section | `scripts/checks/changelog-structure.sh` |
| The section being cut opens its `### Security` with the reason when it mixes security and functional changes, and every Security entry names a GHSA id or a `security/vex/` path | `scripts/checks/changelog-security.sh --section X.Y.Z` |
| No fragment other than `changelog.d/README.md` is left for the cut to have assembled, each leftover named | `plan.sh` |
| The changelog section becomes the notes, with the support period and the CRA user-information link | `plan.sh notes`, `scripts/checks/release-notes.sh` |

The support period's end month is five years from the date on the section's
heading (`bash scripts/release/plan.sh support-end YYYY-MM-DD` prints it), and
the link goes to the book's
[CRA information for users](https://ferroterm.eu/docs/evaluate/cra-user-information.html).
The previous release is resolved the way the plan job resolves it: from
`git ls-remote --tags origin 'v*'`, by version order. A tag that was pushed but
whose release never published still counts, and ancestry plays no part
(`bash scripts/release/plan.sh prev-tag vX.Y.Z` prints it).

You need `gh` logged in: the milestone check reads the tracker. A missing tool
refuses its check; it is never skipped.

To add a release fact, add a function and a line to the list in
`scripts/release/plan.sh`. The plan job and the preflight both pick it up.

## The tag

The signed tag is the owner's: `git tag -s vX.Y.Z -m "vX.Y.Z" <commit>` on the
merged release commit, then `git push origin vX.Y.Z`. `release.yml` takes it
from there: the plan job (the preflight's list), the draft release, the four
per-architecture builds, the server, sync and console images pushed by digest,
the vulnerability record, the image scan that tags both digests only once it
passes, the asset verification, the publish, the dispatch that deploys the
book, the crates.io leg behind the `crates-io` environment's required reviewer,
and last the `milestone` job, which closes the milestone titled as the tag once
every other leg has succeeded. When a leg fails or the crates.io approval is
refused, that job names the leg in its log and leaves the milestone open; a
re-run of the failed jobs closes it. An already-closed milestone counts as
done, and a pre-release tag finishes no milestone, so neither milestone step
reads or changes one for it.

A refused vulnerability record names each finding without a judging
statement. Fix the finding with a rebuild or a dependency bump, or add its
statement under `security/vex/` (`security/vex/README.md`), and cut the next
patch: the refused release stays a draft and its images stay untagged. The
same scan runs locally over the published images with
`bash scripts/security/scan-images.sh`, or over a candidate image with
`--candidate <ref>`.

## When the book is published

The site at ferroterm.eu names only released versions. The release PR moves
the book and the landing page to the coming version, so from its merge until
the release publishes, main's book names a version with no image, tarball or
compose file. The Docs workflow refuses to deploy while that is so:
`scripts/checks/site-released.sh` fails when a version the site names is not a
published release carrying its `compose.yaml`, and ferroterm.eu keeps serving
the book of the last release. Once `release.yml` publishes the release, its
`site` leg dispatches the Docs workflow on main, which passes the check and
deploys the new book. A documentation change merged in between is published
with it. To publish by hand after a recovery release, run
`gh workflow run docs.yml --ref main`.

## After the tag

1. **Publish the advisories** the section names, in the same hour the GitHub
   release is published, with the patched version filled in
   (`docs/post-market.md`).
2. **Verify as a consumer**: download a binary and the image, and verify the
   provenance and the SBOM attestations with `gh attestation verify` against
   the signer workflow (`docs/ci-cd.md`).
3. **Post the board status update** with what shipped and what the next
   milestone targets (`.claude/rules/project-board.md`).
4. **The FerroEHR pin follows.** FerroEHR's quickstart overlay
   (`docker-compose.terminology.yml` in that repository) and its
   `adl2_vetdf_ferroterm` test pin the FerroTERM image by tag and digest. Open
   the pull request there that moves both to the release just cut, with the
   image digest read from GHCR after the image lane finishes. The owner asked
   for this on 2026-09-22, starting with v0.1.4.
5. **A bad cut is a new patch version**, never a retag: the platform freezes a
   published release.
6. **The milestone is closed.** The `milestone` job closes it; after a manual
   or recovery release (a re-run, a `publish-crates.yml` dispatch), check that
   it is, and close it with `GITHUB_REPOSITORY=FerroHEALTH/FerroTERM bash
   scripts/release/milestone.sh close vX.Y.Z` when it is not.
