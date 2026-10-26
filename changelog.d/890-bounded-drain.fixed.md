- The server, the synchronisation service and the console bound the drain on
  `SIGTERM` or `SIGINT`: readiness turns `503` at once, the listeners keep
  accepting for `server.drain_delay_ms`, then close, and the requests in
  flight get `server.shutdown_timeout_ms` (10 seconds by default) to finish
  before the process exits, where a connection held open by a slow client
  kept the process up until the container runtime killed it. The server's
  admin listener drains with the FHIR listener under the same bound.
