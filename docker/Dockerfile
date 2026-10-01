# syntax=docker/dockerfile:1
# The FerroTERM image: the server and the offline index builder, two static
# binaries on distroless static, per platform. The binaries are not built here. The release workflow builds and
# attests them in the SLSA L3 lane (release-build.yml), verifies each
# attestation, and stages the binaries under dist/<os>/<arch>/ before this
# file is built for linux/amd64 and linux/arm64 (release-image.yml). The
# viewer bundle is compiled into the server binary by that same lane, so the
# image gains no stage, no file, and no second binary for the web UI.
#
# Build locally from a staged dist/ tree, from the repository root:
#   docker buildx build -f docker/Dockerfile --platform linux/arm64 .

# distroless static: ca-certificates, tzdata, /etc/passwd with the nonroot
# user (uid 65532), a /tmp, no shell, no package manager, about 2 MiB. Pinned
# by index digest; Dependabot (docker ecosystem) proposes the bumps.
FROM gcr.io/distroless/static-debian13:nonroot@sha256:e2e927ec666bae08560abb3c55d0659eceabb657f56b6782ab500a9fc7f555e3

# The platform of the manifest being built, set by buildx per platform.
ARG TARGETOS
ARG TARGETARCH

# Root-owned and not writable by the runtime user. The server is the
# entrypoint; the offline index builder is run as a command
# (`docker compose run --rm build`).
COPY --chmod=0555 dist/${TARGETOS}/${TARGETARCH}/ferroterm /usr/local/bin/ferroterm
COPY --chmod=0555 dist/${TARGETOS}/${TARGETARCH}/ferroterm-build /usr/local/bin/ferroterm-build

# The binary's default listen address is loopback, which no container can
# publish; the image default is every interface on the documented port.
ENV FERROTERM_LISTEN=0.0.0.0:8080
EXPOSE 8080/tcp

# Numeric on purpose: the kubelet cannot resolve a user name against an image
# it does not read, so `runAsNonRoot: true` refuses a named USER.
USER 65532:65532

# The base has no shell and no HTTP client, so the probe is the binary itself:
# one GET /health on the loopback (FERROTERM_LISTEN says where), exit 0 on
# 200. The server binds only after every artifact is open, so healthy means it
# answers terminology requests. The start period covers reading an edition
# (the longest start in the committed records is 23 s); a slower disk or a
# larger deployment raises it in the compose file. Kubernetes ignores this
# instruction and keeps its own probes.
HEALTHCHECK --interval=10s --timeout=5s --start-period=60s --retries=3 \
  CMD ["/usr/local/bin/ferroterm", "healthcheck"]

# Exec form: the binary is PID 1 and receives SIGTERM itself (there is no
# shell to forward it); it stops gracefully on SIGTERM and SIGINT.
ENTRYPOINT ["/usr/local/bin/ferroterm"]

# Build-independent OCI labels; the release workflow adds the build-dependent
# ones (version, revision, created) and mirrors every label as an index
# annotation, which is what GHCR reads for the package page.
LABEL org.opencontainers.image.title="FerroTERM" \
      org.opencontainers.image.description="A pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical code systems" \
      org.opencontainers.image.url="https://ferroterm.eu" \
      org.opencontainers.image.documentation="https://ferroterm.eu/docs/" \
      org.opencontainers.image.source="https://github.com/FerroHEALTH/FerroTERM" \
      org.opencontainers.image.vendor="FerroTERM" \
      org.opencontainers.image.licenses="BUSL-1.1" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot"
