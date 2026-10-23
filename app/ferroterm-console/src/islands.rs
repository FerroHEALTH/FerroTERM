// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own island: its keyboard shortcuts.
//!
//! FerroUI binds no key; a console listens for its own in an island of its
//! own and opens the library's legend from it (`widget::shortcuts`). Every
//! shortcut has a control on the screen that does the same, so the page
//! loses nothing when its WebAssembly has not loaded. No specification
//! governs the shortcuts: our own design.
#![expect(
    clippy::mem_forget,
    reason = "the island macro of leptos_macro 0.8 keeps each hydrated island alive by forgetting its view state, in the browser entry it writes"
)]
#![expect(
    clippy::must_use_candidate,
    reason = "the #[island] macro drops a #[must_use] from the function it writes"
)]
#![expect(
    clippy::same_name_method,
    reason = "the #[island] macro derives a TypedBuilder whose `builder` shadows a trait method"
)]
#![expect(
    missing_docs,
    reason = "the #[island] macro writes an undocumented struct of each island's props as they cross to the browser"
)]

use ferrohealth_ui::view::kbd::ShortcutGroup;
use ferrohealth_ui::widget::shortcuts::{LegendWords, shortcut_legend};
use leptos::prelude::*;

/// What a key does, once the listener has asked whether the reader types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    /// Nothing: the key is no shortcut here.
    Nothing,
    /// The first key of a go-to pair: wait for the second.
    Wait,
    /// Open the legend.
    Legend,
    /// Follow the link to this address.
    Go(String),
    /// Submit the form marked with this attribute.
    Submit(&'static str),
}

/// The attribute a form that Ctrl Enter runs carries.
pub const RUN: &str = "data-run";

/// The attribute a form that Ctrl S saves carries.
pub const SAVE: &str = "data-save";

/// Where a key is pressed, which decides whether it can be a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// The phone layout, which has no shortcuts.
    Phone,
    /// With Control (or Command) held, inside a field or not.
    Control,
    /// In a field the reader types in.
    Typing,
    /// Anywhere else.
    Free,
}

impl Context {
    /// The context of a key pressed with `control` held, while the reader
    /// is `typing`, in the `phone` layout or not.
    #[must_use]
    pub const fn of(phone: bool, control: bool, typing: bool) -> Self {
        if phone {
            Self::Phone
        } else if control {
            Self::Control
        } else if typing {
            Self::Typing
        } else {
            Self::Free
        }
    }
}

/// What `key` does after `pending` (whether a go-to key came first), pressed
/// in `context`, the go-to targets `goto` each a letter and an address.
#[must_use]
pub fn press(key: &str, pending: bool, context: Context, goto: &[(String, String)]) -> Press {
    match context {
        Context::Phone | Context::Typing => Press::Nothing,
        Context::Control => match key {
            "Enter" => Press::Submit(RUN),
            "s" | "S" => Press::Submit(SAVE),
            _ => Press::Nothing,
        },
        Context::Free if pending => goto
            .iter()
            .find(|(letter, _)| letter.eq_ignore_ascii_case(key))
            .map_or(Press::Nothing, |(_, href)| Press::Go(href.clone())),
        Context::Free => match key {
            "?" => Press::Legend,
            "g" | "G" => Press::Wait,
            _ => Press::Nothing,
        },
    }
}

/// The console's keyboard shortcuts: `?` opens the legend of `groups` in
/// `words`, a go-to pair from `goto` follows its link, Ctrl Enter submits
/// the page's run form and Ctrl S its save form. The legend shows as the
/// page arrives when `open`.
#[island]
pub fn Shortcuts(
    /// The legend's groups, their words in the page's locale.
    groups: Vec<ShortcutGroup>,
    /// The legend's own words, in the page's locale.
    words: LegendWords,
    /// The go-to targets: each the second key of a `g` pair and the address
    /// it leads to.
    goto: Vec<(String, String)>,
    /// Whether the legend shows as the page arrives.
    open: bool,
) -> impl IntoView {
    let (shown, set_shown) = signal(open);
    #[cfg(target_arch = "wasm32")]
    {
        listen(goto, set_shown);
        mark_hydrated();
    }
    #[cfg(not(target_arch = "wasm32"))]
    drop(goto);
    shortcut_legend(
        groups,
        words,
        shown.into(),
        Callback::new(move |()| set_shown.set(false)),
    )
}

