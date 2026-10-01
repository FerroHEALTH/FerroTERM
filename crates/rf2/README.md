# rf2

SNOMED CT RF2 loading and the typed component model.

Streams the files of a SNOMED CT RF2 release (concepts, descriptions,
relationships, concrete-value relationships, alternate identifiers, and every
reference set) into typed rows keyed by distinct identifier newtypes, following
the [release file specification](https://docs.snomed.org/snomed-ct-specifications/release-file-specification).
Releases arrive as directories or zip archives; reading is streamed and never
loads a file whole.

The crate ships no SNOMED CT content. A deployment brings its own licensed
release from SNOMED International or a national release centre.

## Where it sits

`rf2` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/rf2>.

## Licence

Apache License, Version 2.0 (`LICENSE`). This crate is the one the workspace
publishes under Apache 2.0 rather than its Business Source License 1.1, so any
Rust project can depend on it without a licence conversation. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
