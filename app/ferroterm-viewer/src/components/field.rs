//! The labelled controls a runner's form is built from.
//!
//! One `Field` describes a control: the name it carries on the wire, the label
//! a reader reads, and the sentence that explains the parameter. Every runner
//! draws its controls from here, so the three of them stay one form.
//!
//! A hint reaches the accessibility tree on every render through
//! `aria-describedby`, and is drawn for a sighted reader only while the form's
//! help switch is on. `sr-only` is what keeps it reachable while it is out of
//! the way (<https://www.w3.org/WAI/WCAG22/Techniques/css/C7>).

use leptos::ev::Event;
use leptos::html::Input;
use leptos::prelude::*;

use ferrohealth_ui::view::form::{INPUT, NARROW, SELECT, fieldset, help_id};
use ferrohealth_ui::view::layout::{FieldRow, field_row};
use ferrohealth_ui::widget::combobox::{ComboOption, Combobox, combobox};
use ferrohealth_ui::widget::help::{explained_check_field, explained_field, help_line};
use ferrohealth_ui::widget::pick_or_type::{PickOrType, Picker as PickerControl, pick_or_type};

use crate::fhir::named::Choice;

/// The option value that asks for the text field rather than a listed one.
///
/// It is not a canonical, so no resource can collide with it.
const TYPE_IT: &str = "\u{1}another";

/// The name a combobox posts its choice under before the page hydrates: the
/// text field beside it carries the value the run sends, so this one is
/// ignored by every screen.
const PICKED_NAME: &str = "picked";

/// One control: what it is called on the wire, what a reader calls it, and the
/// sentence that explains it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    /// The `id` the label points at.
    pub(crate) id: &'static str,
    /// The `name` the control carries.
    pub(crate) name: &'static str,
    /// The label a reader reads.
    pub(crate) label: &'static str,
    /// The sentence that explains the parameter.
    pub(crate) hint: &'static str,
}

/// One labelled text control, seeded from its own parameter.
pub(crate) fn text_field(field: Field, node: NodeRef<Input>, value: Memo<String>) -> AnyView {
    text_field_showing(field, node, value, None)
}

/// The placeholder a display language field shows while it is empty.
pub(crate) const LANGUAGE_EXAMPLE: &str = "for example en or nl-NL";

/// One labelled text control with a placeholder showing the shape of what it
/// takes while it is empty.
pub(crate) fn text_field_showing(
    field: Field,
    node: NodeRef<Input>,
    value: Memo<String>,
    placeholder: Option<&'static str>,
) -> AnyView {
    let input = view! {
        <input
            id=field.id
            name=field.name
            type="text"
            class=INPUT
            placeholder=placeholder
            aria-describedby=help_id(field.id)
            node_ref=node
            value=move || value.get()
            prop:value=move || value.get()
        />
    };
    explained_field(field.id, field.label, field.hint, input)
}

/// One labelled checkbox, seeded from its own parameter.
pub(crate) fn check_field(field: Field, node: NodeRef<Input>, value: Memo<bool>) -> AnyView {
    let checkbox = view! {
        <input
            id=field.id
            name=field.name
            type="checkbox"
            aria-describedby=help_id(field.id)
            node_ref=node
            checked=move || value.get()
            prop:checked=move || value.get()
        />
    };
    explained_check_field(field.id, field.label, field.hint, checkbox)
}

/// One labelled whole-number control, seeded from its own parameter.
pub(crate) fn number_field(
    field: Field,
    node: NodeRef<Input>,
    value: Memo<String>,
    max: u32,
) -> AnyView {
    let input = view! {
        <input
            id=field.id
            name=field.name
            type="number"
            min="1"
            max=max.to_string()
            class=format!("{INPUT} {NARROW}")
            aria-describedby=help_id(field.id)
            node_ref=node
            value=move || value.get()
            prop:value=move || value.get()
        />
    };
    explained_field(field.id, field.label, field.hint, input)
}

/// One labelled choice control, seeded from its own parameter.
///
/// The choice is a navigation on change: it decides which operation the runner
/// sends and which controls belong on the form, so it cannot wait for a submit.
pub(crate) fn select_field(
    field: Field,
    value: Memo<String>,
    options: impl Fn() -> Vec<AnyView> + Send + Sync + 'static,
    change: impl FnMut(Event) + 'static,
) -> AnyView {
    let select = view! {
        <select
            id=field.id
            name=field.name
            class=SELECT
            aria-describedby=help_id(field.id)
            prop:value=move || value.get()
            on:change=change
        >
            {options}
        </select>
    };
    explained_field(field.id, field.label, field.hint, select)
}

