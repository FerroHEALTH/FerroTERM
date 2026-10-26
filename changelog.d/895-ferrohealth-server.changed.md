- Breaking: `ferroterm healthcheck`, `ferroterm-sync healthcheck` and
  `ferroterm-console healthcheck` ask `GET /health/readiness` and exit 1 while
  the process drains, where they asked `GET /health`. The reason on stderr
  reads `unhealthy: readiness answered 503`, `unhealthy: cannot connect to
  <address>: …` or `unhealthy: readiness did not answer in time`, and the
  console prints `healthy` on success. Upgrade: a script that matched the old
  reasons (`cannot reach`, `answered`, `did not answer within`) matches the
  new ones.
- Breaking: the server refuses a request body larger than 1 MiB with `413`,
  the default of `server.body_limit_bytes`, where the limit of 2 MB applied.
  Upgrade: a deployment that writes larger resources raises
  `server.body_limit_bytes`.
- Breaking: `server.listen` takes an IP address and a port, and a host name
  such as `localhost:8080` refuses the start. Upgrade: write the address,
  such as `127.0.0.1:8080`.
- The boot banner names the build as `ferroterm <version> (<commit>)` and
  lists the served FHIR versions, the site and the maintainer, the banner
  every FerroHEALTH product prints.
- The synchronisation service's admin listener runs the FerroHEALTH HTTP
  stack: a request id on every request, a request timeout of 30 seconds, a
  body limit of 1 MiB and a panic answered `500`. The console sends the
  FerroHEALTH console headers, which add
  `Cross-Origin-Opener-Policy: same-origin`, and marks its stylesheets
  `no-cache` so a new release is fetched.
