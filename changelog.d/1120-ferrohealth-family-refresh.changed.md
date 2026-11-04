- The server, the sync service and the console run on the newest
  FerroHEALTH family releases: `ferrohealth-config` 0.2.8, which masks a
  refused configuration value of any type in its message,
  `ferrohealth-server` 0.4.3, `ferrohealth-telemetry` 0.2.4, whose panic
  line names the request id and whose exported request span is named
  `{method} {route}`, `ferrohealth-session` 0.2.2, `ferrohealth-smart`
  0.8.8 and FerroUI 0.6.6.
- The console's **Sign out** takes you to the provider's end-session
  endpoint only when the session holds an ID Token (OpenID Connect
  RP-Initiated Logout 1.0 §2), as `ferrohealth-session` 0.2.1 does; a
  sign-out with no such session lands on the signed-out page.
