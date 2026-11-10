- The named refusals of the configuration keys 0.4.5 retired. The console's
  `[ferroterm] base_url`, `timeout`, `sync_url` and `admin_url`, the sync
  service's `server_admin_url`, `[server_admin_credentials]`,
  `fhir_base_url`, `webhook_url` and `[nts] client_id`, `client_secret` and
  `client_secret_file`, and `base_url` and `issuer` in a `[[source]]` block's
  `config` are now unknown keys, which the strict load refuses like any other
  key it does not read. A deployment that moved to the endpoint tables in
  0.4.5 changes nothing.
