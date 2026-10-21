---
name: enum-width-sets-every-value
description: A Rust enum costs its largest variant for every value, so box the wide rare variants; this is the biggest single perf lever found in FerroTERM's answer path
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A Rust enum is as large as its largest variant, so one rare wide variant sets the
size every value of that enum pays, including the small common ones. Boxing the
wide variants moves them to the heap and leaves the enum as narrow as the
variants that are actually hot.

Cloudflare's write-up is the reference case:
<https://blog.cloudflare.com/dns-cache-memory-optimization-1111/>. Their
`RecordData` enum was 144 bytes because of a rare NAPTR variant, while A (4
bytes) and AAAA (16) were over 80% of traffic. Boxing the large variants, with
four other layout changes, cut per-entry memory 56% and raised insert throughput
43% while cutting lookup latency 19%. The rest of that post is worth re-reading
before any layout work: `Box<[T]>`/`Box<str>` over `Vec`/`String` for data that
is never mutated again (drops the 8-byte capacity field and the over-allocated
tail), one list with offsets instead of three lists, dropping a field that can be
inferred at read time, and storing already-encoded bytes when the read path just
copies them out.

Measured in FerroTERM on 2026-09-06 (#378): the generated `value[x]` open-type
enum admits `Dosage`, which carries a whole `Timing` and is 4512 bytes, so
`ParametersParameter` was 4736 bytes and a `$lookup` answer of 423 parameters
plus 945 nested parts memmoved megabytes per request. Boxing every non-primitive
choice variant in the emitter took `ParametersParameter` to 304 bytes and the
open type to 80. `served/rxnorm_lookup` went from about 630 us to 232 us and
`served/snomed_lookup` from about 300 us to 139 us.

Two lessons worth carrying:

- Check `size_of` before theorising. The issue's own hypothesis (the answer is
  built twice, so the clone is the cost) was wrong: a paired in-process bench put
  the moving and cloning conversions within noise of each other, and the width
  was the whole story. A one-test `panic!` printing `size_of::<T>()` settles it
  in a minute.
- Bound the width by construction, not by a threshold. The emitter boxes every
  complex variant and leaves primitives inline, so the enum stays narrow for
  every FHIR version without anyone re-tuning a number.

Related: [[performance-bar]], [[benchmark-program]].
