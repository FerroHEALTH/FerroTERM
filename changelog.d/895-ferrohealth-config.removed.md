- Breaking: the server's flat variables are gone with no alias. Each is now a
  key of the configuration tree: `FERROTERM_LISTEN` is `server.listen`
  (`FERROTERM__SERVER__LISTEN`), `FERROTERM_ADMIN_LISTEN` is `admin.listen`,
  `FERROTERM_INDEX`, `FERROTERM_CODESYSTEMS`, `FERROTERM_RESOURCES` and
  `FERROTERM_DEFAULT_LANGUAGE` are `terminology.index`,
  `terminology.code_systems`, `terminology.resources` and
  `terminology.default_language`, `FERROTERM_BASE_URL` and
  `FERROTERM_SECURITY_SERVICE` are `fhir.base_url` and
  `fhir.security_services`, `FERROTERM_LOG_FORMAT` is `telemetry.format`, and
  `FERROTERM_USAGE_REPORT`, `FERROTERM_USAGE_REPORT_ENDPOINT`,
  `FERROTERM_STATE_DIR` and `FERROTERM_DEPLOYMENT` are
  `usage_report.enabled`, `usage_report.endpoint`, `usage_report.state_dir`
  and `usage_report.deployment`. `RUST_LOG` still sets the filter. A list is a
  TOML array in the file or comma-separated in a variable, where
  `FERROTERM_INDEX` and `FERROTERM_CODESYSTEMS` took the platform's path
  separator, and `usage_report.enabled` takes `true` or `false`, where
  `FERROTERM_USAGE_REPORT` took `on` or `off`. Upgrade: rename each variable,
  or move the values into a file.
- Breaking: `FERROTERM_AUTH_FILE` and its `FERROTERM_AUTH__<KEY>` overrides are
  gone. The `[auth]` table is a section of the server's configuration file, and
  one key is set as `FERROTERM__AUTH__<KEY>`. Upgrade: move the table into the
  file `FERROTERM_CONFIG` names, and rename each `FERROTERM_AUTH__` variable to
  `FERROTERM__AUTH__`.
- Breaking: the NTS add-on reads its account from the synchronisation
  service's `[nts]` section. `FERROTERM_NTS_USERNAME`, `FERROTERM_NTS_PASSWORD`,
  `FERROTERM_NTS_CLIENT_ID` and `FERROTERM_NTS_CLIENT_SECRET` are
  `FERROTERM_SYNC__NTS__USERNAME` and the other three, the JSON credential
  file and the `credentials` key of the `[source.config]` block are gone, and
  each secret takes a mounted file through `nts.password_file` or
  `nts.client_secret_file`. Upgrade: rename the variables, and point the
  `_file` key at a file that holds the secret alone.
- Breaking: `ferroterm-sync` reads no file unless `--config` or
  `FERROTERM_SYNC_CONFIG` names one, where it read `/etc/ferroterm/sync.toml`
  by default; the container image sets `FERROTERM_SYNC_CONFIG` to that path.
