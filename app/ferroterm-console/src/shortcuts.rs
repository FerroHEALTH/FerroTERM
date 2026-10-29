// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own keyboard shortcuts: a go-to pair follows its link,
//! Ctrl Enter runs the page's run form and Ctrl S saves its save form.
//!
//! FerroUI opens the legend on `?` (`island::ShortcutLegend`, drawn by the
//! shell) and binds no other key; a console listens for its own. Every
//! shortcut has a control on the screen that does the same, so the page
//! loses nothing when its WebAssembly has not loaded. No specification
//! governs the shortcuts: our own design.

#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]
#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "the keys are read in the browser alone; the host builds them for the tests"
    )
)]

use leptos::prelude::*;

/// What a key does, once the listener has asked whether the reader types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Press {
    /// Nothing: the key is no shortcut here.
    Nothing,
    /// The first key of a go-to pair: wait for the second.
    Wait,
    /// Follow the link to this address.
    Go(String),
    /// Submit the form marked with this attribute.
    Submit(&'static str),
}

/// The attribute a form that Ctrl Enter runs carries.
pub(crate) const RUN: &str = "data-run";

/// The attribute a form that Ctrl S saves carries.
pub(crate) const SAVE: &str = "data-save";

/// Where a key is pressed, which decides whether it can be a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
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
    pub(crate) const fn of(phone: bool, control: bool, typing: bool) -> Self {
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
pub(crate) fn press(
    key: &str,
    pending: bool,
    context: Context,
    goto: &[(String, String)],
) -> Press {
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
            "g" | "G" => Press::Wait,
            _ => Press::Nothing,
        },
    }
}

/// The console's keyboard shortcuts: a go-to pair from `goto` follows its
/// link in the page, Ctrl Enter submits the page's run form and Ctrl S its
/// save form.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn Shortcuts(
    /// The go-to targets: each the second key of a `g` pair and the address
    /// it leads to.
    goto: Vec<(String, String)>,
) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    listen(goto);
    #[cfg(not(target_arch = "wasm32"))]
    drop(goto);
}

/// The button a shortcut presses for the form `marker` names.
///
/// A form that asks for what it does by its submitter (Save in an editor,
/// the preview in the composer) is submitted by that button; a form with no
/// such button is submitted as it is.
#[must_use]
pub(crate) const fn submitter_of(marker: &str) -> &'static [&'static str] {
    if marker.len() == SAVE.len() {
        &["button[name='intent'][value='save']"]
    } else {
        &[
            "button[name='intent'][value='preview']",
            "form[data-run] .fui-transform-run button[type='submit']",
        ]
    }
}

/// Listens for the shortcuts on the window, the go-to targets `goto`.
#[cfg(target_arch = "wasm32")]
fn listen(goto: Vec<(String, String)>) {
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
            Press::Go(href) => {
                event.prevent_default();
                ferrohealth_ui::widget::submit::follow(&href);
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
    use super::{Context, Press, RUN, SAVE, Shortcuts, press, submitter_of};

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
            press("g", false, Context::of(false, false, true), &targets())
        );
        assert_eq!(
            Press::Wait,
            press("g", false, Context::of(false, false, false), &targets())
        );
        assert_eq!(
            Press::Nothing,
            press("?", false, Context::of(false, false, false), &targets()),
            "the legend's key is FerroUI's"
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

    #[test]
    fn a_key_with_no_shortcut_does_nothing_in_any_context() {
        for context in [Context::Control, Context::Free] {
            assert_eq!(
                Press::Nothing,
                press("x", false, context, &targets()),
                "{context:?}"
            );
        }
    }

    #[test]
    fn the_go_to_key_and_its_pair_read_in_either_case() {
        assert_eq!(Press::Wait, press("G", false, Context::Free, &targets()));
        assert_eq!(
            Press::Go(String::from("/")),
            press("D", true, Context::Free, &targets())
        );
        assert_eq!(
            Press::Submit(SAVE),
            press("S", false, Context::Control, &targets())
        );
    }

    #[test]
    fn a_save_form_is_submitted_by_its_save_button_and_a_run_form_by_its_run_button() {
        assert_eq!(&["button[name='intent'][value='save']"], submitter_of(SAVE));
        assert_eq!(
            &[
                "button[name='intent'][value='preview']",
                "form[data-run] .fui-transform-run button[type='submit']",
            ],
            submitter_of(RUN)
        );
    }

    #[test]
    fn the_listener_draws_nothing_and_the_shell_s_legend_lists_the_console_s_keys() {
        use ferrohealth_ui::island::ShortcutLegend;
        use ferrohealth_ui::widget::shortcuts::LegendWords;
        use leptos::prelude::{Owner, RenderHtml, view};
        let listener = Owner::new().with(|| view! { <Shortcuts goto=targets() /> }.to_html());
        assert_eq!(
            "<!>", listener,
            "the shortcuts draw no markup of their own, only the empty view's marker"
        );
        let words = LegendWords {
            title: "Keyboard shortcuts".into(),
            intro: "Press a key.".into(),
            or: "or".into(),
            close: "Close".into(),
        };
        let html = Owner::new().with(|| {
            view! { <ShortcutLegend groups=crate::pages::shortcuts::groups() words=words /> }
                .to_html()
        });
        assert!(html.contains("<dialog id=\"shortcuts\""), "{html}");
        assert!(html.contains("Close</button>"), "{html}");
    }
}
