<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The 2026-09-23 record set, Apple M2

Seven code systems, one record each, taken with the 0.1.4 release binaries
serving FHIR R4B. This is the set the README table, the book, and the two
landing pages are rendered from.

No FHIR or SNOMED CT specification governs a benchmark, so the conditions are
part of the result. Each record's `method` field states how every figure in it
was taken. This file states what the instruments cannot read: the condition of
the machine they ran on.

## The machine

Apple M2, 17.18 GB, macOS 26.5 on Darwin 25.5.0, aarch64, release profile.
Every record carries `"container": false`: the run was native, and no part of
it went through Docker.

## What ran on it, and what did not

- FerroTERM's own processes only. The harness measures every system's serving
  first and runs the builds after, so one `ferroterm` server is up at a time
  and no `ferroterm-build` ever shares a CPU with a latency measurement.
- No compiler, no test run, and no editor indexing while the run was going.
- Docker Desktop was installed and idle, with no container started.

`machine.load_average_1m` reads 7.34 in every record. The harness reads
`uptime` once, when it starts, and `bench/run.sh` builds the three release
binaries immediately before it, so that average is the compiler that had just
finished rather than a neighbour of the measurements. A one-minute average
decays across the minute that follows, and the serving pass ran after it. The
figures say the same thing: `$lookup` on the Dutch edition reads 222 µs here,
against 1.01 ms in the 2026-09-08 run taken beside an on-access scanner at 54%
CPU and an indexing IDE (#512).

## Whose memory the figures are

`rss_open_bytes` and `rss_warm_bytes` are the served server process and nothing
else: the median of five readings of that one process id, after it answered
`/health` and again after the warm requests. On macOS they come from
`footprint`'s `phys_footprint`, which counts the pages the kernel compressed
away. `ps -o rss=` does not count them, and read 379 MB and then 15 MB two
seconds later for a server holding the same structures, against a
`phys_footprint` of 896 MB throughout (#512); #554 changed the instrument, and
this is the first published set taken with it.

`ingest.peak_rss_bytes` is the peak of the `ferroterm-build` child over that
one system's release, from `/usr/bin/time`. `artifact_bytes` is the built index
on disk.

Neither figure includes the harness, the shell, or anything else on the
machine.

## What the latency figures are

One HTTP round trip per request, from the harness to the server on the same
machine, 200 warm requests per operation after one cold request, p50, p95, and
p99 nearest-rank over the warm ones. `answer_bytes` is the body the server
wrote for that request, which the served `$lookup` bar in `bench/bars.json` is
stated per byte of and `scripts/checks/served-bars.sh` checks against this set.
