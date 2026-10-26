- Breaking: the server's request metrics follow the OpenTelemetry HTTP
  semantic conventions, the metric every FerroHEALTH product exports through
  `ferrohealth-telemetry`. `ferroterm_http_requests_total` and
  `ferroterm_http_request_duration_seconds` are gone with no alias; one
  histogram, `http_server_request_duration_seconds`, takes their place, with
  the labels `http_request_method`, `http_route`, `http_response_status_code`,
  `network_protocol_version`, `url_scheme` and, on a `5xx`, `error_type`, and
  the buckets the conventions advise (5 ms to 10 s) where the old ones started
  at half a millisecond. Upgrade: count requests with
  `http_server_request_duration_seconds_count`, and rename `method`, `route`
  and `status` in every query and alert.
- Breaking: `GET /metrics` on the server and on the synchronisation service
  answers in the Prometheus text format (`text/plain; version=0.0.4`), where it
  answered in OpenMetrics. The `ferroterm_code_system_loaded`,
  `ferroterm_reloads_total` and `ferroterm_sync_*` series keep their names and
  labels. Upgrade: a scraper that asked for OpenMetrics only reads the text
  format.
- Breaking: the request log carries the server's own id, a UUID it makes for
  every request, in `request_id`, and `client_named` says whether the client
  sent an `X-Request-Id`; the client's value is still echoed on the response
  and never logged, the FerroHEALTH request-id rule, where the log carried the
  client's value. The system and code a request named are fields of their own
  (`system`, `url`, `version`, `code`, `code_a`, `code_b`), where they were
  one `named` field. Upgrade: correlate a client report by its time and route,
  or by the response's `X-Request-Id` when the client sent none, and read the
  new fields in a log pipeline.
