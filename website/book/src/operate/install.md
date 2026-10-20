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
$ docker run --rm -p 8080:8080 ghcr.io/ferrohealth/ferroterm:0.1.8
$ curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL'
```

The server starts with no index, serves the four registry systems, and answers
the lookup with the unit's canonical form. `GET /r4b/metadata?mode=terminology`
lists what a deployment serves.

## Run with Docker Compose

Every release attaches a `compose.yaml`. It runs two services from the same
image: `build` turns a release you hold a licence for into an index, once per
release, and `ferroterm` serves every index it finds. Both see the same two
host directories, at fixed paths inside the container:

| Host path, beside `compose.yaml` | Container path | Moved with | Used by |
|---|---|---|---|
| `./releases` | `/releases`, read-only | `FERROTERM_RELEASES_DIR` | `build` reads the release files |
| `./index` | `/data/index` | `FERROTERM_INDEX_DIR` | `build` writes, `ferroterm` reads read-only |

A command you type after `docker compose run --rm build` runs inside the
container, so every path in it is a container path: the release is under
`/releases`, and the index goes under `/data/index`.

### Build, serve, and look up LOINC

This walk-through uses LOINC 2.83, which needs a free account at
<https://loinc.org>. Every other source follows the same steps (see
[Other code systems](#other-code-systems)).

1. Fetch the compose file and create the two directories. Compose does not
   create a bind-mounted directory for you, so both must exist before the
   first run. Put the release zip in `./releases`:

   ```console
   $ curl -LO https://github.com/FerroHEALTH/FerroTERM/releases/latest/download/compose.yaml
   $ mkdir -p releases index
   $ cp ~/Downloads/Loinc_2.83.zip releases/
   ```

2. Build the index. The host file `./releases/Loinc_2.83.zip` is
   `/releases/Loinc_2.83.zip` in the container. With no `--out`, the build
   writes `/data/index/loinc`, which is `./index/loinc` on the host, and the
   last line it prints names that absolute container path:

   ```console
   $ docker compose run --rm build --loinc /releases/Loinc_2.83.zip
   $ ls index/loinc/manifest.json
   ```

   The image runs as user `65532`. On Linux a bind mount keeps the host's
   ownership, so that user cannot write a directory you own; run the build as
   yourself with `docker compose run --rm --user "$(id -u):$(id -g)" build
   --loinc /releases/Loinc_2.83.zip`. Docker Desktop maps the ownership itself
   and needs no flag. A zip is unpacked to the container's tmpfs and is gone
   with the container.

3. Serve it. `FERROTERM_BIND_HOST` is the host interface the port is
   published on and `FERROTERM_PORT` the host port; the server listens on
   8080 inside the container either way. The defaults are shown here, and
   `--wait` returns once the server answers its health check:

   ```console
   $ FERROTERM_BIND_HOST=127.0.0.1 FERROTERM_PORT=8080 docker compose up -d --wait
   ```

   `127.0.0.1` keeps the server on this machine. To serve another machine,
   set `FERROTERM_BIND_HOST=0.0.0.0` or put the
   [reverse proxy](reverse-proxy.md) in front.

4. Look up a code. Each FHIR version has its own base path, `/r4`, `/r4b`,
   `/r5`, and `/r6`, and `$lookup` is `GET [base]/CodeSystem/$lookup` with the
   `system` and `code` parameters:

   ```console
   $ curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://loinc.org&code=2345-7'
   ```

   The answer is a `Parameters` resource carrying the code's display
   (`Glucose [Mass/volume] in Serum or Plasma`) and the release version,
   `2.83`. With another `FERROTERM_PORT`, use that port in the URL.

When the next LOINC release arrives, build it into a child of its own, such as
`--out /data/index/loinc-2.84`, and run `docker compose restart ferroterm`.
Both releases are then served, and a request that names no version resolves to
the newest (see [Index roots](configuration.md#index-roots)).

### Where a build writes

The `build` service sets `FERROTERM_INDEX_ROOT=/data/index` and runs in
`/data/index`. That gives `ferroterm-build` three rules:

- With no `--out`, it writes a child of `/data/index` named for the source.
- A relative `--out` is read against `/data/index`, so `--out loinc-2.84` is
  `./index/loinc-2.84` on the host.
- An `--out` outside `/data/index`, such as `--out /tmp/loinc`, is refused
  before anything is read. The container's own filesystem is discarded by
  `--rm`, so a build written there would be lost.

Every build prints the absolute path it wrote.

### Other code systems

Each row is the part after `docker compose run --rm build`, with the release
files in `./releases`. [Loading code systems](loading-snomed.md) describes
what each source needs and what the build makes of it.

| Source | Arguments | Written to, on the host |
|---|---|---|
| SNOMED CT | `--rf2 /releases/SnomedCT_Release.zip` | `./index/snomed` |
| SNOMED CT with a refset package | `--rf2 /releases/SnomedCT_Release.zip --rf2-refset /releases/<package>.zip` | `./index/snomed` |
| LOINC | `--loinc /releases/Loinc_2.83.zip` | `./index/loinc` |
| ICD-10, ICD-10-NL, ICPC-2 (ClaML) | `--claml /releases/icd10nl-2021.xml --system http://hl7.org/fhir/sid/icd-10-nl` | `./index/icd-10-nl`, the last segment of `--system` |
| ICD-10-CM | `--icd10cm /releases/<order>.zip --icd10cm /releases/<tables>.zip` | `./index/icd-10-cm` |
| ATC/DDD | `--atc /releases/atc-index-2026.csv --atc-version 2026` | `./index/atc` |
| DHD thesaurus | `--dhd /releases/<delivery>.zip` | `./index/dhd`, with its concept maps in `conceptmaps/` |
| G-Standaard | `--gstandaard /releases/202609 --gstandaard-version 202609` | `./index/gpk`, `./index/prk`, `./index/hpk`, `./index/artikel` |
| Nederlandse Labcodeset | `--labcodeset /releases/Labcodeset_v2026-08.zip` | `./index/labcodeset`, FHIR resources |
| RxNorm | `--rxnorm /releases/RxNorm_full_prescribe_09082026.zip` | `./index/rxnorm` |
| ICD-11 | `--icd11 /releases/icd11-cache` | `./index/mms`, `./index/icf`, `./index/entity` |

