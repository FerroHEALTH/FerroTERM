- The server checks credentials through the FerroHEALTH resource-server crate
  `ferrohealth-smart` 0.1.1, and its own issuer, key set and scope code is
  gone. Authentication is configured by an `[auth]` table, read from the TOML
  file `FERROTERM_AUTH_FILE` names with `FERROTERM_AUTH__<KEY>` overrides
  (`FERROTERM_AUTH__ISSUER`, `FERROTERM_AUTH__AUDIENCE`,
  `FERROTERM_AUTH__ENABLED`, `FERROTERM_AUTH__PRODUCT_SCOPE`, and the rest of
  the keys the configuration page lists). It is on by default and fails
  closed: a server that names `FERROTERM_RESOURCES` or
  `FERROTERM_ADMIN_LISTEN` refuses to start until `[auth]` names an issuer and
  an audience, or says `enabled = false`. A read-only server starts as before.
- An audience is required with an issuer (RFC 9068 §4); the write routes still
  accept every served version's base under `FERROTERM_BASE_URL` beside
  `[auth] audience`, and the admin listener accepts `[auth] audience` alone.
- Access tokens are checked under RFC 9068 by default: the `at+jwt` type and
  the `sub`, `client_id`, `iat` and `jti` claims are required. An issuer that
  mints plain JWTs is admitted with `[auth] token_profile = "jwt"`.
- The admin scope is a product scope in the SMART extension form,
  `[auth] product_scope`, `__ferroterm.admin` when unset; a value that is
  neither a `__` string nor a full URI refuses the start.
- Machine callers authenticate with HTTP Basic as an `[[auth.basic.users]]`
  entry (Argon2id hashes) or with a client credentials token. Several issuers
  are trusted with `[[auth.issuer]]`.
- A refusal names no claim and no scope: every invalid token gets the same
  `401` with `error="invalid_token"` and no `error_description`, a `403` names
  no missing scope, two `Authorization` headers answer `400`
  `invalid_request`, and a key set that cannot be read answers `503`.
- The key set is read again once it is older than ten minutes
  (`jwks_max_age_seconds`), so a key the issuer withdrew stops verifying.
- `scopes_supported` in the SMART discovery document lists
  `[auth] scopes_supported` and no longer republishes the issuer's list.
- `ferroterm-sync` authenticates `POST /reload` with the credential its new
  `[server_admin_credentials]` table names: HTTP Basic (`user`, `password` or
  `password_file`) or the client credentials grant under
  `[server_admin_credentials.oauth]`.
