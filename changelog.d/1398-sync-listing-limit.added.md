- `ferroterm-sync` reads at most `listing_limit` bytes, `64MiB` by default, of
  a terminology service's feed or of one page of its FHIR API search: a longer
  `Content-Length` is refused before reading, and a body that passes the limit
  while it streams is refused. A listing is parsed in memory, so it has a bound
  of its own, far below `download_limit`, which bounds bytes streamed to disk.
