# syntax=docker/dockerfile:1
# The benchmark image: builds the server, the offline index builder, and the
# harness from this checkout, and runs the harness over releases and artifacts
# mounted at run time, so a record is taken in one known Linux environment
# rather than on whatever the developer's shell provides. Base images are
# pinned by digest; Dependabot (docker ecosystem) proposes the bumps.
#
# From the repository root:
#   docker compose -f bench/compose.yaml run --rm bench --skip-ingest

FROM rust:1.99.1-bookworm@sha256:665b6e0a9290a7298236c08c6e4f1e754e3f0d9b70421c434daa3dfba888d63e AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p ferroterm-server -p ferroterm-build -p ferroterm-bench

# The runtime carries GNU time (the build's peak memory) and procps (`ps`, the
# server's resident memory), the two tools the harness reads.
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates=20250419~deb12u1 \
    procps=2:4.0.2-3 \
    time=1.9-0.2 \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /work
COPY --from=build /src/target/release/ferroterm /src/target/release/ferroterm-build /src/target/release/ferroterm-bench /usr/local/bin/
COPY bench/systems.json /work/bench/systems.json
# The harness reads the owner's licensed releases and writes its records through
# bind mounts, so it runs as a normal user; the image installs nothing at run
# time. The served image is distroless and its own user (docker/Dockerfile).
RUN useradd --uid 65532 --create-home --shell /usr/sbin/nologin bench \
  && chown -R 65532:65532 /work
USER 65532:65532
ENTRYPOINT ["ferroterm-bench", "--server", "/usr/local/bin/ferroterm", "--build", "/usr/local/bin/ferroterm-build"]
