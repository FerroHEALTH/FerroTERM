# Install and run

FerroTERM ships as a release tarball and as a container image. Each holds two
binaries, the server and the build tool, so there is no JVM to install, no
Elasticsearch to run, and no database to provision. The registry systems
(UCUM, BCP 47, BCP 13, ISO 3166-1) are inside the binary; every other code
system is an index you build from a release you are licensed for
([Loading code systems](loading-snomed.md)).

<!-- toc -->

## The first call, with nothing to load

```console
$ docker run --rm -p 8080:8080 ghcr.io/ferrohealth/ferroterm:0.1.5
$ curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL'
```

The server starts with no index, serves the four registry systems, and answers
the lookup with the unit's canonical form. `GET /r4b/metadata?mode=terminology`
lists what a deployment serves.

## Run with Docker Compose

Every release attaches a `compose.yaml`. With it and a SNOMED CT release zip
(a licensed download from SNOMED International or your national release
centre), the whole path is two commands: build the index once, then serve it.

```console
$ curl -LO https://github.com/FerroHEALTH/FerroTERM/releases/latest/download/compose.yaml
$ FERROTERM_RF2=/path/to/SnomedCT_Release.zip docker compose run --rm build
$ docker compose up
```

The `build` service runs `ferroterm-build` from the same image with the zip
mounted read-only and writes the index to `./index` (or `FERROTERM_INDEX_DIR`);
the Snapshot is unpacked to the container's tmpfs and gone with it. It sits
under the `build` profile, so `docker compose up` never starts it. To serve
indexes you built elsewhere, skip the first command and point the variable at
an index root: a directory whose child directories are the artifacts, one per
release. A root is the layout the sync service writes (one child per release,
renamed into place when complete), so a deployment that starts with a root
needs no re-layout when the sync is added later. A single artifact directory
is accepted too (see [Configuration](configuration.md#index-roots)).

```console
$ FERROTERM_INDEX_DIR=/srv/ferroterm/index docker compose up
```

The file pulls the image of the release it shipped with, mounts the index
read-only at `/data/index`, publishes port 8080 on the loopback interface, and
runs the container with every capability dropped and a read-only root
filesystem. `FERROTERM_BIND_HOST`, `FERROTERM_PORT`, `FERROTERM_LOG_FORMAT`,
`FERROTERM_DEFAULT_LANGUAGE`, and `RUST_LOG` are variables you set on the
command line.

The `ferroterm` service declares a health check, so `docker compose up --wait`
returns once the server answers `GET /health`, and a service of your own waits
for it with `depends_on: ferroterm: condition: service_healthy`. The shipped
`proxy` service does exactly that. See
[Health, metrics, and request identifiers](observability.md) for what the
probe does.

## Run the container by hand

The image is `ghcr.io/ferrohealth/ferroterm`, published for `linux/amd64` and
`linux/arm64` with every release. It holds the static `ferroterm` and
`ferroterm-build` binaries on a distroless base: no shell, no package manager,
a numeric non-root user (`65532`), and the listen address preset to
`0.0.0.0:8080`.

```console
$ docker run --rm -p 8080:8080 \
    -v /srv/ferroterm/index:/data/index:ro \
    -e FERROTERM_INDEX=/data/index \
    ghcr.io/ferrohealth/ferroterm:0.1.5
```

Mount the index root read-only; each child directory under it is one
artifact. The server writes nothing while serving, so the
container runs with a read-only root filesystem. Tags are `<version>`,
`<major.minor>`, and `latest`; a deployment pins the digest and verifies its
provenance first (see [Verifying releases](verifying-releases.md)):

```console
$ gh attestation verify oci://ghcr.io/ferrohealth/ferroterm:0.1.5 \
    -R FerroHEALTH/FerroTERM \
    --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-image.yml
```

The image carries a `HEALTHCHECK` that runs `ferroterm healthcheck`: the
binary probes `GET /health` on its own listen address, since there is no shell
or `curl` to do it, so `docker inspect` reports the container's health and
Compose can wait on it. Kubernetes ignores the image's `HEALTHCHECK`: point the
readiness and liveness probes at `GET /health`, set `runAsNonRoot: true`,
`readOnlyRootFilesystem: true`, drop every capability, and mount the index
root (not a single artifact) read-only at the path `FERROTERM_INDEX` names. The server stops
cleanly on `SIGTERM`.

## Run the binary

Each release carries a tarball per target (`x86_64` and `aarch64`, glibc and
musl) holding `ferroterm` and `ferroterm-build`, with a checksum, a CycloneDX
SBOM, and Sigstore attestations beside it. Download, verify, unpack, run:

```console
$ gh release download v0.1.5 -R FerroHEALTH/FerroTERM -p 'ferroterm-v0.1.5-x86_64-unknown-linux-musl.tar.gz*'
$ gh attestation verify ferroterm-v0.1.5-x86_64-unknown-linux-musl.tar.gz -R FerroHEALTH/FerroTERM \
    --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-build.yml
$ tar xzf ferroterm-v0.1.5-x86_64-unknown-linux-musl.tar.gz
$ FERROTERM_INDEX=/srv/ferroterm/index ./ferroterm
```

The server opens each index read-only, refuses to start on a missing or
damaged one, and listens on `127.0.0.1:8080`. The listen address, the index
directories, the FHIR resource directories, and the default display language
are environment variables described in [Configuration](configuration.md).

## Check that it is serving

```console
$ curl http://localhost:8080/health
$ curl 'http://localhost:8080/r4b/metadata?mode=terminology'
```

The `TerminologyCapabilities` names every code system and version this
deployment serves, with its filters and properties. See
[The FHIR terminology API](../integrate/fhir-api.md) for the operations.
