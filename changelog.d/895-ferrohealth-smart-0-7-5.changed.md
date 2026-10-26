- The server and the synchronisation service take `ferrohealth-smart`
  0.7.5. A secret or a masked URL in a rendered `[auth]` table now reads
  `***` in place of `[redacted]`, and a URL's query is masked beside its
  userinfo. `[auth] ca_file` names a PEM file of trust roots for an issuer
  behind a private certificate authority, and `[auth.basic]
  cache_ttl_seconds` keeps a verified Basic credential for 300 seconds by
  default, so a repeat request skips the Argon2id derivation; set it to `0`
  to verify every request.
