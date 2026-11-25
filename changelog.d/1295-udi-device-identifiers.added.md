- `ferroterm-build` loads the UDI device identifiers (UDI-DI) of one issuing
  entity from a device table you export (`--udi` with `--udi-issuer` and
  `--udi-version`): the AccessGUDID `device.txt`, or an EUDAMED or issuing
  entity export as a delimited file or workbook. Each issuer is served under
  the URI FHIR names for it (`Device.udiCarrier.issuer`), so `$lookup`,
  `$validate-code` and `$expand` answer for a loaded UDI-DI. FerroTERM ships
  no UDI records.