/// A control and the narrower one beside it, for a value and its version.
///
/// A canonical is a name a reader reads and a version is a number, so the two
/// do not deserve the same width: an even split truncates the name that the
/// picker exists to show.
pub(crate) fn versioned_row(fields: Vec<AnyView>) -> AnyView {
    // NOTE: no specification governs this: our own design; FerroUI's rows put
    // the narrow field first, so the canonical leads in two columns of its own.
    field_row(FieldRow::Halves, fields)
}

/// Controls that sit side by side on a wide screen.
pub(crate) fn row(fields: Vec<AnyView>) -> AnyView {
    field_row(FieldRow::Halves, fields)
}

/// A note beside a control about the run that is showing.
///
/// A parameter the address carried and the runner could not use is not help,
/// so it does not hide behind the help switch.
pub(crate) fn note(text: Memo<String>) -> AnyView {
    view! {
        <p role="status" class="fui-help">
            {move || text.get()}
        </p>
    }
    .into_any()
}

/// A labelled group of controls that belong together.
pub(crate) fn group(label: &'static str, fields: Vec<AnyView>) -> AnyView {
    fieldset(label, fields).into_any()
}

/// What one picker's own two options say.
///
/// The words are the control's, because "the version this server defaults to"
/// and "another canonical" are the same option in two different forms.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Picker {
    /// What the option that names nothing says.
    pub(crate) empty: &'static str,
    /// What the option that asks for the text field says.
    pub(crate) other: &'static str,
    /// Whether the list is long enough to filter as the reader types: a
    /// combobox, where a short list stays a select.
    pub(crate) searched: bool,
}

/// The words a picker over a canonical carries.
pub(crate) const CANONICAL_WORDS: Picker = Picker {
    empty: "Choose one this server holds",
    other: "Another canonical",
    searched: true,
};

/// The words a picker over a version carries.
///
/// A request that names no version is answered against the version the server
/// resolves to, which is what the empty option says rather than leaving a
/// reader to guess what an empty box means
/// (<https://hl7.org/fhir/R5/terminology-module.html#version>).
pub(crate) const VERSION_WORDS: Picker = Picker {
    empty: "The one this server resolves to",
    other: "Another version",
    searched: false,
};

/// One labelled value, picked from what this root holds or typed.
///
/// A canonical is what every operation takes and what almost no reader knows
/// by heart, so the control offers what this root holds rather than waiting
/// for one to be typed. The text field stays, as the last option and as what
/// the form submits: a root may hold a resource it does not publish, and a
/// reader may be checking a canonical against a root on purpose.
///
/// The select is a second control over one value, so it carries no `name` and
/// nothing on the wire: picking writes the canonical into the field beside it
/// (<https://developer.mozilla.org/en-US/docs/Web/API/HTMLInputElement/value>).
pub(crate) fn picked_field(
    field: Field,
    node: NodeRef<Input>,
    value: Memo<String>,
    choices: Memo<Vec<Choice>>,
    words: Picker,
) -> AnyView {
    if words.searched {
        searched_field(field, node, value, choices, words)
    } else {
        selected_field(field, node, value, choices, words)
    }
}

