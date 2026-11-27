- The synchronisation overlay `compose.sync.yaml` changes how the server and
  the sync service are deployed together: the sync runs in the server's network
  namespace (`network_mode: service:ferroterm`) and the server's admin listener
  binds `127.0.0.1:8081`, so no other container reaches the admin listener or
  the sync's listener, the console included. To upgrade, take the new overlay
  and point the sync's `[server_admin] url` and `[fhir] url` at `127.0.0.1`.
- The CRA risk assessment records the integrity of a resource read from a
  terminology service's FHIR API as a residual risk: no specification defines
  an integrity field for it, and the transport is held to `https`.
