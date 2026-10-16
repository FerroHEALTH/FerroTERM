# Usage report

Every FerroTERM server sends a small JSON report to FerroPULSE, a collector
that Cadasto B.V. runs. It goes out at each process start and once a day. It
says that the server is alive, which version it runs, which families of code
systems it serves, and how fast it answers, in coarse aggregates. The report is
on by default. Every boot logs one INFO line that says whether it is on and how
to switch it off.

This page lists every field the report carries, says when it is sent, gives the
switch, and carries the privacy notice for the data the collector stores. No
FHIR or SNOMED CT specification governs any of it; the report is FerroTERM's
own design.

<!-- toc -->

## When a report is sent

There are two kinds of report, and both are one HTTPS `POST` to
`https://report.ferropulse.eu/v1/report` with a JSON body of at most 64 KiB.

- **`start`:** at each process start, once the server has loaded its code
  systems. A server sends at most one start report per ten minutes: the time
  of the last one is kept in the state directory. A container in a restart
  loop sends one start report per ten minutes, not one per restart.
- **`daily`:** the first attempt 10 minutes after boot, then about every
  24 hours (23.5 to 24.5 hours after the previous one, spread at random). It
  covers the requests since the previous daily report of the same process, or
  since boot.

The report needs a state directory, `FERROTERM_STATE_DIR`, where the server
keeps the instance id. Without one, or with one it cannot write, the server
sends nothing: without a stable id each restart would count as a new
installation. The container image sets `/data/state` and declares it a volume,
and the compose file mounts a named volume there.

The boot line, with the report on, reads:

```text
usage report ON: instance id, version, licence type, deployment, uptime, host size, loaded code system families and coarse operation latency, at each start and once a day; no codes, URLs or request paths. Off: FERROTERM_USAGE_REPORT=off. Details: https://ferroterm.eu/docs/operate/usage-report.html
```

It carries the configured endpoint as the structured field `endpoint`. With the
report off, the line says why, for example:

```text
usage report OFF: switched off (FERROTERM_USAGE_REPORT=off); nothing is sent. Details: https://ferroterm.eu/docs/operate/usage-report.html
usage report OFF: CI detected (GITHUB_ACTIONS); nothing is sent. Details: https://ferroterm.eu/docs/operate/usage-report.html
usage report OFF: no state directory (FERROTERM_STATE_DIR is unset), and without a stable instance id each restart would count as a new installation; nothing is sent. Details: https://ferroterm.eu/docs/operate/usage-report.html
```

## What a report contains

Every report carries these fields:

| Field | Sent in | What it is |
|---|---|---|
| `schema` | both | Always `2`, the version of the report format. |
| `event` | both | `start` or `daily`. |
| `first_start` | `start` | `true` when this start created the instance id, which marks a new installation; `false` on every later start. |
| `product` | both | Always `ferroterm`. |
| `instance_id` | both | A random UUID (version 4), created at the first start and stored in `<FERROTERM_STATE_DIR>/instance-id`. It is not derived from the host or anything else about the deployment. |
| `version` | both | The server version, for example `0.1.8`. |
| `git_sha` | both | The commit the binary was built from, 7 to 40 hex digits. A build that recorded no commit, such as a local `cargo build`, sends `0000000`. |
| `licence` | both | The licence grant type, `non-commercial` (what the Business Source License grants everyone). |
| `deployment` | both | How the server was deployed: `helm`, `compose`, `binary` or `unknown`. It comes from `FERROTERM_DEPLOYMENT`, which the compose file sets to `compose`. |
| `uptime_s` | both | Seconds since this process started. |
| `cpu_bucket` | both | The CPUs available to the process, as a range: `1-2`, `3-4`, `5-8`, `9-16` or `17+`. |
| `memory_bucket` | both | The memory available to the process in GiB, the smaller of the host's memory and the container limit, as a range: `<4`, `4-8`, `8-16`, `16-32` or `32+`. Off Linux the server cannot read it and sends `<4`. |
| `status` | both | The state facts below. |
| `metrics` | `daily` | The performance aggregates below. |

The `status` object:

| Member | What it is |
|---|---|
| `index_ok` | Whether at least one code system the deployment loaded is serving. |
| `code_systems` | The families of the loaded code systems, sorted: `dhd`, `fhir`, `gstandaard`, `icd10`, `icd10cm`, `icd11`, `labcodeset`, `loinc`, `other`, `rxnorm`, `snomed`. `fhir` is a `CodeSystem` resource from `FERROTERM_CODESYSTEMS` that no other family names, and `other` is an artifact none names. Never an edition, a module id, a version or a URL. |
| `viewer` | Whether the viewer is mounted at `/ui`. |
| `persistence` | Whether a resources database (`FERROTERM_RESOURCES`) is configured. |

