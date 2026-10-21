<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-server

The `axum` HTTP server: FHIR endpoints, content negotiation, and runtime
routing across the served FHIR versions. Hand-written; the FHIR RESTful API
and operations framework for the served version are the authority
(`.claude/rules/fhir-terminology.md`).

- `main.rs` stays thin: configuration in, `ferroterm_server::serve` out. Every
  behaviour lives in `lib.rs` so `tests/it` can drive the router with
  `tower::ServiceExt::oneshot` and no socket.
- `anyhow` is allowed in `main.rs` only; the library returns typed errors.
- An HTTP status is a `StatusCode`, compared as one, never as a number
  (`.claude/rules/rust-style.md`).
- Client input errors become an `OperationOutcome`, never a 500; a panic
  unwinds into a clean 500 (`.claude/rules/reliability.md`).
- Never hold a `std::sync` lock across an `.await`; blocking work goes through
  `spawn_blocking`.
