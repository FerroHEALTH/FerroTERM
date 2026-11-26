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
and validator 7.0.6, `/r4` passes 713, `/r4b` 699 and `/r5` 713. Every case
that does not pass falls into one of the rows below. None of them needs
licensed content: the cases that do (SNOMED CT, LOINC, ICD-11, OMOP) sit in
the suite's other modes, which run by hand and are not in these counts.

| Why the case does not pass | Tracked in | R4 | R4B | R5 |
|---|---|---|---|---|
| **Not run on this surface.** The case is gated to another FHIR version or to the `tx.fhir.org` mode, and the runner still counts it in the 805. | | 5 | 9 | 9 |
| **The R4B surface.** The suite targets R4 and R5: on `/r4b` its `$version$` stands for R4's `4.0.1` (the `metadata`, `exclude` and two `contained-nested` cases), and its `!4.0` gate lets two R5-only reverse `$translate` cases run against R4B, which keeps `reverse`. | | 0 | 11 | 0 |
| **No published definition.** The cases rest on extensions no released package defines (`extension-codes`, `global-lang-pack`). | [#1067](https://github.com/FerroHEALTH/FerroTERM/issues/1067) | 42 | 42 | 42 |
| The cases send `reset` or `dispose` to `$closure`, or `useSupplement` to `CodeSystem/$validate-code`, which no OperationDefinition declares; nine more `closure` cases fail because the closure the first one sets up is never made. | [#1068](https://github.com/FerroHEALTH/FerroTERM/issues/1068) | 15 | 15 | 15 |
| The cases expect `$lookup` outputs `warning` and `used-supplement`, which no OperationDefinition declares. | [#1069](https://github.com/FerroHEALTH/FerroTERM/issues/1069) | 4 | 4 | 4 |
| The cases expect concept properties (`label`, `order`) and extension placements no published definition allows. | [#1107](https://github.com/FerroHEALTH/FerroTERM/issues/1107) | 9 | 9 | 9 |
| The cases turn on `versionsMatch`, an expansion parameter no publication defines. | [#507](https://github.com/FerroHEALTH/FerroTERM/issues/507) | 8 | 8 | 8 |
| The cases expect R5's `$lookup` `definition` output, or send R5's `$translate` `targetcode`, on R4 and R4B, whose OperationDefinitions declare neither. | [#1330](https://github.com/FerroHEALTH/FerroTERM/issues/1330) | 3 | 3 | 0 |
| The cases send an input the operation does not declare (`codingX` in a batch, `targetcode` on R4), and FerroTERM refuses an undeclared input. | [#349](https://github.com/FerroHEALTH/FerroTERM/issues/349) | 3 | 2 | 2 |
| **Waiting on the FHIR model.** `$translate` checks each target code and answers `error`; the output enters through the `fhir-types` ecosystem overlay, which no release carries yet. | [#1308](https://github.com/FerroHEALTH/FerroTERM/issues/1308) | 3 | 3 | 3 |
| **Total not passing** | | 92 | 106 | 92 |

The rows marked as having no published definition are upstream reports:
FerroTERM answers what the published OperationDefinitions and extensions
define, and each report names the change upstream that would settle it.
No case of the general mode is left as a gap in FerroTERM itself: the
`$translate` cases wait on the FHIR model's next release, and every other
row is a gate or an upstream report.

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
