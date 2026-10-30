// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The `ferroterm-console` binary: a thin entry point over the library run
//! path.

fn main() -> std::process::ExitCode {
    ferroterm_console::command::run(std::env::args())
}
