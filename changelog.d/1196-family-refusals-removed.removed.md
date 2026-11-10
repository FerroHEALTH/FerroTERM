- The named refusals of the keys 0.4.4 retired in the family crates are
  gone, so each is now an unknown key that the strict load refuses: the
  `[server]` keys that named their unit (`request_timeout_ms`,
  `body_limit_bytes` and the rest), the `[auth]` and issuer keys that named
  theirs (`clock_skew_seconds`, `timeout_ms` and the rest), `auth.ca_file`,
  the console's `[session]` and `[oidc]` keys that named their unit,
  `oidc.ca_file`, and `telemetry.otlp.trust_roots_file` and
  `client_identity_file`. The earlier `[auth]` shape, a flat
  `issuer = "…"` or an issuer setting on `[auth]` itself, is refused the
  same way. Upgrade step: none for a configuration that started on 0.4.4.
