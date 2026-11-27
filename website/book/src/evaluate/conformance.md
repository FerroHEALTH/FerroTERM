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
work. The suite's other modes (ICD-11, LOINC) need licensed content and run by
hand; their lists live beside the general ones.

## Where the remaining cases stand

The `general` mode counts 805 cases on every surface. With suite `9c2bbce`
and validator 7.0.6, `/r4` passes 742, `/r4b` 728 and `/r5` 742. Every case
that does not pass falls into one of the rows below. None of them needs
licensed content: the cases that do (SNOMED CT, LOINC, ICD-11, OMOP) sit in
the suite's other modes, which run by hand and are not in these counts.

| Why the case does not pass | Tracked in | R4 | R4B | R5 |
|---|---|---|---|---|
| **Not run on this surface.** The case is gated to another FHIR version or to the `tx.fhir.org` mode, and the runner still counts it in the 805. | [#1361](https://github.com/FerroHEALTH/FerroTERM/issues/1361), [#1380](https://github.com/FerroHEALTH/FerroTERM/issues/1380) | 5 | 9 | 9 |
| **The R4B surface.** The suite targets R4 and R5: on `/r4b` its `$version$` stands for R4's `4.0.1` (the `metadata`, `exclude` and two `contained-nested` cases), and its `!4.0` gate lets two R5-only reverse `$translate` cases run against R4B, which keeps `reverse`. | [#1361](https://github.com/FerroHEALTH/FerroTERM/issues/1361), [#1380](https://github.com/FerroHEALTH/FerroTERM/issues/1380) | 0 | 11 | 0 |
| **No published definition.** The `$lookup` cases expect a `source` part on a designation, and `ec-validate-outside` expects no `version`, which no publication defines. | [#1376](https://github.com/FerroHEALTH/FerroTERM/issues/1376) | 10 | 10 | 10 |
| The cases send `reset` or `dispose` to `$closure`, or `useSupplement` to `CodeSystem/$validate-code`, which no OperationDefinition declares; nine more `closure` cases fail because the closure the first one sets up is never made. | [#1068](https://github.com/FerroHEALTH/FerroTERM/issues/1068) | 21 | 21 | 21 |
| The cases expect the `$lookup` output `warning`, which no OperationDefinition declares. | [#1069](https://github.com/FerroHEALTH/FerroTERM/issues/1069) | 2 | 2 | 2 |
| An `is-not-a` filter over a code system with no hierarchy is refused. | [#1377](https://github.com/FerroHEALTH/FerroTERM/issues/1377) | 2 | 2 | 2 |
| The cases expect concept properties (`label`, `order`) and extension placements no published definition allows. | [#1107](https://github.com/FerroHEALTH/FerroTERM/issues/1107) | 9 | 9 | 9 |
| The cases turn on `versionsMatch`, an expansion parameter no publication defines. | [#507](https://github.com/FerroHEALTH/FerroTERM/issues/507) | 8 | 8 | 8 |
| The cases expect R5's `$lookup` `definition` output, or send R5's `$translate` `targetcode`, on R4 and R4B, whose OperationDefinitions declare neither. | [#1330](https://github.com/FerroHEALTH/FerroTERM/issues/1330) | 3 | 3 | 0 |
| The cases send an input the operation does not declare (`codingX` in a batch, `targetcode` on R4), and FerroTERM refuses an undeclared input. | [#349](https://github.com/FerroHEALTH/FerroTERM/issues/349) | 3 | 2 | 2 |
| **Total not passing** | | 63 | 77 | 63 |

The first two rows do not apply on their surface. Each badge counts them
as `n/a` beside the pass count, so R4 reads `742 / 805 (5 n/a)`: 58 of its
cases are FerroTERM's to answer for, 54 on R5 and 57 on R4B. The cases and
their reasons are listed in `conformance/tx-ecosystem/not-applicable.tsv`,
which every CI run of the suite checks against the cases the runner skipped.

The rows marked as having no published definition are upstream reports:
FerroTERM answers what the published OperationDefinitions and extensions
define, and each report names the change upstream that would settle it.
Every row is a gate, an upstream report, or FerroTERM work its issue
tracks (the flat `is-not-a` row).

## The modes run by hand

The suite's other modes need licensed content or a code system FerroTERM does
not load, so they run by hand before a release, not in CI. Each has its own
badge, which reads `passed/ran, by hand <day>`: `ran` is the number of cases
that run executed and `<day>` the day it was measured, both recorded in
`conformance/tx-ecosystem/hand-run.tsv`. A badge that names a day is a record
of that run, not a live result.

![tx-ecosystem snomed R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-snomed-r4b.json)
![tx-ecosystem icd-11 R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-icd-11-r4b.json)
![tx-ecosystem icf R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-icf-r4b.json)
![tx-ecosystem loinc R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-loinc-r4b.json)
![tx-ecosystem loinc R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-loinc-r5.json)
![tx-ecosystem tx.fhir.org R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-tx-fhir-org-r4b.json)
![tx-ecosystem tx.fhir.org R5](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-tx-fhir-org-r5.json)
![tx-ecosystem mimetypes R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-mimetypes-r4b.json)
![tx-ecosystem omop R4B](https://img.shields.io/endpoint?url=https%3A%2F%2Fferroterm.eu%2Fconformance%2Fhand-omop-r4b.json)

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

- **MyHealth@EU MVC 10.1.0 (CI) import** counts the value sets of a synthetic
  package shaped like the Master Value Sets Catalogue that the server answers
  after `ferroterm-build --fhir-package` imports it
  (`conformance/ehds/mvc-synthetic.tsv`, held to the fixture by a server
  test). The real catalogue is the operator's to load.
- **EEHRxF draft 2026-11-23 Art 6 code systems** counts the coding systems of
  the draft EEHRxF implementing act whose loader a test proves over a
  synthetic fixture, of the 16 the draft names
  (`conformance/ehds/art6-code-systems.tsv`, each row naming its test,
  checked by `scripts/checks/ehds-matrix.sh`).
