- `[server] idle_timeout_ms` (default `120000`) closes a kept-alive HTTP/1.1
  connection that sends no next request within that time, with a clean end
  of stream. `[server] header_read_timeout_ms` now counts from the first byte
  of each request head; it used to count from the previous response, so it
  also closed an idle connection after 30 s, sometimes while a client was
  sending its next request into it. A deployment that relied on the 30 s
  idle close sets `idle_timeout_ms`.
