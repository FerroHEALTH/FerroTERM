- `ConceptMap/$translate` applies a map to a code only when the code is a
  member of the map's source scope (`sourceScope[x]` in R5, `source[x]` in R4
  and R4B), whether the map was chosen, named by `url` or reached through an
  `other-map` rule, so its `unmapped` rule no longer fires for a code outside
  that value set. A code outside every scope answers `result` false with a
  message naming the code and the maps. A source scope the server cannot
  resolve is an error naming the value set.
