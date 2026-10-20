# Configuration

The server reads its configuration from environment variables at start, and
has no command-line flag; a container sets the variables, a host sets them in
the service unit. The one file it reads is the optional `[auth]` table that
`FERROTERM_AUTH_FILE` names. `ValueSet/$expand` returns
at most 1,000 members without `count` and asks for paging beyond that
(`too-costly`); the limit is not configurable.

<!-- toc -->

## The variables

| Variable | Meaning | Default |
|---|---|---|
| `FERROTERM_INDEX` | The artifact directories to serve, one per code system version (a SNOMED CT edition, a LOINC release, an ICD-10 classification, an RxNorm release, or one of the three ICD-11 code systems; the manifest says which), separated by the platform's path separator (`:` on Linux). Each holds the `store.redb` with its `store.*.col` column files, `hierarchy.bin`, `text.bin`, and `manifest.json` that `ferroterm-build` wrote. The server opens them read-only and refuses to start when one is missing, damaged, or duplicates another's system version. A path that holds no `manifest.json` of its own is an index root: every child directory that holds one is an artifact, and the server lists the root again on every reload (see [Index roots](#index-roots)). | none (the server starts with no code systems) |
| `FERROTERM_CODESYSTEMS` | Directories of FHIR `CodeSystem` resources to serve, separated by the platform's path separator: a FHIR package's `package/` directory (HL7 Terminology, for example) or a directory of `CodeSystem` JSON files. Files are read in the FHIR version the directory's `package.json` declares, or as R4B when there is none. A resource with `content = supplement` is applied to the system it `supplements` and is read and searched as a `CodeSystem` of its own; the server refuses to start when that system is not loaded. `ValueSet` resources in the same directories are served by `url` and `version` for `ValueSet/$expand` and `ValueSet/$validate-code` (without a version, the greatest version answers). | none |
| `FERROTERM_RESOURCES` | The database file holding the `CodeSystem`, `ValueSet`, and `ConceptMap` resources clients write through the REST API, and the closure tables `$closure` maintains. The server creates the file when it does not exist, serves every resource in it exactly as it serves one from `FERROTERM_CODESYSTEMS`, and loads them again after a restart. A deployment that names no file refuses every write with a `422` and declares no write interaction in its capability statement. With a file named, the server refuses to start until `[auth]` names an issuer or turns authentication off (see [Authentication](#authentication)). | none (the server persists nothing) |
| `FERROTERM_LISTEN` | The socket address to bind. | `127.0.0.1:8080` (the container image sets `0.0.0.0:8080`) |
| `FERROTERM_ADMIN_LISTEN` | The socket address of the admin listener, which serves `POST /reload` and nothing else (see [Reloading the served set](#reloading-the-served-set)). Every route on it requires the admin scope (see [Authentication](#authentication)), and the server refuses to start with it until `[auth]` names an issuer or turns authentication off. Unset, the server binds no second listener and `SIGHUP` is the only reload trigger. | none (no admin listener) |
| `FERROTERM_BASE_URL` | The URL clients reach this server at, without a version prefix and without a trailing slash (`https://tx.example.org`). Each version states it as `implementation.url` in its capability statements, so a client behind a reverse proxy learns the address it should use rather than the socket the process bound. Setting it also adds every served version's base under this URL to the audiences the write routes accept, beside `[auth] audience` (see [Behind a reverse proxy](reverse-proxy.md#the-audience-a-token-carries)). A deployment that names none states no URL. | none |
| `FERROTERM_SECURITY_SERVICE` | The authentication in front of the server, as comma-separated codes of the FHIR `restful-security-service` value set (`OAuth`, `SMART-on-FHIR`, `Basic`, `Certificates`, `Kerberos`, `NTLM`). The codes declare what a proxy enforces, in `CapabilityStatement.rest.security.service`; with an `[auth]` issuer, `SMART-on-FHIR` is added to whatever you declare here. | none |
| `FERROTERM_AUTH_FILE` | A TOML file holding the `[auth]` table and nothing else (see [Authentication](#authentication)). Use it for what the environment cannot say well: several issuers, `[[auth.basic.users]]`, the token rules. Every `FERROTERM_AUTH__<KEY>` variable is applied over it. | none |
| `FERROTERM_AUTH__<KEY>` | One key of the `[auth]` table, named by its path below `[auth]` with `__` between the segments: `FERROTERM_AUTH__ISSUER`, `FERROTERM_AUTH__AUDIENCE`, `FERROTERM_AUTH__ENABLED`, `FERROTERM_AUTH__PRODUCT_SCOPE`. The value reads as TOML (`false`, `["https://tx.example.org/r4"]`) or, when it is not TOML, as the string it is. A name that is no key refuses the start. | see [Authentication](#authentication) |
| `FERROTERM_USAGE_REPORT` | Whether the server sends the [usage report](usage-report.md) to FerroPULSE. Reads `on` or `off` (`true`/`false`, `1`/`0`, `yes`/`no`). `on` cannot override CI detection: with `GITHUB_ACTIONS` or `CI` set to `true` or `1`, the report is off. | `on` |
| `FERROTERM_USAGE_REPORT_ENDPOINT` | The collector the usage report is posted to: an `https` URL without credentials, or plain `http` to a loopback host. The server refuses to start on anything else. | `https://report.ferropulse.eu/v1/report` |
| `FERROTERM_STATE_DIR` | A writable directory where the server keeps its own state: the usage report's instance id and the time of its last start report. Without one, or with one the server cannot write, the usage report is off. | none (the compose file sets `/data/state`, on a named volume; the container image sets none) |
| `FERROTERM_DEPLOYMENT` | How the server was deployed, as the usage report states it: `helm`, `compose`, `binary`, or `unknown`. The compose file sets `compose`. | `unknown` |
| `FERROTERM_DEFAULT_LANGUAGE` | The BCP 47 language used for `display` when a request names none. | `en` |
| `FERROTERM_LOG_FORMAT` | `auto`, `json`, or `pretty`. `json` writes one object per line (`timestamp`, `level`, `message`, and the fields) for a log pipeline; `pretty` writes aligned human lines with colour; `auto` picks `pretty` when stdout is a terminal and `json` otherwise. The startup banner prints only with `pretty`. | `auto` |
| `RUST_LOG` | The `tracing` filter. | `info,hyper=warn,tower=warn,h2=warn` |

## Authentication

The server is a SMART App Launch resource server
(<https://hl7.org/fhir/smart-app-launch/conformance.html>) over the FerroHEALTH
crate `ferrohealth-smart`: it checks credentials and never issues one. The read
surface stays open. The write routes (`POST`, `PUT` and `DELETE` on
`CodeSystem`, `ValueSet` and `ConceptMap`, and `POST [base]/$closure`) and every
route of the admin listener require a credential.

Authentication is on by default and fails closed: a server that names
`FERROTERM_RESOURCES` or `FERROTERM_ADMIN_LISTEN` refuses to start until
`[auth]` names an issuer and an audience, or says `enabled = false`. A
read-only server with neither starts without `[auth]`.

```sh
FERROTERM_AUTH__ISSUER=https://auth.example.org/realms/tx
FERROTERM_AUTH__AUDIENCE='["https://tx.example.org"]'
# or, on a network only trusted callers reach:
FERROTERM_AUTH__ENABLED=false
```

The keys of `[auth]`, in the file `FERROTERM_AUTH_FILE` names or as
`FERROTERM_AUTH__<KEY>`:

| Key | Meaning | Default |
|---|---|---|
| `enabled` | Whether the write routes and the admin listener authenticate their callers. | `true` |
| `issuer` | The issuer identifier of the authorization server whose access tokens are accepted, an `https` URL (plain `http` on a loopback host only). The server reads its metadata (RFC 8414, with OpenID Connect Discovery as the fallback) and its key set before it binds, and refuses to start when either does not answer. `[[auth.issuer]]` entries name several issuers, each with any of the keys below that differ. | none |
| `audience` | The `aud` values a token may carry; one of them is required (RFC 9068 §4). The write routes also accept every served version's base under `FERROTERM_BASE_URL`; the admin listener accepts this list alone. | none (required with an issuer) |
| `product_scope` | The scope the admin listener requires, a string starting with `__` or a full URI (SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility). Anything else refuses the start. | `__ferroterm.admin` |
| `algorithms` | The signature algorithms accepted; an HMAC algorithm refuses the start. | `RS256`, `RS384`, `PS256`, `PS384`, `ES256`, `ES384` |
| `token_profile` | `rfc9068`, which requires the `at+jwt` type and the RFC 9068 claims (`iss`, `sub`, `aud`, `exp`, `iat`, `jti`, `client_id`), or `jwt` for an issuer that mints plain JWTs. | `rfc9068` |
| `clock_skew_seconds` | The skew allowed on `exp`, `nbf` and `iat`, at most 300. | `60` |
| `jwks_max_age_seconds` | How long a key set is used before it is read again. | `600` |
| `jwks_refresh_interval_seconds` | The shortest time between two reads of the key set, so an unknown `kid` reads it at most once per interval. | `60` |
| `jwks_file` | A key set file handed over out of band, read in place of the issuer's `jwks_uri`. | none |
| `timeout_ms` | How long one call to the authorization server may take. | `10000` |
| `realm` | The realm every challenge names. | `FERROTERM_BASE_URL`, else `FerroTERM` |
| `scopes_supported` | The scopes the discovery document lists. List only scopes the server honours: "the server SHALL support all scopes listed here". | none (the member is omitted) |
| `capabilities` | SMART capabilities the discovery document adds to the ones it derives. | none |
| `scopeless_subjects` | The exact `sub` values granted every write without a scope. | none |
| `[[auth.basic.users]]` | HTTP Basic users (RFC 7617) for machine callers: `user`, `password_hash` or `password_hash_file` (an Argon2id PHC string), and `scopes`, which decide what the user reaches as a token's scopes would. | none (no Basic) |

A machine caller authenticates with HTTP Basic as one of those users or with an
access token it obtained with the OAuth 2.0 client credentials grant; the
server accepts no static token. A file holding a Basic user:

```toml
[auth]
issuer = "https://auth.example.org/realms/tx"
audience = ["https://tx.example.org"]

[[auth.basic.users]]
user = "ferroterm-sync"
password_hash_file = "/run/secrets/sync-password-hash"
scopes = ["__ferroterm.admin"]
```

A refused credential answers `401` with an `OperationOutcome` and a
`WWW-Authenticate` challenge that names no claim and no scope; a credential
that grants no permission for the route answers `403` (RFC 6750 §3.1). The
scopes each route requires are in
[Behind a reverse proxy](reverse-proxy.md#validating-tokens-in-the-server).

## What the server logs

At start: the banner (pretty only), one `ferroterm starting` line with the
version, the served FHIR versions (`fhir_versions`), and the count of code
systems, one `serving code system` line per loaded version (`id`, `system`,
`version`, `concepts`, `languages`, `path`), one `listening` line with the
address and the base path of every served version (`bases`), and one `usage
report ON` or `usage report OFF` line (see [Usage report](usage-report.md)).
Per request: one
`request` line with `method`, `route`, `status`, `latency_ms`, and the
`system`, `url`, `version`, `code`, `codeA`, `codeB` query parameters the
request named (`named`); bodies and free-text parameters are never logged. A
client error logs at `warn`, a server error at `error`. On `SIGTERM` or
`SIGINT`: the signal, then `ferroterm stopped`. A refused start is one
`cannot start` line with the reason. On a reload: `the served set was
reloaded` with the count of code systems, or `the served set was not
reloaded` at `error` level with the reason.

## Reloading the served set

The server reads the directories again and swaps the whole served set on two
triggers:

- `SIGHUP` to the process (`kill -HUP <pid>`, `docker kill --signal=HUP <name>`).
- `POST /reload` on the admin listener, when `FERROTERM_ADMIN_LISTEN` names an
  address.

```console
$ curl -sS -X POST http://127.0.0.1:8081/reload
{"outcome":"ok","systems":[{"id":"snomed.info-sct-…","system":"http://snomed.info/sct","version":"…"}]}
```

A reload opens every artifact in `FERROTERM_INDEX` and every resource in
`FERROTERM_CODESYSTEMS` again, read-only, builds a new registry, and exchanges
a pointer. A request already in flight finishes on the set it started on, and
that set is dropped when the last request holding it ends, so the memory of the
swapped system is resident twice for a few seconds. `FERROTERM_RESOURCES` is
live already and is carried over untouched: the resources clients wrote stay
written, and the new set serves them over the new artifacts.

An artifact is replaced by building it in a new directory and renaming that
directory over the configured path, which is one atomic step and leaves the
old files in place for the handles still reading them. Writing into the
directory the running server is reading corrupts what it reads. A directory in
`FERROTERM_CODESYSTEMS` is different: add or remove a resource file in it, and
the reload picks up the change.

Adding or removing an artifact PATH means changing `FERROTERM_INDEX`, which
the process reads once at start, so that still takes a restart. Adding or
removing a release inside an index root does not, because a reload lists every
root again.

The admin listener carries `POST /reload` and nothing else and answers `404` to
anything else. Every route on it requires a credential carrying the admin
scope, `[auth] product_scope` (`__ferroterm.admin` when unset), unless `[auth]`
turns authentication off. Bind it to a loopback or internal
address (`127.0.0.1:8081`), keep it off the address your clients reach, and
leave it out of the reverse proxy that publishes the FHIR surface (see
[Behind a reverse proxy](reverse-proxy.md)). The FHIR listener never serves
`/reload`: that path is the `not-found` `OperationOutcome` any unknown path
answers.

## Index roots

The server reads each path in `FERROTERM_INDEX` in one of two ways:

- **An artifact path** holds a `manifest.json` of its own. The server opens
  that directory as one code system version.
- **An index root** holds no `manifest.json`. The server lists it and opens
  every child directory that holds one, in the order their names sort. A child
  without a manifest is passed over, as is a file beside the children.

```text
/srv/ferroterm/snomed-nl/        FERROTERM_INDEX names this root
    20260331/                    an artifact: the release in production
    20260930/                    an artifact: the release that followed it
    .incoming-20261031/          no manifest yet, so the server passes it over
```

Every reload lists the roots again, so a release directory that appeared since
the last scan is opened and one that disappeared is dropped. That is what lets
a synchronisation job publish a release beside the running one and keep the
previous release for a rollback. Both are served, and a request that names no
version still resolves to the greatest one, so publishing a release is what
promotes it. A retired release keeps answering until you remove its directory
and reload.

Write a release into a root in two steps: build or download it into a directory
outside the root, then rename it in. `rename` is one atomic step
(<https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html>), so
a reload sees the new directory whole or not at all. The manifest is the marker
that makes the directory an artifact, and `ferroterm-build` writes it after
everything else, so a build straight into a root is picked up only once it has
finished.

A root that holds no artifact serves nothing and logs `the index root holds no
artifact`. A path that names nothing at all still refuses the start, and a
reload that meets one keeps the old set answering.

## A refused artifact stops the start

An artifact the server cannot open stops the whole start. Nine good artifacts
and one damaged one serve nothing, and the `cannot start` line names the
directory and the reason.

That is deliberate. A server that dropped the damaged one and served the other
nine would answer every code of the tenth system with "not found", and a caller
cannot tell that from "this code does not exist". A terminology answer that is
wrong and confident is worse for a clinical system downstream than a server
that did not start, so the refusal is loud and total.

The practical consequence for an operator is that a rebuilt artifact is
swapped in before a restart, not after one. The practical consequence for a
benchmark or a conformance run is that it fails at once, naming the artifact,
rather than reporting a figure taken over a system nothing served.

A reload holds the same bar without the downtime. One artifact that does not
open refuses the whole swap: the server keeps serving the set it already had,
logs `the served set was not reloaded` with the reason, counts
`ferroterm_reloads_total{outcome="failed"}`, and answers the admin request
with a `500` naming the artifact. A half-applied release never reaches a
client. A child directory under an index root is an artifact like any other
here: once it holds a manifest the server cannot read, it refuses the reload
rather than serving the root without it.

## The registry systems

Four systems ship with the server and are served without configuration: BCP
47 language tags (`urn:ietf:bcp:47`), BCP 13 media types (`urn:ietf:bcp:13`),
UCUM units (`http://unitsofmeasure.org`), and ISO 3166-1 country codes
(`urn:iso:std:iso:3166`). The first three are grammars over their registries
and cannot be expanded; the fourth is a table from Unicode CLDR. Their data
is vendored in the binary.

## What a code system version is served as

Each loaded code system version becomes one `CodeSystem` instance. **A resource
that carries an `id` keeps it:** the id is the logical id the resource is read
by, so a client that knows it reads the resource there
([id](https://hl7.org/fhir/R4B/resource.html#id)). The server mints an id only
for a version that arrives without a resource, an index built from an RF2
release or a LOINC delivery, for example: the version URI when that URI carries
the system (a SNOMED CT edition is
`snomed.info-sct-<module>-version-<YYYYMMDD>`), otherwise the system URL and
the version (`terminology.hl7.org-CodeSystem-v2-0001-2.0.0`), reduced to the
FHIR id alphabet. The instance reads at `[base]/r4b/CodeSystem/<id>`, is found
by `[base]/r4b/CodeSystem?url=…&version=…`, and takes an instance-level
operation at `[base]/r4b/CodeSystem/<id>/$validate-code`. The id is stable for
a given version and is listed at startup and in
`GET /r4b/metadata?mode=terminology`.

A `ValueSet` or `ConceptMap` in a `FERROTERM_CODESYSTEMS` directory becomes an
instance the same way: its own `id` when it has one, else one minted from its
`url` and `version`. It reads at `[base]/r4b/ValueSet/<id>` or
`[base]/r4b/ConceptMap/<id>`, is found by `[base]/r4b/ValueSet?url=…&version=…`
or the matching `ConceptMap` search, and carries that id as its `Resource.id` in
both answers, so a client can read what a search returned. Every served version
answers both.

Two ids never compete. An `id` outside the FHIR id alphabet (at most 64 of
`A-Z`, `a-z`, `0-9`, `-`, and `.`,
[id](https://hl7.org/fhir/R4B/datatypes.html#id)) refuses the load, and so does
a second loaded resource of the same type carrying an id the first one holds, or
a loaded id a resource written through the REST API already answers on. The
diagnostic names the canonicals involved; the server never renames a resource to
make room. From the other side, a `PUT` or `POST` onto an id a loaded resource
is read at is refused with `409 Conflict`, so a client cannot shadow what the
deployment serves.

## What you do not configure

You do not configure a database connection, a search cluster, or a JVM heap.
FerroTERM has none of these. The inputs the server needs are the built indexes and any
FHIR resource directories, and you build the indexes offline with
`ferroterm-build` as described in [Loading code systems](loading-snomed.md).
