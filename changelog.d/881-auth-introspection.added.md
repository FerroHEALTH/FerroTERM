- `[auth] mode = "introspection"` checks an opaque access token at the
  issuer's introspection endpoint (RFC 7662), on the server's write routes
  and admin listener and on the synchronisation service's admin listener.
  The endpoint is the one the issuer's metadata names, or
  `[auth.introspection] endpoint`; the call authenticates with
  `[auth.introspection.client]` or a `[auth.introspection.bearer]` client
  credentials token. An inactive token answers `401`, an endpoint that
  cannot answer `503`, and an issuer that names no endpoint refuses the
  start. The discovery document and the capability statement name the
  introspecting issuer's endpoints.
