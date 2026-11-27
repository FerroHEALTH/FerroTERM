<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The HL7 terminology ecosystem suite

The suite (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/>) groups its
cases into modes, and a run picks one. This directory holds one committed pass
list per mode and served surface: the tests FerroTERM passes, one name per
line, sorted. CI runs the suite with the FHIR Validator's `txTests` command
(`scripts/checks/tx-ecosystem.sh`, the validator and the suite commit pinned
there) and fails when a listed test stops passing. A test that starts passing
is printed by the script; add it to the list in the same change that earns it.
`total.txt` records the number of cases the `general` mode runs; the script
fails when a run disagrees with it. `hand-run.tsv` records, for each mode run
by hand, the cases the run ran and the day it was measured; the script fails a
hand run whose count differs from its row. `scripts/checks/conformance-badges.sh`
turns the lists, the total and `hand-run.tsv` into the README's badges at site
build time, each hand-run badge naming its day, and fails when the table below
disagrees with them.

`not-applicable.tsv` lists, per surface, the `general` cases the runner
counts in that total and that do not apply there, each with its reason.
`gated` rows are the cases whose own `mode` or `version` gate makes the
runner skip them. `r4b-surface` rows are the cases that fail on `/r4b` only
because the suite targets R4 and R5 (the IG's `r4.md`) and leaves `$version$`
unsubstituted there. The script fails a `general` run whose skips differ
from the `gated` rows, or in which an `r4b-surface` case passes or is
missing. Each badge shows the count as `n/a` beside the pass count, so the
cases a reader sees as not passing are the ones FerroTERM answers for
(the upstream report is #1380).

## What is committed, per mode

The `general` rows were recorded on 2026-11-24 with validator 7.0.6 and suite
`9c2bbce57366319d6a5265cbfbf6697f094822e3` (test cases 1.11.0). Every
mode run also carries the two mode-independent `metadata` cases and the 11
`closure` cases, so a mode's count is 13 above the suite's own. The other
rows were measured by hand on 2026-11-27 on the same pins, over the SNOMED CT
International edition 20261101, LOINC 2.83, and the three ICD-11 artifacts
built from the WHO ICD-API local deployment of release 2026-01 in English;
the `icf` row over the ICF `CodeSystem` resources of `fhir.tx.support` 1.0.5.

| mode | surface | pass list | passed of ran | needs | open failures |
|---|---|---|---|---|---|
| `general` | `/r4b` | `passing.txt` | 730 of 805 | nothing | #1068, #1069, #1376, #1107, #507, #349, #1330 (the breakdown on the book's conformance page) |
| `general` | `/r4` | `passing-r4.txt` | 744 of 805 | nothing | #1068, #1069, #1376, #1107, #507, #349, #1330 (the breakdown on the book's conformance page) |
| `general` | `/r5` | `passing-r5.txt` | 744 of 805 | nothing | #1068, #1069, #1376, #1107, #507, #349, #1330 (the breakdown on the book's conformance page) |
| `snomed` | `/r4b` | `passing-snomed.txt` | 2 of 378 | a SNOMED CT edition | #344, #1068, #1380 |
| `icd-11` | `/r4b` | `passing-icd-11.txt` | 10 of 145 | the three ICD-11 artifacts | #1391, #1068, #1380 |
| `icf` | `/r4b` | `passing-icf.txt` | 79 of 96 | the standalone ICF releases | #1330, #1376, #1068, #1380 |
| `loinc` | `/r4b` | `passing-loinc.txt` | 56 of 110 | a LOINC release | #1383, #1068, #1380 |
| `loinc` | `/r5` | `passing-r5-loinc.txt` | 57 of 110 | a LOINC release | #1383, #1068 |
| `tx.fhir.org` | `/r4b` | `passing-tx.fhir.org.txt` | 29 of 189 | a LOINC release | #1384, #1068, #1380 |
| `tx.fhir.org` | `/r5` | `passing-r5-tx.fhir.org.txt` | 31 of 189 | a LOINC release | #1384, #1068 |
| `mimetypes` | `/r4b` | `passing-mimetypes.txt` | 40 of 52 | nothing | #1068, #1380 |
| `omop` | `/r4b` | `passing-omop.txt` | 1 of 40 | an OMOP vocabulary the server does not load | #345, #1364, #1068, #1380 |

`general` is the only mode CI runs, on all three surfaces. The others need
licensed content or a code system the server does not serve, so they are run by
hand before a release and their lists are refreshed in the same change.

## What each mode needs to run

- **`general`** needs nothing to run: the runner supplies every code system and
  value set the cases declare in their own setup. Four of them reach past that
  setup for terminology the FHIR specification defines
  (<https://hl7.org/fhir/R4B/terminologies-systems.html>), which the server
  serves itself.
- **`snomed`** needs a SNOMED CT artifact (`--mode snomed --index
  <edition>`). It scores 2 of 378 whatever edition you point it at, because
  the cases pin the reference server's own edition
  (`http://snomed.info/xsct/31000003106/version/20250909`, 991 times in the
  suite at `9c2bbce`), which no release centre distributes, and the runner
  has no parameter that points them at another edition (the suite's own
  `tests/readme.md`). 216 of its cases answer 404 on that version. #344
  holds the evidence. The two cases that pass assert the refusal of an
  edition the server does not have.
- **`icd-11`** needs the three ICD-11 artifacts built from the WHO ICD-API
  local deployment (`--mode icd-11 --index <mms>,<icf>,<entity>`), which
  needs the WHO container and its licence. Test cases 1.11.0 rewrite it (132
  cases) against the IG's own `icd-11.md`. The 44 cases the 1.9.3 suite
  passed now expect `post-coordination-metadata` and the `classificationCode`
  output, which #1391 builds.
- **`icf`** (70 cases, and `icf-ec` with 13) needs the standalone ICF releases
  of `icf.md`, `http://hl7.org/fhir/sid/icf` `2001a` and `2017a`, in a
  directory `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` names. They come as
  `CodeSystem` resources in the `fhir.tx.support` package, under WHO's terms,
  and FerroTERM ships none of them. Every `icf` case passes. Of `icf-ec`, the
  three `$lookup` cases want a top-level `definition` (#1330) or a designation
  `source` (#1376), and two send `useSupplement` on `CodeSystem/$validate-code`
  (#1068).
- **`loinc`** needs a LOINC artifact (`--mode loinc --index <loinc>`), since
  the release needs a LOINC account. Test cases 1.11.0 moved the LOINC cases
  here from `tx.fhir.org`, and every request names LOINC 2.83. #1383
  clusters what stays open by cause.
- **`tx.fhir.org`** runs over the same LOINC artifact. The suite's own
  `tests/readme.md` says this mode is "for tests that are intended to be and
  written specifically for tx.fhir.org - internal QA. There is no need for
  other servers to pass these tests". The section below clusters what stays
  open.
- **`mimetypes`** needs nothing: BCP 13 is served from the vendored IANA media
  types registry. What stays open is the cross-mode `closure` and `/r4b`
  `metadata` cases.
- **`omop`** needs the OMOP standardized vocabularies, which this server does
  not load and which OHDSI distributes through Athena under per-vocabulary
  licences. Its setup ValueSet states no `ValueSet.status` (#1364), and its
  27 cases reach the missing vocabulary (#345).
- **Every mode** also runs the 11 `closure` cases (#1068) and, on `/r4b`, the
  `metadata` case the suite writes for R4 (#1380).

## The `tx.fhir.org` mode, clustered by cause

160 cases stay open on `/r4b` and 158 on `/r5`. Beyond the 11 `closure`
cases and the `/r4b` `metadata` case, #1384 holds the clusters that go past
the IG's requirements:

- **61 cases call `ValueSet/$compare`** (the `compare` and `related2` suites).
  The IG says `$compare` is "**not** a requirement for registration in the
  ecosystem" (`requirements.md` §$compare at `9c2bbce`), and no FHIR
  version's core package declares the operation.
- **26 `langcodes` `$validate-code` cases**: 14 send `system` on
  `CodeSystem/$validate-code`, which every version declares as an output only
  (#349), and 12 forbid the `version` output that R5 declares.
- **`langcodes/expand-lang-language` and `-both`** expand as the IG requires
  and differ only on the wording of `valueset-unclosed-reason`.
- **`extension-codes-3166` (6)** want ISO 3166's user-assigned codes as a
  reserved code space of the server's ISO 3166 system.
- **`bugs`, `UCUM` and `tx.fhir.org`** are the reference server's regression
  corpora: SNOMED CT, CPT, NDC and HGVS content this run does not serve, and
  that server's own data and wording (ISO 3166 totals and versions, composed
  BCP 47 displays, UCUM canonical forms and designations).

The IG's own BCP 47 expansion rules are served: a `region` or a `script`
filter alone answers `too-costly`, and all language codes expand to the
common-languages value set, marked unclosed. `langcodes/expand-lang-script`
left the pass lists for it: the case expects `not-supported`, where the IG
says a `script` filter alone "SHALL answer `too-costly`" (`requirements.md`
at `9c2bbce`, IETF Language Codes). `expand-langcodes-all` differs on the
unversioned `used-codesystem`, the `total` and some displays. #1384 records
both.

## Known failures that cut across every mode

- **Nine cases send a parameter the operation does not declare.** Seven send
  `version` on `ValueSet/$validate-code`, which declares `systemVersion` and
  `valueSetVersion` as its inputs and `version` only as an output; one sends
  `limit` on `$expand`, declared in the specification only on
  `Observation/$stats`; one sends `allowMaximumSizeExpansion`, declared
  nowhere. The IG's own requirements page asks for none of the three and
  requires `count` for `$expand` instead. So each answers 400 before the
  behaviour the case is about is reached. #349 holds the adjudication.
- **The `metadata` case fails on `/r4b`.** It asserts
  `CapabilityStatement.fhirVersion` is `4.0.1`; `/r4b` correctly reports
  `4.3.0`. It is on the `/r4` and `/r5` lists and off every `/r4b` one, and
  `not-applicable.tsv` counts it on `/r4b` (#1380).

## What the `general` mode still holds open

The book's conformance page (`website/book/src/evaluate/conformance.md`,
"Where the remaining cases stand") breaks down every `general` case that
does not pass, per surface, and each row names its open issue or its gate.
