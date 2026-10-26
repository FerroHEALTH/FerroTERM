- The server's `[server]` section takes the keys of the FerroHEALTH HTTP
  stack, for the FHIR listener and the admin listener alike:
  `request_timeout_ms`, `header_read_timeout_ms`, `body_limit_bytes`,
  `drain_delay_ms`, `shutdown_timeout_ms`, `max_concurrent_requests` with
  `retry_after_s` (past the bound a request is answered `503` with
  `Retry-After`, and the health routes are never refused), `trusted_proxies`
  with `forwarded_header`, `compression` with `compression_min_bytes`, and
  `base_path`, a path every route is served under. A `[server.tls]` table
  (`certificate_file`, `key_file`, `min_version`, `client_auth`,
  `client_ca_file`, `reload_interval_s`, `healthcheck_identity_file`) serves
  both listeners over TLS, reads the files again on `SIGHUP`, and
  `ferroterm healthcheck` then probes over TLS, accepting only the server's
  own certificate. `[server.caller_rate]` is refused, since the server limits
  no caller's rate.
- `ferroterm serve` and `ferroterm-sync serve` name the default command, as
  `ferroterm-console serve` does.
