---
name: post-release-verification-pause
description: "After the v0.0.3 cut, pause new feature work and verify the release end to end (binaries, image, SLSA L3 provenance, SBOMs, signatures) before continuing"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-02T20:30:12.303Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

After cutting v0.0.3 (the first release with the container image lane), stop
and verify the whole release as a consumer would before starting the next
unit: every binary tarball with `gh attestation verify --signer-workflow
…/release-build.yml`, the image tag and index and each platform manifest with
`gh attestation verify oci://… --signer-workflow …/release-image.yml` (plus
`--predicate-type https://spdx.dev/Document/v2.3` for the SBOMs), the SBOM
contents (crates listed, not just two files), the GHCR package page (public,
linked, description shown from the index annotation), the BuildKit provenance
in the index, and a `docker run` of the published image. Post the results on
the release's tracking issue and fix anything wrong before moving on.

**Why:** The owner said on 2026-09-02: "after we cut the 0.0.3 we should
pauze a little okay to see if everything is correct and also cehck the sbom
with L3 and evrything so we need to check if everything with the signing and
all is propperly configured and working". The v0.0.1 cut failed silently at
packaging; the owner wants the supply chain proven, not assumed.

**How to apply:** Treat the verification as a unit of work in the release
milestone (an issue with a checklist), not an afterthought; see
[[release-cut-cadence]] and [[container-image-decisions]].
