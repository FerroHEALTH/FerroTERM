- `ferroterm-build` loads four of the code systems the EEHRxF binds from the
  release file you download from each publisher: NPU (`--npu`, the IFCC code
  file), ICD-O-3 (`--icdo`, the WHO topography and morphology tables), EMDN
  (`--emdn`, the Commission's workbook) and ISCO-08 (`--isco`, the ILO
  structure workbook). Each is served under the canonical URI HL7
  Terminology or the published FHIR `CodeSystem` names, versioned from the
  release, through `$lookup`, `$validate-code` and `$expand`. FerroTERM ships
  none of these releases.
