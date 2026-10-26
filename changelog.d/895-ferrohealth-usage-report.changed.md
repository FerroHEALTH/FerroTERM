- The usage report runs on `ferrohealth-usage-report`, the reporter every
  FerroHEALTH product sends with; the `[usage_report]` keys and the report's
  contents stay as they were. The boot line says "no clinical content,
  identifiers, URLs or request paths", the family's wording.
- Breaking: the daily report's `counters` carry only what counted something
  in the window, where the four per-version counters were always sent, and
  add `responses_4xx`, the `4xx` answers under a FHIR base. Upgrade: read a
  missing version counter as zero.
- Breaking: the request log and the `http_route` label of
  `http_server_request_duration_seconds` name a request under a FHIR base that
  no route matched `/r4/{*unmatched}`, `/r4b/{*unmatched}`, `/r5/{*unmatched}`
  or `/r6/{*unmatched}`, where it was `<unmatched>`; any other unmatched
  request stays `<unmatched>`. Upgrade: match those four routes where a query
  or alert matched `<unmatched>` under a FHIR base.
