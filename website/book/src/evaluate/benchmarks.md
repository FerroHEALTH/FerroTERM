# Benchmarks

Every figure FerroTERM quotes about speed or footprint comes from a record the
benchmark harness wrote, never from a number typed by hand. A record names the
machine, the FerroTERM version, the code system release, and the method beside
every figure, so you can rerun it and compare.

<!-- toc -->

## What a record holds

For one code system and one run, `ferroterm-bench` writes a JSON record with:

- the machine (operating system, architecture, CPU, memory) and the FerroTERM
  version;
- the code system, its version, its concept count, and the artifact's size on
  disk;
- the ingest: wall time around `ferroterm-build` over the release, and the
  build's peak resident memory (from `/usr/bin/time`), when the release is at
  hand;
- the time from starting the server until `/health` answers, and the server's
  resident memory after start and after the warm requests, the median of five
  readings of that one process (`footprint`'s `phys_footprint` on macOS, which
  counts the pages the kernel compressed away, and `ps -o rss=` elsewhere,
  where the resident set already counts them);
- per operation (`$lookup`, `$validate-code`, `$subsumes`, a small and a large
  `$expand`, and a designation search through `filter`): the first request
  cold, and the nearest-rank p50, p95, and p99 over the warm requests that
  follow, all as HTTP round trips from the same machine;
- the comparison field, which states a reference server's numbers taken on the
  same machine over the same release with its configuration, or says the
  comparison was not run.

## Reproduce a record

The harness runs over releases and artifacts you are licensed for; the
repository ships none. Build the artifacts as the [loading
page](../operate/loading-snomed.md) shows, put their paths in
`bench/systems.json`, and run:

```console
$ bench/run.sh
```

`bench/run.sh --skip-ingest` measures the artifacts as they are without
rebuilding them; `--only LOINC` restricts the run to systems whose name
contains the text. Records land under `bench/results/`, one file per system
and run, named by the system and the timestamp. Close other work on the
machine during a run; the latency of a request that shares a CPU with a
compiler is the compiler's number, not the server's.

## Run in a container

A record taken in the same container image is comparable across machines and
free of whatever else a workstation has installed. `bench/compose.yaml` builds
the server, the build tool, and the harness from the checkout on digest-pinned
Debian images, mounts `data/` and `artifacts/` read-only, and writes the records
to `bench/results/`:

```console
$ docker compose -f bench/compose.yaml build
$ docker compose -f bench/compose.yaml run --rm bench --skip-ingest --only LOINC
```

The record marks a container run (`machine.container`), and on macOS or Windows
the machine it names is the Docker virtual machine, with that machine's CPU
count and memory, so a container record from a laptop reads slower than a native
one on the same hardware. The README table says which kind it shows.

## Units, and what a record refuses

The records store raw numbers: milliseconds for latency, bytes for memory and
disk, seconds for ingest and time to ready. Everything rendered from them (the
console summary, the README table) uses the unit that fits the value: seconds,
milliseconds, or microseconds for time; GB, MB, or KB for bytes. A request that
answers anything other than a 2xx status fails the run for that system and no
record is written, so a record never holds the latency of an error response. A
cell reads `n/a` when the system defines no such operation, for instance a
code system with no whole-system value set to expand.

## The published figures

The README table, the figure tiles on the landing page, and the tables on the
site's [benchmarks page](https://ferroterm.eu/benchmarks.html) are rendered
from one committed record set, a directory under `bench/records/` named by the
date and the machine, by `scripts/checks/bench-table.sh render <target>`; CI
runs `check` and fails when any of the three and the records disagree. Every
record in a set comes from the same machine, FerroTERM version, and FHIR
version, or the renderer refuses the set. A new set is a new directory copied
from `bench/results/` by hand, so every published number names the run it came
from and links to its record.

## The current record set

`bench/records/2026-09-23-apple-m2/`, taken on 2026-09-22 with the 0.1.4
release binaries on an Apple M2 with 17.18 GB, natively rather than in a
container. Nothing else of the owner's ran while it was taken: no compiler, no
test run, no editor indexing, and Docker Desktop idle with no container
started. FerroTERM's own processes are the only ones in the figures, one
server at a time, and the harness measures every system's serving before it
runs any build, so no build shares a CPU with a latency measurement.