/// The button a shortcut presses for the form `marker` names.
///
/// A form that asks for what it does by its submitter (Save in an editor,
/// the preview in the composer) is submitted by that button; a form with no
/// such button is submitted as it is.
#[must_use]
pub const fn submitter_of(marker: &str) -> &'static [&'static str] {
    if marker.len() == SAVE.len() {
        &["button[name='intent'][value='save']"]
    } else {
        &[
            "button[name='intent'][value='preview']",
            "form[data-run] .fui-transform-run button[type='submit']",
        ]
    }
}

/// Marks `<html>` once this island, the last of the shell's, has hydrated,
/// so a browser journey acts on controls that answer.
#[cfg(target_arch = "wasm32")]
fn mark_hydrated() {
    // NOTE: no specification governs this: our own design; the mark is for the
    // journeys, which wait for it before they press anything.
    if let Some(root) = document().document_element()
        && root.set_attribute("data-hydrated", "").is_err()
    {
        leptos::logging::warn!("the browser refused the hydrated mark");
    }
}

/// Listens for the shortcuts on the window, the go-to targets `goto`,
/// opening the legend through `set_shown`.
#[cfg(target_arch = "wasm32")]
fn listen(goto: Vec<(String, String)>, set_shown: WriteSignal<bool>) {
    use leptos::wasm_bindgen::JsCast as _;
    let pending = StoredValue::new(false);
    let handle = window_event_listener(leptos::ev::keydown, move |event| {
        let typing = ferrohealth_ui::widget::shortcuts::typing(&event);
        let control = event.ctrl_key() || event.meta_key();
        let phone = ferrohealth_ui::widget::shortcuts::phone_layout();
        let decided = press(
            &event.key(),
            pending.get_value(),
            Context::of(phone, control, typing),
            &goto,
        );
        pending.set_value(decided == Press::Wait);
        match decided {
            Press::Nothing | Press::Wait => {}
            Press::Legend => {
                event.prevent_default();
                set_shown.set(true);
            }
            Press::Go(href) => {
                event.prevent_default();
                if window().location().assign(&href).is_err() {
                    leptos::logging::warn!("the browser refused to follow a shortcut");
                }
            }
            Press::Submit(marker) => {
                let form = document()
                    .query_selector(&format!("form[{marker}]"))
                    .ok()
                    .flatten()
                    .and_then(|element| element.dyn_into::<web_sys::HtmlFormElement>().ok());
                let submitter = submitter_of(marker)
                    .iter()
                    .find_map(|selector| document().query_selector(selector).ok().flatten())
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
                if let Some(form) = form {
                    event.prevent_default();
                    let sent = match &submitter {
                        Some(button) => form.request_submit_with_submitter(Some(button)),
                        None => form.request_submit(),
                    };
                    if sent.is_err() {
                        leptos::logging::warn!("the browser refused to submit a form");
                    }
                }
            }
        }
    });
    on_cleanup(move || handle.remove());
}

#[cfg(test)]
mod tests {
    use super::{Context, Press, RUN, SAVE, press};

    fn targets() -> Vec<(String, String)> {
        vec![
            (String::from("d"), String::from("/")),
            (String::from("c"), String::from("/systems")),
        ]
    }

    #[test]
    fn a_go_to_pair_follows_its_link() {
        assert_eq!(
            Press::Wait,
            press("g", false, Context::of(false, false, false), &targets())
        );
        assert_eq!(
            Press::Go(String::from("/systems")),
            press("c", true, Context::of(false, false, false), &targets())
        );
        assert_eq!(
            Press::Nothing,
            press("x", true, Context::of(false, false, false), &targets()),
            "a pair the console has no target for"
        );
    }

    #[test]
    fn a_single_key_never_acts_while_the_reader_types() {
        assert_eq!(
            Press::Nothing,
            press("?", false, Context::of(false, false, true), &targets())
        );
        assert_eq!(
            Press::Legend,
            press("?", false, Context::of(false, false, false), &targets())
        );
        assert_eq!(
            Press::Submit(RUN),
            press("Enter", false, Context::of(false, true, true), &targets()),
            "Ctrl Enter runs from inside a field"
        );
        assert_eq!(
            Press::Submit(SAVE),
            press("s", false, Context::of(false, true, true), &targets())
        );
    }

    #[test]
    fn a_phone_has_no_shortcuts() {
        assert_eq!(
            Press::Nothing,
            press("?", false, Context::of(true, false, false), &targets())
        );
    }
}
