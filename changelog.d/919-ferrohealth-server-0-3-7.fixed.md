- A response to a request whose body FerroTERM left unread, such as a `404`
  to a `POST` of a path it does not serve or a declared-length `413`, now
  carries `Connection: close`, so a client opens a new connection for its
  next request where it used to meet a closed socket (RFC 9112 §9.6).
- Every request leaves one request line, a timeout `408`, a declared-length
  `413` and the `500` of a panicked handler included, and a `413` for a body
  with no declared length that passes `[server] body_limit_bytes` is
  answered with an `OperationOutcome`, as the declared-length `413` is.
- Every accepted connection sets `TCP_NODELAY`, so a small response no
  longer waits on the client's delayed acknowledgment, and an HTTP/2 request
  still running when the drain bound passes is cancelled before the server
  exits, as an HTTP/1.1 request already was.
