- Every release publishes the synchronisation service as its own image,
  `ghcr.io/ferrohealth/ferroterm-sync`, for `linux/amd64` and `linux/arm64`,
  built by the same SLSA Build Level 3 lane as the server image with
  provenance and an SBOM per platform. The release tarballs carry
  `ferroterm-sync` beside `ferroterm` and `ferroterm-build`, with its own
  CycloneDX SBOM, and the release is not published until all three images
  are tagged at the digests their lanes pushed.
