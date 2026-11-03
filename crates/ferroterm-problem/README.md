<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-problem

The RFC 9457 problem document (`application/problem+json`) that the
FerroTERM server and the sync service write for every error outside the FHIR
bases: the base URI of the problem types, the registry a service builds its
types into, the access refusals both services share, and the responses.

Each service defines its own problem types and documents them on the book page
under `BASE`. The crate is consumed inside this workspace only.
