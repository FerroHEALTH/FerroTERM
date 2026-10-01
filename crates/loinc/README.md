# loinc

LOINC release loading and the typed row model.

Reads a LOINC release (the unpacked `Loinc_<version>` directory) into typed
rows: the term table, the parts and the multiaxial hierarchy, the answer lists,
and the linguistic variants, each file located by name and read by column name
([the LOINC database structure](https://loinc.org/kb/users-guide/loinc-database-structure/)).
The crate ships no LOINC content; a deployment brings its own release under the
LOINC licence.

## Where it sits

`loinc` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/loinc>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
