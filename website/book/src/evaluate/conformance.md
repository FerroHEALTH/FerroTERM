<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Conformance

The badges on the repository's README show how much of the HL7 terminology
ecosystem test suite each served FHIR version passes:

![tx-ecosystem R4](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4.json)
![tx-ecosystem R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr4b.json)
![tx-ecosystem R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fr5.json)

## What the suite is

The [HL7 terminology ecosystem implementation guide](https://hl7.org/fhir/uv/tx-ecosystem/)
publishes the test cases the FHIR Validator uses against terminology servers.
FerroTERM runs the suite's `general` mode with the FHIR Validator's `txTests`
command against a release build of the server, once per served FHIR version
(`/r4`, `/r4b`, `/r5`), on every pull request and every push to `main`. The
suite's commit, its test case version, and the validator version are pinned in
`docs/VERSIONS.md` and in `scripts/checks/tx-ecosystem.sh`.

## What the numbers mean

A badge reads `passed / total`. `passed` is the number of cases on the
committed pass list for that version (`conformance/tx-ecosystem/passing*.txt`
in the repository); `total` is the number of cases the suite ran. Continuous
integration fails when a listed case stops passing, and a case joins the list
in the same change that makes it pass, so the badge never says more than CI
has verified. The site regenerates the badge data from those lists on every
deploy.

The pass lists are the honest state, not a ceiling: the remaining cases and
their clusters are tracked on the issue tracker, and the lists grow with the
work. The suite's other modes (SNOMED CT, ICD-11, LOINC and more) have their
own lists beside the general ones, described [below](#the-other-modes).

## Where the remaining cases stand

The `general` mode counts 805 cases on every surface. With suite `9c2bbce`
and validator 7.0.6, `/r4` passes 745, `/r4b` 730 and `/r5` 744. Every case
that does not pass falls into one of the rows below. None of them needs
licensed content: the cases that do (SNOMED CT, LOINC, ICD-11, OMOP) sit in
the suite's other modes, which are not in these counts.

| Why the case does not pass | Tracked in | R4 | R4B | R5 |
|---|---|---|---|---|
| **Not run on this surface.** The case is gated to another FHIR version or to the `tx.fhir.org` mode, and the runner still counts it in the 805. | [#1361](https://github.com/FerroHEALTH/FerroTERM/issues/1361), [#1380](https://github.com/FerroHEALTH/FerroTERM/issues/1380) | 5 | 9 | 9 |
| **The R4B surface.** The suite targets R4 and R5: on `/r4b` its `$version$` stands for R4's `4.0.1` (the `metadata`, `exclude` and two `contained-nested` cases), and its `!4.0` gate lets two R5-only reverse `$translate` cases run against R4B, which keeps `reverse`. | [#1361](https://github.com/FerroHEALTH/FerroTERM/issues/1361), [#1380](https://github.com/FerroHEALTH/FerroTERM/issues/1380) | 0 | 11 | 0 |
| **No published definition.** The `$lookup` cases expect a `source` part on a designation, and `ec-validate-outside` expects no `version`, which no publication defines. | [#1376](https://github.com/FerroHEALTH/FerroTERM/issues/1376) | 10 | 10 | 10 |
| The cases send `reset` or `dispose` to `$closure`, or `useSupplement` to `CodeSystem/$validate-code`, which no OperationDefinition declares; nine more `closure` cases fail because the closure the first one sets up is never made. | [#1068](https://github.com/FerroHEALTH/FerroTERM/issues/1068) | 21 | 21 | 21 |
| The cases expect the `$lookup` output `warning`, which no OperationDefinition declares. | [#1069](https://github.com/FerroHEALTH/FerroTERM/issues/1069) | 2 | 2 | 2 |
| The cases expect concept properties (`label`, `order`) and extension placements no published definition allows. | [#1107](https://github.com/FerroHEALTH/FerroTERM/issues/1107) | 9 | 9 | 9 |
| The cases turn on `versionsMatch`, an expansion parameter no publication defines. | [#507](https://github.com/FerroHEALTH/FerroTERM/issues/507) | 8 | 8 | 8 |
| The cases expect R5's `$lookup` `definition` output on R4 and R4B, whose OperationDefinitions do not declare it. | [#1330](https://github.com/FerroHEALTH/FerroTERM/issues/1330) | 3 | 3 | 0 |
| The cases send an input the operation does not declare (`codingX` in a batch), and FerroTERM refuses an undeclared input. | [#349](https://github.com/FerroHEALTH/FerroTERM/issues/349) | 2 | 2 | 2 |
| **Total not passing** | | 60 | 75 | 61 |

The first two rows do not apply on their surface: 5 cases on R4, 20 on
R4B and 9 on R5. Each badge shows the pass count alone, so R4 reads
`745/805`, and of the 60 cases R4 does not pass, 55 are FerroTERM's to
answer for, 52 on R5 and 55 on R4B. The cases and
their reasons are listed in `conformance/tx-ecosystem/not-applicable.tsv`,
which every CI run of the suite checks against the cases the runner skipped.

The rows marked as having no published definition are upstream reports:
FerroTERM answers what the published OperationDefinitions and extensions
define, and each report names the change upstream that would settle it.
Every row is a gate or an upstream report.

## The other modes

Each of the suite's other modes has its own badge, labelled
`tx <suite> <mode> <surface>`, which reads `passed/ran`: `ran` is the number of
cases the mode runs.

A mode whose content CI may fetch runs in CI, weekly and on demand
(`.github/workflows/tx-ecosystem-modes.yml`), against its committed pass list
and the count `conformance/tx-ecosystem/ci-modes.tsv` records. The lane fetches
the content from its public source, checks it against a pinned sha256, and
deletes it when the run ends; none of it is committed, cached or uploaded.
`snomed` runs this way, over the suite's own SNOMED CT test edition.

The modes CI does not run yet are measured by hand before a release. Their
counts, the day of each run and the reason are in
`conformance/tx-ecosystem/hand-run.tsv`, and the table below is written from
it:

<!-- hand-run.tsv: written by scripts/checks/conformance-badges.sh --write-hand-runs -->
| mode | surface | ran | measured | why by hand |
|---|---|---|---|---|
| `icd-11` | `/r4b` | 145 | 2026-11-27 | needs the WHO ICD-API container; its CI lane is not built yet (#1437) |
| `icf` | `/r4b` | 96 | 2026-11-27 | needs the ICF releases of fhir.tx.support; its CI lane is not built yet (#1437) |
| `loinc` | `/r4b` | 110 | 2026-11-27 | needs a LOINC release; its CI lane is not built yet (#1437) |
| `loinc` | `/r5` | 110 | 2026-11-27 | needs a LOINC release; its CI lane is not built yet (#1437) |
| `tx.fhir.org` | `/r4b` | 189 | 2026-11-27 | needs a LOINC release; its CI lane is not built yet (#1437) |
| `tx.fhir.org` | `/r5` | 189 | 2026-11-27 | needs a LOINC release; its CI lane is not built yet (#1437) |
| `mimetypes` | `/r4b` | 52 | 2026-11-27 | its CI lane is not built yet (#1437) |
| `omop` | `/r4b` | 40 | 2026-11-27 | its CI lane is not built yet (#1437) |
<!-- end of hand-run.tsv -->

![tx-ecosystem snomed R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fsnomed-r4b.json)
![tx-ecosystem icd-11 R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Ficd-11-r4b.json)
![tx-ecosystem icf R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Ficf-r4b.json)
![tx-ecosystem loinc R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Floinc-r4b.json)
![tx-ecosystem loinc R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Floinc-r5.json)
![tx-ecosystem tx.fhir.org R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Ftx-fhir-org-r4b.json)
![tx-ecosystem tx.fhir.org R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Ftx-fhir-org-r5.json)
![tx-ecosystem mimetypes R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fmimetypes-r4b.json)
![tx-ecosystem omop R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fomop-r4b.json)

The `snomed` mode runs over the suite's own SNOMED CT test edition with
`terminology.default_language = "en-US"`, because its expected displays are the
US English preferred terms. That is a deployment's choice of language
reference set: no specification picks one, and the shipped default is `en`.

`conformance/tx-ecosystem/README.md` says what each mode needs and why its
open cases stay open.

## What the numbers are not

The suite is the ecosystem's test suite, not a certification. Passing a case
means the server answers as the suite expects; it does not make the server an
HL7-certified product, and HL7 issues no such certification for terminology
servers. Registration in the HL7 terminology ecosystem is a separate step and
will be linked from this page when it exists.

## The EHDS badges

Two badges under **EHDS (self-tested)** in the README report the European
Health Data Space terminology work. Each counts a committed corpus that a
test proves over synthetic or freely licensed data, because FerroTERM ships
no licensed content; neither is a certification or a conformity assessment.

- **MyHealth@EU MVC 10.1.0 import** counts the value sets of a synthetic
  package shaped like the CI build of the Master Value Sets Catalogue 10.1.0
  that the server answers after `ferroterm-build --fhir-package` imports it
  (`conformance/ehds/mvc-synthetic.tsv`, held to the fixture by a server
  test). The real catalogue is the operator's to load.
- **EEHRxF draft 2026-11-23 Art 6** counts the coding systems of
  the draft EEHRxF implementing act whose loader a test proves over a
  synthetic fixture, of the 16 the draft names
  (`conformance/ehds/art6-code-systems.tsv`, each row naming its test,
  checked by `scripts/checks/ehds-matrix.sh`).
