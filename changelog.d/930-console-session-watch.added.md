- A signed-in console page watches its session: it warns before the idle
  timeout with Stay signed in and Sign out, leaves for the sign-in page the
  moment the session ends, and a sign-out in one tab ends every tab of the
  console together. The console answers FerroUI's session route at
  `/auth/session`, where a read extends nothing and only the reader's own
  activity extends the session.
