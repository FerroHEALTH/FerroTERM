---
name: benchmark-record-instrument
description: A benchmark record needs a quiet machine; a build beside a server ruins its figures, and macOS RSS is unreliable under memory pressure
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A `ferroterm-bench` record set is only worth publishing when nothing else runs
on the machine. Measured 2026-09-08 while taking one:

- A build immediately before a server made the same artifact report 1101 MB
  resident instead of 831 MB. `ferroterm-bench` runs two passes now (every
  serving figure, then every build), and takes the median of five `ps`
  readings.
- With an IDE indexing and a Docker VM resident, macOS compresses enough that
  `ps` reports 155 MB for an edition holding 830 MB, and `/usr/bin/time -l`
  reports a build peak of 1.32 GB where the same build peaks at 2.57 GB.
- Docker Desktop gives the VM 8.2 GB; a SNOMED build peaks at 3.84 GB inside
  it, so the container run swaps and `$lookup` came out 556 µs and 304 µs
  twenty minutes apart for the same edition.

So: quit Docker Desktop, close the IDE, then run `bench/run.sh`. The open work
is [[milestone-autonomy]]'s #512.

`$lookup` latency is the size of its answer: a fixed ~70 µs plus ~4.5 ns per
byte, on every system. RxNorm looks slow because its concept carries 109
designations and 309 properties, which is 63 KB.
