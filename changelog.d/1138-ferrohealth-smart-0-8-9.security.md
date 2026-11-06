- `[auth] scopeless_subjects` and `role_claims` hold per issuer
  (GHSA-pfgx-xq28-2629, CWE-863, CWE-639). Before, beside several
  `[[auth.issuer]]` entries both lists applied to every trusted issuer's
  tokens: a token from any of them whose `sub` was listed was granted every
  write without a scope. Through `ferrohealth-smart` 0.9.0 a
  `scopeless_subjects` entry is a table `{ issuer = "…", sub = "…" }` that
  admits a `System` caller of that issuer alone, an entry naming an issuer
  the section does not trust refuses the start, and `role_claims` is a key of
  each `[[auth.issuer]]` table, read from that issuer's tokens only. Upgrade
  step: write each `scopeless_subjects` entry as `{ issuer, sub }`, and set
  `role_claims` on the issuer tables that need other claims than the default
  (the server grants nothing by role).
