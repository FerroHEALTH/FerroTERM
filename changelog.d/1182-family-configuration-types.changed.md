- The server, the synchronisation service and the console read every span of
  time as a duration and every amount of data as a size (ferrohealth-config
  0.3): a whole number and a UCUM unit (`"500ms"`, `"30s"`, `"10min"`,
  `"12h"`, `"7d"`) or an IEC binary unit (`"1MiB"`). A key that holds one no
  longer names its unit, and in 0.4.4 alone the old key refuses the start,
  all of them at once, each naming its replacement. Upgrade step: in `[server]`, write
  `request_timeout`, `header_read_timeout`, `idle_timeout`, `body_limit`,
  `drain_delay`, `shutdown_timeout`, `retry_after`, `compression_min_size`
  and `[server.tls] reload_interval` for the keys ending in `_ms`, `_s` or
  `_bytes`; on each `[[auth.issuer]]`, `clock_skew`, `jwks_max_age`,
  `jwks_refresh_interval`, `timeout` and `introspection.cache_max_age`; in
  `[auth.basic]`, `cache_ttl`; on `[server_admin_credentials.oauth]`,
  `refresh_margin` and `timeout`; in the console, `[ferroterm] timeout`,
  `[oidc] timeout` and `[session] sign_in_timeout`, `idle_timeout` and
  `absolute_timeout`. The `FERROTERM__…`, `FERROTERM_SYNC__…` and
  `FERROTERM_CONSOLE__…` variables follow the keys.
- `[schedule] every` of the synchronisation service is a duration of the
  same form, so `"24h"` and `"90min"` read as before, while the ISO 8601
  `"PT24H"`, a compound such as `"1h 30m"` and `m` for minutes refuse the
  start. Upgrade step: write one whole number and one unit, `min` for
  minutes.
- Every outbound TLS table takes the same keys (`trust_roots_file`,
  `client_identity_file` or `client_identity`, `min_version`), read once at
  start through ferrohealth-server: `[auth.tls]` replaces `auth.ca_file`,
  `[telemetry.otlp.tls]` replaces `telemetry.otlp.trust_roots_file` and
  `client_identity_file`, a client credentials grant's `tls` table replaces
  its `ca_file`, `client_certificate_file` and `client_key_file`, and the
  console's `[oidc.tls]` replaces `oidc.ca_file` and `[oidc.tls]
  client_certificate`, `client_key` and `client_key_file`. In 0.4.4 alone,
  each old key refuses the start naming its replacement. Upgrade step: move each file
  under the table's `tls` key, and join a separate certificate and key into
  one PEM for `client_identity_file`.
