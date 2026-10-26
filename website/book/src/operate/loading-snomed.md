<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Loading code systems

FerroTERM serves the code systems you load. Each arrives as the release its
owner distributes, turned once into an index by `ferroterm-build`, and served
read-only. This page covers the licensing rule, the build step, and the
command for every supported source; the sections below name each system.

<!-- toc -->

## You bring your own content

> [!WARNING]
> This repository ships no code system content, and no build of FerroTERM
> contains any. SNOMED CT is licensed by SNOMED International, LOINC by
> Regenstrief, ICD by the WHO, RxNorm by the NLM. You must hold whatever
> licence the release you load requires.

The split is firm: the software in this repository is under the Business Source
License 1.1, and the code system content is the property of its owner and is
not distributed here. A SNOMED CT licence is free within member countries, the
Netherlands among them, and available under the affiliate licence elsewhere;
LOINC needs a free account; ICD-10-CM and the RxNorm prescribable subset are
public downloads; ICD-11 comes through the WHO's own container under its
licence.

## The offline build

The running server never parses a release. `ferroterm-build` turns it into the
index once, and the server opens the index read-only and reads it in at
startup. The tool ships in every release tarball beside `ferroterm` and in the
container image.

```mermaid
graph LR
    REL["A release you are licensed for"] --> BUILD["ferroterm-build (offline)"]
    BUILD --> IDX["index: store.redb + its column files + hierarchy.bin + text.bin + manifest.json"]
    IDX --> SRV["ferroterm (read-only)"]
```

Every source builds into the same layout, and the manifest names the system,
so `FERROTERM__TERMINOLOGY__INDEX` takes any mix of them. When a new release arrives, rebuild
its index and restart the server against it.

