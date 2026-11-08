// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the four operation runners share: the Operations heading over the
//! binder tabs, an options row in a plain `GET` form that Run submits, the
//! verdict line, and the request the answer came from.
//!
//! Every parameter of a run lives in the address, so a run is a link a
//! reader can share, the back button walks the runs, and the page works
//! with no script. The operations read and never write. No
//! specification governs the layout: our own design.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
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

use crate::fhir::error::FhirError;
use crate::fhir::named::NamedSearch;
use crate::locale::{Msg, tr};
use crate::pages::parts::{failure, heading};
use crate::pages::shortcuts::RUN_KEYS;
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
    fn words(self, locale: &Locale) -> String {
        tr(
            locale,
            match self {
                Self::Lookup => Msg::OpsLookUp,
                Self::Check => Msg::OpsCheck,
                Self::Expand => Msg::OpsExpand,
                Self::Translate => Msg::OpsTranslate,
            },
        )
    }
}

/// The Operations page on the tab of `op`: the options row `form` over the
/// `answer`, both in the one plain `GET` form Run submits, which is the
/// tab's panel (`fui-panel`) so its sections run edge to edge.
pub(crate) fn frame(request: &Request, op: Op, form: Html, answer: Html) -> Html {
    let locale = &request.locale;
    let tabs = Op::ALL
        .into_iter()
        .map(|each| {
            Tab::new(
                each.path(),
                ui_link(each.path(), request.version),
                each.words(locale),
            )
        })
        .collect();
    let panel = Html::raw(format!(
        r#"<form method="get" action="{}" class="fui-panel">{}</form>"#,
        Html::text(op.route()),
        Html::join([form, answer])
    ));
    Html::join([
        heading(
            request,
            tr(locale, Msg::OpsTitle),
            Some(tr(locale, Msg::OpsSubtitle)),
            Vec::new(),
            ActionGroup::default(),
        ),
        binder(locale, tr(locale, Msg::OpsTabs), tabs, op.path(), panel),
    ])
}

/// The options row: `fields` with `run` at its end, keeping the FHIR version
/// and the `hidden` fields.
pub(crate) fn options(
    request: &Request,
    hidden: &[(&'static str, String)],
    fields: Html,
    run: Html,
) -> Html {
    let hidden = Html::join(hidden.iter().map(|(name, value)| hidden_field(name, value)));
    transform_options(
        Density::Dense,
        Html::join([
            hidden_field(VERSION_PARAM, request.version.segment()),
            hidden,
            fields,
        ]),
        run,
    )
}

/// A hidden field named `name` carrying `value`.
pub(crate) fn hidden_field(name: &str, value: &str) -> Html {
    Html::raw(format!(
        r#"<input type="hidden" name="{}" value="{}">"#,
        Html::text(name),
        Html::text(value)
    ))
}

/// Run, its `label` and `glyph`, naming the `operation` it calls and
/// pressed by [`RUN_KEYS`].
pub(crate) fn run(locale: &Locale, id: &str, label: String, glyph: Glyph, operation: &str) -> Html {
    run_button(Run {
        id: id.to_owned(),
        label,
        busy: tr(locale, Msg::OpsRunning),
        glyph,
        shortcut: Some(RUN_KEYS.to_owned()),
        operation: Some(operation.to_owned()),
        state: RunState::Ready,
    })
}

/// An `<option>` of `value` reading `words`, chosen when `selected`.
pub(crate) fn option(value: &str, words: &str, selected: bool) -> Html {
    Html::raw(format!(
        r#"<option value="{}"{}>{}</option>"#,
        Html::text(value),
        if selected { " selected" } else { "" },
        Html::text(words)
    ))
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
) -> Html {
    let blank = blank.map(|words| option("", &words, picked.is_empty()));
    let kept = (!picked.is_empty() && !choices.iter().any(|(value, _)| value == picked))
        .then(|| option(picked, picked, true));
    let offered = Html::join(
        choices
            .iter()
            .map(|(value, words)| option(value, words, value == picked)),
    );
    Field::new(id, label).view(Html::raw(format!(
        r#"<select id="{id}" name="{name}" class="{SELECT}">{}{}{offered}</select>"#,
        blank.unwrap_or_default(),
        kept.unwrap_or_default(),
    )))
}

/// A text field named `name` holding `value`, with the hint `placeholder`.
pub(crate) fn text(
    id: &'static str,
    name: &'static str,
    label: String,
    value: &str,
    placeholder: String,
) -> Html {
    Field::new(id, label).view(Html::raw(format!(
        r#"<input id="{id}" name="{name}" class="{INPUT}" value="{}" placeholder="{}" autocomplete="off">"#,
        Html::text(value),
        Html::text(placeholder)
    )))
}

/// The resources of `resource_type` this root publishes, as a picker offers
/// them: `None` for a refused search, whose picker then offers nothing.
///
/// # Errors
///
/// Returns the failure of an answer that does not decode at its release, a
/// defect the page draws as its error state rather than an empty picker.
pub(crate) async fn published(
    request: &Request,
    resource_type: &str,
) -> Result<Option<NamedSearch>, FhirError> {
    match request
        .client
        .published_names(request.version, resource_type)
        .await
    {
        Ok(named) => Ok(Some(named)),
        Err(error @ FhirError::Decode { .. }) => Err(error),
        Err(_refused) => Ok(None),
    }
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
pub(crate) fn before(locale: &Locale, line: String) -> Html {
    panel_section(locale, None, transform_quiet(line))
}

/// The verdict line of an answer: how it went, in a sentence, and its one
/// action.
pub(crate) fn verdict(
    locale: &Locale,
    outcome: Outcome,
    title: Html,
    action: Option<Html>,
) -> Html {
    panel_section(
        locale,
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

/// A run the server refused, or that got no answer, with the request it was
/// sent as.
pub(crate) fn refused(locale: &Locale, error: &FhirError) -> Html {
    Html::join([
        panel_section(locale, None, failure(locale, error)),
        sent(locale, error.url(), None),
    ])
}

/// The request an answer came from, `GET url`, with Copy, and the answer
/// as the server sent it under `raw` when there is one.
pub(crate) fn sent(locale: &Locale, url: &str, raw: Option<(String, String)>) -> Html {
    let raw = raw.map(|(summary, text)| {
        let code = Code {
            label: summary.clone(),
            language: Language::Json,
            text,
            ids: None,
            current: None,
            diagnostics: Vec::new(),
        };
        (summary, code_viewer(locale, None, &code))
    });
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::OpsRequest),
            count: None,
            actions: None,
        }),
        request(
            locale,
            ferrohealth_ui::view::transform::Request {
                text: format!("GET {url}"),
                copy: Some(String::from("operation-request-copy")),
                raw,
            },
        ),
    )
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;
    use ferrohealth_ui::icon::Glyph;

    use super::run;

    #[test]
    fn run_announces_ctrl_enter_for_the_keys_module() {
        let html = run(
            &Locale::default(),
            "lookup-run",
            String::from("Look up"),
            Glyph::PLAY,
            "$lookup",
        );
        let html = html.as_str();
        assert!(
            html.contains("aria-keyshortcuts=\"Control+Enter\" data-fui=\"keys\""),
            "{html}"
        );
    }
}
