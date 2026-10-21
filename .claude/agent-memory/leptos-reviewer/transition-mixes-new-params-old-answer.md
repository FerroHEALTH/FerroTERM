---
name: transition-mixes-new-params-old-answer
description: under <Transition> the URL-derived params advance immediately while the resource still holds the previous answer, so a pager, a summary, or a live region built from params describes a page that is not on screen
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`<Transition>` keeps the previous children mounted while the resource
refetches, and a `LocalResource` keeps its previous value until the new future
resolves. Anything in that subtree that reads the ROUTE instead of the ANSWER
therefore updates a paint earlier than the rows do.

**The symptom to look for:** a page control, a "Concepts 21 to 40 of 45"
summary, or an `aria-live` sentence computed from `use_query_map` while the
table is still drawn from the previous answer. On the last page it announces a
range that does not exist ("Concepts 41 to 60 of 45"), then corrects itself, so
a screen reader hears the wrong number first.

**The fix:** derive everything the reader is told about a page from the answer
itself. `$expand` sends `expansion.offset` and the length of
`expansion.contains` (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>),
so the offset the server applied, not the offset the address asked for, is what
the summary and the pager should use; fall back to the requested offset only
when the server declares none.

Related: [[polled-resource-needs-transition]], [[resource-read-registers-suspense]]
