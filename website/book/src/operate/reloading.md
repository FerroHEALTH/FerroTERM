<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Reloading and replacing content

**Audience:** operators who add, replace or retire a code system release
on a running server.

<!-- toc -->

## Reloading the served set

The server reads the directories again and swaps the whole served set on two
triggers:

- `SIGHUP` to the process (`kill -HUP <pid>`, `docker kill --signal=HUP <name>`).
- `POST /reload` on the admin listener, when `admin.listen` names an
  address.

```console
$ curl -sS -X POST http://127.0.0.1:8081/reload
{"outcome":"ok","systems":[{"id":"snomed.info-sct-…","system":"http://snomed.info/sct","version":"…"}]}
```

A reload opens every artifact in `terminology.index` and every resource in
`terminology.code_systems` again, read-only, builds a new registry, and exchanges
a pointer. A request already in flight finishes on the set it started on, and
that set is dropped when the last request holding it ends, so the memory of the
swapped system is resident twice for a few seconds. `terminology.resources` is
live already and is carried over untouched: the resources clients wrote stay
written, and the new set serves them over the new artifacts.

An artifact is replaced by building it in a new directory and renaming that
directory over the configured path, which is one atomic step and leaves the
old files in place for the handles still reading them. Writing into the
directory the running server is reading corrupts what it reads. A directory in
`terminology.code_systems` is different: add or remove a resource file in it, and
the reload picks up the change.

Adding or removing an artifact PATH means changing `terminology.index`, which
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

The server reads each path in `terminology.index` in one of two ways:

- **An artifact path** holds a `manifest.json` of its own. The server opens
  that directory as one code system version.
- **An index root** holds no `manifest.json`. The server lists it and opens
  every child directory that holds one, in the order their names sort. A child
  without a manifest is passed over, as is a file beside the children.

```text
/srv/ferroterm/snomed-nl/        terminology.index names this root
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
finished. `ferroterm-build --index-root` writes straight into a root, one child
per source.

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
with a `500` [`reload-failed`](problems.md#reload-failed) problem naming the artifact. A half-applied release never reaches a
client. A child directory under an index root is an artifact like any other
here: once it holds a manifest the server cannot read, it refuses the reload
rather than serving the root without it.
