- `fhir-terminology` 0.1.134 keeps the types the provider seam hands across
  under `provider`: `filter` moves to `provider::filter`, the
  `ValueSet.compose` model (`Compose`, `Include`, `ConceptRef`, `SystemRef`,
  `Side`, `FilterAt`) to `provider::compose`, `fhir_codesystem::model` to
  `provider::code_system` (with `defined_property_uri`),
  `conceptmap::model` to `provider::concept_map`, and `artifact::Source` to
  `provider::source`. `valueset::negotiation` moves to `compose::negotiation`,
  the `$expand` outcome types (`ExpansionOutcome`, `Contains`,
  `ExpansionParameter`, `ExpansionProperty`, `ParameterValue`) to
  `valueset::expansion`, and `language::check` returns its own
  `language::InvalidLanguage`. The `LoadError` of the resource loaders carries
  one `Model` variant, naming the resource type, for all three resources.
  No top-level module of the crate imports one that imports it.
