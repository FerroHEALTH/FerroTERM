- `ferroterm config check` no longer prints the password of an issuer
  written with userinfo (`https://user:password@…`) in
  `[[auth.attributes]]`, `[[auth.federation]]`, `[[auth.party]]` or
  `auth.scopeless_subjects`: it renders as `https://***@…`, as
  `[[auth.issuer]]` already did. The server moves to `ferrohealth-smart`
  0.19, which holds the issuer of every configuration table as a
  `SecretUrl`, with `ferrohealth-session` 0.14 and `ferrobridge-term` 0.6,
  the releases on the same line (#1235).
