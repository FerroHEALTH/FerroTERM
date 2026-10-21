<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# fhir-terminology

The engine. Hand-written; the FHIR terminology module for the served version
and each code system's FHIR page (`terminologies-systems.html`) are the
authority (`.claude/rules/fhir-terminology.md`, `snomed-terminology.md`).

- The operations talk to the code system provider seam only
  (`docs/architecture.md` §5). Nothing in an operation branches on "is this
  SNOMED"; a per-system behaviour (an implicit value set form, a filter, a
  property) is a provider capability the system declares.
- Consume the generated `fhir-types` types directly; never re-model FHIR here.
  A missing shape is a generator fix upstream (`.claude/rules/codegen.md`).
- One operation, one module; the parameter set an operation accepts is what
  the version's `OperationDefinition` declares, and a version difference is a
  generated difference, never a hand-written conditional.
- Every failure is a typed error the server maps to an `OperationOutcome`;
  a client input error is never a panic.
- Tests cite the FHIR operation page or the code system clause they assert.
