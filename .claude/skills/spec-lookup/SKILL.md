---
name: spec-lookup
description: Look up the authoritative FHIR, SNOMED CT, or ECL requirement for any spec-facing behaviour, in the correct oracle precedence. Use before implementing or reviewing terminology behaviour.
allowed-tools: Read, Grep, Glob, WebFetch
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Spec lookup

Answer a "what does the spec say" question about the FHIR terminology wire,
SNOMED CT, ECL, or RF2, and cite the source (file + section). Never resolve a
spec-facing question from memory or from a reference server's behaviour alone.

## Oracle precedence (highest first)

1. **The FHIR normative specification for the SERVED wire version:** the
   terminology module and the specific `OperationDefinition` (R4/R4B/R5/R6).
   Check the served version's own definition, not a remembered one.
2. **SNOMED CT specifications:** the URI Standard and the ECL specification
   (docs.snomed.org).
3. **The SNOMED-on-FHIR Implementation Guide** (snomedct.html + the IHTSDO IG)
   for how SNOMED binds to the FHIR wire (implicit value sets/concept maps).
4. **The HL7 `fhir-tx-ecosystem-ig`** requirements + test cases (the server
   conformance policy).

Reference servers (**Snowstorm**, Ontoserver, tx.fhir.org) are BEHAVIOURAL
oracles for spec-silent edge cases only. Cite them explicitly as such and record
the decision; never treat them as spec authority.

## Where to look

- **The machine-readable contract** (`StructureDefinition`,
  `OperationDefinition`) reaches this repository through the generated
  `fhir-types` crate; the packages it is emitted from are vendored in the
  FerroBRIDGE repository. Read the served version's operation page for the
  authoritative parameter set.
- **The project rules** encode the distilled requirements with citations:
  `.claude/rules/fhir-terminology.md` (wire conformance) and
  `.claude/rules/snomed-terminology.md` (SNOMED URI / ECL / RF2). Start there,
  then confirm against the primary source below.
- **Primary sources (fetch to confirm):**
  - FHIR terminology module: <https://hl7.org/fhir/R4/terminology-module.html>
    · <https://www.hl7.org/fhir/terminology-module.html> (R5)
  - Operations: `$lookup`, `$validate-code`, `$subsumes`, `$expand`, `$translate`
    under `https://hl7.org/fhir/R4/…-operation-….html` (and the served version).
  - FHIR SNOMED CT page: <https://hl7.org/fhir/R4/snomedct.html>
  - SNOMED URI Standard:
    <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>
  - ECL 2.2:
    <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>
    · grammar <https://github.com/IHTSDO/snomed-expression-constraint-language>
  - RF2:
    <https://docs.snomed.org/snomed-ct-specifications/release-file-specification>
  - Terminology Ecosystem IG:
    <https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>

## How to answer

State the requirement, quote the decisive sentence, and cite file + section. If
the sources are silent, say so explicitly and name the reference-server
behaviour you would match; flag it as a spec-silent deviation to record, never
a spec fact.
