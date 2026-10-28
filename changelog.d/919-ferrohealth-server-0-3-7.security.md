- The server, the sync service and the console take `ferrohealth-server`
  0.3.7 in place of 0.3.4 (GHSA-2mc2-h3jq-c5wg). Every earlier release held
  a plain-HTTP connection that never sent a byte for as long as the client
  kept it open, so silent connections from a client with no credentials
  could exhaust the process's file descriptors and stop it answering, health
  checks included; the listener then retried a failed `accept` at once and
  logged every failure. Such a connection now closes once
  `[server] header_read_timeout_ms` passes (30 s by default), counted from
  accept, and after an `EMFILE` or `ENFILE` the listener pauses up to 1 s
  before it accepts again and logs one warning every 10 seconds.
