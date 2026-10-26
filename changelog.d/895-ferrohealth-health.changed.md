- Breaking: `GET /health` on the server, the synchronisation service and the
  console answers the family's summary, `{"state":"up"}`, where the server
  and the sync service answered an empty body and the console
  `{"status":"up"}`. It answers `503` with `{"state":"down"}` from the moment
  the process receives `SIGTERM` or `SIGINT` until it exits, so a load
  balancer stops sending requests while the ones in flight finish. Upgrade: a
  probe that reads only the status needs no change; one that parses the
  console's body reads `state` in place of `status`.
