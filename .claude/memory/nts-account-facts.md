---
name: nts-account-facts
description: the owner's Nictiz NTS account (since 2026-09-25), where its credentials live, what its licence scope shows, and what the live feed and FHIR API carry
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner has a Nictiz Nationale Terminologieserver account since 2026-09-25. Credentials sit in `~/.ferroterm-nts.env` (mode 600, `FERROTERM_NTS_USERNAME` / `FERROTERM_NTS_PASSWORD`); source it per Bash call, never write a token to disk (the auto-mode classifier refuses that). Live outputs go under `~/ferroterm-nts-live/`, never in the repo.

What the account showed (record on #602): the syndication feed carries only SNOMED CT NL binary indexes (identifier `http://snomed.info/sct`, edition in the version URI); all other content is behind the FHIR API (Ontoserver 6.25.4, R4). SNOMED and ICD-10-NL lookups work; LOINC and the NHG tables are hidden (404 naming the current version) until Nictiz grants those licences. Tokens: access and refresh both 24 h. Discovery URL 301s to the Keycloak document.

**Why:** the owner assumed NHG access came with the account; it needs a separate NHG licence request, and the sync needs a FHIR API lane (#692) before NHG or the Labcodeset can arrive.

**How to apply:** before any NTS-related work, check whether the LOINC and NHG licences were granted (`$lookup` on `http://loinc.org` 2345-7 and `http://hl7.org/fhir/sid/icpc-1-nl` T90), and treat #692 as the gate for #241. See [[nts-sync-program]].
