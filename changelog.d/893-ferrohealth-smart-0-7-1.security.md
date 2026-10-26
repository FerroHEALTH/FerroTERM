- `ferrohealth-smart` 0.7.1 masks the userinfo of an OAuth section's
  `issuer` and `token_endpoint` in every rendering, a URL that does not parse
  included; before it a credential written into such a URL could reach a
  log through the section's `Debug` or serialized form (CWE-532,
  GHSA-vphj-3f8c-5hpx). FerroTERM moves to it from 0.6.0.
