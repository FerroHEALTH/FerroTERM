- The crate line moves to 0.1.144, and two published crates move public
  items into child modules of the module that held them. In
  `fhir-terminology`, `ProviderError`, `InvalidValue` and
  `MalformedValueSet` move to `provider::error`, `operations::Sources` to
  `operations::sources`, and `snomed::Footprint` to `snomed::edition`. In
  `rf2`, `refset::RefsetKind` moves to `refset::kind`, and the function
  `refset::kind` is now `refset::kind::read`. Upgrade step: import each item
  from its new path. No answer on the wire changes.
