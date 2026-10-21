---
paths: ["crates/**", "app/**", "tools/**", "scripts/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Spec adherence (the FHIR + SNOMED specs are the oracle)

The conformance authority for this project is the published specifications:
the HL7 FHIR specification (per version) and the SNOMED CT / ECL
specifications, not Snowstorm, not Hermes, not memory, not intuition. The
FHIR type system and every operation reach this repository through the
generated `fhir-types` crate, which the FerroBRIDGE repository emits from the
pinned machine-readable HL7 packages (`codegen.md`); the normative text lives
at the URLs cited below.

## Hard rules

- **Before implementing or changing any spec-facing behaviour** (a
  terminology operation, ECL evaluation, RF2 semantics, subsumption,
  value-set expansion, FHIR serialization), **read the governing spec section
  first**. Route by surface:
  - **FHIR terminology operations**, the operation page for the target
    version: `$lookup`, `$validate-code`, `$subsumes`, `$expand`, `$translate`
    on the `CodeSystem` / `ValueSet` / `ConceptMap` resource
    (<http://hl7.org/fhir/R4/terminology-service.html>,
    <http://hl7.org/fhir/R5/terminology-module.html>, and the per-version
    `OperationDefinition` the generated crate carries). **The parameter set a
    version admits is exactly what its `OperationDefinition` declares, plus
    the terminology ecosystem overlay:** R5's `$expand`
    `useSupplement`/`property`/`displayLanguage` appear where the spec has
    them and are absent where it does not. The overlay (the owner's decision
    on #154, 2026-09-03) adds, on every version, the parameters the ecosystem
    requires (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>): the
    R6 ones pre-adopted from the R6 package, and the ecosystem-only ones the
    generator declares, each marked with its source in the generated
    descriptor. The overlay extends a version's definition; it never
    contradicts it.
  - **ECL:** the SNOMED Expression Constraint Language specification
    (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>)
    and its published ANTLR grammar
    (<https://github.com/IHTSDO/snomed-expression-constraint-language>). The
    grammar defines what parses; the specification defines what it means.
  - **SNOMED CT:** the release-file specification (RF2 formats, the
    transitive-closure file), the concept model, and the URI/identifier
    specifications (<https://docs.snomed.org/>).
- **NEVER LAX. Strictness is a hard rule.** The server accepts EXACTLY what
  the governing spec admits: nothing more, nothing less.
  - Everything the spec REFUSES, we refuse, and every refusal is an ASSERTED
    NEGATIVE TEST pinning its error outcome (an unknown code, an unknown
    system, an ECL parse error, a parameter the version does not define), so a
    silently loosened reader is a failing build, never quiet drift.
  - A version that does NOT define a parameter refuses it (or ignores it
    exactly as that version's `OperationDefinition` dictates), and never absorbs
    a later version's parameter silently. The one sanctioned extension is the
    ecosystem overlay above, applied by the generator and marked by source; an
    overlaid parameter whose semantics are not implemented yet is refused with
    `not-supported`, never accepted and ignored.
  - A spec-SILENT form is accepted only with a first-hand citation, recorded
    on a tracker issue; stalled or contradictory upstream material (a draft
    ballot, a reference-server quirk) is never carried silently.
  - Weakening any existing refusal requires a spec-grounded adjudication
    recorded on an issue, with the flipped test updated to assert the new
    expected outcome. Inventing a prohibition the spec does not contain is the
    same defect class as leniency: strict means exact, in both directions.
- **Cite the source.** Conformance-relevant decisions name the spec (the FHIR
  operation page + section, the ECL grammar rule, the SNOMED doc + section) in
  the commit/PR description. A deliberate deviation or gap gets a `// NOTE:`
  with the spec reference and the reason.
- **Cite ONLY durable references, never an internal markdown file as a design
  authority.** In code, doc comments, and findings, justify behaviour by
  citing the FHIR / SNOMED / ECL specification or official external
  documentation (the Rust book/reference, the docs.rs/crates.io page of a
  pinned crate). Internal plan/design markdown is deleted in the PR that
  implements it and is never a citable authority; the durable record is the
  closed issues, PR descriptions, `CHANGELOG.md`, git history, and the living
  reference docs (`docs/architecture.md`). Where the specs are SILENT
  (storage mechanics, the index/artifact format, the CSR/roaring layout,
  infra), flag it explicitly: "no FHIR/SNOMED spec governs this: our own
  design".
- **Snowstorm and Hermes are prior art, never a substitute for the spec.**
  They are the correctness reference for CHECKING answers over the same
  edition (`testing.md`), but if a reference server and the spec text
  disagree, the spec wins and the divergence is worth a note. Never resolve a
  spec question from a reference server's observed behaviour alone.
- Subagents doing spec-facing work must be handed the relevant spec
  URLs/sections (and the vendored package path) in their prompt, and reviewers
  verify claims against them.

## Multi-version discipline

FerroTERM serves R4, R4B, R5, and R6 from one server. The correct behaviour for
each version is what THAT version's spec and package define: a single code
path may back several versions only where the versions genuinely agree, and
every per-version difference is driven by the generated `fhir-types` model
(`codegen.md`), never by a hand-maintained per-version conditional that can
drift from the package. R6 is ballot-tracking; a behaviour grounded only in
the ballot is marked as such and re-verified when R6 publishes.
