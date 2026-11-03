- The console's session watch (`GET` and `POST /auth/session`) no longer
  reports more time than a session has when its access token expires and no
  refresh token can replace it: `absolute` ends at that token's expiry and
  `idle` never passes `absolute`, so the warning before the end opens in
  time.
