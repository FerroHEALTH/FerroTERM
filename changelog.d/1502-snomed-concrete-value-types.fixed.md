- SNOMED CT `$lookup` types a concrete number by its attribute's MRCM range,
  as the terminology ecosystem IG requires: an `int(...)` attribute answers
  `valueInteger` and a `dec(...)` one `valueDecimal`, and with no range a
  number without a decimal point is an integer. Before, every number was a
  decimal (#1502).
