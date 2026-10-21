---
name: select-needs-selected-on-every-option
description: a select driven by prop:value alone shows its first option when the options arrive after the form is built, so every option states its own selected and both mechanisms are needed
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A coded control fills its `<select>` from a `ValueSet/$expand` that resolves
after the form is built. Driving the select with `prop:value` alone is not
enough, and the failure is silent: the select shows the first option, and the
next save writes it over what the resource carried. The browser journeys in
`e2e/tests/it/concept_map_editor.rs` caught it on a saved map whose target read
back as `relatedto` instead of `equivalent` (#636).

`prop:value` and a per-option `selected` attribute cover different cases and do
not fight:

- **Build order is attributes then children.** `prop:value` lands with no
  options present. The options then arrive carrying `selected`, and a freshly
  created option has dirtiness false, so the content attribute sets its
  selectedness and insertion runs "ask for a reset", which keeps the last
  option whose selectedness is true
  (<https://html.spec.whatwg.org/multipage/form-elements.html#the-select-element>).
  That is the expansion-lands case, and only the attribute can fix it: `held`
  does not change when the codes resolve, so the `prop:value` effect never
  re-runs ([[prop-value-rebuild-has-no-equality-gate]]).
- **An option the reader picked has dirtiness true**, so a later `selected`
  attribute change is ignored on it. `prop:value` is what corrects the select
  then, which is why dropping it reintroduces a different bug.
- **The option list is a plain `Vec<AnyView>`**, so it rebuilds positionally
  ([[for-retained-key-never-rebuilds]]): dropping the fallback option shifts
  every `value` by one into a reused element. Re-stating `selected` on every
  option on every render is what makes that converge.
- **The fallback option** (the code the resource carries that the expansion did
  not offer) is pushed first and keeps `selected=true`, so a code the server
  did not expand still wins and is never silently rewritten.

The cost is that the whole option list rebuilds whenever `held` changes, so
`held` wants to be a `Memo` rather than a plain derive over the form's draft
([[memo-source-gates-a-resource-refetch]], [[prop-value-caret-only-moves-on-change]]).

**Review consequence:** a `<select>` whose options arrive asynchronously needs
both. Ask for the `selected` attribute on every option, and ask what the
control shows before the expansion lands.
