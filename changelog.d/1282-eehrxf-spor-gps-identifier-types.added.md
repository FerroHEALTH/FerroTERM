- `ferroterm-build` loads EMA SPOR master data from saved SPOR API responses
  (`--spor`: RMS terms of one list, OMS organisations, SMS substances and PMS
  products) and writes the SNOMED CT Global Patient Set freeset as a value
  set over the loaded edition (`--gps`). FerroTERM ships neither release.
- The EHDS identifier types of the EEHRxF act (Annex X: the HL7 identifier
  type with NI, HI, HIN, SS, REGPID and REGION) expand as one value set,
  `https://ferroterm.eu/fhir/ValueSet/ehds-identifier-type`, on every FHIR
  version, with FerroTERM's own code system for the four codes HL7 v2 table
  0203 lacks.
