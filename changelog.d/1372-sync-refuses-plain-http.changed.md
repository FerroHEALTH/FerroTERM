- `ferroterm-sync` refuses to start on a plain `http` URL outside loopback for
  every endpoint it sends a credential to or takes content from: a source's
  `feed_url` and `fhir_api_url`, `[nts]`, the issuer and token endpoint of each
  client credentials grant, `[server_admin]` and `[webhook]`. The refusal names
  the key. The compose overlay now runs the sync in the server's network
  namespace and binds the server's admin listener to `127.0.0.1:8081`. To
  upgrade, point `[server_admin] url` and `[fhir] url` at `127.0.0.1` with the
  new overlay, or give a server on another host `[server.tls]` and the sync
  `[server_admin.tls]`.
