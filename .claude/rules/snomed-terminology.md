---
paths:
  - "crates/rf2/**"
  - "crates/concept-graph/**"
  - "crates/sct-ecl/**"
  - "crates/fhir-terminology/**"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# SNOMED CT on FHIR + ECL + RF2

This file governs the SNOMED CT provider: the RF2 loader, ECL, and the SNOMED
implicit forms. The substrates and the operations are code-system-neutral
(`docs/architecture.md` §5); a rule here never leaks into them as a SNOMED
special case. Other code systems get their own rule file as their providers
land (`docs/terminologies.md`).

SNOMED International's specifications are the oracle for everything SNOMED: the
URI standard, the implicit value-set / concept-map conventions, ECL, and the RF2
release format. Cite the spec section (e.g. "snomedct.html §Implicit Value
Sets", "ECL 2.2 §Refinement", "RF2 spec §Language Reference Set") for any
conformance decision. Reference servers (Snowstorm, Ontoserver, tx.fhir.org) are
behavioural oracles for spec-silent edge cases only.

## URI grammar

- **[S-URI-1]** The code system URI is exactly `http://snomed.info/sct`, never a
  variant. Edition = `http://snomed.info/sct/[moduleSctid]`; edition+version =
  `http://snomed.info/sct/[moduleSctid]/version/[YYYYMMDD]`. The edition and
  version live ONLY in the FHIR `version` element / URI, never in `system`.
  Concept identity is `http://snomed.info/id/[sctid]` (a 303-redirect URI, not
  the terminology API).
- **[S-URI-2]** `$lookup` on a SNOMED concept returns the edition+version URI in
  its `version` property, and a `display` that is the preferred term for the
  requested/default language reference set.

## Implicit content

- **[S-IMP-1]** Implement every implicit value-set form and parse the query part
  exactly, URL-decoding the ECL/SCTID: `?fhir_vs` (all concepts),
  `?fhir_vs=isa/[sctid]` (transitive is-a, self-inclusive),
  `?fhir_vs=ecl/[ecl]` (the primary `$expand`-with-ECL path),
  `?fhir_vs=refset` (concepts that ARE reference sets), and
  `?fhir_vs=refset/[sctid]` (members of a refset). An edition/version base may
  prefix any of these. Reject an unknown `fhir_vs=` form with an
  `OperationOutcome`, never a 500.
- **[S-IMP-2]** Implement implicit concept maps `?fhir_cm=[sctid]` and wire the
  historical-association + map reference sets into `$translate`, so translating an
  inactive concept via `REPLACED BY` / `SAME AS` returns its successor.
- **[S-IMP-3]** A `ValueSet.compose.filter` with op `is-a` / `in` is answered by
  the SAME subsumption/ECL engine as the implicit forms: one evaluation path.

## ECL (Expression Constraint Language)

- **[S-ECL-1]** Target **ECL 2.2** (superset-compatible with 2.0/2.1); build the
  parser faithful to the official ANTLR `ECL.g4` for that version, never a
  hand-rolled divergent grammar. Port it in pure Rust (`logos` + `winnow`,
  preferred over the pre-1.0 `chumsky`) mirroring the grammar, and test against
  the official valid/invalid example corpus.
- **[S-ECL-2]** Support the full operator set: `<` / `<<` / `>` / `>>` / `*`,
  refinement (`:` with attribute groups, cardinality, reverse `R`, dotted `.`
  attribute values), `AND` / `OR` / `MINUS`, concrete values + comparisons,
  `memberOf` (`^`), and the 2.1+ term/description filters (`{{ … }}`), history
  supplements, and alternate identifiers.
- **[S-ECL-3]** Evaluate `<` / `<<` / `>` / `>>` against the **inferred**
  transitive closure of is-a (not the stated view), and bind every evaluation to
  the resolved `(edition, version)`: the same expression yields different sets
  across editions.
- **[S-ECL-4]** Malformed ECL → an `OperationOutcome(invalid)`, never a panic or
  500. Distinguish syntactically-invalid ECL from valid ECL naming an unknown
  SCTID.
- **[S-ECL-5]** Evaluation is set algebra over the materialized closure and
  attribute adjacency (see `docs/architecture.md`): `<<x`, `^refset`, and
  `memberOf` are index lookups, not per-request graph walks; compiled ECL ASTs
  are cached.

## RF2 release-format handling

- **[S-RF2-1]** Load from the **Snapshot** for active state; use **Full** only
  for point-in-time/historical answers; treat **Delta** as an increment onto a
  prior snapshot. Record which release type produced the loaded state; never mix
  them silently.
- **[S-RF2-2]** Component activity is the `active` field of the latest
  `effectiveTime` row per component id (Snapshot resolves this; when reading
  Full, compute the latest-effectiveTime row).
- **[S-RF2-3]** Build subsumption from the **inferred** Relationship file
  (`typeId = 116680003` is-a), never the stated file; materialize the transitive
  closure, using the shipped transitive-closure file when present, else compute
  and persist it deterministically.
- **[S-RF2-4]** Honour the Module Dependency Reference Set
  (`900000000000534007`) to assemble an edition (International core + extension
  modules at compatible versions); warn or refuse on unmet dependencies rather
  than serving a partial edition.
- **[S-RF2-5]** Display selection uses language reference sets and RF2
  acceptability: preferred term = the description whose language-refset
  `acceptabilityId` is Preferred (`900000000000548007`) for the requested
  language; Acceptable (`900000000000549004`) is fallback. Distinguish FSN
  (`typeId 900000000000003001`) from Synonym (`900000000000013009`). Verify any
  specific SCTID against the loaded edition's release notes before relying on it.
- **[S-RF2-6]** Identify the loaded edition/version from the release metadata
  (moduleId + effectiveTime), never inferred from a filename alone; expose it as
  the FHIR `version` URI.
- **[S-RF2-7]** Parse each reference set by its `refsetId`/descriptor, not by
  positional column assumptions: simple, ordered, and map refsets have distinct
  layouts.

## Testing and discipline

- **[S-TEST-1]** Differential-test `$expand` (ECL), `$subsumes`,
  `$validate-code`, and `$translate` against Snowstorm (and where feasible
  Ontoserver/tx.fhir.org) on a PINNED edition+version. A divergence is a defect
  in FerroTERM (or a recorded, spec-cited deviation), never silently accepted.
- **[S-TEST-2]** Run the official ECL valid/invalid example corpus for parse
  conformance; pin the SNOMED release + oracle versions in the test artifacts.
- **[S-TEST-3]** Never tune a test to match observed output; adjudicate against
  the FHIR + SNOMED spec text first, then fix the server (see `testing.md`).
- **[S-DIS-1]** Inactive is not invalid: serve inactive concepts, mark them, and
  route successors via associations.
- **[S-DIS-2]** SNOMED CT content is licence-gated and NEVER committed
  (`vendored-inputs.md`): fixtures are shaped/synthetic only.

## Sources

- FHIR SNOMED CT page: <https://hl7.org/fhir/R4/snomedct.html>
- SNOMED CT URI Standard:
  <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>
- SNOMED on FHIR IG:
  <https://confluence.ihtsdotools.org/display/FHIR/Implementation+Guide+for+using+SNOMED+CT+with+FHIR>
  · CI build <https://build.fhir.org/ig/IHTSDO/snomed-ig/introduction.html>
- ECL specification (2.2):
  <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>
  · grammar <https://github.com/IHTSDO/snomed-expression-constraint-language>
- RF2 release file specification:
  <https://docs.snomed.org/snomed-ct-specifications/release-file-specification>
- Snowstorm (reference oracle): <https://github.com/IHTSDO/snowstorm>
