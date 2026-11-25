- A request that leaves out an input the operation needs (no code system,
  no code) answers `issue.code` `invalid` with the `invalid-data`
  `tx-issue-type`, and every refusal carries its sentence once, in
  `issue.details.text`, with no copy in `issue.diagnostics`. The
  `OperationError::Required` variant of `fhir-terminology` is gone; such
  a request is `OperationError::Invalid`.
