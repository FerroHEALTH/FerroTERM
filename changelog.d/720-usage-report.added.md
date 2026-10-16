- The server sends a usage report to FerroPULSE, operated by Cadasto B.V.: one
  report at each start and one a day, with the instance id, the version, the
  deployment kind, coarse host size, the loaded code system families, and
  latency per operation group. It never carries a code, a URL, or a request
  path. It is on by default, `FERROTERM_USAGE_REPORT=off` stops it, and it is
  off in CI and without a writable `FERROTERM_STATE_DIR`. The image and the
  compose file keep the state on a `/data/state` volume. What is sent, what
  the collector stores (the source IP address and the country derived from
  it, for 13 months), and how to have it erased:
  <https://ferroterm.eu/docs/operate/usage-report.html>.
