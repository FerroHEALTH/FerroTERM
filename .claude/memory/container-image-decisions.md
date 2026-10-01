---
name: container-image-decisions
description: "The settled container image and GHCR publishing design (distroless static base, numeric user, reusable L3 lane, per-platform SBOM attestations, Linux only) and the owner's process expectations for it"
metadata: 
  node_type: memory
  type: project
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-02T20:03:52.397Z
---

Decided 2026-09-02 for the image at `ghcr.io/ferrohealth/ferroterm` (PR #61,
issue #57), after research on primary sources and a read of FerroEHR's setup:

- `docker/Dockerfile` (the Dockerfile lives under `docker/`, only the
  `.dockerignore` sits at the root because it must be at the build context).
  Base `gcr.io/distroless/static-debian12:nonroot` pinned by index digest,
  Dependabot docker ecosystem bumps it. `USER 65532:65532` numeric. No
  `HEALTHCHECK`, no `STOPSIGNAL`, no tini. Binaries are prebuilt musl
  tarballs staged under `dist/<os>/<arch>/`; nothing compiles in the image.
- `release-image.yml` is a `workflow_call` lane (SLSA Build L3 like the binary
  lane); it verifies the tarballs against `release-build.yml` first, builds
  both platforms with `no-cache`, `provenance: mode=max`, `sbom: false`, then
  syft SBOMs per platform manifest and `actions/attest` (provenance for the
  index and each manifest, SBOM per manifest, `push-to-registry`), then
  self-verifies with `gh attestation verify oci://`.
- Release targets are Linux only (x86_64 and aarch64, gnu and musl); the
  macOS target was dropped (#56) and Windows is out of scope for now.
- GHCR facts: no referrers API (attestations land on `sha256-<digest>` tags),
  `unknown/unknown` index entries are BuildKit attestation manifests, the
  first push creates a private package (owner makes it public once), tags are
  mutable so docs tell operators to pin the digest.

**Why:** The owner asked for "the proper way" with "proper research and
docker best practices", wants the FerroEHR setup used as a reference only,
and everything in this repo "clean and proper from scratch".

**How to apply:** Change the image only through `docker/Dockerfile` and the
two release workflows; keep the SBOM source single (syft over the image, not
BuildKit's scanner); re-run the local smoke test (staged `dist/`, `--read-only
--cap-drop ALL`) before touching the lane. See [[repo-merge-gates]] and
[[release-cut-cadence]].
