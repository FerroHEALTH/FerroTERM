---
name: prop-value-caret-only-moves-on-change
description: a driven prop:value input does not move the caret when the model writes the same string back, so the caret argument for seeding with value= is wrong
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The HTML Standard's `input.value` setter, mode "value", sets the value and
then, "if the element's new API value is different from its old API value, move
the text entry cursor position to the end of the text control"
(<https://html.spec.whatwg.org/multipage/input.html#dom-input-value>).

So `prop:value=move || held.get()` plus an `on:input:target` that writes the
typed string straight back is caret-safe: the closure re-runs, tachys writes
the property with no equality check of its own
([[prop-value-rebuild-has-no-equality-gate]]), and the browser compares the
strings and leaves the cursor alone. Typing in the middle of a word is safe for
the same reason.

**The caret DOES move** when the value written back differs from what the
control holds: a trim, a case fold, a normalisation, or a value arriving from
an async round trip after more characters were typed.

**Review consequence** (`.claude/rules/leptos-ui.md` section 5,
`view/05_forms`): `prop:value` is the rule's own idiom and needs no caret
excuse. A control seeded with the `value` attribute instead is the deviation,
and it costs the field every write that does not come from the field itself (a
picker, a fill, a reset). The seeded controls in
`src/pages/editor.rs` argue the opposite; the argument is
wrong, and the seeded controls are the ones to question.
