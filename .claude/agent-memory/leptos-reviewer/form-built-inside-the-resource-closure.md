---
name: form-built-inside-the-resource-closure
description: a form whose RwSignal is created inside the resource-reading view closure is disposed by EVERY signal that closure reads, so a second resource read there discards what is being typed
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The alternative to [[seed-once-form-idiom]]: instead of creating the form's
signals in the setup function and seeding them, the editor creates the whole
`RwSignal<Draft>` **inside** the closure that reads the resource
(`app/ferroterm-console/src/pages/editor.rs`, `form_section` called from the
`<Transition>` child). A refetch then disposes the form and builds a new one
over what came back, which needs no seed guard at all.

The cost is that **form identity is now every dependency of that closure**, not
just the resource. Reading a second, unrelated `LocalResource` there (the
editor first read a derive over the capability statement, to decide whether the
form is read-only) joins that resource's notifications to the form's life:
whenever it resolves or refetches, the form is rebuilt and everything typed is
gone. #634 landed with that read moved into `readonly`, where it belongs. `Signal::derive` has no equality gate
([[resource-read-registers-suspense]]), so even a notification with the same
value rebuilds.

**Review rule:** in a closure that BUILDS form state, the only read allowed is
the resource the form is built from. Anything else is passed in as a
`Signal<T>` and read where it is used (inside `readonly`, inside a class), never
where the form is constructed.

Rule file: `.claude/rules/leptos-ui.md` §6 (async data) and §2 (reactivity);
the book chapter is `async/12_transition`.