The resident and peak-build figures are the server's and the build tool's, not
the machine's. Resident memory is the median of five readings of the one
server process, read with `footprint` so the pages macOS compressed are
counted; peak build memory is the `ferroterm-build` child's peak over one
system's release, from `/usr/bin/time`. The set's own
[README](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/README.md)
carries the machine state in full, including what the one-minute load average
in each record is an average of.

<!-- bench-table:begin -->
| Code system | Release | Concepts | Build | Peak build memory | Index on disk | Resident | `$lookup` | `$validate-code` | `$subsumes` | `$expand` (small) | `$expand` (large) | Search | Snowstorm |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| [ICD-10-CM](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/icd-10-cm-2026-09-22T23-23-19-858381Z.json) | 2026 | 98,827 | 1.51 s | 359 MB | 44 MB | 91 MB | 105 µs | 60 µs | 58 µs | n/a | n/a | n/a | not run |
| [ICD-10-NL](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/icd-10-nl-2026-09-22T23-23-22-144431Z.json) | 2021 | 42,769 | 880.52 ms | 221 MB | 21 MB | 66 MB | 96 µs | 60 µs | 64 µs | n/a | n/a | n/a | not run |
| [ICD-11 MMS](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/icd-11-mms-2026-09-22T23-23-27-084934Z.json) | 2026-01 | 37,211 | n/a | n/a | 44 MB | 99 MB | 138 µs | 59 µs | n/a | 129 µs | 116 µs | 139 µs | not run |
| [LOINC](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/loinc-2026-09-22T23-23-17-47383Z.json) | 2.83 | 257,266 | 8.15 s | 2.54 GB | 310 MB | 440 MB | 136 µs | 56 µs | n/a | 126 µs | 7.23 ms | 283 µs | not run |
| [RxNorm (prescribable subset)](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/rxnorm-prescribable-subset-2026-09-22T23-23-24-615167Z.json) | 09082026 | 81,468 | 4.58 s | 667 MB | 77 MB | 133 MB | 388 µs | 114 µs | n/a | n/a | n/a | n/a | not run |
| [SNOMED CT (International edition)](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/snomed-ct-international-edition-2026-09-22T23-23-13-36281Z.json) | 20260901 | 535,502 | 14.12 s | 3.06 GB | 559 MB | 943 MB | 217 µs | 66 µs | 59 µs | 215 µs | 1.7 ms | 353 µs | not run |
| [SNOMED CT (Netherlands edition)](https://github.com/FerroHEALTH/FerroTERM/blob/main/bench/records/2026-09-23-apple-m2/snomed-ct-netherlands-edition-2026-09-22T23-23-10-140124Z.json) | 20260630 | 548,949 | 17.66 s | 3.88 GB | 864 MB | 1.14 GB | 222 µs | 66 µs | 59 µs | 285 µs | 2.28 ms | 817 µs | not run |

Warm p50 over 200 HTTP round trips on one machine (Apple M2, 17.18 GB, macos/aarch64), FerroTERM 0.1.4 serving FHIR R4B, taken 2026-09-22. The records are under `bench/records/`; the [benchmarks page](https://ferroterm.eu/benchmarks.html) has the method, the cold and tail latencies, and how to reproduce a record.
<!-- bench-table:end -->

## Reading the numbers

A cold request is the first of its kind on a freshly started server, before
`redb`'s page cache and the query caches hold anything for that path; the warm
percentiles are what a busy server answers. The resident memory is what the
server has read in: the index is loaded at startup, which is why the figure
after start and the figure after the warm requests are close. The design target
for point reads is under a millisecond in the release profile on the Dutch
edition; a millisecond-scale figure is a measurement to improve, never a result
to call fine.

`$lookup` reads slower on some systems than on others because its answer is
larger, not because the read is. Across the seven systems in the set, its warm
p50 fits a fixed 98 µs plus 4.6 ns for every byte of the answer, from a 862
byte ICD-10-NL answer at 96 µs to a 63,134 byte RxNorm answer at 388 µs: the
same rate on every system, and RxNorm is the largest answer rather than the
slowest read. The bar the project holds it to has that shape, `100 µs +
6.0 ns/byte` in `bench/bars.json`, and `scripts/checks/served-bars.sh` checks
every record of the published set against it.
