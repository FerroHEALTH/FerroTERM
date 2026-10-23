// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the four operation runners share: the Operations heading over the
//! binder tabs, an options row in a plain `GET` form that Run submits, the
//! verdict line, and the request the answer came from.
//!
//! Every parameter of a run lives in the address, so a run is a link a
//! reader can share, the back button walks the runs, and the page works
//! with no WebAssembly. The operations read and never write. No
//! specification governs the layout: our own design.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::code::{Code, code_viewer};
use ferrohealth_ui::view::form::{Field, INPUT, SELECT};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::transform::{
    Density, Run, RunState, request, run_button, transform_options, transform_quiet,
};
use leptos::prelude::*;

use crate::fhir::named::NamedSearch;
use crate::locale::{Msg, tr};
use crate::pages::parts::heading;
use crate::request::Request;
use crate::routes::{
    EXPAND_PATH, LOOKUP_PATH, TRANSLATE_PATH, VALIDATE_PATH, VERSION_PARAM, ui_link,
};

/// The four runners, one tab each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// `CodeSystem/$lookup`.
    Lookup,
    /// `ValueSet/$validate-code` and `CodeSystem/$subsumes`.
    Check,
    /// `ValueSet/$expand`.
    Expand,
    /// `ConceptMap/$translate`.
    Translate,
}

impl Op {
    /// Every runner, in tab order.
    const ALL: [Self; 4] = [Self::Lookup, Self::Check, Self::Expand, Self::Translate];

    /// The runner's path.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::Lookup => LOOKUP_PATH,
            Self::Check => VALIDATE_PATH,
            Self::Expand => EXPAND_PATH,
            Self::Translate => TRANSLATE_PATH,
        }
    }

    /// The runner's route, which its form submits to.
    pub(crate) fn route(self) -> String {
        format!("/{}", self.path())
    }

    /// The tab's words.
    fn words(self) -> String {
        tr(match self {
            Self::Lookup => Msg::OpsLookUp,
            Self::Check => Msg::OpsCheck,
            Self::Expand => Msg::OpsExpand,
            Self::Translate => Msg::OpsTranslate,
        })
    }
}

/// The Operations page on the tab of `op`: the options row `form` over the
/// `answer`, both in the one plain `GET` form Run submits, which is the
/// tab's panel (`fui-panel`) so its sections run edge to edge.
pub(crate) fn frame(request: &Request, op: Op, form: AnyView, answer: AnyView) -> AnyView {
    let tabs = Op::ALL
        .into_iter()
        .map(|each| {
            Tab::new(
                each.path(),
                ui_link(each.path(), request.version),
                each.words(),
            )
        })
        .collect();
    let panel = view! {
        <form method="get" action=op.route() class="fui-panel" data-run="">
            {form}
            {answer}
        </form>
    }
    .into_any();
    view! {
        {heading(tr(Msg::OpsTitle), Some(tr(Msg::OpsSubtitle)), Vec::new(), ActionGroup::none())}
        {binder(tr(Msg::OpsTabs), tabs, op.path(), panel)}
    }
    .into_any()
}

/// The options row: `fields` with `run` at its end, keeping the FHIR version
/// and the `hidden` fields.
pub(crate) fn options(
    request: &Request,
    hidden: &[(&'static str, String)],
    fields: AnyView,
    run: AnyView,
) -> AnyView {
    let hidden = hidden
        .iter()
        .map(|(name, value)| view! { <input type="hidden" name=*name value=value.clone() /> })
        .collect_view();
    transform_options(
        Density::Dense,
        view! {
            <input type="hidden" name=VERSION_PARAM value=request.version.segment() />
            {hidden}
            {fields}
        },
        run,
    )
}

/// Run, its `label` and `glyph`, pressed by Ctrl Enter and naming the
/// `operation` it calls.
pub(crate) fn run(id: &str, label: String, glyph: Glyph, operation: &str) -> AnyView {
    run_button(Run {
        id: id.to_owned(),
        label,
        busy: tr(Msg::OpsRunning),
        glyph,
        shortcut: Some(String::from("Ctrl Enter")),
        operation: Some(operation.to_owned()),
        state: RunState::Ready,
    })
}

/// A select named `name` of `choices` (each its value and words), `picked`
/// chosen, led by a `blank` choice when there is one; a picked value the
/// choices do not hold is kept as its own choice, so nothing the address
/// carries is lost.
pub(crate) fn select(
    id: &'static str,
    name: &'static str,
    label: String,
    choices: &[(String, String)],
    picked: &str,
    blank: Option<String>,
) -> AnyView {
    let blank = blank.map(|words| {
        view! {
            <option value="" selected=picked.is_empty()>
                {words}
            </option>
        }
    });
    let kept =
        (!picked.is_empty() && !choices.iter().any(|(value, _)| value == picked)).then(|| {
            view! {
                <option value=picked.to_owned() selected=true>
                    {picked.to_owned()}
                </option>
            }
        });
    let offered = choices
        .iter()
        .map(|(value, words)| {
            view! {
                <option value=value.clone() selected=value == picked>
                    {words.clone()}
                </option>
            }
        })
        .collect_view();
    Field::new(id, label).view(view! {
        <select id=id name=name class=SELECT>
            {blank}
            {kept}
            {offered}
        </select>
    })
}

/// A text field named `name` holding `value`, with the hint `placeholder`.
pub(crate) fn text(
    id: &'static str,
    name: &'static str,
    label: String,
    value: &str,
    placeholder: String,
) -> AnyView {
    Field::new(id, label).view(view! {
        <input
            id=id
            name=name
            class=INPUT
            value=value.to_owned()
            placeholder=placeholder
            autocomplete="off"
        />
    })
}

/// The choices a picker of published resources offers: each canonical with
/// the name it was published under.
pub(crate) fn choices(named: Option<&NamedSearch>) -> Vec<(String, String)> {
    named
        .map(|named| {
            named
                .choices()
                .into_iter()
                .map(|choice| (choice.canonical, choice.label))
                .collect()
        })
        .unwrap_or_default()
}

/// The one quiet line that stands where the answer will be.
pub(crate) fn before(line: String) -> AnyView {
    panel_section(None, transform_quiet(line))
}

/// The verdict line of an answer: how it went, in a sentence, and its one
/// action.
pub(crate) fn verdict(outcome: Outcome, title: AnyView, action: Option<AnyView>) -> AnyView {
    panel_section(
        None,
        load_report(LoadReport {
            outcome,
            arrival: Arrival::AfterAction,
            title,
            counts: Vec::new(),
            action,
        }),
    )
}

/// The request an answer came from, `GET url`, with Copy, and the answer
/// as the server sent it under `raw` when there is one.
pub(crate) fn sent(url: &str, raw: Option<(String, String)>) -> AnyView {
    let raw = raw.map(|(summary, text)| {
        let code = Code {
            label: summary.clone(),
            language: Language::Json,
            text,
            ids: None,
            current: None,
            diagnostics: Vec::new(),
        };
        (summary, code_viewer(None, &code))
    });
    panel_section(
        Some(SectionHead {
            title: tr(Msg::OpsRequest),
            count: None,
            actions: None,
        }),
        request(ferrohealth_ui::view::transform::Request {
            text: format!("GET {url}"),
            copy: Some(String::from("operation-request-copy")),
            raw,
        }),
    )
}
