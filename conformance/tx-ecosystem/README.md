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
fails when a run disagrees with it, and `scripts/checks/conformance-badges.sh`
turns the lists and the total into the README's badges at site build time.

## What is committed, per mode

Recorded on 2026-09-08 with validator 6.10.4 and suite
`eaec771d82fba4eac596c14963546f39b4ecffe7` (tests v1.9.3). Every mode run also
carries the two mode-independent `metadata` cases, so a mode's count is two
above the suite's own. The `snomed`, `icd-11`, `tx.fhir.org`, `mimetypes`, and
`omop` rows were last measured on 2026-09-06.

| mode | surface | pass list | passed of ran | needs | open failures |
|---|---|---|---|---|---|
| `general` | `/r4b` | `passing.txt` | 628 of 670 | nothing | #353 |
| `general` | `/r4` | `passing-r4.txt` | 635 of 670 | nothing | #353 |
| `general` | `/r5` | `passing-r5.txt` | 637 of 670 | nothing | #353 |
| `snomed` | `/r4b` | `passing-snomed.txt` | 2 of 170 | a SNOMED CT edition | #344, #352, #349 |
| `icd-11` | `/r4b` | `passing-icd-11.txt` | 44 of 52 | the three ICD-11 artifacts | #350, #349, #117 |
| `tx.fhir.org` | `/r4b` | `passing-tx.fhir.org.txt` | 57 of 227 | a LOINC release | #420, #421, #305, #349 |
| `tx.fhir.org` | `/r5` | `passing-r5-tx.fhir.org.txt` | 59 of 227 | a LOINC release | #420, #421, #305, #349 |
| `mimetypes` | `/r4b` | `passing-mimetypes.txt` | 35 of 37 | nothing | #353 |
| `omop` | `/r4b` | `passing-omop.txt` | 1 of 28 | an OMOP vocabulary the server does not load | #345 |

`general` is the only mode CI runs, on all three surfaces. The others need
licensed content or a code system the server does not serve, so they are run by
hand before a release and their lists are refreshed in the same change.

## What each mode needs to run

