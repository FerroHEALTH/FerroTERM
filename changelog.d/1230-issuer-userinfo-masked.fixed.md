- `ferroterm config check` no longer prints the password of an
  `[[auth.issuer]]` issuer written with userinfo (`https://user:password@…`):
  it renders as `https://***@…`. The server moves to `ferrohealth-smart`
  0.18, which holds every configured issuer identifier as a `SecretUrl`,
  with `ferrohealth-session` 0.12 and `ferrobridge-term` 0.5, the releases
  on the same line (#1230).
