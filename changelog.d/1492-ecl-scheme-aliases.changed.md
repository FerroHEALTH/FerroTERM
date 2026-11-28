- An ECL alternate identifier's scheme alias (`LOINC` in `LOINC#14394-1`)
  resolves through the new `[terminology] ecl_scheme_aliases` table, which
  maps each alias to its identifier scheme concept and ships with
  `{ LOINC = 30051010000102 }`, the LOINC Extension's scheme. A scheme
  concept's term is no longer an alias; an alias with no entry is refused as
  before (#1492).
