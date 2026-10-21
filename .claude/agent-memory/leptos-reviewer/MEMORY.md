<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Memory index: leptos-reviewer

Ported from the sibling FerroEHR viewer programme, where each entry was
confirmed against pinned crate source or a live gate failure. **The FerroTERM
viewer is client-side rendered** (`docs/viewer.md` §1), so every entry carries a
`csr` classification in its front matter: `still-applies`, `changed`, or
`moot`. A moot entry is kept for the record, never deleted, because the fact it
records is true and the reasoning is reusable.

**The viewer no longer depends on `thaw`** (2026-09-07, #454): it used three
widgets and paid 15.5% of the bundle for the library. Every `thaw` entry below
is now moot on that ground as well as the one recorded in it; each is kept
because what it records is true and the method is reusable.

## Confirmed here

- [A select needs `selected` on every option](select-needs-selected-on-every-option.md):
  `prop:value` alone shows the first option when the codes arrive after the
  form is built; both are needed, and they cover different cases
- [Unused dependency weight is already gone](unused-dep-weight-is-already-gone.md):
  LTO plus `--gc-sections` drops a crate the viewer never calls, so `chrono`
  and `icondata_ai` were 0 bytes of the bundle; refuse an unmeasured claim
  about what is heavy
- [`<Show>` children re-run is cleaned up](show-children-rerun-is-cleaned-up.md):
  rebuilding a form's `NodeRef`s inside `<Show>` leaks nothing, and
  `use_navigate()` there is a plain context read; a resource there is the defect
- [`$translate` spelling per version](translate-spelling-per-version.md): the
  request and `match.part` names for R4/R4B/R5/R6, and which names come from the
  ecosystem overlay rather than the vendored `OperationDefinition`
- [`SelectedVersion` is gated](selected-version-derive-is-gated.md): it derives
  from a `use_query` `Memo`, so it is a safe resource source despite being a
  `Signal::derive`; do not file the ungated-derive finding against it
- [An `sr-only` "unavailable" is colour-only](sr-only-unavailable-is-colour-only.md):
  a disabled pager step whose only visible cue is hue fails WCAG 2.2 SC 1.4.1;
  fixed in `listing.rs`, still open in `pages/expand.rs`
- [`prop:value` rewrites on every notification](prop-value-rebuild-has-no-equality-gate.md):
  tachys 0.2.18 sets the property with no equality check, so a field seeded
  from a whole-params memo is wiped by an unrelated navigation
  (`leptos-ui.md` §5)
- [Tree roving tabindex and selection keys](tree-roving-tabindex-and-selection-keys.md):
  the flat `aria-level`/`posinset`/`setsize` shape is sanctioned; the tab stop
  desyncs from real DOM focus without a focus listener, and selection keyed by
  code marks two rows in a poly-hierarchy (`leptos-ui.md` §9)

## Still applies, unchanged

- [Polled resource needs Transition](polled-resource-needs-transition.md): an
  interval-refetched or filter-refetched resource read under `<Suspense>`
  flashes its fallback; use `<Transition>`
- [A resource `.with()` read does suspend](resource-read-registers-suspense.md):
  the read registers with the `SuspenseContext` from context; a `Signal::derive`
  resource source has no equality gate, so an unrelated notify refetches
- [Transition mixes new params with the old answer](transition-mixes-new-params-old-answer.md):
  a pager, summary, or live region computed from the URL describes a page the
  table is not showing yet; derive it from `expansion.offset` instead
- [`resolve: false` under a router base](navigate-resolve-false-under-base.md):
  `use_navigate` with the default options prepends `/ui` a second time, so an
  absolute in-app address must opt out of resolution
- [Plain `<a>` IS client-side nav](internal-nav-uses-plain-anchor.md):
  leptos_router intercepts every same-origin anchor through a window click
  handler; `rel="external"` is the only opt-out
- [Same-route param nav keeps the view](router-same-route-param-nav.md):
  declaration-order first match; the same `<Route>` id means params update and
  the body never re-runs, so untracked params go stale
- [Router never intercepts form submits](leptos-router-form-interception.md): a
  plain `<form>` is safe; `<Form method="GET">` to the same path short-circuits
  `rebuild`
- [Builder signal plus struct_ver](builder-signal-struct-ver.md): the
  focus-preserving deep-tree editing pattern, for the taxonomy tree
- [Tree editor state bleeds on positional keys](directory-tree-editor.md):
  positional `<For>` keys leak collapse and rename state to the sibling that
  shifts into position, and a refetch re-seeds an editor mid-edit
- [A retained `<For>` key never rebuilds](for-retained-key-never-rebuilds.md):
  a keyed row whose key is unchanged keeps its old body, so a plain-value child
  prop goes stale on refetch; a plain `Vec<AnyView>` rebuilds positionally
- [`use_location().pathname` includes the base](use-location-pathname-includes-base.md):
  it is the raw `window.location.pathname`, so compare against `/ui` + path
- [Click nav decodes the path twice](click-nav-decodeuri-asymmetry.md): the
  anchor handler runs the PATH through `decodeURI` before pushing it, the query
  never; only a literal `%` in a path segment is actually lossy

## Changed under CSR (the hydration half is gone, the rest stands)

- [thaw hydration hazards](thaw-hydration-hazards.md): the mismatch is gone;
  the method survives, verify a widget's source before trusting its `id`/`for`
- [Confirmed-good patterns](w2-confirmed-good-patterns.md): the auth half is
  moot; `.into_any()` erasure, fixed-size integers, and the theme effect stand
- [Tabbed screen pattern](tabbed-screen-pattern.md): always-mounted bodies were
  a hydration device; tab-gated resource sources are now the whole point
- [Redirect paths must be encoded](redirect-path-must-be-percent-encoded.md):
  the `leptos_axum` panic is gone; percent-encoding every value that lands in a
  URL grows teeth, because system URIs and ECL land in FHIR request URLs
- [Journeys must click](no-js-journeys-must-click.md): there is no
  no-JavaScript journey; the review rule stands, a source-substring assertion
  proves nothing
- [Seed-once form idiom](seed-once-form-idiom.md): the hydration half is moot;
  a form seeded from a resource still must not overwrite what is being typed

## Moot under CSR (kept for the record)

- [thaw::Field random id](thaw-field-random-id.md): no server pass, so no
  mismatch; an explicit stable id is still wanted for labels and E2E selectors
- [thaw::Input name forwarding OK](thaw-input-name-forwarding-ok.md): recorded
  for `<ActionForm>` progressive enhancement, which does not exist here
- [chartistry Chart hydration](chartistry-chart-hydration.md): the client
  measurement gate was the hydration answer; the residue is an E2E wait
- [Redirect needs SsrMode::Async](redirect-needs-ssrmode-async.md): `SsrMode`
  does not exist under CSR

## Not ported

FerroEHR's `default-style-guard-untracked-blindspot` describes
`scripts/checks/default-style.sh`, a script with no counterpart in this
repository. Nothing in it transfers.
- [One-shot async in a resource re-runs on the way out](one-shot-async-in-a-resource-reruns-on-the-way-out.md), a code exchange or any side effect belongs in spawn_local, never in a Resource (#647)
- [ScopedFuture tracks inside the async block](scopedfuture-tracks-inside-the-async-block.md), a signal read after an await re-subscribes the resource; read inputs first or get_untracked (#647)
- [A form built inside the resource closure](form-built-inside-the-resource-closure.md), creating the form signal in the reading closure ties everything typed to every OTHER signal that closure reads (#634)
- [A narrow model plus a whole-resource PUT](narrow-model-whole-resource-put.md), an update replaces the resource in full, so every element the form does not model is deleted silently (#634)
- [A Memo source gates a resource refetch](memo-source-gates-a-resource-refetch.md), a Signal::derive over the form's draft refetches every per-row resource on every keystroke; a Memo does not (#636)
- [prop:value moves the caret only on a change](prop-value-caret-only-moves-on-change.md), the HTML value setter compares first, so a pass-through driven input is caret-safe and the seeded value= form is the deviation (#636)
- [An sr-only label under a visible p](sr-only-label-beside-a-visible-p.md), passes the e2e label check and fails SC 2.5.3 Label in Name (#636)
- [A write control built from a URL memo and an old answer](a-write-control-built-from-a-url-memo-and-an-old-answer.md), under Transition the new address meets the previous resource, so a restore writes one resource's body to another resource's id (#637)
- [A live region inside a Show is never announced](live-region-inside-a-show-is-never-announced.md), a region mounted with its first message is not monitored yet, and an e2e text assertion passes either way (#637)
