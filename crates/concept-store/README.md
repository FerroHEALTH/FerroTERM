# concept-store

The disk-backed concept store.

A [`redb`](https://docs.rs/redb) database holding one code system version's
concepts, displays, designations, and property values, keyed by dense ordinal,
that terminology operations read point-wise: `$lookup` and `$validate-code`
resolve a code here without touching the graph. Built offline, opened read-only
by a server. No specification governs the layout: it is this project's own
design.

## Where it sits

`concept-store` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/concept-store>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
