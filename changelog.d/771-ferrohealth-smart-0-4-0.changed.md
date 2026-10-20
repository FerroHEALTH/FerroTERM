- `ferrohealth-smart` 0.4.0 (#771). The sync service's
  `server_admin_credentials.oauth` table takes the keys the crate adds:
  `client_auth = "tls_client_auth"` or `"self_signed_tls_client_auth"` with
  `client_certificate_file` and `client_key_file` for RFC 8705 §2 mutual TLS
  to the token endpoint, `ca_file` for trust roots added to the built-in
  ones, and `assertion_audience = "issuer"` for a `private_key_jwt`
  assertion that names the issuer (RFC 7523 §3) in place of the token
  endpoint.
