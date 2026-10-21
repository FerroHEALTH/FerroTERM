# Syncing from a national terminology service

A national terminology service publishes its releases in an Atom syndication
feed: one entry per downloadable item, with the canonical identifier, the
version, the date, and a digest. Some services keep their FHIR resources
behind their FHIR API instead, and the sync lists that API as if it were a
second feed. The FerroTERM sync service reads both on a schedule, takes what
you subscribed to, builds or stages it, and asks the server to reload. It runs
beside the server and never inside it, and it moves content in one direction
only.

## What one run does

1. It reads each configured feed, and the FHIR API of a source that has one
   (a search per `CodeSystem`, `ValueSet`, and `ConceptMap`, paged to the
   end), and compares what they offer with what you already serve: the
   releases under your index root, the resources in your managed directory,
   and the ledger of what the service itself put there.
2. It takes the entries you subscribed to that you do not hold yet, streams
   each one to the staging directory, and refuses any whose digest does not
   match what the feed advertised. A resource read from a FHIR API carries no
   advertised digest; the run records the digest of what arrived instead.
3. An RF2 archive goes through `ferroterm-build` into a staging directory and
   is renamed into the index root as a new release beside the previous one. A
   FHIR resource is written as a file in the managed resource directory,
   corrected first when its service needs a correction.
4. It asks the server to reload, then prunes the releases retention no longer
   keeps and tells the server about that too.
5. It writes one JSON record of everything above and posts a summary to your
   webhook, whether the run worked or not.

