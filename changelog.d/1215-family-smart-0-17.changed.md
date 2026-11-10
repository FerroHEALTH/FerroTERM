- A TLS refusal of an outbound endpoint table names the key under its own
  `tls` table, `server_admin.tls.trust_roots_file` where 0.4.4 named
  `server_admin.trust_roots_file` (ferrohealth-smart 0.17). Every OAuth and
  OpenID Connect endpoint URL a table names, such as the console's
  `[oidc] token_endpoint` and the sync service's
  `[server_admin.credentials.oauth] token_endpoint`, renders with any
  userinfo and query masked in `config check` and the logs. Upgrade step:
  none.
