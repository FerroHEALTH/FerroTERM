---
name: narrow-model-whole-resource-put
description: the viewer's "deserialize only what you render" rule plus a whole-resource PUT silently deletes every element the form does not model, including CodeSystem.concept.concept
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`.claude/rules/leptos-ui.md` §0 requires the viewer to deserialize only the
fields it renders and never to mirror a whole FHIR resource. A FHIR `update`
replaces the resource in full (<https://hl7.org/fhir/R4B/http.html#update>).
The two together are a data-loss defect the moment an authoring screen sends a
body it rebuilt from its own narrow model: every element the model does not
carry is deleted from the stored resource, with nothing on screen saying so.

Caught in review of #634, in `app/ferroterm-viewer/src/editor.rs`
(`Draft::of` / `Draft::body`), whose model carries `url`, `version`, `status`,
`content`, `caseSensitive`, `property`, and a FLAT `concept` list. A save would
have dropped `name`, `title`, `description`, `identifier`, `hierarchyMeaning`,
`valueSet`, `supplements`, `filter`, `text`, every `meta` element but
`versionId`, and `concept.concept`, the code system's whole hierarchy. It
landed with the fix below.

**The fix that keeps both properties:** hold the read resource's raw
`serde_json::Value` beside the model and merge the authored elements into it on
save. A passthrough of bytes the screen never interprets is not a model of the
resource, so §0 still holds.

**Review test:** the round-trip unit test must carry at least one element the
form does not model and assert it survives `Draft::of` then `Draft::body`. A
fixture built only from modelled elements cannot fail.
