- The server answers a request that runs past `server.request_timeout_ms`
  with `408`, a body larger than `server.body_limit_bytes` with `413`, and a
  handler that panics with `500`, each as an `OperationOutcome` (`timeout`,
  `too-long`, `exception`), where a panic dropped the connection with no
  answer and a slow request had no deadline. The `500` names the request id,
  and the panic message reaches neither the body nor the log.