Each command below names its output directory with `--out`. Give
`--index-root DIR` (or set `FERROTERM_INDEX_ROOT`) instead, and the build
writes a child of that root named for the source (`snomed`, `loinc`,
`rxnorm`, ...) and refuses an `--out` outside it. The compose `build` service
sets the root to its index mount (see
[Where a build writes](install.md#where-a-build-writes)). Every build prints
the absolute path it wrote.

## Loading a SNOMED CT edition

The command takes the release zip as SNOMED International or your national
release centre distributes it, or the unpacked release directory, and writes
the index:

```console
$ ferroterm-build --rf2 /path/to/SnomedCT_Release.zip --out /path/to/ferroterm-index
```

From a zip, only the `Snapshot/` tree is unpacked, to a temporary directory
that is removed when the build ends; the Full and Delta trees are never read.
With the container image the same build is one compose command (see
[Install and run](install.md)). The build reads the concepts, descriptions,
language reference sets, and inferred relationships, computes the transitive
closure, and writes the store, the hierarchy, and the text index. The
[benchmarks page](../evaluate/benchmarks.md) records how long each release
takes and how large its index is.

The edition and version come from the release itself (the module and
effective time), and `$lookup` returns them as the version URI
(`http://snomed.info/sct/11000146104/version/20260630`).

## Loading LOINC

The same tool builds a LOINC release into the same artifact layout. Download
the release from <https://loinc.org/downloads/> (a free account; the licence
allows use and redistribution with the LOINC copyright notice, and the
repository ships no release) and build it:

```console
$ ferroterm-build --loinc /path/to/Loinc_2.82.zip --out /path/to/loinc-index
```

The zip's term table, parts, hierarchy, answer lists, and linguistic variants
are unpacked to a temporary directory; the part-link tables are not read. The
version is taken from the release name (`Loinc_2.82`), else pass
`--loinc-version`. Point `FERROTERM__TERMINOLOGY__INDEX` at the directory beside your SNOMED
CT index; the server opens each by the system its manifest names.

## Loading ICD-10 and ICD-10-CM

The same tool builds the ICD-10 family. A ClaML document (WHO ICD-10 from
your WHO licence, the Dutch translation from the WHO-FIC Collaborating Centre
at RIVM, or any other ClaML classification) is served under the system URI
you name:

```console
$ ferroterm-build --claml /path/to/icd10nl-2021.xml \
    --system http://hl7.org/fhir/sid/icd-10-nl --out /path/to/icd10nl-index
```

The version comes from the document's `Title`; pass `--claml-version` when
the title has none. A zip is accepted; its largest `.xml` entry is the
document.

ICD-10-CM is a free download from CMS
(<https://www.cms.gov/medicare/coding-billing/icd-10-codes>): the "Code
Descriptions in Tabular Order" zip holds the order file and the "Code Tables,
Tabular and Index" zip holds the tabular XML. Give both:

```console
$ ferroterm-build --icd10cm 2026-code-descriptions-tabular-order.zip \
    --icd10cm 2026-code-tables-tabular-and-index.zip --out /path/to/icd10cm-index
```

Codes come out with the period (`A00.0`, `S02.0XXA`); chapters are named by
their range (`A00-B99`); the order file's header flag is the `valid`
property. Point `FERROTERM__TERMINOLOGY__INDEX` at each directory as for LOINC.

## Loading ATC

The WHO Collaborating Centre sells the ATC/DDD index as a spreadsheet; export
it to CSV (any of comma, semicolon, or tab) with its columns `ATC code`,
`ATC level name`, `DDD`, `U`, `Adm.R`, and `Note`, and build it with the index
year as the version:

```console
$ ferroterm-build --atc /path/to/atc-index-2026.csv --atc-version 2026 --out /path/to/atc-index
```

A G-Standaard subscriber builds the same tree from the `BST801T` file (the
Dutch and English names, no DDDs):

```console
$ ferroterm-build --atc /path/to/BST801T --atc-version 2026 --out /path/to/atc-index
```

The system is `http://www.whocc.no/atc`; the five levels are the `kind`
property and filter; every DDD is a `ddd` property.

## Loading the DHD thesauri

A DHD licensee receives the Diagnosethesaurus and the Verrichtingenthesaurus
as zips of CSV tables ("Uitleverformaat 5.0"). Build a delivery as it comes;
the version is read from the zip name:

```console
$ ferroterm-build --dhd 20260901_120000_Diagnosethesaurus_2.40_uitleverformaat_5.0.zip \
    --out /path/to/dhd-diagnoses
```

The system is `urn:oid:2.16.840.1.113883.2.4.3.120.5.1`, a flat table: the
preferred term is the display, synonyms and patient-friendly terms are
designations, and the SNOMED CT identifier, the ICD-10, DBC, and ZA
derivations, the roles, and the umbrella terms are properties. Concepts ended
before the delivery date are served inactive. The build also writes two
`ConceptMap` resources under `conceptmaps/` in the output directory (the
thesaurus to SNOMED CT and to ICD-10); point `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` at that
directory to serve them with `$translate`. Pass `--dhd-version` when the zip
name does not carry the version.

## Loading the G-Standaard product ladder

A Z-Index subscriber builds the GPK, PRK, HPK, and article code systems from
the monthly release directory in one run; the release is the version:

```console
$ ferroterm-build --gstandaard /path/to/g-standaard/202609 --gstandaard-version 202609 \
    --out /path/to/gstandaard
```

Four artifact directories appear under the output (`gpk`, `prk`, `hpk`,
`artikel`), one `urn:oid` system each; list them all in `FERROTERM__TERMINOLOGY__INDEX`.
Each is a flat table: the full name from the names file is the display, the
short and label names are designations, and the rungs above a concept (`gpk`,
`prk`, `hpk`), the ATC code, form, route, brand, and firm are properties.

## Loading the Nederlandse Labcodeset

A Labcodeset licensee receives one XML publication per release. Build it as
it comes; the release date is read from the document:

```console
$ ferroterm-build --labcodeset Labcodeset_v2026-08.zip --out /path/to/labcodeset
```

The build writes a directory of FHIR resources under `labcodeset/` in the
output directory: the value set of the active concepts over LOINC
(`https://ferroterm.eu/fhir/ValueSet/nl-labcodeset`, Dutch displays, English
designations), a LOINC supplement carrying the Dutch names and the
publication's facts as properties (materials, units, outcome lists, statuses,
replacements), and one value set per ordinal outcome list under its OID
(`urn:oid:…`). Point `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` at that directory beside a LOINC
artifact in `FERROTERM__TERMINOLOGY__INDEX`; the SNOMED CT materials and outcome refsets
resolve when a SNOMED CT edition is loaded too. Retired concepts stay in the
supplement, marked `labcodeset-status = retired`, and leave the value set.

## Loading the NHG ICPC-1 to SNOMED CT map

Nictiz and the NHG distribute the ICPC-1 (NHG table 24) to SNOMED CT mapping
as a FHIR R4 `ConceptMap`. Put the file in a directory with a `package.json`
naming its FHIR version, and point `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` at it:

```json
{"name": "nl.nictiz.nhg-icpc-snomed", "version": "2026-03-31", "fhirVersions": ["4.0.1"]}
```

`$translate` then answers from `http://hl7.org/fhir/sid/icpc-1-nl` to SNOMED
CT on every version endpoint. The 2026-03-31 release writes `experimental` as
the string `"false"`; FHIR JSON requires a boolean there
(<https://hl7.org/fhir/R4/json.html>), and the server refuses the file until
it reads `"experimental": false`.

## Loading RxNorm

The "Current Prescribable Content" subset needs no licence
(<https://www.nlm.nih.gov/research/umls/rxnorm/docs/prescribe.html>); the
full monthly release needs a UMLS licence. Both have the same shape:

```console
$ ferroterm-build --rxnorm RxNorm_full_prescribe_09082026.zip --out /path/to/rxnorm-index
```

The version is the release date from the readme inside the zip (`09082026`);
pass `--rxnorm-version` when the release has none. The build keeps the names
of the unrestricted sources (`RXNORM`, `MTHSPL`). A full release carries
sources the UMLS licence restricts by category; name the ones your licence
covers with `--rxnorm-sources MSH,VANDF` to serve their names too. Point
`FERROTERM__TERMINOLOGY__INDEX` at the directory as for LOINC.

## Loading ICD-11

ICD-11 comes from the WHO ICD-API. Run the local deployment WHO publishes
(<https://icd.who.int/icdapi>; the licence, CC BY-ND 3.0 IGO, allows the
local copy but not passing it on), then let the build walk it into a cache
and build the three code systems:

```console
$ docker run -d -p 8080:80 -e acceptLicense=true -e saveAnalytics=false \
    -e include=2026-01_en whoicd/icd-api
$ ferroterm-build --icd11 /path/to/icd11-cache --icd11-api http://127.0.0.1:8080 \
    --icd11-release 2026-01 --icd11-languages en --out /path/to/icd11-index
```

The cache holds one JSON file per entity and language (about 110,000 files
for the MMS, the ICF, and the Foundation) and is reused by later builds, so
the API is only needed once per release. The build writes
`icd11-index/mms`, `icd11-index/icf`, and `icd11-index/entity`; point
`FERROTERM__TERMINOLOGY__INDEX` at each of the three. Languages beyond English need the
deployment to include them (`include=2026-01_fr`); the build records every
language the cache holds.

## Test content

FerroTERM's own tests use shaped, synthetic content only. They never contain real
content extracted from a release, which keeps the licence line clean in the
repository itself.

## Derivative reference set packages

SNOMED International publishes packages that carry only a module and its
reference sets, over an edition they depend on. The ICNP Nursing Practice
package is one: two concepts, a reference set of about two thousand
International Edition concepts, and a module dependency naming the edition it
was authored against.

Layer one onto the edition it depends on with `--rf2-refset`, repeatable:

```console
$ ferroterm-build --rf2 /path/to/SnomedCT_InternationalRF2.zip \
    --rf2-refset /path/to/SnomedCT_ICNPNursingPractice.zip \
    --out /path/to/ferroterm-index
```

The package's concepts, descriptions, and language reference set members join
the edition's, and its reference sets serve as implicit value sets of the
edition, so `http://snomed.info/sct?fhir_vs=refset/1157358007` expands to the
ICNP members. The manifest records what was layered.

Each package's module dependency is checked against the edition before
anything is read. An edition at or newer than the date a package was authored
against meets the dependency; an older one does not and the build refuses,
naming the module and both dates. A package naming a module the edition does
not contain is refused the same way.
