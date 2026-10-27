<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Verifying releases

Every FerroTERM release artifact carries a signed provenance attestation.
Verify it before you run a binary or pull an image, so you know the artifact
was built by the project's own release workflow and not tampered with. The
pipeline and its rationale are in
[`docs/ci-cd.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/docs/ci-cd.md).

<!-- toc -->

## What a release carries

For each target (`x86_64` and `aarch64`, glibc and musl), the release holds:

| File | What it is |
|---|---|
| `ferroterm-<tag>-<target>.tar.gz` | `ferroterm`, `ferroterm-build` and `ferroterm-sync`, built with `cargo auditable` so the dependency list is embedded in the binary |
| `….tar.gz.sha256sum` | the checksum |
| `….tar.gz.sigstore.json` and `….tar.gz.intoto.jsonl` | the SLSA provenance, signed keyless through Sigstore by the release workflow's identity |
| `ferroterm-<tag>-<target>.cdx.json`, `ferroterm-build-<tag>-<target>.cdx.json` and `ferroterm-sync-<tag>-<target>.cdx.json` | CycloneDX SBOMs of the three binaries |
| `….tar.gz.sbom.sigstore.json`, `….tar.gz.build-sbom.sigstore.json` and `….tar.gz.sync-sbom.sigstore.json` | the SBOMs' attestations |
| `compose.yaml` | the quickstart, pinned to the release's image tag |

The container images carry the same provenance and SBOM attestations per
platform: `ghcr.io/ferrohealth/ferroterm` (the server),
`ghcr.io/ferrohealth/ferroterm-sync` (the synchronisation service, from
v0.3.0), both signed by `release-image.yml`, and
`ghcr.io/ferrohealth/ferroterm-console` (the console), signed by
`release-console.yml`. Verify the sync image the way the server image is
verified below, with its name in the reference.

## Verify the provenance

```console
$ gh attestation verify ferroterm-v0.3.1-x86_64-unknown-linux-musl.tar.gz \
    -R FerroHEALTH/FerroTERM \
    --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-build.yml
$ gh attestation verify oci://ghcr.io/ferrohealth/ferroterm:0.3.1 \
    -R FerroHEALTH/FerroTERM \
    --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-image.yml
```

The `--signer-workflow` flag is the point of the check. It requires that the
attestation was produced by that exact reusable workflow in this repository, so
a signature from any other workflow or repository fails. FerroTERM builds its
releases in reusable workflows to reach SLSA Build Level 3, where the signing
identity is not reachable by user build steps.

Releases up to v0.1.5 were built before the repository moved from the
`rubentalstra` account to the `FerroHEALTH` organisation. Their attestations
stay with the account that signed them, so verify one of those with
`--owner rubentalstra --signer-workflow rubentalstra/FerroTERM/.github/workflows/release-build.yml`
(or `release-image.yml`) in place of the two flags above.

## Check the checksum

```console
$ sha256sum -c ferroterm-v0.3.1-x86_64-unknown-linux-musl.tar.gz.sha256sum
```

Run both checks. The checksum tells you the bytes are intact, and the
attestation tells you where the bytes came from.
