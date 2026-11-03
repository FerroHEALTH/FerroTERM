- `$lookup` leaves out a requested `property` that neither the FHIR
  specification nor the code system defines, with no issue, where it
  answered 400 `invalid`. A property the code system defines and the server
  does not generate still answers `not-supported`.