A start report, formatted here for reading (the server sends it on one line):

```json
{
  "schema": 2,
  "event": "start",
  "first_start": true,
  "product": "ferroterm",
  "instance_id": "5f0c6b2e-3d4a-4c8b-9e1f-2a3b4c5d6e7f",
  "version": "0.1.8",
  "git_sha": "1a2b3c4d",
  "licence": "non-commercial",
  "deployment": "compose",
  "uptime_s": 0,
  "cpu_bucket": "3-4",
  "memory_bucket": "8-16",
  "status": {
    "index_ok": true,
    "code_systems": ["loinc", "snomed"],
    "viewer": true,
    "persistence": false
  }
}
```

### The daily metrics

The daily report adds one object that covers the window since the previous
daily report:

```json
"metrics": {
  "window_24h": {
    "requests_bucket": "1k-10k",
    "groups": {
      "lookup": {
        "p50_ms": 1.2, "p95_ms": 4.0, "p99_ms": 9.5,
        "errors_5xx": 0,
        "histogram": [1200, 800, 300, 90, 20, 6, 2, 1, 0, 0, 0]
      }
    },
    "counters": { "fhir_r4": 120, "fhir_r4b": 2300, "fhir_r5": 0, "fhir_r6": 3 }
  }
}
```

- **`requests_bucket`:** the number of requests under a FHIR base (`/r4`,
  `/r4b`, `/r5`, `/r6`) in the window, as a range: `0`, `1-100`, `100-1k`,
  `1k-10k`, `10k-100k` or `100k+`. Each range includes its lower bound and
  excludes its upper one, so `1-100` means 1 to 99. `/health`, `/metrics`,
  `/ui` and the admin listener are not counted.
- **`groups`:** one entry per group of requests that saw at least one in the
  window. A request is placed in a group by the route template it matched,
  never by the path or the parameters the client sent: `lookup`,
  `validate_code`, `expand`, `subsumes`, `translate` and `closure` for the
  operations, `metadata`, `read` (a resource, its history, or a version),
  `search`, `write` (create, update, delete), and `other` for everything else.
- **`p50_ms`, `p95_ms`, `p99_ms`:** the median, 95th and 99th percentile
  latency of the group in milliseconds, estimated from its histogram.
- **`errors_5xx`:** the responses with a 5xx status.
- **`histogram`:** 11 request counts, one per latency bucket. The bucket upper
  bounds in milliseconds are 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000,
  and no bound for the last.
- **`counters`:** the requests per served FHIR version.

## What a report never contains

A report carries no code, no display or other text from a request, no system,
value set or concept map URL, no edition, module id or version of a code
system, no request path or parameter, no error message, no user name, no client
IP address, no host name and no customer or staff name. The report types in the
server have no field that could hold any of these, and the collector refuses a
report with a value that is not a boolean, a number or a short lower-case
token.

