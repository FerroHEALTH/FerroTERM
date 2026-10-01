# <img src="https://raw.githubusercontent.com/FerroHEALTH/FerroTERM/main/assets/brand/ferroterm-lockup-auto.svg" alt="FerroTERM" width="284" height="64">


[![CI](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/ci.yml/badge.svg)](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/ci.yml)
[![CodeQL](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/codeql.yml/badge.svg)](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroTERM/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroTERM)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_FerroTERM&metric=alert_status)](https://sonarcloud.io/summary/overall?id=rubentalstra_FerroTERM)
[![Coverage](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_FerroTERM&metric=coverage)](https://sonarcloud.io/summary/new_code?id=rubentalstra_FerroTERM)
[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)
[![GitHub Release](https://img.shields.io/github/release/FerroHEALTH/FerroTERM.svg?logo=github)](https://github.com/FerroHEALTH/FerroTERM/releases/latest)
[![Image pulls](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fghcr-badge.elias.eu.org%2Fapi%2FFerroHEALTH%2FFerroTERM%2Fferroterm&query=downloadCount&label=image%20pulls&logo=github)](https://github.com/FerroHEALTH/FerroTERM/pkgs/container/ferroterm)

[![tx-ecosystem R4](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4.json)](https://ferroterm.eu/docs/evaluate/conformance.html)
[![tx-ecosystem R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4b.json)](https://ferroterm.eu/docs/evaluate/conformance.html)
[![tx-ecosystem R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr5.json)](https://ferroterm.eu/docs/evaluate/conformance.html)

A pure-Rust FHIR terminology server for SNOMED CT, LOINC, ICD-10, ICD-11,
RxNorm, UCUM, and any FHIR `CodeSystem`, served from one binary over an index
built once per release and read at startup. No JVM, no Elasticsearch, no
database to run.

[Five minutes to a running server](#five-minutes-to-a-running-server) ·
[What it serves](#what-it-serves) · [The viewer](#the-viewer) ·
[The API](#the-api) ·
[Do you need a commercial licence?](#do-you-need-a-commercial-licence)

## Five minutes to a running server

The image serves UCUM, BCP 47, BCP 13, and ISO 3166-1 with no configuration,
so the first call needs nothing beyond Docker:

```console
$ docker run --rm -p 8080:8080 ghcr.io/ferrohealth/ferroterm:0.1.5
$ curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL'
```

```json
{
  "resourceType": "Parameters",
  "parameter": [
    { "name": "name", "valueString": "Unified Code for Units of Measure (UCUM)" },
    { "name": "version", "valueString": "2.2" },
    { "name": "display", "valueString": "mg/dL" },
    { "name": "property", "part": [
      { "name": "code", "valueCode": "canonical" },
      { "name": "value", "valueCode": "m-3.g" } ] }
  ]
}
```

A clinical code system is a release you hold a licence for, built once into an
index by `ferroterm-build` (in the same image and every release tarball) and
served read-only. With the release `compose.yaml` and a SNOMED CT release zip:

```console
$ curl -LO https://github.com/FerroHEALTH/FerroTERM/releases/latest/download/compose.yaml
$ FERROTERM_RF2=/path/to/SnomedCT_Release.zip docker compose run --rm build
$ docker compose up
$ curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code=404684003&displayLanguage=nl'
```

What each code system costs to build and serve, and how fast each operation
answers, is in the [speed and footprint table](#speed-and-footprint) below,
rendered from the benchmark records. The book's
[Install and run](https://ferroterm.eu/docs/operate/install.html) and
[Loading code systems](https://ferroterm.eu/docs/operate/loading-snomed.html)
pages cover every system and the binary distribution.

## What it serves

<!-- code-systems:begin -->
SNOMED CT, LOINC, UCUM, BCP 47, BCP 13, ISO 3166-1, ICD-10 (WHO), ICD-10-NL,
ICD-10-CM, ICD-11 MMS, ICD-11 ICF, ICD-11 Foundation, ATC/DDD, ICPC-2, RxNorm,
the DHD Diagnosethesaurus and Verrichtingenthesaurus, the G-Standaard, the
Nederlandse Labcodeset, the NHG ICPC-1 to SNOMED CT map, and any FHIR
`CodeSystem`, `ValueSet`, and `ConceptMap` resources (HL7 Terminology's 900+
systems load this way).
<!-- code-systems:end -->

The [code systems page](https://ferroterm.eu/docs/evaluate/code-systems.html)
of the book is the one list: per system the canonical URI, the versions and
editions handled, the build command, and the licence position. You bring the
release you are licensed for; UCUM and the registries are vendored into the
binary and need nothing. Every system reaches the operations through one
provider seam, so nothing in an operation is a special case for one system.

## Speed and footprint

Measured by the benchmark harness on one machine, one record per
code system, warm p50 per operation; nothing here is typed by hand, and CI fails
when the table drifts from the records under `bench/records/`.

The set below was taken on 2026-09-22 with the 0.1.4 release binaries, natively
on an Apple M2 with nothing else running: a record taken on a busy machine
measures the operating system rather than the server. The resident and
peak-build figures are the server's and the build tool's own, and the set's
[README](bench/records/2026-09-23-apple-m2/README.md) states the machine's
condition and the instrument behind each figure.

<!-- bench-table:begin -->
| Code system | Release | Concepts | Build | Peak build memory | Index on disk | Resident | `$lookup` | `$validate-code` | `$subsumes` | `$expand` (small) | `$expand` (large) | Search | Snowstorm |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| [ICD-10-CM](bench/records/2026-09-23-apple-m2/icd-10-cm-2026-09-22T23-23-19-858381Z.json) | 2026 | 98,827 | 1.51 s | 359 MB | 44 MB | 91 MB | 105 µs | 60 µs | 58 µs | n/a | n/a | n/a | not run |
| [ICD-10-NL](bench/records/2026-09-23-apple-m2/icd-10-nl-2026-09-22T23-23-22-144431Z.json) | 2021 | 42,769 | 880.52 ms | 221 MB | 21 MB | 66 MB | 96 µs | 60 µs | 64 µs | n/a | n/a | n/a | not run |
| [ICD-11 MMS](bench/records/2026-09-23-apple-m2/icd-11-mms-2026-09-22T23-23-27-084934Z.json) | 2026-01 | 37,211 | n/a | n/a | 44 MB | 99 MB | 138 µs | 59 µs | n/a | 129 µs | 116 µs | 139 µs | not run |
| [LOINC](bench/records/2026-09-23-apple-m2/loinc-2026-09-22T23-23-17-47383Z.json) | 2.83 | 257,266 | 8.15 s | 2.54 GB | 310 MB | 440 MB | 136 µs | 56 µs | n/a | 126 µs | 7.23 ms | 283 µs | not run |
| [RxNorm (prescribable subset)](bench/records/2026-09-23-apple-m2/rxnorm-prescribable-subset-2026-09-22T23-23-24-615167Z.json) | 09082026 | 81,468 | 4.58 s | 667 MB | 77 MB | 133 MB | 388 µs | 114 µs | n/a | n/a | n/a | n/a | not run |
| [SNOMED CT (International edition)](bench/records/2026-09-23-apple-m2/snomed-ct-international-edition-2026-09-22T23-23-13-36281Z.json) | 20260901 | 535,502 | 14.12 s | 3.06 GB | 559 MB | 943 MB | 217 µs | 66 µs | 59 µs | 215 µs | 1.7 ms | 353 µs | not run |
| [SNOMED CT (Netherlands edition)](bench/records/2026-09-23-apple-m2/snomed-ct-netherlands-edition-2026-09-22T23-23-10-140124Z.json) | 20260630 | 548,949 | 17.66 s | 3.88 GB | 864 MB | 1.14 GB | 222 µs | 66 µs | 59 µs | 285 µs | 2.28 ms | 817 µs | not run |

Warm p50 over 200 HTTP round trips on one machine (Apple M2, 17.18 GB, macos/aarch64), FerroTERM 0.1.4 serving FHIR R4B, taken 2026-09-22. The records are under `bench/records/`; the [benchmarks page](https://ferroterm.eu/benchmarks.html) has the method, the cold and tail latencies, and how to reproduce a record.
<!-- bench-table:end -->

## The viewer

The same binary serves a web viewer at `/ui`, and `/` redirects onto it. Open
`http://localhost:8080/ui` after the first command above and the server you just
started is the one it reads.

It is a FHIR client and nothing else: it reaches the server over the same public
API any client uses, from the browser, same-origin. Anything it does, your own
client can do.

- **Check a code**, **List a value set**, and **Map a code** run
  `$validate-code`, `$subsumes`, `$expand`, and `$translate`. Each control
  offers the code systems, value sets, and concept maps this deployment
  publishes, so nothing waits for a canonical to be typed, and a run puts every
  parameter in the address, so it is a link you can send.
- **The overview** is one table of what the deployment loaded, a row per served
  version, leading with the name each code system was published under. It reads
  a twenty-system deployment on one screen.
- **The concept browser** walks a hierarchy with the keyboard, following the
  ARIA tree view pattern, and offers only what the served version declares it
  can do.

It ships inside the binary, so there is nothing to deploy and no path reaches
the filesystem. `FERROTERM_UI=off` removes the routes. The
[viewer page](https://ferroterm.eu/docs/operate/viewer.html) has the screens.

## The API

FHIR R4 under `/r4`, R4B under `/r4b`, R5 under `/r5`, and the R6 ballot under
`/r6`, each in its own version's shapes:
`CodeSystem/$lookup`, `CodeSystem/$validate-code`,
`CodeSystem/$subsumes`, `ValueSet/$expand` (paging, `filter`, version
pins, nested `contains`, inline and request-scoped value sets),
`ValueSet/$validate-code`,
`ConceptMap/$translate` (loaded, inline, and the SNOMED implicit concept maps),
`$closure` (on `/r4`, `/r4b`, and `/r5`; the R6 ballot defines none),
`POST [base]` with a `batch` `Bundle`, `GET ValueSet/{id}` and
`GET ValueSet?url=`, `$versions`, `$cache-control`, `metadata`, and
`metadata?mode=terminology`. A deployment that names a database in
`FERROTERM_RESOURCES` also serves `POST`, `PUT`, `GET`, `DELETE`, `_history`
reads, and `?url=` search of `CodeSystem`, `ValueSet`, and `ConceptMap`, with
`ETag` and `If-Match`. Every failure is an `OperationOutcome`, never a bare
500, and a terminology failure carries a `tx-issue-type` coding.

Conformance is measured rather than claimed. CI runs the HL7 terminology
ecosystem suite against every pull request and holds a committed pass list per
served version: **637 of the 670 general cases on R5, 635 on R4, and 628 on
R4B**. A case that newly passes is added to the list in the change that earned
it, and a case on the list that stops passing fails the build, so the figure
only ratchets. What the remaining cases are waiting on is on the tracker,
cluster by cluster: most are a parameter no published `OperationDefinition`
declares, or a suite mode that pins an edition no release centre distributes.
Every route answers FHIR JSON or FHIR XML, by `_format` or `Accept`.

## What is next

v0.1.5 is the current release. The tracker's milestones are the roadmap, and
the open issues under each are the worklist:

- **[v0.1.5](https://github.com/FerroHEALTH/FerroTERM/milestone/19)**: the
  first real run of the synchronisation against the Nationale
  Terminologieserver once an account exists, the viewer restructure and its
  design, the benchmark record set retaken on a quiet machine with the
  published table re-rendered from it, the resident memory of a served edition
  accounted for structure by structure, and the `x-caused-by-unknown-system`
  parameter the terminology ecosystem requires but no `OperationDefinition`
  declares.
- **[v0.3.0](https://github.com/FerroHEALTH/FerroTERM/milestone/16)**: the
  differential check against the Nictiz Nationale Terminologieserver for the
  Dutch variants (the Snowstorm differential harness runs today), and the
  suite cases that are waiting on an upstream ruling rather than on work here.

## How it is built

The design, with its citations, is [`docs/architecture.md`](docs/architecture.md).
The short form:

- **Offline once, online from precomputed structures.** `ferroterm-build`
  turns a release into a `redb` store with its dense columns (concepts,
  designations, properties), a CSR is-a adjacency with roaring
  transitive-closure bitmaps (`hierarchy.bin`), and an `fst` word index
  (`text.bin`). The server reads
  them at startup and answers from memory: subsumption is a bitmap test and a
  descendant set is a bitmap. Nothing is memory-mapped, because mapping a file
  takes `unsafe` and the workspace forbids it.
- **FHIR is generated, never hand-written.** The model is the `fhir-types`
  crate, emitted from the pinned HL7 packages (R4 4.0.1, R4B 4.3.0, R5 5.0.0,
  R6 ballot 5, HL7 Terminology) so each version's operation surface is right by
  construction. The FerroBRIDGE repository generates and publishes it; this one
  consumes it from crates.io at a pinned version.
- **The engine is code-system-neutral.** Providers own the semantics
  (`crates/fhir-terminology`); the operations talk to the seam.
- **Supply chain.** Releases are built in a reusable workflow to SLSA Build
  Level 3: signed provenance, an SBOM per artifact (CycloneDX for the
  binaries, SPDX for the image), `cargo auditable` binaries, a distroless
  image for `linux/amd64` and `linux/arm64`, verifiable with
  `gh attestation verify`.

## Do you need a commercial licence?

Production use is free for Non-Commercial Purposes and needs a commercial
licence otherwise. `LICENSE` is the authority; this table is the same boundary
in the order people ask about it.

| What you are doing | What you need | Why |
|---|---|---|
| Reading, building, modifying, or redistributing the source | Free | The licence grants this without a fee and without asking anyone; only production use is restricted. |
| Development, testing, evaluation, prototyping | Free | None of these is production use, which is the only thing the Additional Use Grant limits. |
| Personal use | Free | Named in the grant's definition of Non-Commercial Purposes. |
| Academic or scientific research, or teaching | Free | Named in the grant's definition of Non-Commercial Purposes. |
| Production use by a non-profit or public body that is not in the course of a business, does not deliver a service for payment, and is not for commercial advantage | Free | The grant's definition of Non-Commercial Purposes, in the licence's own words. |
| Treating patients, or delivering any other service for payment | Commercial licence | The grant ends with "any other production use, including the delivery of health care or any other service for payment, requires a commercial license from the Licensor". |
| Any production use by a vendor or integrator | Commercial licence | A business's production use is not a Non-Commercial Purpose, so the same closing sentence of the grant applies. |
| Offering FerroTERM, or a work derived from it, to third parties as a hosted, managed, or embedded terminology service | Commercial licence | Carve-out (a) of the grant, which the licence defines as a service through which anyone other than you and your affiliates stores, manages, or queries terminology, code system content, or health data. |
| Selling, sublicensing, or otherwise distributing it for a fee, on its own or inside another product | Commercial licence | Carve-out (b) of the grant. |

The last two rows hold whoever you are: hosting it for third parties and
distributing it for a fee need a commercial licence in every case, including
for an organisation the free rows above would otherwise cover.

**Each version becomes Apache License 2.0 four years after that version is
published.** A commercial licence starts with a short conversation with the
maintainer named in [MAINTAINERS.md](MAINTAINERS.md).

## Licensing

The software is source-available under the Business Source License 1.1
([`LICENSE`](LICENSE), [`NOTICE`](NOTICE)), with no open-core tier: the
engine, the server, and the tools are in this repository under the one
licence, and nothing is held back to be sold back to you.

The licence lets you read, build, modify, and redistribute the source without
a fee and without asking anyone, and it covers every non-production use:
development, testing, evaluation, and prototyping. Production use is free for
Non-Commercial Purposes, which the licence defines as personal use, academic or
scientific research, teaching, and use by a non-profit organisation or public
body that is not in the course of a business, does not deliver a service for
payment, and is not for commercial advantage. Any other production use needs a
commercial licence from the Licensor: a hospital, clinic, or care provider
running FerroTERM for its patients needs one, and so does a vendor, integrator,
or any company running it in production. Offering FerroTERM, or a work derived
from it, to third parties as a hosted, managed, or embedded terminology
service, and selling, sublicensing, or otherwise distributing it for a fee on
its own or inside another product, need a commercial licence in every case.
Each version becomes Apache License 2.0 four years after that version is
published. One crate is outside all of this: `rf2` (the SNOMED CT release file
reader) is Apache 2.0 on crates.io, so any Rust project can use it without a
licence conversation. The commercial licence starts with a short conversation with the
maintainer named in [MAINTAINERS.md](MAINTAINERS.md).

The code systems are licensed separately: SNOMED CT by SNOMED International,
LOINC by Regenstrief, ICD by WHO, RxNorm by NLM, and the repository ships none
of their content. You bring the release you are licensed for; UCUM and the IANA
and Unicode registries are vendored under their own licences, recorded beside
the data.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) has the rules; the open issues are the
worklist. Every change ships with tests, and conformance-facing behaviour cites
the FHIR or SNOMED specification it implements.
