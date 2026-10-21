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

## What the numbers are not

The suite is the ecosystem's test suite, not a certification. Passing a case
means the server answers as the suite expects; it does not make the server an
HL7-certified product, and HL7 issues no such certification for terminology
servers. Registration in the HL7 terminology ecosystem is a separate step and
will be linked from this page when it exists.
