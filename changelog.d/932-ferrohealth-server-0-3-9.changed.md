- The server, the sync service and the console take `ferrohealth-server`
  0.3.9. A `[server.tls]` table that sets `client_ca_file` while
  `client_auth` is `none`, the default, is now refused at start: before, the
  CA was read and ignored, so a deployment that meant mutual TLS served
  without it. Set `client_auth = "required"` (or `"optional"`) beside the
  file, or remove it.
