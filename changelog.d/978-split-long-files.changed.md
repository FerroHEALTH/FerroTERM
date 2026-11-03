- `fhir-terminology` 0.1.136 keeps what a provider declares about its code
  system in `provider::declaration`: `ContentMode`, `HierarchyMeaning`,
  `PropertyKind`, `PropertyDefinition`, `FilterDefinition`, `Capability`,
  `Compositional`, `Declaration`, `ImplicitForm`, and `ImplicitArgument` move
  there from `provider`. No other public path changes; the crate and tool
  files over 1000 lines are split by concern into child modules.
