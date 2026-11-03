- The console signs operators in and keeps their sessions through
  `ferrohealth-session` 0.1.0, the FerroHEALTH consoles' shared sign-in, in
  place of its own code. The sign-in routes move: `GET /auth/login?back=` is
  `GET /login?return=`, `POST /auth/sign-out` is `POST /logout`, and the
  session watch `/auth/session` is `/session`; the sign-in page stays at
  `/auth/sign-in` and the redirect URI registered at the provider stays
  `/auth/callback`. `[oidc]` may leave out `authorization_endpoint`,
  `token_endpoint`, `jwks_uri`, `end_session_endpoint` and
  `revocation_endpoint`, which the console then reads from the provider's
  metadata when it starts (`config check` reads none and works offline), and
  gains `ca_file` and `timeout_s`; `[session]` gains `store` (`memory`, the
  default, or `sealed` with `key` or `key_file`). `session.absolute_timeout_s`
  below `session.idle_timeout_s` is now refused. Upgrade: change any link or
  bookmark to the old sign-in, sign-out or session paths; the configuration
  keys a deployment already sets keep their meaning.
