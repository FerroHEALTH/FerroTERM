// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A control over a code of a value set the served root expands.
//!
//! Every coded element of an authored resource is bound to a value set
//! (<https://hl7.org/fhir/R4B/terminologies.html#binding>), and the editor
//! offers what the root expands that value set to rather than a list compiled
//! into the bundle. One control draws all of them, so a root that admits
//! another code offers it on every screen without a new build.

use leptos::ev::Event;
use leptos::prelude::*;

use crate::components::failure::Failure;
use crate::fhir::FhirClient;
use crate::fhir::error::FhirError;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::styles;

/// One code offered by a coded control.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Coded {
    /// The code itself, which is what the resource carries.
    pub(crate) code: String,
    /// The display the server sent for it, empty where it sent none.
    pub(crate) display: String,
}

/// The codes one coded control offers, and why it has none when it has none.
#[derive(Clone, Copy)]
pub(crate) struct Codes {
    /// The codes the value set expanded to.
    pub(crate) offered: Signal<Vec<Coded>>,
    /// Why the expansion did not answer, when it did not.
    ///
    /// The refusal itself, so the control renders the server's own
    /// `OperationOutcome` rather than the status number it came with.
    pub(crate) refused: Signal<Option<FhirError>>,
}

/// What one coded control is called, and how its label is drawn.
///
/// The four travel together so the control takes one argument for its naming
/// rather than four the caller could pass in the wrong order.
pub(crate) struct Control {
    /// The `id` the label points at.
    pub(crate) id: String,
    /// The `name` the control carries.
    pub(crate) name: &'static str,
    /// The label a reader reads, which is also the control's accessible name.
    ///
    /// It is a signal because a label can name the element the served version
    /// writes, and the version switcher changes that without leaving the
    /// route. The two are one string, because the accessible name has to
    /// contain the visible text
    /// (<https://www.w3.org/TR/WCAG22/#label-in-name>).
    pub(crate) label: Signal<String>,
    /// Whether the label is for a screen reader alone, because a column
    /// header already names the control for a sighted reader.
    pub(crate) sr_only: bool,
}

/// A label that is the same whatever the served version is.
pub(crate) fn fixed(label: String) -> Signal<String> {
    Signal::derive(move || label.clone())
}

/// One value of an authoring draft, as a signal that only fires when it
/// changes.
///
/// The reader is boxed rather than generic, so the memo machinery compiles
/// once for every screen that draws a control; a generic one is monomorphized
/// per call site, and an authoring screen has one per control.
///
/// Every read of a draft touches the one signal the whole form is held in, so
/// a plain derive over it re-runs on every keystroke anywhere. A `Memo`
/// compares before it notifies, which is what keeps a control's own property
/// write, and its option list's per-option `selected`, to the field that
/// actually changed
/// (<https://docs.rs/reactive_graph/0.2/reactive_graph/computed/struct.Memo.html>).
pub(crate) fn gated(read: Box<dyn Fn() -> String + Send + Sync>) -> Signal<String> {
    Signal::from(Memo::new(move |_| read()))
}

/// The codes one value set expands to, as the signals a control reads.
///
/// `canonical` is a signal because the value set an element is bound to can
/// differ per served version, and the version switcher changes it without
/// leaving the route.
///
/// A value set the root refuses to expand leaves the control with the code the
/// resource already carries and nothing else, and the control says so in the
/// server's own words rather than looking like a list with one entry in it.
pub(crate) fn codes_of(
    client: &FhirClient,
    version: Signal<FhirVersion>,
    canonical: Signal<String>,
) -> Codes {
    let client = client.clone();
    let expanded = Resource::new(
        move || (version.get(), canonical.get()),
        move |(version, canonical)| {
            let client = client.clone();
            async move { client.value_set_codes(version, &canonical).await }
        },
    );
    Codes {
        offered: Signal::derive(move || {
            expanded.with(|answered| {
                answered
                    .as_ref()
                    .and_then(|read| read.as_ref().ok())
                    .map(|rows| {
                        rows.iter()
                            .map(|row| Coded {
                                code: row.code.clone(),
                                display: row.display.clone().unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            })
        }),
        refused: Signal::derive(move || {
            expanded.with(|answered| {
                answered
                    .as_ref()
                    .and_then(|read| read.as_ref().err().cloned())
            })
        }),
    }
}

/// One control over a code of a value set the server expanded.
///
/// The code the resource already carries is offered whether or not the
/// expansion did: a control that dropped it would silently rewrite the
/// resource on the next save.
///
/// Each option states its own selectedness, and the select is driven by
/// `prop:value` as well. Both are needed. The options arrive after the form is
/// built, and a select whose `value` names an option it does not have yet
/// falls back to its first one
/// (<https://html.spec.whatwg.org/multipage/form-elements.html#the-select-element>),
/// which then silently rewrote the resource on the next save; the `selected`
/// attribute is read as each option is inserted, so the expansion landing puts
/// the selection back where the resource had it.
///
/// The callback is boxed rather than generic, so every control on every screen
/// compiles to one copy of this function. A generic one is monomorphized per
/// call site, and the editor bundle draws dozens of them.
pub(crate) fn coded_control(
    control: Control,
    codes: Codes,
    readonly: Signal<bool>,
    held: Signal<String>,
    mut chose: Box<dyn FnMut(String)>,
) -> AnyView {
    let Control {
        id,
        name,
        label,
        sr_only,
    } = control;
    let offered = move || {
        let held = held.get();
        let mut drawn: Vec<AnyView> = Vec::new();
        let listed = codes
            .offered
            .with(|codes| codes.iter().any(|coded| coded.code == held));
        if !listed {
            let text = if held.is_empty() {
                tr(Msg::ChooseOne).clone()
            } else {
                held.clone()
            };
            drawn.push(
                view! {
                    <option value=held.clone() selected=true>
                        {text}
                    </option>
                }
                .into_any(),
            );
        }
        drawn.extend(codes.offered.with(|codes| {
            codes
                .iter()
                .map(|coded| {
                    let code = coded.code.clone();
                    let text = if coded.display.is_empty() {
                        coded.code.clone()
                    } else {
                        format!("{} ({})", coded.display, coded.code)
                    };
                    let chosen = code == held;
                    view! {
                        <option value=code selected=chosen>
                            {text}
                        </option>
                    }
                    .into_any()
                })
                .collect::<Vec<AnyView>>()
        }));
        drawn
    };
    let named = StoredValue::new(id);
    let label_class = if sr_only {
        "fui-sr-only"
    } else {
        styles::LABEL
    };
    view! {
        <div class="grid gap-tight">
            <label for=move || named.with_value(Clone::clone) class=label_class>
                {move || label.get()}
            </label>
            <select
                id=move || named.with_value(Clone::clone)
                name=name
                class=ferrohealth_ui::view::form::SELECT
                disabled=move || readonly.get()
                prop:value=move || held.get()
                on:change=move |event: Event| chose(event_target_value(&event))
            >
                {offered}
            </select>
            <Show when=move || codes.refused.with(Option::is_some) fallback=|| ()>
                <p role="status" class=styles::HINT>
                    {tr(Msg::ServerDidNotExpandValueSet)}
                </p>
                {move || {
                    codes
                        .refused
                        .with(|held| {
                            held.as_ref()
                                .map(|error| {
                                    view! { <Failure error=error.clone() /> }.into_any()
                                })
                        })
                }}
            </Show>
        </div>
    }
    .into_any()
}
