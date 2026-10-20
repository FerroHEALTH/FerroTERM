- `ferrohealth-smart` 0.3.1. A write route and the admin listener refuse a
  token whose `sub` is empty or whitespace only (RFC 9068 §2.2, RFC 7519
  §4.1.2) and a bearer value outside the RFC 6750 §2.1 `b64token` syntax,
  each with the generic `401 invalid_token` and no `error_description`.
