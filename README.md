<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->
<div align="center">

<img src="assets/brand/ferroterm-lockup-auto.svg" alt="FerroTERM" width="290">

**The FHIR terminology server for SNOMED CT, LOINC, ICD and every other code system, in one binary.**

[Documentation](https://ferroterm.eu/docs/) ·
[Quick start](#quick-start) ·
[What ships today](#what-ships-today) ·
[Licensing](https://ferroterm.eu/docs/evaluate/licensing.html)

**Project**<br>
[![Release](https://img.shields.io/github/v/release/FerroHEALTH/FerroTERM?sort=semver&logo=github&label=release)](https://github.com/FerroHEALTH/FerroTERM/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroTERM/ci.yml?branch=main&logo=githubactions&logoColor=white&label=CI)](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/ci.yml)
[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-ferroterm.eu-blue?logo=mdbook)](https://ferroterm.eu/docs/)
[![GHCR](https://img.shields.io/badge/ghcr.io-ferroterm-2496ED?logo=docker&logoColor=white)](https://github.com/FerroHEALTH/FerroTERM/pkgs/container/ferroterm)
[![Image pulls](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fghcr-badge.elias.eu.org%2Fapi%2FFerroHEALTH%2FFerroTERM%2Fferroterm&query=downloadCount&label=image%20pulls&logo=github)](https://github.com/FerroHEALTH/FerroTERM/pkgs/container/ferroterm)

**Quality and security**<br>
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroTERM/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroTERM)
[![Quality gate](https://img.shields.io/sonar/quality_gate/FerroHEALTH_FerroTERM?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=quality%20gate)](https://sonarcloud.io/summary/overall?id=FerroHEALTH_FerroTERM)
[![Coverage](https://img.shields.io/sonar/coverage/FerroHEALTH_FerroTERM?server=https%3A%2F%2Fsonarcloud.io&logo=sonarqubecloud&label=coverage)](https://sonarcloud.io/summary/new_code?id=FerroHEALTH_FerroTERM)
[![CodeQL](https://img.shields.io/github/actions/workflow/status/FerroHEALTH/FerroTERM/codeql.yml?branch=main&logo=github&label=CodeQL)](https://github.com/FerroHEALTH/FerroTERM/actions/workflows/codeql.yml)
[![Signed releases](https://img.shields.io/badge/releases-signed-brightgreen?logo=github)](https://ferroterm.eu/docs/operate/verifying-releases.html)
[![SLSA Build Level 3](https://slsa.dev/images/gh-badge-level3.svg)](https://ferroterm.eu/docs/operate/verifying-releases.html)

**Conformance and compliance**<br>
[![tx-ecosystem R4](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4.json)](https://ferroterm.eu/docs/evaluate/conformance.html)
[![tx-ecosystem R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4b.json)](https://ferroterm.eu/docs/evaluate/conformance.html)
[![tx-ecosystem R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr5.json)](https://ferroterm.eu/docs/evaluate/conformance.html)
<br>
[![CRA](https://img.shields.io/badge/CRA-risk%20assessment-blue?logo=europeanunion)](https://ferroterm.eu/docs/evaluate/cra-risk-assessment.html)

</div>

## What FerroTERM does

Health records are full of codes. A diagnosis is a SNOMED CT or ICD code, a
lab result a LOINC code, a medicine a code from a national drug database.
These code systems (the shared lists of medical terms) are what let two
systems agree that they mean the same thing. Every application that records
or exchanges clinical data needs to look codes up, check them and search
them.

FerroTERM answers those questions for every application at once. It loads
the code systems you are licensed for and serves them over FHIR, the
standard interface health software uses for terminology: look up a code,
check that a code belongs to a list, find every narrower term, translate a
code from one system to another. It runs as one program, with no database
or search engine to operate beside it.

### Who it is for

- **Hospitals and national eHealth programmes** that need one terminology
  service for all their applications.
- **Software vendors and integrators** that need SNOMED CT, LOINC or ICD
  in their products without running a heavy terminology stack.
- **Terminology teams** that edit and publish value sets (curated lists of
  codes) in the console.

### Why you can trust what it claims

Every release is signed and comes with a software bill of materials (the
list of every component inside) and SLSA build provenance, so you can check
that what you install is what was built here. The source is public, and
every feature is in this repository: there is no paid edition with extra
functions. The conformance badges come from the HL7 terminology test suite,
run against every change, with a committed pass list per FHIR version, so
you can check a result against the standard yourself.

## Part of FerroHEALTH

FerroTERM is one of five [FerroHEALTH](https://ferrohealth.eu) servers.
Each does one job and reaches the others only over published standards:
FerroEHR stores the record, FerroBRIDGE translates it to FHIR and the OMOP
research model, FerroFED connects records across organisations and borders,
FerroPIX matches patient identities, and FerroTERM serves the code systems.
Any FHIR client can use FerroTERM, not only the FerroHEALTH products. Sign-in,
the audit trail and the web consoles come from three shared libraries:
FerroAUTH, FerroAUDIT and FerroUI.

<img src="website/book/src/operate/img/console/dashboard.png" alt="The FerroTERM console dashboard">

## Quick start

The image serves UCUM, BCP 47, BCP 13 and ISO 3166-1 with no configuration,
so the first call needs nothing beyond Docker:

```sh
docker run --rm -p 8080:8080 ghcr.io/ferrohealth/ferroterm:0.3.2
```

In a second terminal, look up a unit of measure:

```sh
curl 'http://localhost:8080/r4b/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL'
```

A clinical code system such as SNOMED CT is a release you hold a licence
for. `ferroterm-build`, in the same image, turns it into an index once, and
the server reads that index at startup.
[Install and run](https://ferroterm.eu/docs/operate/install.html) and
[loading code systems](https://ferroterm.eu/docs/operate/loading-snomed.html)
continue from here.

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
lists, per system, the versions handled, the build command and the licence
position. You bring the release you are licensed for; UCUM and the
registries are built in.

## What ships today

### Terminology

- [The FHIR terminology API](https://ferroterm.eu/docs/integrate/fhir-api.html): `$lookup`, `$validate-code`, `$subsumes`, `$expand`, `$translate` and `$closure`
- [FHIR R4, R4B, R5 and the R6 ballot](https://ferroterm.eu/docs/integrate/fhir-versions.html), each in its own version's shapes, in JSON and XML
- [Value sets, implicit value sets and ECL](https://ferroterm.eu/docs/integrate/ecl-value-sets.html), the SNOMED CT expression language
- [Stored value sets, code systems and concept maps](https://ferroterm.eu/docs/integrate/fhir-api.html#persisted-resources) with history, when you name a database

### Operations

- [The console](https://ferroterm.eu/docs/operate/console.html): browse, look up, expand and translate, and compose value sets
- [Syncing from a national terminology service](https://ferroterm.eu/docs/operate/sync.html), one way into a running server
- [Health, metrics and request identifiers](https://ferroterm.eu/docs/operate/observability.html), and [hardware sizing](https://ferroterm.eu/docs/operate/hardware-sizing.html)
- [Verifiable releases](https://ferroterm.eu/docs/operate/verifying-releases.html): images for amd64 and arm64 and release binaries, signed, with a software bill of materials

### Libraries

- The SNOMED CT, LOINC, ICD, RxNorm and Dutch release readers and the terminology engine, published as Rust crates (listed below)

<details>
<summary><b>Speed and footprint</b>, measured per code system</summary>

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

</details>

<details>
<summary><b>Published crates</b> (14)</summary>

| Crate | What it does | Version | Docs | Downloads |
|---|---|---|---|---|
| [`rf2`](https://crates.io/crates/rf2) | The SNOMED CT RF2 release loader and typed component model (Apache-2.0) | [![crates.io](https://img.shields.io/crates/v/rf2?logo=rust&label=crates.io)](https://crates.io/crates/rf2) | [![docs.rs](https://img.shields.io/docsrs/rf2?logo=docsdotrs&label=docs.rs)](https://docs.rs/rf2) | [![downloads](https://img.shields.io/crates/d/rf2?logo=rust&label=downloads)](https://crates.io/crates/rf2) |
| [`concept-graph`](https://crates.io/crates/concept-graph) | The SNOMED CT ontology as CSR adjacency and roaring transitive-closure bitmaps | [![crates.io](https://img.shields.io/crates/v/concept-graph?logo=rust&label=crates.io)](https://crates.io/crates/concept-graph) | [![docs.rs](https://img.shields.io/docsrs/concept-graph?logo=docsdotrs&label=docs.rs)](https://docs.rs/concept-graph) | [![downloads](https://img.shields.io/crates/d/concept-graph?logo=rust&label=downloads)](https://crates.io/crates/concept-graph) |
| [`concept-store`](https://crates.io/crates/concept-store) | The disk-backed columnar concept and description store | [![crates.io](https://img.shields.io/crates/v/concept-store?logo=rust&label=crates.io)](https://crates.io/crates/concept-store) | [![docs.rs](https://img.shields.io/docsrs/concept-store?logo=docsdotrs&label=docs.rs)](https://docs.rs/concept-store) | [![downloads](https://img.shields.io/crates/d/concept-store?logo=rust&label=downloads)](https://crates.io/crates/concept-store) |
| [`designation-index`](https://crates.io/crates/designation-index) | The fst and roaring description search index | [![crates.io](https://img.shields.io/crates/v/designation-index?logo=rust&label=crates.io)](https://crates.io/crates/designation-index) | [![docs.rs](https://img.shields.io/docsrs/designation-index?logo=docsdotrs&label=docs.rs)](https://docs.rs/designation-index) | [![downloads](https://img.shields.io/crates/d/designation-index?logo=rust&label=downloads)](https://crates.io/crates/designation-index) |
| [`sct-ecl`](https://crates.io/crates/sct-ecl) | The SNOMED CT Expression Constraint Language lexer, parser and evaluator | [![crates.io](https://img.shields.io/crates/v/sct-ecl?logo=rust&label=crates.io)](https://crates.io/crates/sct-ecl) | [![docs.rs](https://img.shields.io/docsrs/sct-ecl?logo=docsdotrs&label=docs.rs)](https://docs.rs/sct-ecl) | [![downloads](https://img.shields.io/crates/d/sct-ecl?logo=rust&label=downloads)](https://crates.io/crates/sct-ecl) |
| [`sct-scg`](https://crates.io/crates/sct-scg) | The SNOMED CT Compositional Grammar lexer, parser and printer | [![crates.io](https://img.shields.io/crates/v/sct-scg?logo=rust&label=crates.io)](https://crates.io/crates/sct-scg) | [![docs.rs](https://img.shields.io/docsrs/sct-scg?logo=docsdotrs&label=docs.rs)](https://docs.rs/sct-scg) | [![downloads](https://img.shields.io/crates/d/sct-scg?logo=rust&label=downloads)](https://crates.io/crates/sct-scg) |
| [`loinc`](https://crates.io/crates/loinc) | The LOINC release loader and typed row model | [![crates.io](https://img.shields.io/crates/v/loinc?logo=rust&label=crates.io)](https://crates.io/crates/loinc) | [![docs.rs](https://img.shields.io/docsrs/loinc?logo=docsdotrs&label=docs.rs)](https://docs.rs/loinc) | [![downloads](https://img.shields.io/crates/d/loinc?logo=rust&label=downloads)](https://crates.io/crates/loinc) |
| [`classification`](https://crates.io/crates/classification) | The ClaML and ICD-10-CM tabular readers into one classification model | [![crates.io](https://img.shields.io/crates/v/classification?logo=rust&label=crates.io)](https://crates.io/crates/classification) | [![docs.rs](https://img.shields.io/docsrs/classification?logo=docsdotrs&label=docs.rs)](https://docs.rs/classification) | [![downloads](https://img.shields.io/crates/d/classification?logo=rust&label=downloads)](https://crates.io/crates/classification) |
| [`dhd-thesaurus`](https://crates.io/crates/dhd-thesaurus) | The DHD Diagnosethesaurus and Verrichtingenthesaurus reader | [![crates.io](https://img.shields.io/crates/v/dhd-thesaurus?logo=rust&label=crates.io)](https://crates.io/crates/dhd-thesaurus) | [![docs.rs](https://img.shields.io/docsrs/dhd-thesaurus?logo=docsdotrs&label=docs.rs)](https://docs.rs/dhd-thesaurus) | [![downloads](https://img.shields.io/crates/d/dhd-thesaurus?logo=rust&label=downloads)](https://crates.io/crates/dhd-thesaurus) |
| [`gstandaard`](https://crates.io/crates/gstandaard) | The G-Standaard (Z-Index) product ladder reader | [![crates.io](https://img.shields.io/crates/v/gstandaard?logo=rust&label=crates.io)](https://crates.io/crates/gstandaard) | [![docs.rs](https://img.shields.io/docsrs/gstandaard?logo=docsdotrs&label=docs.rs)](https://docs.rs/gstandaard) | [![downloads](https://img.shields.io/crates/d/gstandaard?logo=rust&label=downloads)](https://crates.io/crates/gstandaard) |
| [`labcodeset`](https://crates.io/crates/labcodeset) | The Nederlandse Labcodeset publication reader | [![crates.io](https://img.shields.io/crates/v/labcodeset?logo=rust&label=crates.io)](https://crates.io/crates/labcodeset) | [![docs.rs](https://img.shields.io/docsrs/labcodeset?logo=docsdotrs&label=docs.rs)](https://docs.rs/labcodeset) | [![downloads](https://img.shields.io/crates/d/labcodeset?logo=rust&label=downloads)](https://crates.io/crates/labcodeset) |
| [`icd11`](https://crates.io/crates/icd11) | ICD-11 entities from the WHO ICD-API local deployment, cached and read | [![crates.io](https://img.shields.io/crates/v/icd11?logo=rust&label=crates.io)](https://crates.io/crates/icd11) | [![docs.rs](https://img.shields.io/docsrs/icd11?logo=docsdotrs&label=docs.rs)](https://docs.rs/icd11) | [![downloads](https://img.shields.io/crates/d/icd11?logo=rust&label=downloads)](https://crates.io/crates/icd11) |
| [`rxnorm-rrf`](https://crates.io/crates/rxnorm-rrf) | The RxNorm Rich Release Format reader and typed row model | [![crates.io](https://img.shields.io/crates/v/rxnorm-rrf?logo=rust&label=crates.io)](https://crates.io/crates/rxnorm-rrf) | [![docs.rs](https://img.shields.io/docsrs/rxnorm-rrf?logo=docsdotrs&label=docs.rs)](https://docs.rs/rxnorm-rrf) | [![downloads](https://img.shields.io/crates/d/rxnorm-rrf?logo=rust&label=downloads)](https://crates.io/crates/rxnorm-rrf) |
| [`fhir-terminology`](https://crates.io/crates/fhir-terminology) | The FHIR terminology operations over the code system provider seam | [![crates.io](https://img.shields.io/crates/v/fhir-terminology?logo=rust&label=crates.io)](https://crates.io/crates/fhir-terminology) | [![docs.rs](https://img.shields.io/docsrs/fhir-terminology?logo=docsdotrs&label=docs.rs)](https://docs.rs/fhir-terminology) | [![downloads](https://img.shields.io/crates/d/fhir-terminology?logo=rust&label=downloads)](https://crates.io/crates/fhir-terminology) |

</details>

## Learn more

- [Documentation](https://ferroterm.eu/docs/) and the [FerroHEALTH book](https://ferrohealth.eu/docs/)
- [What FerroTERM is](https://ferroterm.eu/docs/evaluate/what-ferroterm-is.html), [architecture at a glance](https://ferroterm.eu/docs/evaluate/architecture.html) and [how it compares](https://ferroterm.eu/docs/evaluate/comparison.html)
- [Conformance](https://ferroterm.eu/docs/evaluate/conformance.html) and [benchmarks](https://ferroterm.eu/docs/evaluate/benchmarks.html)
- [Changelog](CHANGELOG.md) and [milestones](https://github.com/FerroHEALTH/FerroTERM/milestones)
- [Security policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md), [code of conduct](CODE_OF_CONDUCT.md), [support](SUPPORT.md), [governance](GOVERNANCE.md), [maintainers](MAINTAINERS.md) and the [AI statement](AI_STATEMENT.md)

## Licence

Business Source License 1.1 ([LICENSE](LICENSE)), with Apache License 2.0 as
the Change License four years after each version; the `rf2` crate is
Apache-2.0 today. The code systems keep their own licences, and the
[licensing page](https://ferroterm.eu/docs/evaluate/licensing.html) says
when you need a commercial licence from Cadasto B.V.