The collector does record the IP address the report arrives from. See
[What the collector stores](#what-the-collector-stores).

## Switch it off

```shell
FERROTERM_USAGE_REPORT=off
```

The compose file forwards this variable from your shell, so
`FERROTERM_USAGE_REPORT=off docker compose up` switches the report off there
too. In a CI job the report is off by itself: when `GITHUB_ACTIONS` or `CI` is
`true` or `1`, the server sends nothing, and `FERROTERM_USAGE_REPORT=on` does
not change that. A container does not inherit the runner's environment, so a
test lane that starts the image passes `--env FERROTERM_USAGE_REPORT=off`.

With the report off, the server sends nothing to FerroPULSE and writes nothing
to the state directory for the report, and the boot line says `usage report
OFF`. Nothing else in the server changes. Switching it off stops new reports
only. Reports already stored at the collector stay until the retention period
ends, unless you ask for them to be erased.

## Network

- **Destination:** outbound HTTPS to `report.ferropulse.eu`, port 443.
  `FERROTERM_USAGE_REPORT_ENDPOINT` moves it; the server refuses to start on an
  endpoint that is plain HTTP (except to a loopback host) or that carries
  credentials.
- **One direction only:** the server reads the status code of the answer and
  nothing else. It does not follow redirects, and nothing the collector answers
  changes the server's configuration or behaviour.
- **Failures stay local:** a send gives up after 5 seconds without a
  connection or 10 seconds in total. A failure is logged once at DEBUG. Boot,
  `/health` and request handling never wait on the report, so a slow or
  unreachable collector affects none of them. A daily report that does not
  reach the collector is tried again an hour later with the same window; one
  the collector answered, even with a refusal, is not resent.

## Privacy notice

This section gives the information that Articles 13 and 14 of the
[GDPR](https://eur-lex.europa.eu/eli/reg/2016/679/oj) list, for the personal
data a report and its stored record can contain. A report names no person.
It can still relate to one: the collector stores the address each report
comes from, and a server run by one person on their own internet connection
can be linked to that person through the address or through the instance id.
Where that is so, the report is personal data, and this notice applies to it.

### Controller

Cadasto B.V., the Netherlands. Write to
[info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Cadasto has not appointed a data
protection officer.

### Purposes and lawful basis

Cadasto processes the reports for three purposes:

- to know which FerroTERM versions run in the field, so it can tell how many
  installations a security release concerns and whether they upgrade;
- to know which families of code systems the installations serve;
- to see how the operations perform in real use, so it can find and fix slow
  paths.

The lawful basis is Art. 6(1)(f) GDPR, legitimate interest. The interest is
Cadasto's, as the maker of FerroTERM: keeping the product it publishes secure
and maintained, which needs the versions in use and the reach of each security
release, and improving it, which needs the code system mix and field
performance. The data come from the FerroTERM server itself. You are not
obliged to send them: the report can be switched off, and switching it off
changes nothing else in FerroTERM.

### What the collector stores

- Every report as it arrives, with the fields listed above.
- With each report: the time of receipt, the UTC day, the source IP address
  (IPv4 or IPv6), and the country derived from that address.
- Per instance id: when it was last seen, its current version, and whether it
  is alive.
- Per day: latency totals summed across the whole fleet, without instance ids.

The country comes from an IP-to-country database bundled with the collector,
so no address leaves the collector host for the lookup. IP geolocation by
[DB-IP](https://db-ip.com), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Reports from the
address ranges of GitHub-hosted CI runners are marked as such and left out of
every dashboard.

### Recipients and location

Hetzner Online GmbH (Gunzenhausen, Germany) hosts the collector in its cloud
and processes the reports as Cadasto's processor, under a data processing
agreement Cadasto has signed (Art. 28 GDPR). The collector runs in Hetzner's
data centre in Falkenstein, Germany. The FerroPULSE dashboard that shows the
reports sits behind a GitHub login with no public or anonymous access; only
members of the FerroHEALTH organisation can sign in.

### Retention

A daily job deletes reports and start reports 13 months after they were
received, an instance record 13 months after the instance was last seen, and
the fleet-wide daily totals 13 months after their day. Backups of the
collector's database are kept for 14 days, so a deleted report can remain in a
backup for at most 14 days more.

### Your right to object

> [!IMPORTANT]
> **You have the right to object** at any time, on grounds relating to your
> particular situation, to the processing of reports that are personal data
> concerning you (Art. 21(1) GDPR).
>
> **To stop future reports**, switch the report off as described in
> [Switch it off](#switch-it-off). No request to anyone is needed.
>
> **To have the reports already stored erased**, write to
> [info@cadasto.com](mailto:info@cadasto.com) with the instance id, which
> the server keeps in `<FERROTERM_STATE_DIR>/instance-id`. Under the compose
> file it sits on the `ferroterm-state` volume:
> `docker run --rm -v <project>_ferroterm-state:/s busybox cat /s/instance-id`
> prints it. Cadasto then
> deletes every report, start report and instance record stored for that
> instance id, source addresses included (Art. 17(1)(c) GDPR). Switch the
> report off first, or the next report starts a new record.
>
> The fleet-wide daily totals hold no instance ids, so an erased instance's
> share in days already summed stays in those totals.

### Your other rights

At the same address you can ask for access to the reports stored for your
instance id (Art. 15), for their rectification (Art. 16) or erasure (Art. 17),
or for restriction of their processing (Art. 18). You have the right to lodge
a complaint with a supervisory authority, in particular in the Member State of
your habitual residence, place of work or place of the alleged infringement
(Art. 77).

### Operators in Switzerland

Under the Swiss Federal Act on Data Protection
([DSG](https://www.fedlex.admin.ch/eli/cc/2022/491/de) Art. 19 Abs. 4), these
are the states your report's data are disclosed to: **Germany**, where the
Hetzner data centre in Falkenstein runs the collector, and **the Netherlands**,
where Cadasto B.V. is established. Both are listed in Annex 1 of the
Swiss Data Protection Ordinance
([DSV](https://www.fedlex.admin.ch/eli/cc/2022/568/de)) as states whose
legislation provides adequate protection (DSG Art. 16 Abs. 1).
