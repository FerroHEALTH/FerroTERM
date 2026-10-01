# fhir-terminology

The FHIR terminology operations over a code system provider seam.

Implements `$lookup`, `$validate-code` (on `CodeSystem` and `ValueSet`),
`$expand`, `$subsumes`, and `$translate` in version-neutral terms over a code
system provider trait, so one engine answers every served FHIR version through
the generated [`fhir-types`](https://crates.io/crates/fhir-types) contracts.
Providers exist for SNOMED CT (RF2 through the materialized graph, store, and
index), LOINC, RxNorm, ICD-11, the ClaML classifications, FHIR `CodeSystem`
resources, and the IANA and UCUM registries; value sets and concept maps are
read from FHIR resources. This is the engine of the FerroTERM server.

## Where it sits

`fhir-terminology` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/fhir-terminology>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
