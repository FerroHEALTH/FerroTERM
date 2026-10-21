<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-bench

The benchmark harness: one record per code system and run, from the
owner-licensed releases and artifacts under `data/` and `artifacts/`, never
committed. Hand-written; no FHIR or SNOMED specification governs a benchmark,
so every number carries its method.

- A record names the machine, the FerroTERM version, the code system release,
  and the method beside every figure; a figure without its conditions is not
  reported.
- Ingest is timed around `ferroterm-build` as a child process, its peak memory
  read from `/usr/bin/time`; the server is started as a child process and its
  memory sampled through the crate's `memory` module, which reads
  `footprint`'s `phys_footprint` on macOS and `ps -o rss=` elsewhere; latency
  is measured over HTTP with a fixed request set, cold (the first request) and
  warm (percentiles over the rest).
- Both binaries read memory from that one module. `ps -o rss=` on macOS counts
  only the pages that are resident and uncompressed, so a second reader over
  it understates a served edition by an order of magnitude (#322).
- `ferroterm-residency --report` loads a whole edition and prints what each
  structure reports it holds, from the structures' own `size_in_bytes`, beside
  the process footprint; the difference is the residual, and the caller names
  it rather than leaving it as a gap.
- Reference-server comparisons run on the same machine over the same release
  or are recorded as not run; nothing is typed by hand.
