---
name: unused-dep-weight-is-already-gone
description: LTO plus --gc-sections removes a dependency the viewer never calls, so "an unused crate is most of the bundle" is almost always false; the weight is the library the viewer does call
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Measured 2026-09-07 with `twiggy` 0.8.0 over
`target/wasm32-unknown-unknown/wasm-release/ferroterm-viewer.wasm`, the module
before `wasm-opt` strips the name section, aggregated by the crate that owns
each item, and confirmed by an A/B rebuild of the gzipped `dist/` bundle.

The viewer had carried a claim, never measured, that `chrono` and `icondata_ai`
were most of its wasm weight because `thaw` pulled them in and has no
per-widget features. **The claim was false.** In a 239,430-byte gzipped bundle:

- `chrono`: **0 bytes.** No item in the module belongs to it.
- `icondata_ai`: **0 bytes.** The only trace was 2,265 bytes of
  `reactive_graph` glue monomorphized over `Option<&icondata_core::IconData>`,
  the type of `thaw::Button`'s icon prop.
- `palette`, `pure-rust-locales`, `num-traits`: **0 bytes each.**

`[profile.wasm-release]` sets `lto = true` and `codegen-units = 1`, and the
linker drops an unreferenced section, so a dependency nothing calls is not in
the artifact. What the linker cannot drop is a library the code does call:
`thaw` itself was 34,997 bytes of its own code plus the instantiations it
induced, 37,155 gzipped bytes of the shipped bundle, 15.5%.

**The rule this gives a reviewer.** Refuse an unmeasured claim about what is
heavy in the bundle. `cargo tree` shows what resolves, never what ships. The
question is what the viewer *calls*, and the answer comes from `twiggy` plus a
rebuild without the suspect, in that order. `docs/viewer.md` §12 records the
method as the path a breaching slice follows.
