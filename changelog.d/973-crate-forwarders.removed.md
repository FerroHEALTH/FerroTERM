- The crate line 0.1.131 drops the functions that only forwarded to another
  call: `Memberships::new`, `RefsetMembers::new`, `IndexBuilder::new`,
  `Interned::new`, `Registry::new`, `VersionedStore::new`, `Holdings::new`
  and `Subscription::new` (use `default()`), `fhir_terminology::artifact::reader`
  (use `concept_graph::read::buffered`), `registries::bcp47::analyze_tag`
  (use `analyze` with `subtags::REGISTRY_DATA`), `text_match::query_words`
  (use `designation_index::tokenize::prefixes`), and
  `terminology_syndication::download::to_file_unverified`: `download::to_file`
  now takes the expected digest as an `Option`.
