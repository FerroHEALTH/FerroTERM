// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the three editors share: the one form an editor is, posted to its
//! own address; the rows a post carries; the docked Save; and what the
//! server answered a save with.
//!
//! An editor works with no WebAssembly: Add, Remove, Discard and Save are
//! submit buttons of the editor's form, so every press posts the whole draft
//! and the server draws the page again from what was posted
//! (`crate::posted`). A save is an ordinary create or update; an update
//! states the version the reader opened in `If-Match`, so an edit made
//! elsewhere since is refused with `412` and the draft stays in the form
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>). No specification
//! governs the layout: our own design.

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dock::{
    Decision, DecisionBar, DecisionState, DockForm, DockLine, decision_bar,
};
use ferrohealth_ui::view::form::{Field, INPUT};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::refusal::refusal;
use ferrohealth_ui::view::surface::{Crumb, Header};
use ferrohealth_ui::widget::action_menu::{Act, Action};
use http::StatusCode;
use leptos::prelude::*;

use crate::fhir::error::FhirError;
use crate::fhir::write::Written;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::detail::Kind;
use crate::pages::parts::{ago, headed, refusal_of};
use crate::posted::Posted;
use crate::request::Request;
use crate::routes::sign_in_link;

/// The id of an editor's form, which the docked bar's decisions join.
pub(crate) const FORM_ID: &str = "editor";

/// The posted field carrying the version the reader opened.
pub(crate) const OPENED: &str = "opened";

/// The posted field carrying when the reader opened it.
pub(crate) const OPENED_AT: &str = "openedAt";

/// The intent of Save.
pub(crate) const SAVE: &str = "save";

/// The intent of Discard.
pub(crate) const DISCARD: &str = "discard";

/// The address parameter naming the version a save wrote.
pub(crate) const SAVED_PARAM: &str = "saved";

/// The rows of the list `prefix` a post carries, in the order the form
/// drew them: each row's key and its fields, from names written
/// `prefix.key.field`.
#[must_use]
pub(crate) fn rows(posted: &Posted, prefix: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut found: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    let lead = format!("{prefix}.");
    for (name, value) in posted.pairs() {
        let Some(rest) = name.strip_prefix(&lead) else {
            continue;
        };
        let Some((key, field)) = rest.split_once('.') else {
            continue;
        };
        match found.iter_mut().find(|(held, _)| held == key) {
            Some((_, fields)) => {
                fields
                    .entry(field.to_owned())
                    .or_insert_with(|| value.clone());
            }
            None => found.push((
                key.to_owned(),
                BTreeMap::from([(field.to_owned(), value.clone())]),
            )),
        }
    }
    found
}

