- `ferroterm-build` loads ORPHAcodes (`--orpha`, the Orphadata nomenclature
  files, one per language), the DICOM Controlled Terminology (`--dicom`,
  NEMA's `dcm.owl`) and EDQM Standard Terms (`--edqm`, saved Standard Terms
  API responses) from the release you download. NEMA's PS3.16 context group
  value sets load as supplied resources and expand over the loaded DCM
  codes. FerroTERM ships none of these releases.
