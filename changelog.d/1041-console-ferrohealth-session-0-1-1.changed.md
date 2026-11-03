- The console takes `ferrohealth-session` 0.1.1. The crate now refuses an
  `[oidc]` a console cannot sign in with safely (a provider URL that is
  not `https` off loopback, a redirect URI with a fragment, `scopes`
  without `openid` or with `offline_access`), so the console's own checks
  are gone. A `post_logout_redirect_uri` without an `end_session_endpoint`
  is no longer refused. The session watch again answers a session that is
  gone with an RFC 9457 problem whose detail says to sign in again, and a
  sign-in never returns to a page under the sign-in page.