/// A long list of canonicals as a combobox the reader filters by typing,
/// each offer its name over its canonical, with the text field behind the
/// last option.
fn searched_field(
    field: Field,
    node: NodeRef<Input>,
    value: Memo<String>,
    choices: Memo<Vec<Choice>>,
    words: Picker,
) -> AnyView {
    let typing = RwSignal::new(false);
    let unlisted = move || {
        value.with(|value| {
            !value.is_empty()
                && choices.with(|choices| !choices.iter().any(|choice| &choice.canonical == value))
        })
    };
    let typed = move || typing.get() || unlisted();
    let picked = RwSignal::new(None::<String>);
    // The address and the offers both move the chosen value, and the combobox
    // follows them; a choice the reader makes writes the field the form reads.
    Effect::new(move |_| {
        let named = value.get();
        picked.set((!named.is_empty()).then_some(named));
    });
    Effect::new(move |_| match picked.get() {
        Some(chosen) if chosen == TYPE_IT => typing.set(true),
        Some(chosen) => {
            typing.set(false);
            if let Some(input) = node.get_untracked() {
                input.set_value(&chosen);
            }
        }
        None => {}
    });
    let typed_id = format!("{}-typed", field.id);
    let combo = move || {
        let mut options: Vec<ComboOption> = choices.with(|choices| {
            choices
                .iter()
                .map(|choice| {
                    ComboOption::new(choice.canonical.clone(), choice.label.clone())
                        .detail(choice.canonical.clone())
                })
                .collect()
        });
        options.push(ComboOption::new(TYPE_IT, words.other));
        combobox(Combobox {
            id: field.id,
            name: PICKED_NAME,
            label: field.label,
            placeholder: words.empty,
            options,
            selected: picked,
        })
    };
    let input = view! {
        <input
            id=typed_id.clone()
            name=field.name
            type="text"
            class=INPUT
            aria-describedby=help_id(field.id)
            node_ref=node
            value=move || value.get()
            prop:value=move || value.get()
        />
    };
    pick_or_type(PickOrType {
        label: field.label.to_owned(),
        picker: PickerControl::Labelled(combo.into_any()),
        typed_id,
        typed: input.into_any(),
        typing: Signal::derive(typed),
        after: Some(help_line(field.id, field.hint)),
    })
}

/// A short list as a select, with the text field behind the last option.
fn selected_field(
    field: Field,
    node: NodeRef<Input>,
    value: Memo<String>,
    choices: Memo<Vec<Choice>>,
    words: Picker,
) -> AnyView {
    // Whether the reader asked for the text field. It is a signal of this
    // control rather than of the screen, because one form can carry two
    // pickers and each is on its own choice.
    let typing = RwSignal::new(false);
    let described_by = help_id(field.id);
    let picker = format!("{}-pick", field.id);
    let offered = move || {
        choices.with(|choices| {
            choices
                .iter()
                .map(|choice| {
                    let canonical = choice.canonical.clone();
                    let label = choice.label.clone();
                    let named = canonical.clone();
                    let selected = move || value.with(|value| *value == named);
                    view! {
                        <option value=canonical.clone() title=canonical selected=selected>
                            {label}
                        </option>
                    }
                    .into_any()
                })
                .collect::<Vec<AnyView>>()
        })
    };
    // A canonical the root does not publish is a reader's own, so the control
    // opens on the text field and stays there rather than snapping back to a
    // list the value is not in.
    let unlisted = move || {
        value.with(|value| {
            !value.is_empty()
                && choices.with(|choices| !choices.iter().any(|choice| &choice.canonical == value))
        })
    };
    let typed = move || typing.get() || unlisted();
    // A select drops a value it has no option for, and the address is read
    // before the search that fills the list answers, so the offers are read
    // here: reading them is what writes the value again when they arrive.
    let selected = move || {
        choices.with(Vec::len);
        value.get()
    };
    let choose = move |event: Event| {
        let canonical = event_target_value(&event);
        if canonical == TYPE_IT {
            typing.set(true);
            return;
        }
        typing.set(false);
        if let Some(input) = node.get() {
            input.set_value(&canonical);
        }
    };
    let select = view! {
        <select
            id=picker.clone()
            class=SELECT
            aria-describedby=described_by.clone()
            prop:value=selected
            on:change=choose
        >
            <option value="">{words.empty}</option>
            {offered}
            <option value=TYPE_IT>{words.other}</option>
        </select>
    };
    let input = view! {
        <input
            id=field.id
            name=field.name
            type="text"
            class=INPUT
            aria-describedby=described_by
            node_ref=node
            value=move || value.get()
            prop:value=move || value.get()
        />
    };
    // Each control carries its own label, hidden with it, so a reader meets
    // one (<https://www.w3.org/TR/wai-aria-1.2/#namecalculation>).
    pick_or_type(PickOrType {
        label: field.label.to_owned(),
        picker: PickerControl::Control {
            id: picker,
            control: select.into_any(),
        },
        typed_id: field.id.to_owned(),
        typed: input.into_any(),
        typing: Signal::derive(typed),
        after: Some(help_line(field.id, field.hint)),
    })
}
