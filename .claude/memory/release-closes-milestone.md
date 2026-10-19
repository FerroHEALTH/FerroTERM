---
name: release-closes-milestone
description: "The release workflow refuses a tag whose milestone is missing or not empty, and closes the milestone after every leg succeeds; after a manual or recovery release, check it is closed; owner 2026-10-19"
metadata:
  type: feedback
---

A published release closes its milestone: `release.yml` refuses a `vX.Y.Z` tag in `plan` when the milestone titled exactly as the tag is missing or holds an open item, and its last job closes that milestone once every leg, crates.io included, has succeeded (`scripts/release/milestone.sh`).

**Why:** FerroSMART v0.1.0 was tagged and published on 2026-10-19 while its milestone stayed open, and the owner asked that this be impossible in every product (#758).

**How to apply:** the workflow enforces it. After any manual or recovery release (a re-run, a `publish-crates.yml` dispatch, a leg finished by hand), check that the milestone is closed, and run `GITHUB_REPOSITORY=FerroHEALTH/FerroTERM bash scripts/release/milestone.sh close vX.Y.Z` when it is not.
