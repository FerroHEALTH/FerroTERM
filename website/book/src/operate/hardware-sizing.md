<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Hardware sizing

FerroTERM is built to run an edition on ordinary hardware. This page says
where the memory goes and how to size a machine; the measurements themselves
(build time and peak build memory per release, the index on disk, the server's
resident memory, the latency of every operation) are on the
[benchmarks page](../evaluate/benchmarks.md), rendered from the committed
records so no figure on this site is typed by hand. Your numbers vary with the
edition and the machine; the shape below holds.

<!-- toc -->

## What the records show

The resident figure is what the server has read in and holds. The index is read
at startup, not paged in as requests arrive, so the figure is at its full size
by the time `/health` answers 200; the records carry the resident memory at
that point and again after the warm requests, and the two are close. Size the
machine for that figure. The build is the expensive step: it holds a whole
release in memory while it computes the transitive closure, so peak build
memory is several times the size of the finished index and is the figure to
size a build machine by. That peak lands in the write phase, where the rows
the read pass produced, the rows the store builder holds for its one write
transaction, and the word dictionary under construction are alive together.
The build also reads and writes on every core, so size a build machine for
cores as well as memory. The
design target for point reads is under 1 ms measured over HTTP on the same
machine; a millisecond-scale figure in a record is tracked as a performance
issue, not called done.

## Where the memory goes

Every structure a served edition holds reports what its own allocations hold,
and `ferroterm-residency --report` prints those counts beside the process
footprint, so the accounting adds up from the structures rather than being
inferred from a total. The two SNOMED editions below were measured on an
Apple M2 with the release binaries. The top rows are the structures' own
counts; the bottom three are the process that loaded them and nothing else, so
they run under a serving process, which also holds the FHIR core code systems
and the HTTP runtime. The benchmarks page reports the serving process, from
whichever record set is committed there.

| Structure | On disk (NL) | Resident (NL) | Resident (International) |
|---|---|---|---|
| Concept records, a dense column by ordinal | `store.concepts.col` 20.0 MB | 20.0 MB | 19.4 MB |
| Preferred displays, a dense column by ordinal | `store.displays.col` 73.5 MB | 73.5 MB | 55.6 MB |
| Properties, a dense column by ordinal | `store.properties.col` 62.9 MB | 62.9 MB | 61.1 MB |
| Acceptability, a dense column by ordinal | `store.acceptability.col` 49.8 MB | 49.8 MB | 37.1 MB |
| Transitive closure, both directions, roaring bitmaps | part of `hierarchy.bin` | 150.6 MB | 143.6 MB |
| CSR is-a adjacency | part of `hierarchy.bin` | 4.9 MB | 4.7 MB |
| CSR child adjacency, transposed when the edition opens | derived | 4.9 MB | 4.7 MB |
| `fst` word dictionary and roaring postings | `text.bin` 76.6 MB | 115.7 MB | 69.1 MB |
| Reference set member tables | `members.bin` 35.2 MB | 60.0 MB | 57.0 MB |
| Attribute rows | `attributes.bin` 12.5 MB | 12.5 MB | 11.9 MB |
| Attribute inverted index, derived when the edition opens | derived | 4.9 MB | 4.7 MB |
| Reference set membership bitmaps | `refsets.bin` 0.7 MB | 0.7 MB | 0.6 MB |
| Alternate identifiers | `identifiers.bin` | 0 MB | 0 MB |
| **Every structure** | | **560.4 MB** | **469.5 MB** |
| Unattributed: the `redb` page cache, the allocator, the binary, the runtime | | 55.1 MB | 49.6 MB |
| **The process, measured** | | **615.5 MB** | **519.0 MB** |

The designation text is the one part of an edition that stays on disk: it is
the largest thing an artifact holds and a point read through `redb` answers it
in microseconds, so it is read per request rather than held. Everything else
above is resident because a read reaches it: `$lookup` reads the displays and
the properties, `$expand` reads the concepts and the closure, `$subsumes` is a
membership test on the closure, ECL reads the attributes and the membership
bitmaps, and a `filter` search reads the word dictionary. Nothing in the list
is loaded and never touched.

The unattributed remainder is worth naming. Each column is its own file, read
once into the structure it is served from, so nothing of it arrives twice; the
`redb` page cache is capped at 64 MiB and fills only with the designation rows
a request reads, against a default of a gibibyte. What is left is the
allocator's slack, the binary, and the async runtime.

Both directions of the closure are stored on purpose: subsumption needs one
direction, and a descendant set is returned directly from the other. Roaring
compresses SNOMED-shaped sets heavily, which is why the closure of half a
million concepts fits in hundreds of megabytes rather than the gigabytes a
plain bitset would need.

## Disk and CPU

Size the disk for the indexes you load; the benchmarks page gives each index's size. The
offline build is the CPU-heavy step, once per release, in `ferroterm-build`.
Serving is a point read or a bitmap operation, so a modest CPU handles it: a
2 to 4 GB box with two cores serves an edition beside other services. A paged
`$expand` is cut from the selection before any concept is read, so the
concepts a page reads are the concepts it returns, not the whole selection;
an unpaged expansion beyond 1,000 members is refused with `too-costly`.
Finding the page still costs something on a large selection: the committed
records put a hundred members of a SNOMED subtree at 253 microseconds and a
thousand at 2.5 milliseconds on the Netherlands edition.
