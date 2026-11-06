- The HTTP Basic form of the synchronisation service's
  `[server_admin_credentials]` (`user`, `password`, `password_file`): the
  reload is a machine's call, and the server admits a Basic user as a person
  only. Upgrade step: create a client for the service at the authorization
  server the server trusts and name it under
  `[server_admin_credentials.oauth]` (`issuer` or `token_endpoint`,
  `client_id`, `client_secret_file`, `scope = "__ferroterm.admin"`); a file
  that still sets the Basic keys fails to load, naming `oauth`.
- The flat `[auth] issuer = "…"` key, `[auth] token_profile`, the
  section-level issuer settings, and with them the variables
  `FERROTERM__AUTH__ISSUER`, `FERROTERM__AUTH__AUDIENCE` and
  `FERROTERM__AUTH__TOKEN_PROFILE`. Upgrade step: see the `[[auth.issuer]]`
  table under Changed; an issuer that mints plain JWTs takes
  `token_types = ["JWT"]` on its table.
