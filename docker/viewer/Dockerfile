# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The FerroTERM console image: one static binary and its site
# bundle on distroless static, per platform. Nothing is compiled here. The
# release lane (.github/workflows/release-viewer.yml) builds and attests the
# musl binaries and the site bundle and stages them under dist/viewer/ before
# this file is built for linux/amd64 and linux/arm64. The root .dockerignore
# holds the build context to dist/.
#
# Build from a staged dist/viewer tree, at the repository root:
#   docker buildx build -f docker/viewer/Dockerfile --platform linux/arm64 .

# distroless static: ca-certificates, tzdata, an /etc/passwd carrying the
# nonroot user (uid 65532), a /tmp, no shell and no package manager. The same
# base and index digest as docker/Dockerfile: docs/VERSIONS.md pins it,
# scripts/checks/versions.sh holds both files to that row, and Dependabot (the
# docker ecosystem over /docker and /docker/viewer) proposes the bumps.
# https://github.com/GoogleContainerTools/distroless
FROM gcr.io/distroless/static-debian13:nonroot@sha256:516757106de35205efb70b91ba03f6f1298424a0678c603255cf2852a7000087

# The platform of the manifest being built, set by buildx per platform.
# https://docs.docker.com/reference/dockerfile/#automatic-platform-args-in-the-global-scope
ARG TARGETOS
ARG TARGETARCH

# Root-owned, executable, and not writable by the runtime user. The site
# bundle is the same for every architecture and root-owned too, so the
# runtime user reads it and cannot change it.
COPY --chmod=0555 dist/viewer/${TARGETOS}_${TARGETARCH}/ferroterm-viewer /usr/local/bin/ferroterm-viewer
COPY dist/viewer/site/ /site/

# The binary's own defaults are loopback and a checkout's target/site, so the
# image listens on every interface on the documented port and serves the
# bundle it carries. Every override is FERROTERM_VIEWER__<SECTION>__<KEY>,
# and every secret is read from the file its `_file` sibling names.
ENV FERROTERM_VIEWER__SERVER__LISTEN=0.0.0.0:3000 \
    FERROTERM_VIEWER__SERVER__SITE_ROOT=/site
EXPOSE 3000/tcp

# Numeric on purpose: the kubelet cannot resolve a user name against an image
# it does not read, so `runAsNonRoot: true` refuses a named USER.
USER 65532:65532

# The base has no shell and no HTTP client, so the probe is the binary's own
# `healthcheck` subcommand: it asks GET /health on the configured listen port
# over loopback and exits 0 on 200, 1 otherwise. Kubernetes ignores the
# instruction and keeps its own probes.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/ferroterm-viewer", "healthcheck"]

# Exec form: the binary is PID 1 and receives SIGTERM itself.
ENTRYPOINT ["/usr/local/bin/ferroterm-viewer"]
CMD ["serve"]

# Build-independent labels only. The release lane sets the version, revision
# and created labels at build time and mirrors every label as an index
# annotation, which is what GHCR reads for the package page.
LABEL org.opencontainers.image.title="FerroTERM Console" \
      org.opencontainers.image.description="The FerroTERM Console: browse, check and edit the code systems, value sets and concept maps a FerroTERM server serves, over its FHIR API" \
      org.opencontainers.image.url="https://ferroterm.eu" \
      org.opencontainers.image.documentation="https://ferroterm.eu/docs/operate/viewer.html" \
      org.opencontainers.image.source="https://github.com/FerroHEALTH/FerroTERM" \
      org.opencontainers.image.vendor="FerroTERM" \
      org.opencontainers.image.licenses="BUSL-1.1" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot"
