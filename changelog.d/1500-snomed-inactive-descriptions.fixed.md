- An expansion of SNOMED CT, LOINC, RxNorm or a classification leaves the
  concept's inactive descriptions out of its designations, and `$lookup`
  lists them marked `withdrawn`, as the terminology ecosystem IG requires of
  an inactive designation; `$validate-code` given one as the display warns
  that it is no longer a correct display and stays valid. Before, an inactive
  description came back as a plain designation (#1500).
