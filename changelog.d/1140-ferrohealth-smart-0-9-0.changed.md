- The server, the synchronisation service and the console take
  `ferrohealth-smart` 0.9.0 and `ferrohealth-session` 0.3.0. Every issuer of
  `[auth]` is an `[[auth.issuer]]` table of its own, with a required `kind`
  (`machines` for backend services, `persons` for people signing in to an
  app such as the console, which the table lists in `first_party_clients`),
  an optional `profile` (`rfc9068` by default, or `keycloak`, `entra`,
  `okta`) and the issuer's `audience`, `algorithms`, `mode`,
  `[auth.issuer.introspection]`, `role_claims` and key set settings. Every
  token is classified by its issuer's kind, and one that matches no caller of
  that kind answers `401`. Upgrade step: move `issuer`, `audience` and every
  other issuer setting from `[auth]` onto an `[[auth.issuer]]` table with its
  `kind`, in the configuration file, since a table cannot be a variable;
  write each `scopeless_subjects` entry as `{ issuer = "…", sub = "…" }`. The
  old shape refuses the start, naming the key and the table it moves to, and
  an issuer without `kind` refuses it naming `auth.issuer.kind`.
- A `[[auth.basic.users]]` entry is a person, on the server and on the
  synchronisation service's admin listener: a user granted a `system/` scope
  refuses the start, naming the user, and a machine calls with a client
  credentials token (RFC 6749 §4.4) of a `machines` or `persons` issuer.
  Upgrade step: give each machine that used Basic a client at the
  authorization server, and keep Basic for people.
- `compose.sync.yaml` mounts the server's configuration file
  (`FERROTERM_SERVER_CONFIG`, `./ferroterm.toml` by default), which holds its
  `[[auth.issuer]]` table or `[auth] enabled = false`, and no longer passes
  `FERROTERM__AUTH__ISSUER` and `FERROTERM__AUTH__AUDIENCE` through.
