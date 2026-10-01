# gstandaard

The G-Standaard (Z-Index) product ladder.

Reads the fixed-length G-Standaard files (`BST711T` generic products,
`BST052T` prescription products, `BST031T` trade products, `BST004T` articles,
with `BST020T` names and `BST902T` thesauri) into four flat classifications, one
per rung of the product ladder, following the
[Z-Index file descriptions](https://www.z-index.nl/documentatie/bestandsbeschrijvingen).
The crate ships no G-Standaard content; a deployment brings its own licensed
release.

## Where it sits

`gstandaard` is one crate of [FerroTERM](https://github.com/FerroHEALTH/FerroTERM),
a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other clinical
code systems. The crates are published so other projects can reuse them; the
API is pre-1.0 and moves with the FerroTERM release train. Documentation:
<https://docs.rs/gstandaard>.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical terminology content (SNOMED
CT, LOINC, RxNorm, ICD, the Dutch national code systems) is licensed by its
publisher and is never part of this crate.
