- The synchronisation service's admin listener requires a credential carrying
  `__ferroterm.sync` (or `[auth] product_scope`) on every route but
  `GET /health`, through `ferrohealth-smart`, and refuses to start while its
  `[auth]` table admits no credential, unless it says `enabled = false` (#764).
  The console reads the run list with the signed-in reader's access token.
