---
name: release-cut-cadence
description: Cut the release the moment a milestone empties, bump the version EVERYWHERE, create the next milestone for stragglers, close the milestone by hand, and sign and push the tag yourself when the owner asks (done for v0.1.3 on 2026-09-12)
metadata:
  node_type: memory
  type: feedback
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-03T09:32:09.439Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

When a `vX.Y.Z` milestone reaches zero open issues, open the release PR right
away, then push a GPG-signed tag (`git tag -s vX.Y.Z`) after it merges so
`release.yml` publishes. The release PR bumps the version everywhere the
release is named, not only the manifests: root `Cargo.toml` (workspace and
path-dependency versions), `Cargo.lock`, `CITATION.cff`, `compose.yaml`,
`docs/VERSIONS.md`, `README.md` (image tag, roadmap list), the landing page
`website/landing/index.html` (JSON-LD `version`, the status line, the status
panel), the book `website/book/src/**` (install, verifying-releases,
introduction, comparison, examples, hardware-sizing, what-ferroterm-is), and
the `## [X.Y.Z] - date` changelog section. Run `bash scripts/checks/versions.sh`
before pushing: its "stale product versions" sweep fails on any lower version
string in those files (CI `versions` job, required).

**Why:** On 2026-09-02 the owner wrote "you did not cut an release from the
0.0.1 and now starting 0.0.2 already". On 2026-09-03, after v0.0.7, the owner
wrote "you have not updated all the version number everywhere": the README,
landing page, and book still said 0.0.6 after the tag was pushed, fixed in
PR #139 together with the guard.

**How to apply:** Check the milestone count after every merge report; if it is
zero, the next unit of work is the release PR. In that PR, `git grep` the old
version across the whole tree (excluding CHANGELOG.md and Cargo.lock) and run
`scripts/checks/versions.sh` before opening it. See [[repo-merge-gates]] and
[[milestone-autonomy]].

**Update 2026-09-12 (v0.1.3):** the owner asked for the whole cut, tag
included: "please cut the release for me please and close the milestone".
`git tag -s v0.1.3 -m v0.1.3 <merge commit>` signed with the configured
openpgp key without a prompt and `git push origin v0.1.3` started
`release.yml`; nine jobs, 33 assets, about eleven minutes. Do the same when
asked; otherwise hand the two commands over as `docs/release.md` says. Two
things nothing automates: the stragglers need a NEW next milestone
(`gh api -X POST repos/.../milestones -f title=vX.Y.Z`), and the emptied
milestone is closed by hand (`gh api -X PATCH .../milestones/<n> -f
state=closed`). Verify afterwards as a consumer: `gh attestation verify` on a
tarball (signer `release-build.yml`) and on `oci://ghcr.io/...:X.Y.Z` (signer
`release-image.yml`), and read the published image config through the GHCR
API with an anonymous pull token when Docker should stay stopped. Checklist
step 6 (fresh benchmark records) was skipped at v0.1.2 and v0.1.3 for want of
a quiet machine; #512 carries it.

- After every FerroTERM release, open the FerroEHR pull request that moves its FerroTERM image pin (quickstart overlay and the `adl2_vetdf_ferroterm` test) to the new tag and digest; the owner asked for this on 2026-09-22, first for v0.1.4. The step is in `docs/release.md` §After the tag.

- v0.1.4 (2026-09-23): cut from a git worktree. The worktree has no `data/` or `artifacts/` (ignored, main clone only), and `data/*/.gitkeep` dirs are tracked, so do not symlink over them; run `ferroterm-bench` from the main clone with `--config`, `--out`, `--server`, `--build` pointing at the worktree. `cargo build --locked` fails right after the version bump until `cargo update -w` follows the lock. In zsh `status` is read-only; never use it as a variable name in a Bash call.

v0.1.5 (2026-09-24): the owner moved the unfinishable items (the quiet-machine
record set #642, the NTS live feed #602, the viewer review #515) to v0.1.6 and
asked for the cut the moment the last PR merged, then said "you push the tag"
when I left the tag to them. Cut in one commit (versions, fixtures, changelog),
tag signed and pushed, artifacts verified, then the FerroEHR pin PR (#3484
there): compose overlay, hosted compose, `_helpers.tpl` pinned version,
`values.yaml` digest, Chart.yaml annotation and version, `validate.sh --update`
for the goldens, and `helm-docs` for the chart README.