- **`general`** needs nothing to run: the runner supplies every code system and
  value set the cases declare in their own setup. Four of them reach past that
  setup for terminology the FHIR specification defines
  (<https://hl7.org/fhir/R4B/terminologies-systems.html>), which the server does
  not hold yet (#435).
- **`snomed`** needs a SNOMED CT artifact
  (`--mode snomed --index <edition>`). It scores 2 of 170 whatever edition you
  point it at, because every case pins the reference server's own edition
  (`http://snomed.info/xsct/31000003106/version/20250909`), which no release
  centre distributes. #344 holds the evidence. The second case that passes,
  `snomed/subsumes-bad-version`, passes because it asserts the refusal of an
  edition the server does not have, which is the state the other 168 are stuck
  in. The suite's runner takes five parameters (`-output`, `-externals`,
  `-filter`, `-modes`, `-api-key`) and none of them points the cases at another
  edition, so there is no override to reach for (the suite's own `tests/readme.md`). Its 30 `$subsumes` cases reach
  the server now; 21 answer 404 on that pinned version and the other 9 differ
  on the code or the text of the outcome. Five of its cases also send a
  parameter no `OperationDefinition` declares (#349).
- **`icd-11`** needs the three ICD-11 artifacts built from the WHO ICD-API
  local deployment (`--mode icd-11 --index <mms>:<icf>:<entity>`); the
  artifacts need the WHO container and its licence. Four of its cases send
  `version` on `ValueSet/$validate-code`, which declares no such input, and
  answer 400; they are the whole gap between the committed 44 and 48 (#349).
  Of the remaining four, `metadata` is the cross-mode `/r4b` case below and
  `lookup-mms-fr` and `cs-validate-lang` ask for the French display of `1A00`
  in release `2026-01`. The WHO ICD-API local deployment publishes no French
  bundle for that release: `include=2026-01_fr` aborts the container with
  "Couldn't find the version", and `2025-01_fr` is the nearest bundle that
  carries French. Both cases need a French artifact, so they stay unpassed
  while the suite pins 2026-01 (#350). `expand-adhoc-enum` is the fourth: its
  value set fixes `Shigellosis` as the display of `1A02` in
  `compose.include.concept.display`, which every FHIR version defines as "the
  text to display to the user for this concept in the context of this
  valueset", and the case expects the expansion to carry the code system's
  title instead. The compose display stands and the case stays unpassed
  (#350).
- **`tx.fhir.org`** needs a LOINC artifact (`--mode tx.fhir.org --index
  <loinc>`), since the release needs a LOINC account. The suite's own
  `tests/readme.md` says this mode is "for tests that are intended to be and
  written specifically for tx.fhir.org - internal QA. There is no need for
  other servers to pass these tests", so the LOINC subset is the part this
  project chases. The section below clusters what stays open.
- **`mimetypes`** needs nothing: BCP 13 is served from the vendored IANA media
  types registry. Two cases are open: the cross-mode `/r4b` `metadata` case
  below, and `mimetype-subsumes-invalid-code`, which asks for the `issue.code`
  the next section settles (#353).
- **`omop`** needs the OMOP standardized vocabularies, which this server does
  not load and which OHDSI distributes through Athena under per-vocabulary
  licences. The mode runs and scores 1 of 28. Its setup ValueSet states no
  `ValueSet.status`; the server reads a supplied resource leniently and its
  cases now reach the missing vocabulary. #345 holds the finding.

## The `tx.fhir.org` mode, clustered by cause

171 cases stay open on `/r5` and 172 on `/r4b`, the extra being the `metadata`
case at the end of this file. Each cluster below is the whole reason its cases
fail.

- **61 cases call `ValueSet/$compare`** (the `compare` and `related2` suites)
  and answer 405. No `OperationDefinition` in `hl7.fhir.r4.core`,
  `hl7.fhir.r4b.core`, `hl7.fhir.r5.core`, or `hl7.fhir.r6.core` declares a
  `compare` operation on any resource. It is a reference-server operation, 405
  is the right answer for an operation a version does not define, and the
  cases stay unclaimed while that holds.
- **20 cases of the `tx.fhir.org` suite ask for content this run does not
  serve**: 19 SNOMED and one HGVS, against a server holding a LOINC artifact.
  Point the mode at a SNOMED edition and the 19 meet the edition wall above
  (#344).
- **14 `langcodes` cases send `system` on `CodeSystem/$validate-code`.** Every
  vendored version declares that operation's inputs as `url`, `codeSystem`,
  `code`, `version`, `display`, `coding`, `codeableConcept`, `date`,
  `abstract`, and `displayLanguage`; R5 and R6 declare `system` only as an
  output. So each answers 400 before the behaviour the case is about is
  reached, the same adjudication as #349.
- **12 `langcodes` `ValueSet/$validate-code` cases differ on the answer's
  parameters.** The server states `version`, the IANA registry's `File-Date`,
  which R5 and R6 declare as an output of `$validate-code`; the expected files
  carry no `version` and do not mark it `$optional$`, while the same suite's
  LOINC cases do. Their composed displays also differ (`English (Latin, United
  States)` here against `English (Script=Latin, Region=United States)`), and
  no specification prescribes a display for a composed tag. Both spellings are
  tx.fhir.org's own and stay unadopted.
- **5 `langcodes` `$expand` cases** want the tag families the registry makes
  finite enumerated (#420).
- **2 LOINC cases want a DISCOURAGED concept to be inactive.**
  `loinc-validate-filter-status-good` and `loinc-validate-discouraged-code`
  expect `inactive = true` and `status = DISCOURAGED`. The FHIR LOINC page
  says "Codes with Property STATUS = DEPRECATED are considered inactive for
  use in ValueSet.compose.inactive"
  (<https://hl7.org/fhir/R4B/loinc.html>, the same sentence on
  <https://terminology.hl7.org/en/LOINC.html> where R5 and R6 redirect), and
  neither page mentions DISCOURAGED at all. DEPRECATED alone is inactive here,
  so both stay unpassed.
- **5 cases send an `$expand` limit no `OperationDefinition` declares**: four
  send `_limit` (#305) and `loinc-expand-all` sends the unprefixed `limit`
  (#349). Both refusals stand.
- **The `bugs` and `UCUM` suites are the reference server's own regression
  corpora, triaged case by case on #421.** 24 `bugs` cases and 7 `UCUM` cases
  stay open on `/r4b`:
  - **13 reach for content this run does not serve**: nine SNOMED
    (`sct-ver-ex`, `sct-parse`, `sct-parse-pc`, `sct-isa`, `sct-ver`,
    `sct-msg-4`, `sct-msg-5`, `sct-display-1`, `sct-display-2`), three CPT
    (`cpt`, `cpt-0`, `cpt-100`), and `ndc`. CPT is licensed by the AMA and NDC
    is an FDA registry; neither is served here.
  - **6 are ISO 3166**, and one of them was a defect that is now fixed. FHIR
    selects all three code forms from `urn:iso:std:iso:3166` with a `code`
    regex per form (`ValueSet/iso3166-1-2`, `-1-3`, `-1-N`), so an alpha-3 or
    a numeric country code the specification binds has to validate; only the
    alpha-2 form was served, and `NLD` and `528` were refused. All three forms
    are codes now. The six still fail on figures that are the reference
    server's data rather than the specification's requirement:
    `country-codes` wants a total of 789 (263 territories in three forms)
    against 906 here (302 in three forms, CLDR 48's set including the
    user-assigned ranges), and `3166-a` and `3166-c` want the version `2018`,
    the ISO edition year, where this server states `48`, the version of the
    CLDR data it actually holds. Naming a version this server does not hold
    would be a claim about content it cannot serve.
  - **`lang-case` and `lang-case2`** are the BCP 47 `$validate-code` parameter
    roster adjudicated for the `langcodes` suite above; same cause.
  - **`undefined`, `no-system`, `country-code` and `provenance`** differ on
    the parameter roster of a `$validate-code` answer. `provenance` passes on
    `/r5`.
  - **7 `UCUM` cases** want three designations and an `abstract` output where
    this server answers two designations and none (`lookup`,
    `lookup-with-annotation`), a canonical form `$expand` and `$validate-code`
    do not compute (`expand-ucum-canonical`, `validate-ucum-canonical-bad`),
    and two enumerations of a grammar-defined system (`expand-ucum-all-4`,
    `-5`). `ucum-subsumes-invalid-code` was the `issue.code` split and now
    passes on both surfaces.
- **The remaining LOINC cases** are the subset this project chases, carried
  from #13 and #245: the filters `$expand` still refuses, the `$lookup`
  designation shapes, and the `$validate-code` message texts.

Its 12 `langcodes` `$subsumes` cases pass from #348 on: the
`urn:ietf:bcp:47` provider reads each tag as the extended language range of
the same spelling and matches it under RFC 4647 §3.3.2, whose own example has
"de-\*-DE" and "its synonym `de-DE`" matching `de-Latn-DE` while `de-x-DE`
fails on the singleton. A grandfathered tag is one opaque subtag, because RFC
5646 §2.2.8 says such a tag "in its entirety, represents a language or
collection of languages", so `zh` does not subsume `zh-min-nan`.

## Known failures that cut across every mode

- **`$subsumes` reaches the server from validator 6.10.4 on.** 6.10.3 threw
  `Unknown Operation subsumes` before it connected, so the suite's 98
  `$subsumes` cases across four modes were scored against a request nobody
  sent. 6.10.4 issues the operation, and 62 of the 98 pass: 24 of the 27
  `general` cases on each surface, 12 of 13 in `mimetypes`, 26 of 28 in
  `tx.fhir.org`, none of the 30 in `snomed`. Of the 36 that stay open, 30 are
  the `snomed` edition wall above and 6 are the `issue.code` split below.
  #346.
- **An unknown code answers `not-found`, and the suite asks two operations for
  two different codes.** 11 cases fail on `OperationOutcome.issue.code` alone,
  each already carrying the `tx-issue-type` coding `invalid-code` the case
  asks for: 3 in `general` on each surface, 1 in `mimetypes`, 2 in
  `tx.fhir.org`, and 5 counted inside the `snomed` 30 above. The suite
  expects `code-invalid` from `$subsumes` and `not-found` from `$lookup` for
  the same fact, a code that is not in the code system: compare
  `simple/simple-subsumes-unknown-code-response-parameters.json` with
  `icd-11/lookup-bad-code-response.json`, both at the pinned commit. No FHIR
  version's `CodeSystem/$subsumes` `OperationDefinition` fixes the value, and
  R4B's `IssueType` admits both readings ("the code or system could not be
  understood" against "the reference provided was not found"), so the server
  keeps one answer for one fact and passes the `$lookup` shape.

  The ecosystem's own guidance closes it. Of the two fields an issue carries,
  the IG binds only one: "any issue entries in the OperationOutcome SHALL have
  a severity, type, expression, details.coding, and details.text. The coding
  SHALL be taken from the `http://hl7.org/fhir/tools/CodeSystem/tx-issue-type`
  system", and then, of the other, "the correct value for issue.type and
  issue.details.coding may be found in the text cases. At least with regard to
  issue.type, the correct code is sometimes unclear, and more than one type is
  accepted"
  (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>,
  `$validate-code` return parameters). These 11 answers carry the
  `tx-issue-type` coding `invalid-code` the cases ask for, with the message id
  and the text, so they meet the requirement the IG states; only the field it
  says more than one value is accepted for differs. The refusal to answer one
  fact with two issue codes stands, and the 11 stay unpassed. Upstream report
  for the owner to post: the runner compares `issue.code` exactly, which is
  stricter than the IG's own rule, and the two families disagree on the same
  fact.

  The server does spell the same fact two ways, and the suite asks for both:
  a refused operation answers `not-found`, and one code itemised inside a
  `$validate-code` answer is `code-invalid`. Compare
  `icd-11/lookup-bad-code-response.json` with
  `validation/simple-codeableconcept-bad-code-response-parameters.json`, both
  passing. The two surfaces are separate on purpose.
- **Nine cases send a parameter the operation does not declare.** Seven send
  `version` on `ValueSet/$validate-code`, which declares `systemVersion` and
  `valueSetVersion` as its inputs and `version` only as an output; one sends
  `limit` on `$expand`, declared in the specification only on
  `Observation/$stats`; one sends `allowMaximumSizeExpansion`, declared
  nowhere. The IG's own requirements page asks for none of the three and
  requires `count` for `$expand` instead. So each answers 400 before the
  behaviour the case is about is reached, the same as `_limit` (#305) and
  `mode`/`valueSetMode` (#289). #349 holds the adjudication.
- **The `metadata` case fails on `/r4b`.** It asserts
  `CapabilityStatement.fhirVersion` is `4.0.1`; `/r4b` correctly reports
  `4.3.0`. It is on the `/r4` and `/r5` lists and off every `/r4b` one.

## What the `general` mode still holds open

- **Nine of the 670 entries are skips, not failures.** `simple-expand-isa-o2`,
  `-c2`, and `-o2c2` declare `"mode": "tx.fhir.org"`, so the general run lists
  them and runs neither; the six `translate`/`translate2` cases declare
  `"version"`, so the three `-r4` ones run only on `/r4` and the two `r5+` ones
  only on `/r4b` and `/r5`. Each surface therefore reports its own skip set and
  no case is silently lost.
- **`$lookup` answers `definition` in the `property` group on `/r4b` and
  `/r4`, and the suite asks for a named parameter there.**
  `simple-lookup-1`, `simple-lookup-2`, and `parameters-lookup-supplement-none`
  expect a top-level `definition` output parameter on every surface. Only R5
  and R6 declare one: `hl7.fhir.r4.core` 4.0.1 and `hl7.fhir.r4b.core` 4.3.0
  declare `name`, `version`, `display`, `designation`, and `property` as the
  outputs of `CodeSystem/$lookup`, and their `property` input says the defined
  properties "are returned explicit in named parameters (when the names match),
  and the rest (except for lang.X) in the property parameter group".
  `definition` matches no R4-family output name, so it belongs in `property`
  there, and so does `abstract`, which those two versions do not declare
  either. All three pass on `/r5`, and the three stay unpassed on `/r4b` and
  `/r4` because the case reads an R5 shape onto an R4-family surface.
- **`reverse` on `$translate` is an R4-family input, and the suite refuses it
  on `/r4b`.** `translate2/translate-reverse-r5+` and
  `translate-reverse-r5+-a` run wherever the server is not R4, and the second
  wants a 4xx "reverse is not allowed in R5". `hl7.fhir.r4b.core` 4.3.0
  declares `reverse` among the inputs of `ConceptMap/$translate`, and
  `hl7.fhir.r5.core` 5.0.0 does not, so `/r4b` honours it and `/r5` refuses
  it. Both cases pass on `/r5` and stay unpassed on `/r4b`, where the case
  reads an R5 restriction onto an R4-family surface. Upstream report for the
  owner to post: gate the pair to `"version": "5.0"`, or give it an R4-family
  response that translates in reverse.
- **`versionsMatch` is a parameter no publication defines.** Eight `overload`
  expansion cases turn on whether two codes from two versions of one code
  system are one member: `expand-all-merged`, `expand-exclude`, and
  `expand-exclude-merged` want them merged, `expand-exclude-versioned` wants
  them kept apart, and `expand-all`, `expand-all-sysver`,
  `expand-all-versioned`, and `expand-exclude-enum` differ only on which of two
  entries with the same code comes first. The behaviour is carried by a
  `valueset-expansion-parameter` extension naming `versionsMatch`, which
  appears nowhere in `hl7.fhir.r4.core` 4.0.1,
  `hl7.fhir.r4b.core` 4.3.0, `hl7.fhir.r5.core` 5.0.0, or `hl7.fhir.r6.core`
  6.0.0-ballot5, and nowhere in the ecosystem IG outside these fixtures. No
  FHIR version prescribes the order of two `expansion.contains` entries
  either. The server keeps one entry per include, in include order, and the
  eight stay unpassed. #507 holds the upstream report: define `versionsMatch`,
  its default, and the member order in the IG, or drop the cases.
- **Four `exclude` cases compare against an unsubstituted placeholder on
  `/r4b`.** `exclude/exclude-gender`, `exclude-gender2`, `exclude-combo`, and
  `include-combo` name `http://hl7.org/fhir/administrative-gender` and
  `http://hl7.org/fhir/ValueSet/administrative-gender`
  (<https://hl7.org/fhir/R4B/terminologies-systems.html>). #435 shipped those
  systems, so all four pass on `/r4` and `/r5`. On `/r4b` each compares
  `expansion.parameter[i].valueUri` against the literal `$version$`: the
  runner substitutes that placeholder on the two surfaces whose FHIR version
  it knows and leaves it as written on `/r4b`, where the server correctly
  answers `4.3.0` for a FHIR core code system. Same family as the `metadata`
  case above.
- **A supplied `tx-resource` is read leniently and refuses only the request
  that resolves it.** Cardinality is an aspect of validating a resource, which
  a server performs at its discretion, and an implementation "should be
  conservative in its sending behavior, and liberal in its receiving behavior"
  (<https://hl7.org/fhir/R4B/validation.html>). A required primitive that
  states no value reads as the value-less element it would be with only an
  extension, so `other/dual-filter` and the two `validation-dual-filter` cases
  pass over a value set with no `status`. A resource the server still cannot
  use is recorded, so `errors/unknown-system1`, `unknown-system2`,
  `combination-ok`, and `combination-bad` are no longer refused for a broken
  filter they never touch. The three `errors/broken-filter*` cases do resolve
  it and answer `invalid` with the `vs-invalid` classification, the
  `IssueType` reading of "content invalid against the specification"
  (`structure` stays for a body the server cannot parse at all).
