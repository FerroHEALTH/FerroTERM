- `[auth] scopeless_subjects` and `role_claims` hold per issuer, through
  `ferrohealth-smart` 0.8.9 (GHSA-pfgx-xq28-2629, CWE-863, CWE-639). Before
  it, beside several `[[auth.issuer]]` entries both lists applied to every
  trusted issuer's tokens: a token from any of them whose `sub` was listed
  was granted every write without a scope. An entry is now written
  `<issuer>#<value>` and holds for that issuer alone; beside several issuers
  a bare entry, or one naming an issuer the section does not trust, refuses
  the start. Upgrade step for a deployment with several issuers: write each
  `scopeless_subjects` entry as `<issuer>#<sub>`, and set `role_claims = []`
  (the server grants nothing by role), since the default `role_claims` is
  bare and refused. A deployment with one issuer changes nothing.