Two releases of one source share a default child, so name one of them: the
Dutch and the International SNOMED CT editions as `--out
/data/index/snomed-nl` and `--out /data/index/snomed-int`, or the
Diagnosethesaurus and the Verrichtingenthesaurus as `--out
/data/index/dhd-diagnoses` and `--out /data/index/dhd-procedures`. The ClaML
fallback child is `claml` when `--system` ends in no plain name (an `urn:oid`).

The Labcodeset resources and the DHD concept maps are FHIR resources, read
through `FERROTERM_CODESYSTEMS`. They already sit on the index mount, so add
the variable to the `ferroterm` service's `environment` in `compose.yaml`, with
the paths separated by `:`, for example `FERROTERM_CODESYSTEMS:
/data/index/labcodeset:/data/index/dhd/conceptmaps`. The release mount is
read-only, so filling an ICD-11 cache with `--icd11-api` (which writes into
the cache) runs with the binary; a build from a filled cache in `./releases`
runs here. `docker compose run --rm build` with no arguments prints the
builder's help. The service sits under the `build` profile, so `docker compose
up` never starts it.

### Serve indexes built elsewhere

To serve indexes you built elsewhere, skip the build and point the variable at
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
    ghcr.io/ferrohealth/ferroterm:0.1.8
```

Mount the index root read-only; each child directory under it is one
artifact. The server writes nothing while serving, so the
container runs with a read-only root filesystem. Tags are `<version>`,
`<major.minor>`, and `latest`; a deployment pins the digest and verifies its
provenance first (see [Verifying releases](verifying-releases.md)):

```console
$ gh attestation verify oci://ghcr.io/ferrohealth/ferroterm:0.1.8 \
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
$ gh release download v0.1.8 -R FerroHEALTH/FerroTERM -p 'ferroterm-v0.1.8-x86_64-unknown-linux-musl.tar.gz*'
$ gh attestation verify ferroterm-v0.1.8-x86_64-unknown-linux-musl.tar.gz -R FerroHEALTH/FerroTERM \
    --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-build.yml
$ tar xzf ferroterm-v0.1.8-x86_64-unknown-linux-musl.tar.gz
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
