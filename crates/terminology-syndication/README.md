# terminology-syndication

A client for the Atom syndication dialect terminology services publish.

National terminology services announce their downloadable content in an Atom
feed (RFC 4287) carrying the NCTS Atom Syndication Format extensions, which
Ontoserver documents. This crate reads that feed into a typed model, decides
what a run takes from it, fetches one content item with its digest verified,
and offers the `Source` trait as the seam each service add-on implements.

Nothing here is specific to a code system, a country, or an operator. The feed
dialect is the same wherever it is served, and a service's identity lives
entirely in its add-on: where its feed is, and how a listing and a download are
authorized.

## Where it sits

`terminology-syndication` is one crate of
[FerroTERM](https://github.com/FerroHEALTH/FerroTERM), a pure-Rust FHIR
terminology server for SNOMED CT, LOINC, and other clinical code systems. This
crate is not published on crates.io: it is consumed inside the FerroTERM
workspace by the sync service and its add-ons, and its version moves with the
workspace's crate line. The Rust module path is `terminology_syndication`;
the documentation is `cargo doc` in the workspace.

## Licence

Business Source License 1.1 (`LICENSE`): free to read, build, modify, and
redistribute, free for non-production use and for non-commercial production
use; commercial production use needs a licence from the Licensor; each version
becomes Apache License 2.0 four years after it is published. Clinical
terminology content (SNOMED CT, LOINC, RxNorm, ICD, the national code systems)
is licensed by its publisher and is never part of this crate.
