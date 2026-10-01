# designation-index

The designation search index.

An [`fst`](https://docs.rs/fst) map over designation word tokens plus roaring
posting bitmaps per word, with language, designation-use, language reference
set, and status filters as bitmaps, sorted by term length then designation
ordinal. This is the `$expand` `filter` engine for every loaded code system.
No specification governs the index format: it is this project's own design.

## Where it sits

`designation-index` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/designation-index>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
