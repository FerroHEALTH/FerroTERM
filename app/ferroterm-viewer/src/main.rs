// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The `ferroterm-viewer` binary: a thin entry point over the library run
//! path.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    ferroterm_viewer::command::run(std::env::args())
}

// NOTE: no specification governs this: our own design; the browser half
// starts at `ferroterm_viewer::hydrate`, so the binary is empty on wasm32.
#[cfg(target_arch = "wasm32")]
fn main() {}
