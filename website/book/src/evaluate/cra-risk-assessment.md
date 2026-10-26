<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CRA risk assessment

This page is the cybersecurity risk assessment Cadasto B.V., the manufacturer
of each tagged FerroTERM release, keeps under the Cyber Resilience Act
(Regulation (EU) 2024/2847). CRA Art. 13(2) has the manufacturer "undertake an
assessment of the cybersecurity risks associated with a product with digital
elements and take the outcome of that assessment into account during the
planning, design, development, production, delivery and maintenance phases".
Art. 13(3) says what the assessment holds: "an analysis of cybersecurity risks
based on the intended purpose and reasonably foreseeable use, as well as the
conditions of use", and an indication of "whether and, if so in what manner,
the security requirements set out in Part I, point (2), of Annex I are
applicable to the relevant product with digital elements and how those
requirements are implemented". Where a requirement does not apply, Art. 13(4)
asks for "a clear justification to that effect" in the technical
documentation. The assessment is point 3 of the technical documentation of CRA
Annex VII.

The manufacturer's side, which is the same for every FerroHEALTH product (who
the manufacturer is, what applies from when, reporting, the support period and
vulnerability handling), is in the FerroHEALTH book:
[The Cyber Resilience Act](https://ferrohealth.eu/docs/cra.html). This page is
FerroTERM's own part. The information and instructions Annex II asks for are on
[CRA information for users](cra-user-information.md).

<!-- toc -->

> [!WARNING]
> The quotations are from the Official Journal text of the
> [CRA](https://eur-lex.europa.eu/eli/reg/2024/2847/oj) at EUR-Lex. No
> conformity assessment has been carried out, no EU declaration of conformity
> exists and FerroTERM carries no CE marking. A status on this page describes
> the evidence the repository holds, never conformity.

## Scope and dates

- **The product assessed.** Each tagged release as Cadasto B.V. publishes it:
  the `ferroterm` and `ferroterm-build` binaries in the release tarballs, the
  `ferroterm` container image, the console's `ferroterm-console` binaries, site
  bundle and image, and the source archive. The library crates FerroTERM
  publishes on crates.io are products of their own; this page covers them only
  where a row says so, and [the crates](#the-crates-on-cratesio) says how.
- **The synchronisation service.** `ferroterm-sync` is built in the repository
  and documented in the book, but no release lane publishes its binary or its
  image yet, so it is not placed on the market and this assessment does not
  cover it.
- **Who the requirements bind.** Annex I Part I sets properties of the product,
  which the manufacturer ensures (CRA Art. 6, Art. 13(1)). Part II sets the
  manufacturer's processes. A deploying organisation that runs an unmodified
  release is the addressee of neither.
- **When.** Annex I and Art. 13 apply from 11 December 2027, Art. 14 from 11
  September 2026 (CRA Art. 71(2)). The assessment runs now so that the gaps
  are closed before that date.
- **Default or important product.** FerroTERM is a default product, the
  owner's decision of 2026-10-24. CRA Art. 7(1) makes a product an important
  product when it has "the core functionality of a product category set out
  in Annex III", and a terminology server answering the FHIR `CodeSystem`,
  `ValueSet` and `ConceptMap` operations matches no category of Annex III.
  Of the procedures CRA Art. 32(1) offers, the owner chose "the internal
  control procedure (based on module A) set out in Annex VIII" (Art. 32(1)(a),
  Annex VIII Part I), under which the manufacturer declares conformity on its
  sole responsibility, with no notified body. The assessment, the EU
  declaration of conformity (Art. 28) and the CE marking apply from 11
  December 2027 (Art. 71(2)). No assessment has been carried out and no
  declaration exists yet
  ([how conformity is assessed](https://ferrohealth.eu/docs/cra.html#how-conformity-is-assessed-product-by-product)).

## The basis: intended purpose, use and environment

**Intended purpose.** FerroTERM answers the HL7 FHIR terminology operations
(`$lookup`, `$validate-code`, `$subsumes`, `$expand`, `$translate`,
`$closure`) over the code systems a deployment loads, for FHIR clients such as
a clinical data repository, a validator or a clinical application resolving
value set bindings ([what FerroTERM is](what-ferroterm-is.md)). It reads
indexes built ahead of time by `ferroterm-build` from a release the deploying
organisation brings under its own licence, and, where the deployment names a
database, keeps the `CodeSystem`, `ValueSet` and `ConceptMap` resources its
clients write. The console is a browser interface over the same public FHIR
API.

**The data it processes** is terminology content and the resources clients
author. A request names a code system, a code and sometimes a display or a
search term. FerroTERM holds no patient record, and its read path receives no
patient data in normal use.

**The operational environment** is a server on the deploying organisation's
network, beside the systems that call it, behind a reverse proxy that
terminates TLS, authenticates readers where the organisation wants readers
authenticated, and rate-limits
([behind a reverse proxy](../operate/reverse-proxy.md)). An authorization
server the organisation runs issues the tokens the write routes, the admin
listener and the console's sign-in use
([authentication](../operate/configuration.md#authentication)).

**Reasonably foreseeable use and misuse**, each with what the product does:

| Case | What FerroTERM does | What remains |
|---|---|---|
| The FHIR listener published to the internet without a proxy | the server speaks plain HTTP and serves its read surface to anyone who reaches it; the book says to put TLS and reader authentication in the proxy | the deployment decides; a licence that limits who may read the content is the deployment's to keep |
| Writes switched on without authentication | a server that names `FERROTERM__TERMINOLOGY__RESOURCES` or `FERROTERM__ADMIN__LISTEN` refuses to start until `[auth]` names an issuer or says `enabled = false` | `enabled = false` is accepted, for a network only trusted callers reach |
| The admin listener published beside the FHIR surface | every route on it requires the admin scope, and the book says to bind it to loopback or an internal address | a deployment that turns authentication off and publishes it lets anyone make the server re-read its disks |
| A damaged or half-written index served | an artifact that does not open refuses the start, and a reload that meets one keeps the previous set ([a refused artifact stops the start](../operate/configuration.md#a-refused-artifact-stops-the-start)) | none |
| Patient data sent as a search term | the request log never carries a body or a free-text parameter | a proxy's own access log may; the book says to keep it to the same standard |
| A release run after a newer one is published | the release notes and `SECURITY.md` say only the newest release is supported | the running server does not state its support end date ([(c)](#c-security-updates-notification-and-postponement)) |

**The assets:** the licensed code-system content in the indexes, the
resources clients wrote, the correctness of every terminology answer (a wrong
answer that looks right reaches a clinical system downstream), the
authentication configuration and its credentials (`[auth]` and the Basic users'
password hashes, the console's OIDC client secret and sessions), the release
artefacts, and availability.

## How the risks were analysed

CRA Art. 3(37) defines a cybersecurity risk as "the potential for loss or
disruption caused by an incident", "expressed as a combination of the magnitude
of such loss or disruption and the likelihood of occurrence of the incident".
The register rates each risk on the intended purpose and the shipped
controls: the magnitude by the asset it reaches, the likelihood by what an
actor needs to cause it. The ratings are the manufacturer's judgement, on three
levels (high, medium, low).

| # | Risk | Magnitude | Likelihood with the shipped controls | Controls | Annex I Part I(2) |
|---|---|---|---|---|---|
| T1 | An unauthenticated caller writes, changes or deletes a resource, or a closure table | high: a changed value set changes every validation that uses it | low: writes need a credential, and a server with a database refuses to start without `[auth]` | SMART App Launch resource server over `ferrohealth-smart`; fail-closed start; scopes per route | (b), (d) |
| T2 | Someone the content's licence does not cover reads licensed content | medium | medium: the read surface asks for no credential by design | reader authentication in the proxy; the book names it | (d), (e) |
| T3 | The admin listener is used to make the server re-read its disks | medium | low | a separate listener, off by default, the admin scope on every route | (d), (j) |
| T4 | A damaged, partial or substituted index answers requests | high | low | artifacts opened read-only; a refused artifact stops the start or the reload; atomic rename into an index root | (f) |
| T5 | A malformed or hostile request crashes the server or corrupts what it holds | medium | low | memory-safe Rust with `unsafe` forbidden; overflow checks in release; fuzzed parsers | (j), (k) |
| T6 | The service is made unavailable by expensive requests | medium | medium: the server has no rate limit, timeout or body cap of its own | `$expand` capped at 1,000 members without `count`; the proxy limits rates, timeouts and bodies | (h) |
| T7 | A vulnerable component ships in a release | high | medium | `cargo deny` in CI; Dependabot; `cargo auditable` binaries; no image scan before tagging yet | (a) |
| T8 | A tampered artefact reaches a deployment | high | low | SLSA Build Level 3 provenance; SBOM attestations; digest-pinned base image | (f) |
| T9 | A console session is taken over, or the console is used to write on a reader's behalf | medium | low | server-held session behind one `HttpOnly` cookie; `SameSite=Lax`; PKCE; a nonce CSP with `frame-ancestors 'none'` | (d), (e) |
| T10 | Request content leaks into logs or outbound reports | medium | low | no body or free-text parameter logged; the usage report carries no code, URL or request path | (e), (g) |

## Annex I Part I(1): the level of cybersecurity

"Products with digital elements shall be designed, developed and produced in
such a way that they ensure an appropriate level of cybersecurity based on the
risks." The manufacturer applies it through the register above: each risk has
a control, and each control is traced to a point of Part I(2) below. The level
is set against the risks of a terminology server: it holds no patient record,
and the harms that matter are a wrong answer a clinical system trusts, an
unauthorised change to a value set, and disclosure of licensed content.

## Annex I Part I(2), point by point

The chapeau reads: "On the basis of the cybersecurity risk assessment referred
to in Article 13(2) and where applicable, products with digital elements
shall:". Each entry gives the requirement, whether it applies, how FerroTERM
implements it, the evidence, and the status (met, partial or gap, describing
the evidence only).

### (a) No known exploitable vulnerabilities

"be made available on the market without known exploitable vulnerabilities"

- **Applies:** yes.
- **How:** the `cargo-deny` job in CI fails on a RustSec advisory and on a
  yanked crate, with each accepted advisory reasoned in `deny.toml`; Dependabot
  opens the updates. The release binaries are built with `cargo auditable`, so
  each carries its dependency list for a scanner.
- **Evidence:** `deny.toml`; `.github/workflows/ci.yml` (`cargo-deny`);
  `.github/dependabot.yml`; `.github/workflows/release-build.yml`.
- **Status:** partial. Two advisories are accepted in `deny.toml`, both
  `unmaintained` rather than a vulnerability, both compile-time only in the
  console's Leptos tree, tracked on #432. No release lane scans the container
  images before they are tagged.

### (b) Secure by default, and reset to the original state

"be made available on the market with a secure by default configuration,
unless otherwise agreed between manufacturer and business user in relation to
a tailor-made product with digital elements, including the possibility to
reset the product to its original state"

- **Applies:** yes. The tailor-made carve-out does not: a public release is
  not a tailor-made product.
- **How:** the server binds `127.0.0.1:8080` unless told otherwise, persists
  nothing and accepts no write until `FERROTERM__TERMINOLOGY__RESOURCES` names a database,
  binds no admin listener until `FERROTERM__ADMIN__LISTEN` names one, and refuses
  to start with either until `[auth]` names an issuer and an audience or turns
  authentication off. `[auth]` refuses an HMAC algorithm and a plain `http`
  issuer outside loopback, and an unknown `FERROTERM__AUTH__` key stops the
  start. The console offers no sign-in or editing without an `[oidc]` section,
  and its session cookie is `Secure` by default. The images run as uid 65532
  on distroless static, with no shell; the quickstart adds a read-only root
  filesystem, all capabilities dropped and `no-new-privileges`.
- **Reset:** FerroTERM keeps its state in files the deployment names: the
  index directories, the `FERROTERM__TERMINOLOGY__RESOURCES` database and the
  `FERROTERM__USAGE_REPORT__STATE_DIR` directory. Deleting the database and the state
  directory and restarting returns a server to its original state over the
  same indexes ([CRA information for users, 8(d)](cra-user-information.md#8d-secure-decommissioning-and-removing-user-data)).
- **Evidence:** `app/ferroterm-server/src/config.rs`;
  [configuration](../operate/configuration.md); `docker/Dockerfile`;
  `docker/console/Dockerfile`; `compose.yaml`.
- **Status:** met for the defaults. The image binds `0.0.0.0:8080` in plain
  HTTP, because TLS belongs to the proxy in the environment above. The reset
  is documented and not exercised by a test.

### (c) Security updates, notification and postponement

"ensure that vulnerabilities can be addressed through security updates,
including, where applicable, through automatic security updates that are
installed within an appropriate timeframe enabled as a default setting, with
a clear and easy-to-use opt-out mechanism, through the notification of
available updates to users, and the option to temporarily postpone them"

- **Applies:** the security-update limb, yes. The automatic-update limb, no
  ([the justification](#automatic-security-updates-annex-i-part-i2c)). The
  notification limb, yes. Postponement is inherent: the deploying organisation
  decides when to upgrade.
- **How:** a fix ships in the next tagged release, and only the newest release
  is supported (`SECURITY.md`, CRA Art. 13(10)). Users learn of it through the
  GitHub release, its notes, the security advisory and the changelog, and can
  follow the release feed
  (`https://github.com/FerroHEALTH/FerroTERM/releases.atom`). The server makes
  no outbound call to learn of a newer release.
- **Evidence:** [`SECURITY.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/SECURITY.md#supported-versions);
  [the family security policy](https://ferrohealth.eu/docs/security.html#how-a-fix-reaches-you).
- **Status:** partial. The running server states its version (the start log
  line and `CapabilityStatement.software.version`) but not the end month of
  its support period, which CRA Art. 13(19) asks to be stated.

### (d) Protection from unauthorised access, and reporting it

"ensure protection from unauthorised access by appropriate control mechanisms,
including but not limited to authentication, identity or access management
systems, and report on possible unauthorised access"

- **Applies:** yes.
- **How:** the server is a SMART App Launch resource server over the
  FerroHEALTH crate `ferrohealth-smart`: it checks RFC 9068 access tokens from
  the configured issuers, or HTTP Basic users with Argon2id hashes, and
  requires a scope per write route and the admin scope on the admin listener.
  The read surface is open by design: terminology is reference content, and
  where a licence or a policy limits who may read it, the proxy authenticates
  readers. The console signs a user in with the authorization code flow and
  PKCE and keeps the token on its own server. A refused credential answers
  `401` and a missing permission `403`, each logged as a request line with its
  status at `warn`.
- **Evidence:** `app/ferroterm-server/src/smart/`;
  [authentication](../operate/configuration.md#authentication);
  [validating tokens in the server](../operate/reverse-proxy.md#validating-tokens-in-the-server);
  `app/ferroterm-console/src/oidc/`.
- **Status:** met for writes and the admin listener. Reporting is the request
  log and `ferroterm_http_requests_total` by status; there is no separate
  security event record.

### (e) Confidentiality of stored, transmitted and processed data

"protect the confidentiality of stored, transmitted or otherwise processed
data, personal or other, such as by encrypting relevant data at rest or in
transit by state of the art mechanisms, and by using other technical means"

- **Applies:** yes. The means are a menu ("such as").
- **How:** in transit, TLS is terminated by the proxy, and the server's own
  outbound calls (the issuer's metadata and keys, the usage report) require
  `https` outside loopback. The console holds tokens server side and never in
  the browser. Other technical means: request bodies and free-text parameters
  are never logged; secrets are read from files; the Basic users carry
  password hashes, never passwords.
- **Evidence:** [what the server logs](../operate/configuration.md#what-the-server-logs);
  [the console, signing in](../operate/console.md#signing-in).
- **Status:** partial. The server and the console speak plain HTTP on their
  own listeners and rely on the proxy for TLS. FerroTERM does not encrypt the
  indexes or the resource database at rest
  ([the justification](#encryption-at-rest-annex-i-part-i2e)).

### (f) Integrity, and reporting corruptions

"protect the integrity of stored, transmitted or otherwise processed data,
personal or other, commands, programs and configuration against any
manipulation or modification not authorised by the user, and report on
corruptions"

- **Applies:** yes.
- **How:** data: the server opens every index read-only, refuses to start or
  to reload when one does not open, and logs the reason; an index root takes a
  release by atomic rename; a write through the REST API takes `If-Match`.
  Programs: SLSA Build Level 3 provenance, Sigstore-signed SBOM attestations,
  checksums, and a base image pinned by digest. Configuration: a
  `FERROTERM__AUTH__` variable that names no key of `[auth]` stops the start.
- **Evidence:** [a refused artifact stops the start](../operate/configuration.md#a-refused-artifact-stops-the-start);
  [verifying releases](../operate/verifying-releases.md);
  `.github/workflows/release-build.yml`.
- **Status:** met. The indexes carry no signature of their own: they are built
  by the deploying organisation from its own release, on its own machine, and
  stored on its own disks, so their protection against a writer who reaches
  those disks is the platform's.

### (g) Data minimisation

"process only data, personal or other, that are adequate, relevant and limited
to what is necessary in relation to the intended purpose of the product with
digital elements (data minimisation)"

- **Applies:** yes.
- **How:** the request log carries the method, route, status, latency and the
  `system`, `url`, `version`, `code`, `codeA` and `codeB` parameters, never a
  body or a free-text parameter. The usage report carries no code, URL or
  request path ([outbound data flows](#outbound-data-flows-annex-i-part-i2g)).
- **Status:** met.

### (h) Availability, also after an incident

"protect the availability of essential and basic functions, also after an
incident, including through resilience and mitigation measures against
denial-of-service attacks"

- **Applies:** yes.
- **How:** the indexes are precomputed, so an answer is a set of point reads
  and bitmap operations; `ValueSet/$expand` answers at most 1,000 members
  without `count`; a reload swaps the served set without dropping the requests
  in flight, and a failed reload keeps the old set answering; the process
  drains on `SIGTERM`. Rate limits, timeouts and a body cap are placed in the
  proxy ([rate limits and timeouts](../operate/reverse-proxy.md#rate-limits-and-timeouts)).
- **Status:** partial. The server has no rate limit, request timeout or body
  cap of its own beyond the framework's default body limit, so a deployment
  without a proxy has none.

### (i) Minimal impact on other devices and networks

"minimise the negative impact by the products themselves or connected devices
on the availability of services provided by other devices or networks"

- **Applies:** yes.
- **How:** the server binary links no general HTTP client
  (`scripts/checks/no-client-in-server.sh`, run in CI). Its outbound calls are
  the issuer's metadata and keys, read at most once per
  `jwks_refresh_interval_seconds`, and the usage report, at most one start
  report per ten minutes and one daily report, each at most 64 KiB.
- **Status:** met.

### (j) Limited attack surface

"be designed, developed and produced to limit attack surfaces, including
external interfaces"

- **Applies:** yes.
- **How:** the server has one listener by default; the admin listener serves
  `POST /reload` alone and is off by default; writes are off without a
  database; the images are distroless static with no shell or package manager;
  the console is a separate program and the server serves no UI.
- **Evidence:** `docker/Dockerfile`; `docker/console/Dockerfile`;
  [the admin listener stays behind the proxy](../operate/reverse-proxy.md#the-admin-listener-stays-behind-the-proxy).
- **Status:** met. `/health` and `/metrics` answer on the FHIR listener, and
  the book says to keep them off the public route.

### (k) Exploitation mitigation

"be designed, developed and produced to reduce the impact of an incident using
appropriate exploitation mitigation mechanisms and techniques"

- **Applies:** yes.
- **How:** Rust with `unsafe_code = "forbid"` across the workspace and
  `overflow-checks = true` in release builds; fuzzing of the ECL parser, the
  FHIR JSON and XML codecs, the RF2 row reader and the SNOMED URI parser; a
  non-root uid, no shell and, in the quickstart, a read-only root filesystem
  with every capability dropped.
- **Evidence:** `Cargo.toml` (`[workspace.lints.rust]`, `[profile.release]`);
  `fuzz/fuzz_targets/`; `.github/workflows/fuzz.yml`.
- **Status:** met.

### (l) Recording and monitoring, with an opt-out

"provide security related information by recording and monitoring relevant
internal activity, including the access to or modification of data, services
or functions, with an opt-out mechanism for the user"

- **Applies:** yes.
- **How:** one request line per request with its status, at `warn` for a
  client error and `error` for a server error; a line for each reload and its
  outcome; Prometheus metrics on `/metrics`; the `_history` of every resource
  written through the REST API. The opt-out is the `RUST_LOG` filter.
- **Evidence:** [what the server logs](../operate/configuration.md#what-the-server-logs);
  [health, metrics, and request identifiers](../operate/observability.md).
- **Status:** met. The request line does not name the authenticated subject of
  a write.

### (m) Secure removal of all data and settings, and secure transfer

"provide the possibility for users to securely and easily remove on a
permanent basis all data and settings and, where such data can be transferred
to other products or systems, ensure that this is done in a secure manner"

- **Applies:** yes.
- **How:** every piece of state is a file or directory the deployment names,
  so removing them removes all data and settings
  ([CRA information for users, 8(d)](cra-user-information.md#8d-secure-decommissioning-and-removing-user-data)).
  A single resource is removed with `DELETE` over the REST API. Transfer is
  the FHIR API itself: any client reads every resource.
- **Status:** met. FerroTERM has no erase command; sanitising the media is the
  deploying organisation's.

## Requirements and limbs that do not apply

CRA Art. 13(4): "Where certain essential cybersecurity requirements are not
applicable to the product with digital elements, the manufacturer shall include
a clear justification to that effect in that technical documentation."

### Automatic security updates (Annex I Part I(2)(c))

Part I(2)(c) asks for automatic security updates "where applicable", and Part
II(7) for distribution "where applicable for security updates, in an automatic
manner". Neither applies to FerroTERM:

- FerroTERM is a server run under its operator's change control, beside the
  clinical systems that call it. An update restarts the process and can change
  an answer those systems rely on, so the operator schedules it.
- The operator controls the host, the image registry and the network.
  FerroTERM has no channel of its own into a deployment, and the server binary
  links no HTTP client by design, so adding one would widen (j).
- Updates are distributed securely by other means: signed, attested releases,
  digest-identified images, and the advisory that says to upgrade.

So the automatic-installation limb and its opt-out do not apply, and CRA
Annex II point 8(e) has nothing to describe.

### Encryption at rest (Annex I Part I(2)(e))

Part I(2)(e) names encryption at rest as one of the means ("such as").
FerroTERM does not encrypt the indexes or the resource database:

- The indexes hold terminology content, not personal data, and the server
  reads them on every request; encryption at rest is provided where they are
  stored, at the volume or disk layer the deploying organisation provisions.
- The resource database holds the `CodeSystem`, `ValueSet` and `ConceptMap`
  resources clients author, which the server serves to anyone who reaches the
  read surface.

## Outbound data flows (Annex I Part I(2)(g))

Every connection FerroTERM opens on its own initiative, with what it carries
and why.

| Flow | Default | Carries | Purpose |
|---|---|---|---|
| Usage report to `report.ferropulse.eu` | on, when `FERROTERM__USAGE_REPORT__STATE_DIR` names a writable directory | instance id, version, licence type, deployment, uptime, host size, loaded code system families and coarse operation latency ([fields](../operate/usage-report.md#what-a-report-contains)) | the manufacturer's view of which versions and deployment sizes run; no code, URL or request path; `FERROTERM__USAGE_REPORT__ENABLED=false` turns it off |
| Metadata and key set of the `[auth]` issuer | when an issuer is configured | the issuer's well-known URLs; no request data | verifying the tokens the write routes and the admin listener accept |
| The console to the terminology server | always | the FHIR requests a screen makes, with the signed-in user's token on a write | the console's purpose |
| The console to the OpenID Provider | when `[oidc]` is configured | the authorization code exchange, refresh and revocation | signing a user in |

## The crates on crates.io

Each library crate FerroTERM publishes on crates.io (`rf2`, `concept-graph`,
`concept-store`, `designation-index`, `sct-ecl`, `sct-scg`,
`fhir-terminology`, `loinc`, `classification`, `dhd-thesaurus`, `gstandaard`,
`labcodeset`, `icd11`, `rxnorm-rrf`) is a product of its own. Each is a
library with no network listener and no stored state of its own: of Part I(2),
(a), (f), (j) and (k) apply to it as they apply above (`cargo deny`, the same
lints, `unsafe` forbidden, the fuzzed parsers), and the points about
configuration, access control, logging and removal are the integrating
product's. `rf2` is published under Apache-2.0, which makes it free and
open-source software under CRA Art. 3(48); the others are under the Business
Source License 1.1, as the FerroHEALTH book explains
([not free and open-source software](https://ferrohealth.eu/docs/cra.html#not-free-and-open-source-software)).

## Annex I Part II: vulnerability handling

Part II binds Cadasto B.V. Its policy is the same for every FerroHEALTH
product and is published in the FerroHEALTH book
([security](https://ferrohealth.eu/docs/security.html)); what is FerroTERM's
own is below.

| Point | How the manufacturer applies it for FerroTERM | Status |
|---|---|---|
| (1) Identify components; SBOM | a CycloneDX SBOM per release binary and an SBOM per image platform, each attested ([verifying releases](../operate/verifying-releases.md)); the dependency list embedded in each binary | met |
| (2) Remediate without delay; security updates apart from functionality | the family policy; a fix ships in the newest release | met as policy |
| (3) Regular security tests and reviews | weekly fuzzing, CodeQL, OpenSSF Scorecard, clippy with the reliability lints, `cargo deny`, the conformance suite | met |
| (4) Disclose fixed vulnerabilities | a GitHub security advisory for every fixed vulnerability (the family policy); none has been published yet | met as policy |
| (5) Coordinated vulnerability disclosure | the family policy, with FerroTERM's routes and scope in `SECURITY.md` | met |
| (6) Share vulnerability information; a contact address | GitHub private vulnerability reporting and the manufacturer's address | met |
| (7) Distribute updates securely | provenance, attestations, checksums, digest-pinned images, crates.io Trusted Publishing; the automatic limb does not apply | met |
| (8) Disseminate updates without delay, free of charge, with advisories | public releases; advisories under (4); whether "free of charge" holds for every commercial licence is a question for counsel the family book records | partial |

## Revisions

CRA Art. 13(3) has the assessment "updated as appropriate during a support
period". It is revised when a release changes an asset or a control in the
register, when a vulnerability or an incident shows a risk the register does
not name, and when the intended purpose changes.

| Version | Date | Change |
|---|---|---|
| 1 | 2026-10-21 | First assessment (#790) |
