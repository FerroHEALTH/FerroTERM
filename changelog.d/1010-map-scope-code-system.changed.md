- The console's concept map editor refuses to save a map whose source or
  target scope names a code system the map's groups map between. FHIR
  defines both scopes as value sets (`ConceptMap.source[x]` and
  `target[x]` in R4 and R4B, `sourceScope[x]` and `targetScope[x]` in R5),
  and `$translate` cannot read such a map's scope.
