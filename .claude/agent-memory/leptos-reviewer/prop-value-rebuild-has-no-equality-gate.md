---
name: prop-value-rebuild-has-no-equality-gate
description: prop:value with a reactive closure writes the DOM property on every notification, with no equality check, so a field seeded from a whole params memo is wiped by any unrelated navigation
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Verified first-hand in the pinned source (tachys 0.2.18).

`prop:value=move || …` builds `IntoProperty for F: ReactiveFunction`
(`tachys/src/reactive_graph/property.rs`), whose state is a `RenderEffect` that
calls `value.rebuild(&mut state, key)` on every re-run. The concrete `rebuild`
for a string property (`tachys/src/html/property.rs`, the `prop_type_str!`
arm) is:

```rust
fn rebuild(self, state: &mut Self::State, key: &str) {
    let (el, prev) = state;
    let value = JsValue::from(&*self);
    Rndr::set_property_or_value(el, key, &value);
    *prev = value;
}
```

`prev` is stored and never compared. So **every notification of the closure's
dependencies overwrites `input.value`**, including with a value that did not
change. There is no "only writes when it differs" behaviour to rely on.

**The consequence to look for in review** (`.claude/rules/leptos-ui.md` §5,
forms): a text input seeded with `prop:value=move || params.with(|p|
p.term.clone())`, where `params` is a memo over the WHOLE query map. Any other
parameter changing (the reader clicks a result, a version switches, a node
opens) re-runs the closure and discards what the reader was typing. Fixed in
`app/ferroterm-console/src/pages/browse.rs` (the search filter is seeded from a
memo over the one field); still open in the same shape in `pages/expand.rs`,
whose doc comment claims the opposite.

**The fix:** seed from a `Memo` over the ONE field
(`Memo::new(move |_| params.with(|p| p.term.clone()))`), because a memo has an
equality gate and does not notify when an unrelated parameter moves.
`Signal::derive` does NOT fix it: a derive has no equality gate
(see [[resource-read-registers-suspense]]).

Related: [[seed-once-form-idiom]] (the same class from the resource side).
