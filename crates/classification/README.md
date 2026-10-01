# classification

Statistical classifications read into one model.

Reads Classification Markup Language (ISO 13120: WHO ICD-10, the national
ICD-10 translations, ICPC-2), the NCHS ICD-10-CM tabular release, and the WHO
ATC/DDD index into one `Classification` model: classes of a declared kind, each
with one parent, labelled and annotated by rubric kind. The crate ships no
classification content.

## Where it sits

`classification` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/classification>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
