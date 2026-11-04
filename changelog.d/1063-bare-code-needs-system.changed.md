- `ValueSet/$validate-code` refuses a bare `code` with no `system` and no
  `inferSystem` as a request error, as the operation requires; it inferred
  the system from a value set with one system.
