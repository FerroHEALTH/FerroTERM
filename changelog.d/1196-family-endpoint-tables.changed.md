- Every outbound call of the console and the synchronisation service goes
  through the family endpoint table of ferrohealth-smart 0.16: `url`,
  `timeout`, `connect_timeout`, `retry`, `tls` and, for the sync service,
  `credentials`. The client follows no redirect, and a call that did not
  connect, timed out or met a `5xx` is tried again as `retry` allows; the
  console sends a create once. In 0.4.5 alone each earlier key refuses the
  start naming its replacement. Upgrade step for the console: write
  `[ferroterm.server] url` and `timeout` for `[ferroterm] base_url` and
  `timeout`, `[ferroterm.sync] url` for `sync_url` and `[ferroterm.admin]
  url` for `admin_url` (`FERROTERM_CONSOLE__FERROTERM__SERVER__URL` and so
  on); `public_url` stays, and an endpoint table takes no `credentials`.
  Upgrade step for the sync service: write `[server_admin] url` for
  `server_admin_url`, `[server_admin.credentials.oauth]` for
  `[server_admin_credentials.oauth]`, `[fhir] url` for `fhir_base_url` and
  `[webhook] url` for `webhook_url` (`FERROTERM_SYNC__SERVER_ADMIN__URL` and
  so on). A `[fhir]` URL whose last segment names no served release now
  refuses the start, and the revalidation sends `$validate-code` and
  `$expand` as `POST`.
- The server, the synchronisation service and the console take the family
  round of 2026-11-09: ferrohealth-config 0.4.1, ferrohealth-telemetry 0.4,
  ferrohealth-server 0.8.1, ferrohealth-usage-report 0.4,
  ferrohealth-smart 0.16.3, ferrohealth-session 0.10, ferrobridge-term 0.3
  and fhir-types 0.1.130, with the performance gate on ferrohealth-perf
  0.8.
