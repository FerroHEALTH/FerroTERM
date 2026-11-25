# Provenance: the FHIR core terminology bundles

Generated from the pinned HL7 FHIR core packages, never hand-edited. The
generator and those packages left this repository with the FHIR model (#300),
so nothing here regenerates the files; a refresh runs the generator that owns
the packages and lands its output as a change to this directory.

One directory per served FHIR version (`r4`, `r4b`, `r5`, `r6`), each holding
`code-systems.json` and `value-sets.json`: a JSON array with one resource per
line, in canonical order, projected onto the elements the terminology engine
reads.

## What each bundle carries

- Every `CodeSystem` the version's package defines under
  `http://hl7.org/fhir/` with `content = complete`, excluding the
  `http://hl7.org/fhir/sid/` identifiers the specification assigns to code
  systems it does not define
  (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
- Every `ValueSet` under `http://hl7.org/fhir/ValueSet/` whose compose names
  only those code systems and references only value sets in the same bundle
  (<https://hl7.org/fhir/R4B/terminologies-valuesets.html>).
- A resource that states no `status`, which both resources require (1..1,
  <https://hl7.org/fhir/R4B/codesystem.html>), is left out; the run reports
  how many. `hl7.fhir.r4b.core` 4.3.0 ships two, `CodeSystem-catalogType` and
  the value set that selects from it.
- A `CodeSystem` that defines one code twice is left out for the same reason,
  a code being unique in its code system; the run reports how many.
  `hl7.fhir.r4b.core` 4.3.0 ships one, `CodeSystem-therapy-relationship-type`,
  whose `indicated-only-before` appears twice with two definitions.

## Licence

- Source packages: `hl7.fhir.r4.core` 4.0.1, `hl7.fhir.r4b.core` 4.3.0,
  `hl7.fhir.r5.core` 5.0.0, `hl7.fhir.r6.core` 6.0.0-snapshot1, the versions
  these bundles were generated from.
- Upstream license: CC0-1.0 (the `license` field of each package's
  `package/package.json`), so the specification's own terminology is
  redistributable.
- The closure rule above is what keeps that true. Dozens of the
  specification's value sets enumerate SNOMED CT, LOINC, and other licensed
  codes inline; a value set that names any system outside this bundle is left
  out, so no bundle carries content SNOMED International, Regenstrief, or
  another owner licenses separately.
