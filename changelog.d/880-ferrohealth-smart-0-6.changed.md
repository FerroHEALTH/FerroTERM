- The server and the synchronisation service take `ferrohealth-smart`
  0.6.0, the release every FerroHEALTH product runs. `[auth]` accepts the
  new `mode` and `[auth.introspection]` keys, but `mode = "introspection"`
  refuses the start of either service with an error that names it until
  token introspection is served (#881); the default `mode = "jwt"` checks
  tokens exactly as before.
