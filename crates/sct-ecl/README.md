# sct-ecl

The SNOMED CT Expression Constraint Language.

A `logos` lexer and a `winnow` parser faithful to the official ECL ANTLR
grammar ([the ECL specification](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language)),
a syntax tree named after the grammar's rules, a printer whose output parses
back to the same tree, and an evaluator that compiles a constraint to set
algebra over a [`concept-graph`](https://crates.io/crates/concept-graph).

## Where it sits

`sct-ecl` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/sct-ecl>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
