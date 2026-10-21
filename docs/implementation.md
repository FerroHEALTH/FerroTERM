<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Implementation checklist

FerroTERM implements the whole FHIR terminology surface for SNOMED CT, LOINC, and
the other clinical code systems in `docs/terminologies.md`, SNOMED CT first.
This is the master list of what the server must do. Check items off as they land. There is
no fixed scope cut and no "version 1" boundary: what a given release contains is
decided during development, and every item here is in scope until it is built.

Conformance is verified, not asserted: the HL7 `fhir-tx-ecosystem-ig` test cases
(run through the FHIR Validator's `txTests` mode) are the wire check, and
differential testing against Snowstorm and Hermes over a licensed edition is the
behavioural check (`.claude/rules/testing.md`). An item counts as done when its
tests and the relevant conformance cases pass.

Implementation order starts with **R4B**, then R5, R4, and R6. That is a build
sequence, not a scope limit; the server serves every version.

## FHIR terminology operations

- [x] `CodeSystem/$lookup`: display, designations, properties, the SNOMED
  edition+version in `version`
- [x] `CodeSystem/$validate-code`: code validity, display validation, correct
  display returned when the submitted one is wrong
- [x] `CodeSystem/$subsumes`: subsumed / subsumes / equivalent / not-subsumed
- [ ] `ValueSet/$expand`: `filter`, `count`, `offset`, `expansion.total`,
  `expansion.parameter` echo, `includeDesignations`, `designation`,
  `displayLanguage`, `activeOnly`, `property`, `excludeNested`
- [x] `ValueSet/$validate-code`: membership, display, inactive handling
- [ ] `ConceptMap/$translate`: explicit maps and SNOMED implicit maps
- [x] `ConceptMap/$closure`: client-side transitive-closure maintenance (R4, R4B, and R5; the R6 ballot removed the operation)
- [ ] `CodeSystem/$find-matches`
- [x] `metadata` (CapabilityStatement) and `metadata?mode=terminology`
  (TerminologyCapabilities) enumerating supported systems, versions, and
  implicit-value-set capability

## Operation mechanics

- [x] GET (query params) and POST (`Parameters`) invocation for every operation
- [x] Type-level (`ValueSet/$expand?url=`) and instance-level forms
- [x] `(system, version)` identity: resolve the default version and echo the
  resolved version in every response
- [x] `$expand` version pinning: `system-version`, `check-system-version`,
  `force-system-version`
- [x] Deterministic, stable expansion ordering so paging is repeatable
- [x] `OperationOutcome` on every failure, with `severity`, `code`, `expression`,
  `details.coding` (from `tx-issue-type`), `details.text`
- [ ] Unknown-system handling (`x-caused-by-unknown-system`) distinct from
  invalid-code; `not-supported` for unsupported targets; client input errors are
  `OperationOutcome`, never a 500
- [x] Inactive concepts served and marked `inactive`, never a 404; successors
  routed through `$translate`
- [ ] `displayLanguage` parameter and `Accept-Language` header both honoured;
  `designation.language`/`.use`/`.value` returned; deterministic language
  fallback, stated

## FHIR versions (all served)

- [x] R4B (4.3.0)
- [x] R5 (5.0.0)
- [x] R4 (4.0.1)
- [x] R6 (ballot, tracked as it moves)
- [x] Per-version operation parameter sets generated from each version's
  `OperationDefinition`; one server answers every version at runtime

## Code systems (the provider seam, `docs/architecture.md` §5)

- [x] The code system provider seam: identity and versions, metadata for
  `TerminologyCapabilities`, locate, designations by language, typed
  properties, supplements, generic filters, text search, enumeration; hierarchy,
  system-specific filters, implicit value sets, and concept maps as declared
  capabilities
- [x] The compose layer (include, exclude, dedup, `offset`, `count`,
  `expansion.total`) once, above every provider
- [ ] SNOMED CT provider (RF2 loader, ECL, implicit forms; the sections below)
- [x] Generic FHIR `CodeSystem` resource provider (HL7 Terminology, custom code
  systems, supplements; `hierarchyMeaning`, `caseSensitive`, `content`,
  `versionNeeded` honoured); passes the tx-ecosystem `general` mode
- [x] LOINC provider (LoincTable, multiaxial hierarchy, parts, answer lists,
  linguistic variants; the FHIR LOINC filters and `/vs/` implicit value sets)
- [x] UCUM provider (grammar-defined validation, `canonical` filter, no
  enumeration)
- [x] ICD-10 providers (WHO ClaML, ICD-10-NL, ICD-10-CM; `classified-with`
  hierarchy)
- [ ] Further providers per `docs/terminologies.md` (RxNorm, ATC, ICD-11,
  BCP 47, BCP 13, ISO 3166, the Dutch national code systems)
- [ ] `TerminologyCapabilities` enumerates every loaded system, version, filter,
  and property

## SNOMED CT on FHIR

- [x] URI standard: `http://snomed.info/sct`, edition and version URIs, concept
  URIs; edition/version only in `version`
- [ ] Implicit value sets: `?fhir_vs`, `?fhir_vs=isa/[sctid]`,
  `?fhir_vs=ecl/[ecl]`, `?fhir_vs=refset`, `?fhir_vs=refset/[sctid]`, with an
  edition/version base
- [x] Implicit concept maps: `?fhir_cm=[sctid]`; historical-association and
  map reference sets wired into `$translate`
- [x] Refset-only packages layered onto the edition they depend on, checked against the Module Dependency Reference Set
- [ ] Multi-edition support via the Module Dependency Reference Set; default
  edition+version resolution and per-request override
- [x] Preferred-term / display selection from the language reference set and RF2
  acceptability; FSN vs synonym
- [x] `compose.filter` (`is-a`, `in`) answered by the same engine as the implicit
  forms

## ECL (Expression Constraint Language 2.2)

- [x] Parser faithful to the official ANTLR `ECL.g4`; malformed ECL is an
  `OperationOutcome(invalid)`
- [x] Operators: `<`, `<<`, `>`, `>>`, `*`, `^` (memberOf), `.` (dotted
  attribute), `:` refinement with attribute groups and cardinality, reverse `R`,
  `AND`, `OR`, `MINUS`, concrete values with comparisons
- [x] 2.1+ term/description filters (`{{ … }}`), history supplements, alternate
  identifiers
- [x] Evaluation against the inferred transitive closure, edition/version-scoped

## MRCM and post-coordination

- [ ] MRCM domain/attribute/cardinality validation
- [ ] Post-coordinated expression parsing and validation

## CodeSystem supplements

- [ ] Supplement handling and `useSupplement` beyond echoing the parameter

## RF2 ingestion (offline build)

- [ ] Snapshot load; Full for history; Delta for incremental updates; record the
  release type
- [ ] Activity from the latest `effectiveTime` per component
- [ ] Subsumption from the inferred Relationship file; transitive closure
  computed and persisted (shipped closure file used when present)
- [ ] Module Dependency Reference Set assembly; warn on unmet dependencies
- [ ] Reference-set parsing by descriptor (simple, ordered, map layouts)
- [ ] Edition/version identified from release metadata, not filenames; post-load
  verification of counts and known facts

## Engine and storage

- [ ] The generated `fhir-types` model consumed per version, at a pin the
  version guard holds to the lock
- [ ] CSR adjacency (is-a and per-attribute) and roaring transitive-closure
  bitmaps; resident at query time
- [ ] `redb` persistence of the built artifacts
- [ ] `fst` word inverted index for description search (prefix, refset/status
  filter, matched-term-length sort)
- [ ] `spawn_blocking` seam for heavy `$expand` and cold reads

## Deployment and operations

- [ ] Server and build-tool binaries per target (statically linked on the musl
  targets); container image
- [ ] Configuration surface
- [ ] Code system release loading with licence enforcement (bring-your-own,
  content never committed)
- [ ] SLSA Build L3 releases with signed SBOM (`docs/ci-cd.md`)
- [ ] External documentation (`website/book`)

## Conformance and verification

- [ ] `fhir-tx-ecosystem-ig` `txTests` pass, per served version
- [ ] Differential testing against Snowstorm and Hermes over a pinned licensed
  edition
- [ ] ECL evaluated against the official valid/invalid example corpus
