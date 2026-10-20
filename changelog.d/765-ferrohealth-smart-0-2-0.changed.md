- `ferrohealth-smart` 0.2.0 builds the `.well-known/smart-configuration`
  document. It still writes `issuer` only where `sso-openid-connect` is
  claimed, which needs an issuer that lists the `openid` scope. It declares
  `permission-v1` and `permission-user` and never `permission-patient`. The
  capabilities follow the crate's order and are no longer sorted, and
  `token_endpoint_auth_methods_supported` is always present. A refused write
  still answers an `OperationOutcome` in the format `Accept` asks for; the
  gate's renderer now reads it.
