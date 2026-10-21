---
paths:
  - "crates/fhir-terminology/**"
  - "app/ferroterm-server/**"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FHIR terminology service: wire conformance

The FHIR specification is the oracle for the terminology wire. Implement and
review every operation against the terminology module and the specific
`OperationDefinition` of the SERVED wire version (R4/R4B/R5/R6). The operation
contracts are stable across versions but the result-parameter sets grew (R5+
added `$expand` properties, `x-caused-by-unknown-system`, richer
`$validate-code` issues), so check the served version's own definition, never a
remembered one. Each rule below is a checkable assertion; cite the spec section
in the commit/PR for any conformance-relevant decision.

## Operations and capabilities

- **[F-OP-1]** Serve `$lookup`, `$validate-code` (on both CodeSystem and
  ValueSet), `$expand`, and `$translate`. `$subsumes` is required (SNOMED-on-FHIR
  expects it).
- **[F-OP-2]** Serve `GET {root}/metadata` (CapabilityStatement) and
  `GET {root}/metadata?mode=terminology` (TerminologyCapabilities) enumerating
  every supported code system + version and the implicit-value-set capability,
  a SHALL in the terminology ecosystem IG.
- **[F-OP-3]** Accept both GET (query params) and POST (`Parameters` resource);
  support type-level (`ValueSet/$expand?url=…`) and instance-level
  (`ValueSet/{id}/$expand`) forms.

## Versioning

- **[F-VER-1]** `(system, version)` identifies a code-system instance. Absent
  `version`, resolve to a configured default and **echo the resolved version in
  every response**. Unstated or wrong version is the most common terminology bug.
- **[F-VER-2]** Support `$expand` version pinning: `system-version` (default),
  `check-system-version` (verify), `force-system-version` (override embedded).

## Expansion

- **[F-EXP-1]** Honour `count` and `offset`; populate `expansion.total` when
  computable (for SNOMED implicit sets it is, include it); echo
  `expansion.offset`. Ordering is **deterministic** across calls so paging is
  stable; the order is by system, version, then the provider's concept order
  (the ordinal the build assigns from sorted codes), never an incidental
  iteration order, and a page is cut from the selection bitmaps before any
  concept is read.
- **[F-EXP-2]** Echo every effective parameter in `expansion.parameter`,
  including assumed defaults and the resolved code-system version(s) used.
- **[F-EXP-3]** Honour `filter` (text search over designations), `activeOnly`
  (default **false**), `includeDesignations`, `designation`, `displayLanguage`,
  `property`, and `excludeNested`.

## Validation and errors

- **[F-VAL-1]** `$validate-code` returns `result` (boolean) + a human `message` +
  a structured `OperationOutcome`. Validate the submitted `display` against the
  valid designations for the language, and return the correct `display` when the
  submitted one is wrong (a warning, never a silent pass).
- **[F-VAL-2]** Every `OperationOutcome` issue carries `severity`, `code`,
  `expression`, `details.coding` (from
  `http://hl7.org/fhir/tools/CodeSystem/tx-issue-type`, e.g. `not-found`,
  `invalid-code`, `invalid-display`, `this-code-not-in-vs`, `vs-invalid`), and
  `details.text`.
- **[F-VAL-3]** Distinguish "the code is wrong" from "I cannot check this":
  unknown system → the `x-caused-by-unknown-system` parameter (do not hard-fail);
  unsupported target → `not-supported`; any client-input error (bad ECL, unknown
  `fhir_vs`, malformed version URI) → an `OperationOutcome`, **never a 500 or a
  stack trace**.
- **[F-VAL-4]** Inactive is not invalid. `$validate-code` of an inactive-but-valid
  code returns `result = true` with a warning; expansions mark
  `contains.inactive = true` with an inactivation reason; never 404 an inactive
  code.

## Language

- **[F-LANG-1]** Honour BOTH the `displayLanguage` parameter and the
  `Accept-Language` HTTP header (SHALL). Return `designation.language` (BCP-47),
  `designation.use` (the term-type coding), and `designation.value`. Fallback is
  deterministic and the returned language is stated.

## Resources

- **[F-RES-1]** A server-backed SNOMED `CodeSystem` has `content = not-present`
  (never `complete`); every CodeSystem/ValueSet/ConceptMap carries `url`,
  `version`, and `status`.

## Cross-version

- **[F-XV-1]** Serve R4/R4B/R5/R6 each with its own `OperationDefinition`
  parameter shapes (generated per version, see `codegen.md`); test the full
  matrix. `$validate-code` issue structure and `$expand` result parameters
  differ materially in R5/R6.

## Discipline

- **[F-DIS-1]** One evaluation engine for subsumption/ECL/refset across implicit
  value sets, `compose.filter`, and `$subsumes`; divergent code paths are where
  conformance rots.
- **[F-DIS-2]** Acceptance is the HL7 `fhir-tx-ecosystem-ig` test cases run per
  wire version (the FHIR Validator `txTests` mode); a green run is the merge gate
  for terminology behaviour. Reference servers (Snowstorm, Ontoserver,
  tx.fhir.org) are BEHAVIOURAL oracles for spec-silent edge cases only, never
  spec authorities. See `testing.md`.

## Sources

- FHIR terminology module: <https://hl7.org/fhir/R4/terminology-module.html> ·
  <https://www.hl7.org/fhir/terminology-module.html> (R5)
- Using codes: <https://hl7.org/fhir/R4/terminologies.html> · terminology
  service: <https://hl7.org/fhir/R4/terminology-service.html>
- Operations: `$lookup` <https://hl7.org/fhir/R4/codesystem-operation-lookup.html>,
  `$validate-code` <https://hl7.org/fhir/R4/codesystem-operation-validate-code.html>,
  `$subsumes` <https://hl7.org/fhir/R4/codesystem-operation-subsumes.html>,
  `$expand` <https://hl7.org/fhir/R4/valueset-operation-expand.html>,
  `$translate` <https://hl7.org/fhir/R4/conceptmap-operation-translate.html>
- Terminology Ecosystem IG (server requirements + test cases):
  <https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/> ·
  <https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>
