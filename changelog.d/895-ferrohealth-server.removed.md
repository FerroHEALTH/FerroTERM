- Breaking: `ferroterm healthcheck --url` and `ferroterm-sync healthcheck
  --url` are gone with no alias. The probe follows the configured listen
  address, on the loopback when the process listens on every interface.
  Upgrade: set `server.listen` (or the sync service's `listen`) to the
  address to probe, or probe the URL with an HTTP client of your own.
