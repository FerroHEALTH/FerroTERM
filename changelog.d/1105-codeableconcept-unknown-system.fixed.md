- `ValueSet/$validate-code` answers `result` true for a `CodeableConcept`
  with a coding in the value set when another coding names a code system
  the server does not hold and the value set does not name, as the
  operation's `codeableConcept` input requires. That coding is reported as
  a warning that it was not checked (`UNKNOWN_CODESYSTEM_CODING_NOT_CHECKED`)
  and no longer as `x-unknown-system`; the answer failed before.
