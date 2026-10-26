- The server, the synchronisation service and the console serve the
  FerroHEALTH health routes from `ferrohealth-health` 0.1.1 and
  `ferrohealth-server` 0.3.0: `GET /health/liveness` answers `200` while the
  process runs, and `GET /health/readiness` answers the readiness document
  (state, phase and each indicator's state), `200` while ready and `503`
  otherwise. On the sync service both stay open without a credential, as
  `GET /health` does. Point a Kubernetes liveness probe at
  `/health/liveness` and its readiness probe at `/health/readiness`.
