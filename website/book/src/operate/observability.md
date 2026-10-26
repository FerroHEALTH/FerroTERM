<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Health, metrics, and request identifiers

FerroTERM answers the FerroHEALTH health routes under `/health`, a Prometheus scrape at
`GET /metrics`, and names every request with an `X-Request-Id`. None of these
is a FHIR interaction: the routes live off the FHIR base path, so a probe or a
scrape is never mistaken for a terminology request.

## Health

The server, the synchronisation service and the console answer the same
three routes, the ones every FerroHEALTH product serves:

| Route | Answers |
|---|---|
| `GET /health/liveness` | `200` with `{"state":"up"}` while the process runs. It checks nothing. |
| `GET /health/readiness` | `200` while the process is ready and `503` otherwise, with the readiness document: `state` (`up`, `degraded` or `down`), `phase` (`booting`, `serving` or `draining`) and `indicators`, each named with its state and never its detail. |
| `GET /health` | The status of `/health/readiness` with `{"state":…}` alone. |

The server binds its listener only after every index and `CodeSystem`
directory it was configured with is open, and refuses to start on a missing
or damaged one, so a ready answer means the deployment serves what it was told
to serve. On `SIGTERM` or `SIGINT` the process moves to `draining`:
`/health` and `/health/readiness` answer `503` while the requests in flight
finish, so a load balancer stops sending new ones, and `/health/liveness`
keeps answering `200`.

The container image has no shell and no HTTP client, so the probe is a
subcommand of the binary:

```console
$ ferroterm healthcheck
$ ferroterm healthcheck --url http://127.0.0.1:8080/health
```

It sends one `GET /health`, waits at most three seconds, and exits 0 with no
output on `200 OK`. Otherwise it exits 1 with the reason on stderr: `cannot
reach`, `answered 404 Not Found`, or `did not answer within 3s`. Without
`--url` it follows `FERROTERM_LISTEN`, on the loopback when the server listens
on every interface, which is what the image sets. The image declares it as its
`HEALTHCHECK` (every ten seconds, a five-second limit, a sixty-second start
period for reading an edition), and the shipped `compose.yaml` states the same
block, so `docker compose up --wait` and `condition: service_healthy` work.
Kubernetes ignores the image's instruction: point its liveness probe at
`GET /health/liveness` and its readiness probe at `GET /health/readiness`.

## Scraping

```bash
curl -s http://127.0.0.1:8080/metrics
```

The exposition is the text format Prometheus reads, with every metric under the
`ferroterm` prefix:

| Metric | Type | Labels | What it says |
|---|---|---|---|
| `ferroterm_http_requests_total` | counter | `method`, `route`, `status` | the requests answered |
| `ferroterm_http_request_duration_seconds` | histogram | `method`, `route`, `status` | how long each took |
| `ferroterm_code_system_loaded` | gauge | `system`, `version` | one per code system version loaded |
| `ferroterm_reloads_total` | counter | `outcome` (`ok`, `failed`) | the reloads of the served set |

`route` is the matched route, `/r4b/CodeSystem/$lookup`, never the URI, so the
series count stays bounded no matter how many codes clients ask about. The
duration buckets start at half a millisecond and double, which brackets the
bars the engine is held to: a point read under a millisecond, a page of an
expansion under ten.

`ferroterm_code_system_loaded` describes the set the server answers from now:
a [reload](configuration.md#reloading-the-served-set) that drops a version
drops its series, and one that adds a version adds one. Both `outcome` series
of `ferroterm_reloads_total` start at zero, so
`increase(ferroterm_reloads_total{outcome="failed"}[1h]) > 0` is an alert you
can write before the first reload happens. A failed reload means the server is still
answering from the old set and the content on disk did not open; the
`the served set was not reloaded` log line carries the reason.

A scrape configuration is one job:

```yaml
scrape_configs:
  - job_name: ferroterm
    static_configs:
      - targets: ["ferroterm:8080"]
```

The endpoint carries no code system content and no request bodies, so it is
safe to expose to a monitoring network. It is not authenticated: keep it on an
internal listener or behind the same reverse proxy rules as the rest of the
server (see [Configuration](configuration.md)).

## Request identifiers

Every response carries `X-Request-Id`. A client that sends the header gets its
own value back, so a trace started at the caller stays one trace; a client that
sends none gets a UUID the server makes. An id that is empty, longer than 128
characters, or not printable ASCII is replaced rather than echoed, so a header
cannot smuggle a line break into a log.

The same id is a field of the request's log line:

```json
{"timestamp":"2026-09-05T09:21:04.113Z","level":"INFO","message":"request",
 "method":"GET","route":"/r4b/CodeSystem/$lookup","status":200,
 "latency_ms":0.42,"named":"system=http://snomed.info/sct code=73211009",
 "request_id":"0f0f6c1e-9d0e-4a5f-8c1a-1f6b0f1f6b0f"}
```

So a client that reports a slow or refused call gives you the id, and the log
line, the status, and the latency follow from it. Bodies are never logged, and
the `named` field carries only the system, url, version, and code parameters a
request stated.
