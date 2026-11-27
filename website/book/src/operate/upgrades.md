<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading

**Audience:** operators who move a running deployment to a new release.

Only the newest release receives fixes
([`SECURITY.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/SECURITY.md#supported-versions)),
so plan to take each release. The server, the build tool, the
synchronisation service and the console are released together under one
version; run them at the same version.

<!-- toc -->

## 1. Read what the release changes

Read the release's section of the
[changelog](https://github.com/FerroHEALTH/FerroTERM/blob/main/CHANGELOG.md).
Three entries change what you do:

- **A new artifact layout.** The entry says "every index must be rebuilt".
  A server refuses an artifact of another layout, naming the layout it
  found and the one it reads, so an old index never answers through a new
  server.
- **A removed or renamed configuration key.** The entry names the key and
  what replaces it. The new server refuses to start on the old key, naming
  it.
- **A `### Security` section.** It names the advisory each fix closes.

A release inside the current line (`0.4.x`) keeps the promised surfaces
([What stable means](../evaluate/what-stable-means.md)); a new line (`0.5.0`)
may change them, and its changelog says how.

## 2. Verify the release

Check the provenance and the checksum of the tarball or the image before you
run it, as [Verifying releases](verifying-releases.md) shows. Pin the image
by digest in production.

## 3. Check the configuration with the new binary

Run the new binary's `config check` against the configuration you deploy, on
the host or in the image, before you switch:

```console
$ ferroterm config check --config /etc/ferroterm/ferroterm.toml
$ docker run --rm -v /etc/ferroterm:/etc/ferroterm:ro \
    ghcr.io/ferrohealth/ferroterm:<version> config check --config /etc/ferroterm/ferroterm.toml
```

It reads the file, the `FERROTERM__` variables and `--set` the way the server
will, prints every key with its value and where it came from, and exits 78
(`EX_CONFIG`) on a configuration the server would refuse. Run
`ferroterm-sync config check` and `ferroterm-console config check` the same
way when you run those services.

## 4. Rebuild the indexes when the layout moved

When the changelog announces a new layout, build every index again with the
new `ferroterm-build` before you switch the server: build, then switch. A
build writes a new directory and the running server keeps reading the old
one, so the two never meet. With an index root, build each release into a
directory outside the root and rename it in; with fixed paths, build beside
the old directory and rename the new one over it
([Reloading and replacing content](reloading.md)).

Plan the time and the memory for it: the build of a SNOMED CT edition is the
expensive step, and [Benchmarks](../evaluate/benchmarks.md) records how long
each release takes and how much memory it peaks at.

When the layout did not move, the indexes you serve keep working and you
skip this step.

## 5. Switch

- **Compose.** Download the new release's `compose.yaml`, which pins its image
  tag, and run `docker compose up -d --wait`. The server's health check
  decides when it is up.
- **Container.** Change the image reference to the new digest and roll the
  deployment. The readiness probe (`GET /health/readiness`) answers `200`
  only once every index is open.
- **Binary.** Replace `ferroterm`, `ferroterm-build` and `ferroterm-sync`
  with the new tarball's and restart the service.

The resource database that `terminology.resources` names carries over: the
new server opens the same file and serves what clients wrote. Stop the old
server before the new one opens it, because one `redb` file has one writer.

## 6. Check that it serves

```console
$ curl http://localhost:8080/health/readiness
$ curl 'http://localhost:8080/r4/metadata?mode=terminology'
```

The `CapabilityStatement` names the new version as `software.version`, and
`TerminologyCapabilities` lists every code system version the deployment
serves. A server that refused an index did not start; its log names the
artifact and the reason in a `cannot start` line.

## Rolling back

Keep the previous release's binaries or image digest, and its indexes when
you rebuilt them, until the new release has served for a while. Rolling back
is the same switch in the other direction, with the old indexes: a server
refuses an index built for another layout.
