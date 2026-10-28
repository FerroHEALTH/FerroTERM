- The server, the sync service and the console take `ferrohealth-smart`
  0.8.6 and `ferrohealth-config` 0.2.6. Under `[auth] mode =
  "introspection"`, `[auth] ca_file` is now trusted for the issuer's
  metadata, the introspection call and the token call, as it already was
  in JWT mode, so an issuer behind a private certificate authority is
  reached with the same setting.
