---
name: nts-sync-program
description: Owner decisions of 2026-09-22 for the NTS synchronisation program (separate service, addons/ named after the service, native scheduler, one way, milestone v0.1.4)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-09-22 the owner decided how FerroTERM takes content from the Nictiz Nationale Terminologie Server (NTS), after the PvE review of the filled Excel:

- The sync is a separate service, `app/ferroterm-sync`, never inside the server. The server only gains a reload path (#578).
- Sources are add-ons under a top-level `addons/` dir, named after the service the way its operator writes it (`nts`, later `ncts`, `nzhts`, `mlds`, `infoway`), never after a country. NTS first; the `Source` seam in `crates/syndication` (#580) must make a second source a new crate plus one registration line.
- One direction only, NTS into FerroTERM. Never delete, never touch the local write store; national FHIR resources go to a managed read-only directory, not the write API.
- A native tokio scheduler with a manual trigger; the owner refused cron.
- Milestone v0.1.4 (number 18) is reserved for this program; the six items that were there moved to v0.1.5 (number 19).
- The owner has no NTS account yet; the feed format per system (RF2 or Ontoserver binary index) stays open until then. The NTS carries SNOMED, LOINC and the rest.

**Why:** a colleague's review of the PvE said the sync must be a plug-in or a separate service, and the owner agreed; the owner wants the same shape reusable for other national services.

**How to apply:** file and build sync work under #579 and its children; keep FerroTERM code-system-neutral and country-neutral; name any new source after its service. See [[multi-version-program]] for the earlier program shape and [[repo-merge-gates]] for landing.
