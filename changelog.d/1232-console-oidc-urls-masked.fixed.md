- `ferroterm-console config check` and every other rendering of the console's configuration
  show an `[oidc]` issuer, `redirect_uri` or `post_logout_redirect_uri`
  written with userinfo as `https://***@…`, never its password. The console
  moves to `ferrohealth-session` 0.13, which holds the three URLs as
  `SecretUrl` (#1232).
