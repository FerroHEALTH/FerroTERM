- The viewer is now the FerroTERM Console, its own binary and image
  (`ghcr.io/ferrohealth/ferroterm-viewer`), rendered on the server and
  hydrated, built on the FerroHEALTH component library `ferrohealth-ui`. It
  reaches the server over the public FHIR API from its own server, adds a
  dashboard, listings and detail screens for code systems, value sets and
  concept maps, a `$lookup` screen, and the four operation screens laid out
  as forms with a result panel. Reading stays open; editing needs a sign-in
  at the OpenID Provider, which the console's server holds, revoking the
  token on sign-out where the provider publishes a revocation endpoint. Run
  it with the quickstart's `viewer` profile.
