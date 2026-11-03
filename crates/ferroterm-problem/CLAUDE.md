<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-problem

The problem document of the server and the sync service. Hand-written; RFC
9457 (<https://www.rfc-editor.org/rfc/rfc9457>) is the authority, and the
type registry and the response come from `ferrohealth-server::problem`.

- One owner for the document: the base URI, the registry, the access kinds
  both services refuse with, and the responses. A service keeps only the
  problem types that are its own.
- A detail is a fixed sentence and quotes nothing from the request (§5).
- No FHIR here: the FHIR bases answer with an `OperationOutcome`, which the
  server writes.