/// The field `name` of a row's `fields`, trimmed, empty when it has none.
#[must_use]
pub(crate) fn field(fields: &BTreeMap<String, String>, name: &str) -> String {
    fields
        .get(name)
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// The name of a row's field, as [`rows`] reads it back.
#[must_use]
pub(crate) fn name(prefix: &str, key: &str, field: &str) -> String {
    format!("{prefix}.{key}.{field}")
}

/// A labelled text field: its id, the name it posts, its label, its value,
/// and whether it reads in the monospace face, as codes and names do.
#[must_use]
pub(crate) fn text_field(
    id: &'static str,
    name: &'static str,
    label: String,
    value: String,
    mono: bool,
) -> AnyView {
    let class = if mono {
        format!("{INPUT} fui-mono")
    } else {
        INPUT.to_owned()
    };
    Field::new(id, label)
        .view(view! { <input id=id name=name class=class value=value /> })
        .into_any()
}

/// The editor's form around `body`, posted to `action`, Ctrl S saving it and,
/// when it can `run` a preview, Ctrl Enter running it.
pub(crate) fn form(
    action: String,
    hidden: &[(&'static str, String)],
    run: bool,
    body: AnyView,
) -> AnyView {
    let hidden = hidden
        .iter()
        .map(|(name, value)| view! { <input type="hidden" name=*name value=value.clone() /> })
        .collect_view();
    view! {
        <form id=FORM_ID method="post" action=action data-save="" data-run=run.then_some("")>
            {hidden}
            {body}
        </form>
    }
    .into_any()
}

/// The editor's heading: the trail to the listing and the resource, the
/// title `title` and the line `line`.
pub(crate) fn heading(
    request: &Request,
    kind: Kind,
    resource: Option<(String, String)>,
    title: String,
    line: String,
) -> AnyView {
    let mut crumbs = vec![kind.listing(request.version)];
    if let Some((name, href)) = resource {
        crumbs.push(Crumb::new(name, href));
    }
    headed(
        Header {
            title: title.into(),
            subtitle: Some(line.into()),
            crumbs,
            mono: false,
            badge: None,
        },
        ActionGroup::none(),
    )
}

/// The line under an editor's title: the version opened and how long ago.
#[must_use]
pub(crate) fn opened_line(version: &str, opened_at: &str) -> String {
    let since = opened_at
        .parse::<jiff::Timestamp>()
        .map(|then| {
            ago(jiff::Timestamp::now()
                .as_second()
                .saturating_sub(then.as_second()))
        })
        .unwrap_or_default();
    match (version.is_empty(), since.is_empty()) {
        (true, _) => tr(Msg::EditorNew),
        (false, true) => tr_with(
            Msg::EditorVersion,
            &[("version", version.to_owned().into())],
        ),
        (false, false) => tr_with(
            Msg::EditorOpened,
            &[
                ("version", version.to_owned().into()),
                ("since", since.into()),
            ],
        ),
    }
}

/// What an editor's draft stands at, for the docked bar's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Nothing changed since it was opened.
    Unchanged,
    /// The draft differs from what was opened.
    Changed,
    /// The draft names too little to save, for the reason given.
    Incomplete(String),
    /// The last save was refused.
    Refused,
    /// The last save found the session ended.
    Expired,
}

/// The editor's docked bar (R4): Discard and Save, `save` its words, the
/// line saying where the draft stands. It is the last part of the page, so
/// it docks to the foot of the main area while the reader scrolls.
#[must_use]
pub(crate) fn dock(save: String, standing: &Standing) -> AnyView {
    let (line, state) = match standing {
        Standing::Unchanged => (
            Some(state_line(Glyph::INFO, tr(Msg::EditorUnchanged))),
            DecisionState::Ready,
        ),
        Standing::Changed => (
            Some(state_line(Glyph::PENCIL, tr(Msg::EditorChanged))),
            DecisionState::Ready,
        ),
        // NOTE: no specification governs this: our own design; Save stays pressable, since
        // the reader may complete the draft in the form before pressing it.
        Standing::Incomplete(reason) => (
            Some(state_line(Glyph::INFO, reason.clone())),
            DecisionState::Ready,
        ),
        Standing::Refused => (
            Some(state_line(Glyph::CIRCLE_X, tr(Msg::EditorRefusedLine))),
            DecisionState::Ready,
        ),
        Standing::Expired => (
            Some(state_line(
                Glyph::TRIANGLE_ALERT,
                tr(Msg::EditorExpiredLine),
            )),
            DecisionState::Ready,
        ),
    };
    decision_bar(DecisionBar {
        id: String::from("editor-dock"),
        label: tr(Msg::EditorDock),
        form: DockForm::Page(String::from(FORM_ID)),
        fields: Vec::new(),
        line,
        decisions: vec![
            Decision {
                name: String::from(crate::posted::INTENT),
                value: String::from(DISCARD),
                label: tr(Msg::EditorDiscard),
                variant: Variant::Secondary,
                glyph: Some(Glyph::X),
                ..Decision::default()
            },
            Decision {
                name: String::from(crate::posted::INTENT),
                value: String::from(SAVE),
                label: save,
                variant: Variant::Primary,
                glyph: Some(Glyph::SAVE),
                shortcut: Some(String::from("Ctrl S")),
                state,
            },
        ],
    })
}

/// A state line of the docked bar.
fn state_line(glyph: Glyph, said: String) -> DockLine {
    DockLine::State {
        glyph: Some(glyph),
        said,
    }
}

/// What the server answered a save with, over the draft: the session that
/// ended with the way to sign in again, the conflict with the way to the
/// current version at `reload`, or the refusal in the server's words.
pub(crate) fn refused(request: &Request, error: &FhirError, reload: &str) -> (AnyView, Standing) {
    match error.status() {
        Some(StatusCode::UNAUTHORIZED) => {
            let again = action_button(
                Action::new(
                    Some(Glyph::LOG_IN),
                    tr(Msg::EditorSignInAgain),
                    Act::Link(sign_in_link(&request.back())),
                ),
                Variant::Primary,
            );
            (
                view! {
                    {notice(Tone::Warn, Announce::Alert, None, tr(Msg::EditorExpired))}
                    <p>{again}</p>
                }
                .into_any(),
                Standing::Expired,
            )
        }
        Some(StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT) => {
            let mut said = refusal_of(error);
            said.summary = tr(Msg::EditorConflict);
            let again = action_button(
                Action::new(
                    Some(Glyph::REFRESH_CW),
                    tr(Msg::EditorReload),
                    Act::Link(reload.to_owned()),
                ),
                Variant::Primary,
            );
            (
                view! {
                    {refusal(&said)}
                    <p>{again}</p>
                }
                .into_any(),
                Standing::Refused,
            )
        }
        _ => (refusal(&refusal_of(error)).into_any(), Standing::Refused),
    }
}

/// The page for a resource the deployment loaded: it has no write path, so
/// the editor says so and leads back to the resource at `back`.
pub(crate) fn unmanaged(request: &Request, kind: Kind, back: String) -> AnyView {
    let open = action_button(
        Action::new(
            Some(Glyph::ARROW_LEFT),
            tr(Msg::EditorBackToResource),
            Act::Link(back),
        ),
        Variant::Secondary,
    );
    view! {
        {heading(request, kind, None, tr(Msg::EditorEdit), String::new())}
        {notice(Tone::Info, Announce::Status, None, tr(Msg::EditorUnmanaged))}
        <p>{open}</p>
    }
    .into_any()
}

/// A quiet submit button of the editor's form that asks for `intent`, such
/// as Add a concept: it posts the draft, and the server answers with the
/// row added.
#[must_use]
pub(crate) fn add_button(intent: String, words: String) -> AnyView {
    view! {
        <button
            type="submit"
            class=Variant::Ghost.small()
            name=crate::posted::INTENT
            value=intent
            formnovalidate=true
        >
            {ferrohealth_ui::icon::icon(Glyph::PLUS, "16")}
            {words}
        </button>
    }
    .into_any()
}

/// An icon button of the editor's form that asks for `intent`, named
/// `words`, such as the Remove of a clause.
#[must_use]
pub(crate) fn icon_submit(intent: String, glyph: Glyph, words: String) -> AnyView {
    let title = words.clone();
    view! {
        <button
            type="submit"
            class="fui-icon-button"
            name=crate::posted::INTENT
            value=intent
            formnovalidate=true
            aria-label=words
            title=title
        >
            {ferrohealth_ui::icon::icon(glyph, "16")}
        </button>
    }
    .into_any()
}

/// Sends a save of `body`: a create for a resource with no `id`, else an
/// update stating `opened`, the version the reader opened, in `If-Match`
/// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
///
/// # Errors
///
/// Returns the [`FhirError`] the server answered, a `412` among them.
pub(crate) async fn send(
    request: &Request,
    resource_type: &str,
    id: &str,
    body: &str,
    opened: &str,
) -> Result<Written, FhirError> {
    if id.is_empty() {
        request
            .client
            .create(request.version, resource_type, body)
            .await
    } else {
        let stated = Some(opened).filter(|version| !version.is_empty());
        request
            .client
            .update(request.version, resource_type, id, body, stated)
            .await
    }
}

/// A value percent-encoded as one query component.
#[must_use]
pub(crate) fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::{field, rows};
    use crate::posted::Posted;

    #[test]
    fn rows_come_back_in_the_order_the_form_drew_them() {
        let posted = Posted::parse(b"concept.k2.code=B&concept.k1.code=A&concept.k2.display=Bee&other=x&concept.new-1.code=");
        let read = rows(&posted, "concept");
        let keys: Vec<&str> = read.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(vec!["k2", "k1", "new-1"], keys);
        assert_eq!("Bee", field(&read[0].1, "display"));
        assert_eq!("", field(&read[1].1, "display"));
    }
}
