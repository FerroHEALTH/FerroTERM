# Configuration

The server reads its configuration from environment variables at start. There
is no configuration file and no command-line flag; a container sets the
variables, a host sets them in the service unit. `ValueSet/$expand` returns
at most 1,000 members without `count` and asks for paging beyond that
(`too-costly`); the limit is not configurable.

<!-- toc -->

## The variables

| Variable | Meaning | Default |
|---|---|---|
| `FERROTERM_INDEX` | The artifact directories to serve, one per code system version (a SNOMED CT edition, a LOINC release, an ICD-10 classification, an RxNorm release, or one of the three ICD-11 code systems; the manifest says which), separated by the platform's path separator (`:` on Linux). Each holds the `store.redb` with its `store.*.col` column files, `hierarchy.bin`, `text.bin`, and `manifest.json` that `ferroterm-build` wrote. The server opens them read-only and refuses to start when one is missing, damaged, or duplicates another's system version. A path that holds no `manifest.json` of its own is an index root: every child directory that holds one is an artifact, and the server lists the root again on every reload (see [Index roots](#index-roots)). | none (the server starts with no code systems) |
| `FERROTERM_CODESYSTEMS` | Directories of FHIR `CodeSystem` resources to serve, separated by the platform's path separator: a FHIR package's `package/` directory (HL7 Terminology, for example) or a directory of `CodeSystem` JSON files. Files are read in the FHIR version the directory's `package.json` declares, or as R4B when there is none. A resource with `content = supplement` is applied to the system it `supplements` and is read and searched as a `CodeSystem` of its own; the server refuses to start when that system is not loaded. `ValueSet` resources in the same directories are served by `url` and `version` for `ValueSet/$expand` and `ValueSet/$validate-code` (without a version, the greatest version answers). | none |
| `FERROTERM_RESOURCES` | The database file holding the `CodeSystem`, `ValueSet`, and `ConceptMap` resources clients write through the REST API, and the closure tables `$closure` maintains. The server creates the file when it does not exist, serves every resource in it exactly as it serves one from `FERROTERM_CODESYSTEMS`, and loads them again after a restart. A deployment that names no file refuses every write with a `422` and declares no write interaction in its capability statement. | none (the server persists nothing) |
| `FERROTERM_LISTEN` | The socket address to bind. | `127.0.0.1:8080` (the container image sets `0.0.0.0:8080`) |
| `FERROTERM_ADMIN_LISTEN` | The socket address of the admin listener, which serves `POST /reload` and nothing else (see [Reloading the served set](#reloading-the-served-set)). Without `FERROTERM_OIDC_ISSUER` it authenticates nobody, so bind it to an address only your operators reach. Unset, the server binds no second listener and `SIGHUP` is the only reload trigger. | none (no admin listener) |
| `FERROTERM_BASE_URL` | The URL clients reach this server at, without a version prefix and without a trailing slash (`https://tx.example.org`). Each version states it as `implementation.url` in its capability statements, so a client behind a reverse proxy learns the address it should use rather than the socket the process bound. Setting it also turns on the audience check of the SMART gate, whose accepted set is every served version's base under this URL (see [Behind a reverse proxy](reverse-proxy.md#the-audience-a-token-carries)). A deployment that names none states no URL. | none |
| `FERROTERM_SECURITY_SERVICE` | The authentication in front of the server, as comma-separated codes of the FHIR `restful-security-service` value set (`OAuth`, `SMART-on-FHIR`, `Basic`, `Certificates`, `Kerberos`, `NTLM`). The codes declare what a proxy enforces, in `CapabilityStatement.rest.security.service`; with `FERROTERM_OIDC_ISSUER` set, `SMART-on-FHIR` is added to whatever you declare here. | none |
| `FERROTERM_OIDC_ISSUER` | The OpenID Connect issuer whose access tokens gate the write routes and the admin listener (see [Behind a reverse proxy](reverse-proxy.md#validating-tokens-in-the-server)). The server reads `{issuer}/.well-known/openid-configuration` and the JWKS it names before it binds, and refuses to start when either does not answer. Map a terminologist's role in that issuer to `user/CodeSystem.cud`, `user/ValueSet.cud`, and `user/ConceptMap.cud`, and a reader's role to none of them, so a reader's token carries no write scope at all. Unset, the server asks for no token and every route answers as it does today. | none (no token is ever asked for) |
| `FERROTERM_OIDC_AUDIENCE` | One more `aud` value the server accepts, beside the FHIR base of every served version it derives from `FERROTERM_BASE_URL`. Set it when your issuer mints an audience of its own, a resource identifier registered in the issuer, for example. A token passes when its `aud`, a string or an array, holds any accepted value, and is refused with `401 invalid_token` otherwise (see [Behind a reverse proxy](reverse-proxy.md#the-audience-a-token-carries)). With neither this nor `FERROTERM_BASE_URL` set, the server checks no audience and the issuer alone bounds the token. | none |
| `FERROTERM_OIDC_ADMIN_SCOPE` | The scope the admin-listener routes require, compared verbatim against the scopes the token grants. Map an operator's role to it and leave it off a terminologist's and a reader's roles. No specification governs the admin surface, so the name is yours. | `ferroterm/admin` |
| `FERROTERM_VIEWER_CLIENT_ID` | The OAuth client the viewer signs in as. The viewer is a public client: it holds no secret, signs in with the authorization code flow with PKCE, and learns this value from `[base]/.well-known/smart-configuration`, where the server publishes it as `ferroterm_viewer_client_id` (RFC 8414 §2 admits the extra member). Register the client in the same issuer `FERROTERM_OIDC_ISSUER` names, with the redirect address `{base}/ui/editor/callback` (the editing bundle is where the viewer signs in), no client secret, `S256` as a code challenge method, and the scopes `openid`, `fhirUser`, `user/CodeSystem.cud`, `user/ValueSet.cud`, `user/ConceptMap.cud`. Unset, the viewer offers no sign-in and draws no edit control. | none (the viewer stays read-only) |
| `FERROTERM_UI` | Whether the server serves the viewer at `/ui`, the editing bundle at `/ui/editor`, and redirects `/` onto the first. Reads `on` or `off` (`true`/`false`, `1`/`0`, `yes`/`no`). Off, the `/ui` routes are absent and both `/` and `/ui` answer the `not-found` `OperationOutcome` every unknown path answers, for a deployment that wants an API-only surface. A build that carries no viewer bundle serves no `/ui` route whatever this is set to, and says so at start. | `on` |
| `FERROTERM_USAGE_REPORT` | Whether the server sends the [usage report](usage-report.md) to FerroPULSE. Reads `on` or `off` (`true`/`false`, `1`/`0`, `yes`/`no`). `on` cannot override CI detection: with `GITHUB_ACTIONS` or `CI` set to `true` or `1`, the report is off. | `on` |
| `FERROTERM_USAGE_REPORT_ENDPOINT` | The collector the usage report is posted to: an `https` URL without credentials, or plain `http` to a loopback host. The server refuses to start on anything else. | `https://report.ferropulse.eu/v1/report` |
| `FERROTERM_STATE_DIR` | A writable directory where the server keeps its own state: the usage report's instance id and the time of its last start report. Without one, or with one the server cannot write, the usage report is off. | none (the container image sets `/data/state`, a volume) |
| `FERROTERM_DEPLOYMENT` | How the server was deployed, as the usage report states it: `helm`, `compose`, `binary`, or `unknown`. The compose file sets `compose`. | `unknown` |
| `FERROTERM_DEFAULT_LANGUAGE` | The BCP 47 language used for `display` when a request names none. | `en` |
| `FERROTERM_LOG_FORMAT` | `auto`, `json`, or `pretty`. `json` writes one object per line (`timestamp`, `level`, `message`, and the fields) for a log pipeline; `pretty` writes aligned human lines with colour; `auto` picks `pretty` when stdout is a terminal and `json` otherwise. The startup banner prints only with `pretty`. | `auto` |
| `RUST_LOG` | The `tracing` filter. | `info,hyper=warn,tower=warn,h2=warn` |

## What the server logs

At start: the banner (pretty only), one `ferroterm starting` line with the
version and the count of code systems, one `serving code system` line per
loaded version (`id`, `system`, `version`, `concepts`, `languages`, `path`),
one `listening` line with the address and the base path, and one `usage
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
anything else. It authenticates nobody unless `FERROTERM_OIDC_ISSUER` is set,
in which case every route on it requires a bearer token carrying
`FERROTERM_OIDC_ADMIN_SCOPE`. Bind it to a loopback or internal
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
