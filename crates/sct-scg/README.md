# sct-scg

The SNOMED CT Compositional Grammar: the syntax post-coordinated expressions
are written in.

A `logos` lexer and a `winnow` parser faithful to the normative ABNF of the
[Compositional Grammar specification](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-compositional-grammar-specification/design/5-syntax-specification),
a syntax tree named after the grammar's rules, and a printer whose output
parses back to the same tree. Every parse failure carries the byte offset of
the token the grammar does not admit.

The crate is syntax only. It resolves no concept identifier, checks no concept
model, and decides no subsumption; an expression it parses may still name a
concept an edition does not have.

## Where it sits

`sct-scg` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/sct-scg>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical
terminology content (SNOMED CT, LOINC, RxNorm, ICD, the Dutch national code
systems) is licensed by its publisher and is never part of this crate.
