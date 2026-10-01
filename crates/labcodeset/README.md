# labcodeset

The Nederlandse Labcodeset publication.

Reads the Labcodeset publication Nictiz distributes (the `labconcepts-*.xml`
document of a release) into a typed model: the laboratory concepts with their
LOINC axes and Dutch translations, the SNOMED CT materials and outcome lists,
the UCUM units, and the ordinal outcome value sets. The crate ships no
Labcodeset content; a deployment brings its own licensed publication.

## Where it sits

`labcodeset` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/labcodeset>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, UCUM, the Labcodeset) is licensed by its publisher and is never
part of this crate.