A run that fails leaves the served set exactly as it was. A build that fails
never reaches the index root, and a reload the server refuses is rolled back,
including a resource file the run had replaced. A delivered resource is served
at the `id` it carries, so a delivery whose `id` another loaded or persisted
resource of the same type already answers on is one of the reloads the server
refuses (see
[What a code system version is served as](configuration.md#what-a-code-system-version-is-served-as)). Nothing is deleted except by
retention, and the server's own write store (`FERROTERM_RESOURCES`) is never
touched.

## Running it

The service ships as its own image, carrying `ferroterm-sync` and
`ferroterm-build`, and the Compose overlay runs it beside the server over one
volume:

```bash
FERROTERM_SYNC_DATA=/srv/ferroterm FERROTERM_SYNC_CONFIG=/srv/sync.toml \
  docker compose -f compose.yaml -f compose.sync.yaml up
```

One directory on one filesystem holds `index/`, `codesystems/`, `staging/`,
`records/`, and `state/`. That is not a convenience: a run stages a release and
then renames it into the index root, and a rename works only inside one
filesystem. The server mounts `index/` and `codesystems/` read-only; the sync
mounts the whole tree and writes to it as uid 65532.

The overlay also sets `FERROTERM_ADMIN_LISTEN` on the server, because
`POST /reload` on that listener is how the sync asks for the new release to be
served. That listener is a guarded surface, so the server starts only once
`[auth]` names an issuer, with `[server_admin_credentials]` in the sync
configuration, or turns authentication off (see
[Configuration](configuration.md#authentication)).

`ferroterm-sync run-once` performs a single run and exits, which is what a
first run and a cron-style deployment use.

## Configuration

One TOML file, `/etc/ferroterm/sync.toml` unless `--config` says otherwise.
Every key is optional and falls back to the value below; a key the service does
not know is refused at start-up rather than ignored.

| Key | Default | What it names |
|---|---|---|
| `listen` | `127.0.0.1:8181` | the service's own admin listener |
| `server_admin_url` | `http://127.0.0.1:8081` | the server's admin listener, which serves `POST /reload` |
| `fhir_base_url` | none | the server's FHIR base, which the revalidation reads |
| `index_root` | `/data/index` | the index root the server reads |
| `resources` | `/data/codesystems` | the managed resource directory the server reads |
| `staging` | `/data/staging` | where a run downloads and builds |
| `records` | `/data/records` | one JSON record per run |
| `state` | `/data/state` | what the service remembers between runs |
| `retention` | `2` | release directories kept per code system |
| `activation` | `auto` | `auto` or `manual` |
| `webhook_url` | none | where one JSON summary per run is posted |
| `timezone` | `UTC` | the zone a daily schedule is read in |
| `build_command` | `ferroterm-build` | the offline build, on `PATH` or by path |
| `[server_admin_credentials]` | none | the credential of the reload call (see [below](#the-credential-of-the-reload-call)) |
| `[auth]` | on, no issuer | the credentials the admin listener admits (see [The admin listener](#the-admin-listener)) |

A `[[source]]` entry names the add-on that reads a service and carries that
add-on's own block:

```toml
server_admin_url = "http://ferroterm:8081"
fhir_base_url = "http://ferroterm:8080/r4b"
index_root = "/data/index"
resources = "/data/codesystems"
staging = "/data/staging"
records = "/data/records"
state = "/data/state"
retention = 2
activation = "auto"
timezone = "Europe/Amsterdam"

[schedule]
at = "03:00"

[[source]]
kind = "nts"

[source.config]
systems = ["http://snomed.info/sct/11000146104", "http://loinc.org"]
```

An add-on whose service keeps resources behind its FHIR API lists that API by
default; the NTS block takes `fhir_api = false` to keep a run to the feed and
`fhir_api_url` to point at another endpoint.

The file carries no secret in clear. A source add-on reads its credentials from
a file or from the environment, and the reload call reads its own from the
files `[server_admin_credentials]` names, so this file is safe to commit and
safe to show.

### The credential of the reload call

The server's admin listener requires a credential carrying its admin scope
([Configuration](configuration.md#authentication)), and
`[server_admin_credentials]` names the one the sync presents. It is HTTP Basic
(RFC 7617), a user of the server's `[[auth.basic.users]]`:

```toml
[server_admin_credentials]
user = "ferroterm-sync"
password_file = "/run/secrets/sync-password"
```

or a token of the OAuth 2.0 client credentials grant (RFC 6749 §4.4) from the
authorization server the server trusts, fetched before the call, kept until
shortly before it expires, and fetched once more when the server answers
`401`:

```toml
[server_admin_credentials.oauth]
issuer = "https://auth.example.org/realms/tx"   # or token_endpoint = "…"
client_id = "ferroterm-sync"
client_secret_file = "/run/secrets/sync-client-secret"
scope = "__ferroterm.admin"
```

| Key | Meaning | Default |
|---|---|---|
| `user`, `password` or `password_file` | the Basic credential; exactly one of the two secrets | none |
| `oauth.issuer` or `oauth.token_endpoint` | where the token endpoint is read from, or the endpoint itself; exactly one | none |
| `oauth.client_id` | the client at the authorization server | none |
| `oauth.client_auth` | `client_secret_basic`, `client_secret_post` or `private_key_jwt` | `client_secret_basic` |
| `oauth.client_secret` or `oauth.client_secret_file` | the client secret; exactly one | none |
| `oauth.private_key_file`, `oauth.kid`, `oauth.assertion_alg` | the PEM key, `kid` and algorithm of a `private_key_jwt` assertion (RFC 7523) | the RFC 7638 thumbprint, the algorithm of the key |
| `oauth.scope`, `oauth.resource`, `oauth.audience` | the token request's `scope`, RFC 8707 `resource` and `audience` parameters | none |
| `oauth.refresh_margin_ms` | how long before the token expires a new one is fetched | `30000` |
| `oauth.timeout_ms` | how long one call to the authorization server may take | `10000` |

Basic and `oauth` exclude each other. With the table empty the call carries no
credential, which only a server with `[auth]` turned off accepts. There is no
static token.

## The schedule

Two shapes, and no cron syntax:

- `every = "24h"` measures from the end of the last run.
- `at = "03:00"` is a time of day in `timezone`.

With both, the earlier due time wins. With neither, the service runs only when
you ask it to. The end of the last run is written to the state directory, so a
restart between two runs does not replay the run that already happened: a
service that ran at 03:00 and restarts at 04:00 is next due at 03:00 the
following day. A window the service was down for is due at once.

A run also starts on `POST /run` on the admin listener, and as
`ferroterm-sync run-once`.

## Activation

In `auto` the run renames what it built into the index root and asks the server
to reload, in the same run. In `manual` the run stops after staging: nothing is
served, the run record says how many items wait, and `POST /activate` puts them
in front of the server when you are ready. What is staged survives a restart,
and a later run does not build it again.

An activation is not a synchronisation run: it gets its own run record, and it
leaves the schedule where it was, so the next scheduled run still starts at its
own time.

## What a new release did to your own content

A release can retire a code your own value set enumerates, remove it, or leave
it outside the value set it was included through. After a release is activated
the service revalidates every locally authored `ValueSet` and `ConceptMap`
against the set now being served and writes what it found into the run record.

It reports and never edits. A finding is a decision for a terminologist, and
the service changes nothing in your content, ever.

Set `fhir_base_url` to the server's FHIR base, such as
`http://ferroterm:8080/r4b`, to turn the check on. Without it the run record
says the check did not run. The check reads the server's public FHIR API,
`ValueSet/$validate-code` and `ValueSet/$expand`, both with `activeOnly`, so it
sees exactly what any client sees:

- a code that is still a member but is no longer active is `inactive`;
- a code the code system no longer carries is `absent`;
- a code that is still in the code system and no longer falls inside the value
  set is `outside-value-set`.

Each finding names the resource, the system, the code, and the release this run
activated for that system. A run that finds nothing says so in one sentence,
and that sentence and the finding count travel in the webhook summary.
`/metrics` carries `ferroterm_sync_revalidation_findings` as a gauge per
locally authored resource, so a resource that is clean reads zero rather than
disappearing.

The check reads the first 1000 codes of a resource and expands at most 1000
members, which keeps one run bounded; a larger resource is noted in the log.

## Retention

After a successful reload the service keeps the newest `retention` release
directories per code system and removes the rest, then asks the server to
reload once more so the removed release leaves the served set. A release that
is still there is what you roll back to, so keep at least two. A retention of
`0` removes nothing. Each run record says what was removed, how many bytes it
held, and how many bytes the index root uses afterwards.

Retention judges only directories that carry a `manifest.json`, so a build in
progress is never touched, and it groups them by the code system edition the
manifest names.

## The admin listener

The service binds `listen` and serves six routes:

| Route | What it does |
|---|---|
| `POST /run` | performs one run and answers its summary when the run has finished |
| `POST /activate` | puts what is staged in front of the server |
| `GET /runs` | the runs this service performed, newest first |
| `GET /runs/{id}` | one whole run record |
| `GET /metrics` | the Prometheus exposition |
| `GET /health` | `200` while the process is up |

Every route but `GET /health` requires a credential carrying the product
scope, `__ferroterm.sync` unless `[auth] product_scope` names another. The
`[auth]` table is the one the server reads ([Configuration](configuration.md#authentication)):
an `issuer` and an `audience` for the access tokens it accepts, Basic users
under `[[auth.basic.users]]`, or both. A refused request answers `401` with a
challenge naming the realm `ferroterm-sync` (or `[auth] realm`), or `403` for a
credential without the scope, with a JSON `error` body.

The service refuses to start while `[auth]` admits no credential, unless the
file says `enabled = false`, which only a network that trusted callers alone
reach should:

```toml
[auth]
issuer = "https://auth.example.org/realms/tx"
audience = ["ferroterm-sync"]

# or a machine caller with HTTP Basic, the hash an Argon2id PHC string:
[[auth.basic.users]]
user = "monitor"
password_hash_file = "/run/secrets/monitor.phc"
scopes = ["__ferroterm.sync"]
```

Keep the listener unpublished whatever `[auth]` says; the Compose overlay
leaves both listeners reachable only inside the compose network.

The console's history screen reads `GET /runs` and `GET /runs/{id}` to show
what the last run found about the resource a terminologist has open
([The console](viewer.md)). The console's own server makes the call, at
`[ferroterm] sync_url`, with the signed-in reader's access token, so a reader
sees the findings when the identity provider grants them the product scope
and lists the sync's audience in the token. A visitor, a reader without the
scope, or a console with no `sync_url` sees no findings.

`/metrics` carries `ferroterm_sync_runs_total{outcome="ok"|"failed"}`,
`ferroterm_sync_last_run_timestamp_seconds`,
`ferroterm_sync_entries_taken_total`, `ferroterm_sync_bytes_staged_total`,
`ferroterm_sync_index_bytes`, and `ferroterm_sync_revalidation_findings` per
locally authored resource.

## What a run record contains

One JSON file per run under `records`, named by the run identifier, which sorts
by time. It carries:

- the trigger, the start and end, the duration, and the outcome;
- per source: the feed address, how many entries it offered, the FHIR API
  address and how many resources it listed when the source has one, what was
  taken (with its origin, feed or FHIR API, the bytes, the download time, the
  build time, and the corrections that were applied), what was left behind
  with the reason for each one, such as a system you did not subscribe to, a
  release you already serve, a delta rather than a snapshot, Ontoserver's
  binary index, or a `CodeSystem` whose content is `not-present`, and the
  subscribed canonicals no listing carried at all, which means the account
  could not see them and never that the service lacks them;
- the activation: the mode, what reached the server, what is still staged, what
  the server answered to each reload, and whether the run was rolled back;
- the revalidation: how many locally authored resources were read and how many
  codes were checked, one sentence saying what it found, and a line per local
  code the release made inactive, removed, or left outside its value set;
- what retention removed and how many bytes the index root uses;
- every error, in the order it happened.

The same summary is what the webhook receives: one POST per run end, on success
and on failure. A delivery that fails is logged and never fails the run.

## NTS (Nationale Terminologie Server)

The [NTS](https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/)
is the Dutch national terminology service, run by Nictiz. It serves FHIR R4 and
publishes its content at
`https://terminologieserver.nl/synd/syndication.xml`. The feed is challenged:
the listing itself answers `401` with a bearer challenge, so the add-on
authenticates before it can even see what is on offer.

### The account and the licences

You need a personal account on the NTS. Nictiz issues it after you accept its
terms, and each licensed system is unlocked per account. A run with an account
on 2026-09-25 (issue #602) showed how that scoping looks from the outside:

- The syndication feed listed 23 entries, all of them Ontoserver binary
  indexes of the SNOMED CT Netherlands edition (releases 2023-03-31 through
  2026-08-31, 1.4 to 1.6 GB each), each marked `onto:permission` `snomed.read`.
  It carried no RF2 archive and no FHIR resource or package, so the feed
  alone gives the sync nothing it can build or serve. The FHIR content lives
  behind the FHIR API of the same service (1,024 `CodeSystem`, 1,593
  `ValueSet`, and 45 `ConceptMap` resources for that account), which the
  add-on lists beside the feed. The SNOMED CT `CodeSystem` resources there
  carry `content` `not-present` and are reported, never served.
- The feed identifies SNOMED CT as `http://snomed.info/sct` and carries the
  edition in the version URI (`http://snomed.info/sct/11000146104/version/20260831`).
- A system the account holds no licence for is hidden, not absent: `$lookup`
  on LOINC or the NHG tables answers `404` naming the current version
  (`http://loinc.org|2.83`, `http://hl7.org/fhir/sid/icpc-1-nl|12`), and a
  search by that URL returns zero resources. SNOMED CT, ICD-10-NL, the
  Labcodeset LOINC supplement and its three maps, UCUM, and the zib value sets
  were visible to an account that had accepted the SNOMED CT terms only.

Ask Nictiz for each licence you need and check what your account sees before
you subscribe to a canonical. The content stays licensed by its publisher.
FerroTERM distributes none of it, and a deployment brings what it is licensed
for.

### Credentials

The add-on reads its credentials from a file or from the environment, never
from the configuration body, so a configuration file is safe to commit and to
show in a run record.

| Variable | What it holds |
|---|---|
| `FERROTERM_NTS_CLIENT_ID` | The OAuth 2 client, `cli_client` by default |
| `FERROTERM_NTS_CLIENT_SECRET` | A client secret, when your deployment has one |
| `FERROTERM_NTS_USERNAME` | Your account name |
| `FERROTERM_NTS_PASSWORD` | Your account password |

The file form takes the same four field names in a JSON object, and the
configuration points at it with `{"from": "file", "path": "/run/secrets/nts.json"}`.

The add-on reads the token endpoint from
`https://terminologieserver.nl/fhir/.well-known/smart-configuration`, which
redirects (`301`) to the Keycloak discovery document at
`https://terminologieserver.nl/authorisation/auth/realms/nictiz/.well-known/openid-configuration`,
and logs in with the grant Nictiz documents: the password grant with
`cli_client`. If you configured a client secret, the client-credentials grant
is tried first and falls back to the password grant when the realm refuses it.
The realm issues an access token and a refresh token that both live 24 hours
(`expires_in` and `refresh_expires_in` of 86400, measured 2026-09-25). The
access token is refreshed a minute before it expires, and a refused refresh
logs in again from the stored credentials. An unattended run on any day needs
no person.

### The first run

Two scripts in `scripts/live/` run the first hops on your own machine and
never in CI, because both need your personal licensed account:
`scripts/live/nts-feed.sh` reads the feed with your credentials and reports,
per system, whether the service publishes it as an RF2 archive, as FHIR
resources, or only as Ontoserver's binary index; `scripts/live/sync-e2e.sh`
builds the binaries from the checkout, starts the server on an empty index
root, runs the sync once, and shows the run record and what the server serves
afterwards. Both take the `FERROTERM_NTS_*` variables and print no secret.
The FerroEHR hop follows: point its quickstart overlay at the server the
second script started and commit a composition whose binding names a code the
run made available.

1. Get the account, accept the terms, and confirm which systems your licences
   unlock. Run `scripts/live/nts-feed.sh` to see the feed as your account sees
   it.
2. Put the credentials where the add-on reads them, and keep them out of the
   configuration file.
3. Subscribe by canonical identifier. The defaults are the SNOMED CT
   Netherlands edition (`http://snomed.info/sct/11000146104`), LOINC
   (`http://loinc.org`), UCUM (`http://unitsofmeasure.org`), and ICD-10
   (`http://hl7.org/fhir/sid/icd-10`). The service's own canonicals for the
   Dutch content are ICD-10-NL (`http://hl7.org/fhir/sid/icd-10-nl`), the
   Labcodeset (`http://labterminologie.nl/cs/labconcepts`, a LOINC
   supplement, with its maps under `http://labterminologie.nl/cm/`), NHG-Tabel
   24 (`http://hl7.org/fhir/sid/icpc-1-nl`), the other NHG tables
   (`https://referentiemodel.nhg.org/tabellen/nhg-tabel-NN-slug`), and the zib
   value sets (`http://decor.nictiz.nl/fhir/ValueSet/{oid}--{timestamp}`).
4. Run once and read the run record before you activate anything. It lists
   every entry the feed offered, what was taken, and the reason for each entry
   left behind: a system you did not subscribe to, a release you already serve,
   or a system the service offers only as Ontoserver's binary index, which is
   an internal package format and is reported as not syndicable.
5. Check the corrections. The freely published NHG ICPC-1 to SNOMED CT map
   file writes `experimental` as the string `"true"`, and the add-on writes the
   boolean FHIR declares before the file is served. The service itself serves
   `experimental` as a boolean on every resource it lists (checked over 2,662
   summaries on 2026-09-25), so the correction concerns files taken from
   elsewhere. Every correction is listed with the element it changed; a file
   that needs none lands byte-identical.

### What is verified so far

The feed hop ran against the live service on 2026-09-25 with a personal
account (the record is on issue #602): discovery, the password grant, the
token lifetimes, and the listing are confirmed as described above. The run
also showed that the feed carries only the SNOMED CT binary index, so the RF2
lane takes nothing from the NTS until Nictiz publishes RF2 there; the FHIR
resources are taken through the API lane instead.
