# rxnorm-rrf

RxNorm Rich Release Format reading.

Locates the files of an unpacked RxNorm release (`RXNCONSO.RRF`, `RXNREL.RRF`,
`RXNSAT.RRF`, and `RXNSTY.RRF`) and reads them streaming into typed rows by the
documented column positions ([the RxNorm technical documentation](https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html)).
The crate ships no RxNorm content; a deployment brings its own release under
the UMLS licence.

## Where it sits

`rxnorm-rrf` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/rxnorm-rrf>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
