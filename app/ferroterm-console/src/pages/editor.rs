// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the three editors share: the one form an editor is, posted to its
//! own address; the rows a post carries; the docked Save; and what the
//! server answered a save with.
//!
//! An editor works with no script: Add, Remove, Discard and Save are
//! submit buttons of the editor's form, so every press posts the whole draft
//! and the server draws the page again from what was posted
//! (`crate::posted`). A save is an ordinary create or update; an update
//! states the version the reader opened in `If-Match`, so an edit made
//! elsewhere since is refused with `412` and the draft stays in the form
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>). No specification
//! governs the layout: our own design.

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dock::{
    Decision, DecisionBar, DecisionState, DockForm, DockLine, decision_bar,
};
use ferrohealth_ui::view::form::{FORM, Field, INPUT};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::refusal::refusal;
use ferrohealth_ui::view::surface::{Crumb, Header, SectionHead, panel_section};
use http::StatusCode;

use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};

use crate::auth::scopes::Letter;
use crate::fhir::compose::STATUSES;
use crate::fhir::error::FhirError;
use crate::fhir::write::Written;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::p;
use crate::pages::detail::Kind;
use crate::pages::operations::{hidden_field, option};
use crate::pages::parts::{ago, headed, refusal_of};
use crate::pages::shortcuts::SAVE_KEYS;
use crate::posted::Posted;
use crate::request::Request;
use crate::routes::{sign_in_link, ui_link};

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
) -> Html {
    let class = if mono {
        format!("{INPUT} fui-mono")
    } else {
        INPUT.to_owned()
    };
    Field::new(id, label).view(Html::raw(format!(
        r#"<input id="{id}" name="{name}" class="{class}" value="{}">"#,
        Html::text(value)
    )))
}

/// The editor's form around `body`, posted to `action`.
pub(crate) fn form(action: String, hidden: &[(&'static str, String)], body: Html) -> Html {
    let hidden = Html::join(hidden.iter().map(|(name, value)| hidden_field(name, value)));
    Html::raw(format!(
        r#"<form id="{FORM_ID}" method="post" action="{}">{}</form>"#,
        Html::text(action),
        Html::join([hidden, body])
    ))
}

/// The editor's heading: the trail to the listing and the resource, the
/// title `title` and the line `line`.
pub(crate) fn heading(
    request: &Request,
    kind: Kind,
    resource: Option<(String, String)>,
    title: String,
    line: String,
) -> Html {
    let mut crumbs = vec![kind.listing(&request.locale, request.version)];
    if let Some((name, href)) = resource {
        crumbs.push(Crumb::new(name, href));
    }
    headed(
        request,
        Header {
            title: title.into(),
            subtitle: Some(line.into()),
            subtitle_view: None,
            crumbs,
            mono: false,
            badge: None,
        },
        ActionGroup::default(),
    )
}

/// The line under an editor's title: the version opened and how long ago.
#[must_use]
pub(crate) fn opened_line(locale: &Locale, version: &str, opened_at: &str) -> String {
    let since = opened_at
        .parse::<jiff::Timestamp>()
        .map(|then| {
            ago(
                locale,
                jiff::Timestamp::now()
                    .as_second()
                    .saturating_sub(then.as_second()),
            )
        })
        .unwrap_or_default();
    match (version.is_empty(), since.is_empty()) {
        (true, _) => tr(locale, Msg::EditorNew),
        (false, true) => tr_with(
            locale,
            Msg::EditorVersion,
            &[("version", version.to_owned().into())],
        ),
        (false, false) => tr_with(
            locale,
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

/// The editor's docked bar (R4): Discard and Save on [`SAVE_KEYS`], `save`
/// its words, the line saying where the draft stands. It is the last part
/// of the page, so it docks to the foot of the main area while the reader
/// scrolls.
#[must_use]
pub(crate) fn dock(locale: &Locale, save: String, standing: &Standing) -> Html {
    let (line, state) = match standing {
        Standing::Unchanged => (
            Some(state_line(Glyph::INFO, tr(locale, Msg::EditorUnchanged))),
            DecisionState::Ready,
        ),
        Standing::Changed => (
            Some(state_line(Glyph::PENCIL, tr(locale, Msg::EditorChanged))),
            DecisionState::Ready,
        ),
        // NOTE: no specification governs this: our own design; Save stays pressable, since
        // the reader may complete the draft in the form before pressing it.
        Standing::Incomplete(reason) => (
            Some(state_line(Glyph::INFO, reason.clone())),
            DecisionState::Ready,
        ),
        Standing::Refused => (
            Some(state_line(
                Glyph::CIRCLE_X,
                tr(locale, Msg::EditorRefusedLine),
            )),
            DecisionState::Ready,
        ),
        Standing::Expired => (
            Some(state_line(
                Glyph::TRIANGLE_ALERT,
                tr(locale, Msg::EditorExpiredLine),
            )),
            DecisionState::Ready,
        ),
    };
    decision_bar(DecisionBar {
        id: String::from("editor-dock"),
        label: tr(locale, Msg::EditorDock),
        form: DockForm::Page(String::from(FORM_ID)),
        fields: Vec::new(),
        line,
        decisions: vec![
            Decision {
                name: String::from(crate::posted::INTENT),
                value: String::from(DISCARD),
                label: tr(locale, Msg::EditorDiscard),
                variant: Variant::Secondary,
                glyph: Some(Glyph::X),
                leaves: true,
                ..Decision::default()
            },
            Decision {
                name: String::from(crate::posted::INTENT),
                value: String::from(SAVE),
                label: save,
                variant: Variant::Primary,
                glyph: Some(Glyph::SAVE),
                shortcut: Some(SAVE_KEYS.to_owned()),
                state,
                leaves: false,
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
pub(crate) fn refused(request: &Request, error: &FhirError, reload: &str) -> (Html, Standing) {
    let locale = &request.locale;
    match error.status() {
        Some(StatusCode::UNAUTHORIZED) => {
            let again = action_button(
                locale,
                Action::new(
                    Some(Glyph::LOG_IN),
                    tr(locale, Msg::EditorSignInAgain),
                    Act::Link(sign_in_link(&request.back())),
                ),
                Variant::Primary,
            );
            (
                Html::join([
                    notice(
                        Tone::Warn,
                        Announce::Alert,
                        None,
                        Html::text(tr(locale, Msg::EditorExpired)),
                    ),
                    p(again),
                ]),
                Standing::Expired,
            )
        }
        Some(StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT) => {
            let mut said = refusal_of(locale, error);
            said.summary = tr(locale, Msg::EditorConflict);
            let again = action_button(
                locale,
                Action::new(
                    Some(Glyph::REFRESH_CW),
                    tr(locale, Msg::EditorReload),
                    Act::Link(reloaded(reload)),
                ),
                Variant::Primary,
            );
            (
                Html::join([refusal(locale, &said), p(again)]),
                Standing::Refused,
            )
        }
        _ => (
            refusal(locale, &refusal_of(locale, error)),
            Standing::Refused,
        ),
    }
}

/// The page for a resource the deployment loaded: it has no write path, so
/// the editor says so and leads back to the resource at `back`.
pub(crate) fn unmanaged(request: &Request, kind: Kind, back: String) -> Html {
    let locale = &request.locale;
    let open = action_button(
        locale,
        Action::new(
            Some(Glyph::ARROW_LEFT),
            tr(locale, Msg::EditorBackToResource),
            Act::Link(back),
        ),
        Variant::Secondary,
    );
    Html::join([
        heading(
            request,
            kind,
            None,
            tr(locale, Msg::EditorEdit),
            String::new(),
        ),
        notice(
            Tone::Info,
            Announce::Status,
            None,
            Html::text(tr(locale, Msg::EditorUnmanaged)),
        ),
        p(open),
    ])
}

/// A quiet submit button of the editor's form that asks for `intent`, such
/// as Add a concept: it posts the draft, and the server answers with the
/// row added.
#[must_use]
pub(crate) fn add_button(intent: String, words: String) -> Html {
    Html::raw(format!(
        r#"<button type="submit" class="{}" name="{}" value="{}" formnovalidate>{}{}</button>"#,
        Variant::Ghost.small(),
        crate::posted::INTENT,
        Html::text(intent),
        ferrohealth_ui::icon::icon(Glyph::PLUS, "16"),
        Html::text(words)
    ))
}

/// An icon button of the editor's form that asks for `intent`, named
/// `words`, such as the Remove of a clause.
#[must_use]
pub(crate) fn icon_submit(intent: String, glyph: Glyph, words: String) -> Html {
    Html::raw(format!(
        r#"<button type="submit" class="fui-icon-button" name="{}" value="{}" formnovalidate aria-label="{words}" title="{words}">{}</button>"#,
        crate::posted::INTENT,
        Html::text(intent),
        ferrohealth_ui::icon::icon(glyph, "16"),
        words = Html::text(words),
    ))
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

/// One editor: where it is served, who may open it, and the page a reader
/// without the scope is shown.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Guard {
    /// The route the editor is served and posted at.
    pub(crate) route: &'static str,
    /// The resource type the editor writes.
    pub(crate) resource_type: &'static str,
    /// The address parameter that names the resource it edits; empty is a
    /// create.
    pub(crate) param: &'static str,
    /// What the forbidden page says the reader lacks.
    pub(crate) forbidden: Msg,
    /// The SMART scope a write needs, as the forbidden page names it.
    pub(crate) scope: &'static str,
    /// The listing the forbidden page leads back to, and its address path.
    pub(crate) back: (Msg, &'static str),
}

/// The page a reader is shown in place of the editor `guard` names, or
/// `None` when the reader may write what it edits.
///
/// A visitor who is not signed in is sent to sign in when the console offers
/// a sign-in.
fn forbidden(request: &Request, guard: &Guard) -> Option<Html> {
    let letter = if request.text(guard.param).is_empty() {
        Letter::Create
    } else {
        Letter::Update
    };
    if request.can(guard.resource_type, letter) {
        return None;
    }
    if request.session.who().is_none() && request.sign_in {
        request.answering.redirect(&sign_in_link(&request.back()));
    }
    Some(crate::pages::errors::forbidden_page(
        request,
        guard.forbidden,
        guard.scope,
        (
            tr(&request.locale, guard.back.0),
            ui_link(guard.back.1, request.version),
        ),
    ))
}

/// The editor `guard` names, drawn by `edited` over what the reader posted,
/// or the page a reader who may not write it is shown.
pub(crate) async fn guarded(
    request: &Request,
    guard: &Guard,
    edited: impl AsyncFn(&Request, Option<&Posted>) -> Html,
) -> Html {
    match forbidden(request, guard) {
        Some(page) => page,
        None => edited(request, request.posted.as_ref()).await,
    }
}

/// `opened`, the draft an editor opened, or the page `saved_page` draws
/// when the address names the version a save wrote.
///
/// # Errors
///
/// Returns that saved page in place of the draft.
pub(crate) fn unless_saved<D>(
    request: &Request,
    opened: D,
    saved_page: fn(&Request, &D, &str) -> Html,
) -> Result<D, Html> {
    match request.param(SAVED_PARAM) {
        Some(saved) => Err(saved_page(request, &opened, &saved)),
        None => Ok(opened),
    }
}

/// The redirect back to `here` a Discard asks for, in place of the page.
pub(crate) fn discarded(request: &Request, posted: Option<&Posted>, here: &str) -> Option<Html> {
    posted
        .is_some_and(|form| form.intent() == DISCARD)
        .then(|| {
            request.answering.redirect(here);
            Html::empty()
        })
}

/// The address of the editor `guard` names, naming `key` under its
/// parameter on the reader's version; a create names nothing.
pub(crate) fn address(request: &Request, guard: &Guard, key: &str) -> String {
    let base = format!("{}?fhir={}", guard.route, request.version.segment());
    if key.is_empty() {
        base
    } else {
        format!("{base}&{}={}", guard.param, percent(key))
    }
}

/// The draft a post is written over, a copy of `opened`, with the version
/// the reader opened and when they opened it: what the posted form carried,
/// else the version `opened` states and now.
pub(crate) fn reopened<D: Metadata + Clone>(
    posted: Option<&Posted>,
    opened: &D,
) -> (D, String, String) {
    let (version, at) = posted.map_or_else(
        || {
            (
                opened.version_id().to_owned(),
                jiff::Timestamp::now().to_string(),
            )
        },
        |form| (form.text(OPENED), form.text(OPENED_AT)),
    );
    (opened.clone(), version, at)
}

/// The five metadata elements every editor's form posts, in the order of
/// [`Metadata::metadata`]: the canonical, the title, the name, the business
/// version and the status.
const METADATA_FIELDS: [&str; 5] = ["url", "title", "name", "version", "status"];

/// A draft whose canonical, title, name, business version and status a form
/// posts.
pub(crate) trait Metadata {
    /// `meta.versionId`, the version an update states in `If-Match`.
    fn version_id(&self) -> &str;

    /// The five elements, in the order of [`METADATA_FIELDS`].
    fn metadata(&mut self) -> [&mut String; 5];
}

/// Implements [`Metadata`] for `$draft`, a draft with the five elements as
/// `String` fields of their own names.
macro_rules! metadata {
    ($draft:ty) => {
        impl crate::pages::editor::Metadata for $draft {
            fn version_id(&self) -> &str {
                &self.version_id
            }

            fn metadata(&mut self) -> [&mut String; 5] {
                [
                    &mut self.url,
                    &mut self.title,
                    &mut self.name,
                    &mut self.version,
                    &mut self.status,
                ]
            }
        }
    };
}

pub(crate) use metadata;

/// Writes the metadata `form` posted over `draft`.
pub(crate) fn apply_metadata(form: &Posted, draft: &mut impl Metadata) {
    for (name, held) in METADATA_FIELDS.into_iter().zip(draft.metadata()) {
        *held = form.text(name);
    }
}

/// The metadata section of an editor's form, around `fields`.
pub(crate) fn metadata_section(locale: &Locale, fields: &Html) -> Html {
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::EditorMetadata),
            count: None,
            actions: None,
        }),
        Html::raw(format!(r#"<div class="{FORM}">{fields}</div>"#)),
    )
}

/// The name a stored resource is shown by: its title, else its canonical.
pub(crate) fn shown(title: &str, url: &str) -> String {
    if title.is_empty() {
        url.to_owned()
    } else {
        title.to_owned()
    }
}

/// The row key an intent such as `add-target:e3` names, when it names
/// `action`.
pub(crate) fn target<'a>(intent: &'a str, action: &str) -> Option<&'a str> {
    intent
        .strip_prefix(action)
        .and_then(|rest| rest.strip_prefix(':'))
}

/// The words of a publication status.
pub(crate) fn status_words(locale: &Locale, status: &str) -> String {
    tr(
        locale,
        match status {
            "active" => Msg::StatusActive,
            "retired" => Msg::StatusRetired,
            "unknown" => Msg::StatusUnknown,
            _ => Msg::StatusDraft,
        },
    )
}

/// The status select's options, `picked` selected.
pub(crate) fn status_options(locale: &Locale, picked: &str) -> Html {
    Html::join(
        STATUSES
            .iter()
            .map(|status| option(status, &status_words(locale, status), *status == picked)),
    )
}

/// Sends a save of `body`, the draft written at the reader's release, and
/// redirects to the saved page at the address `saved_at` names for what the
/// server wrote; or keeps in `answer` the refusal over the draft at `here`.
///
/// Answers whether the save was written, so the page draws nothing in place
/// of the redirect. A refusal is the server's own, or the release's refusal
/// of the stored resource the draft was written over.
pub(crate) async fn saved(
    request: &Request,
    sent: (&str, &str, Result<String, serde_json::Error>),
    (opened, here): (&str, &str),
    saved_at: impl FnOnce(&Written) -> String,
    answer: &mut Option<(Html, Standing)>,
) -> bool {
    let (resource_type, id, body) = sent;
    let written = match body {
        Ok(body) => send(request, resource_type, id, &body, opened).await,
        Err(error) => Err(FhirError::undecoded(here, request.version, &error)),
    };
    match written {
        Ok(written) => {
            let version = written.version_id().unwrap_or_default().to_owned();
            request.answering.redirect(&format!(
                "{}&{SAVED_PARAM}={}",
                saved_at(&written),
                percent(&version)
            ));
            true
        }
        Err(error) => {
            *answer = Some(refused(request, &error, here));
            false
        }
    }
}

/// The page a save redirects to: the saved resource, the version the server
/// wrote, and `open`, the action that opens it.
pub(crate) fn saved_page(
    request: &Request,
    kind: Kind,
    resource: Option<(String, String)>,
    open: Html,
    saved: &str,
) -> Html {
    let locale = &request.locale;
    Html::join([
        heading(
            request,
            kind,
            resource,
            tr(locale, Msg::EditorSavedTitle),
            tr_with(
                locale,
                Msg::EditorVersion,
                &[("version", saved.to_owned().into())],
            ),
        ),
        load_report(LoadReport {
            outcome: Outcome::Ok,
            arrival: Arrival::AfterAction,
            title: Html::text(tr_with(
                locale,
                Msg::EditorSaved,
                &[("version", saved.to_owned().into())],
            )),
            counts: Vec::new(),
            action: Some(open),
        }),
    ])
}

/// A value percent-encoded as one query component.
#[must_use]
pub(crate) fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// The editor's address `reload` asked for again, marked apart from the
/// page the refused post drew at the same address, so the browser loads it
/// afresh; the editor reads no such parameter.
fn reloaded(reload: &str) -> String {
    let separator = if reload.contains('?') { '&' } else { '?' };
    format!("{reload}{separator}reload=current")
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;

    use super::{Standing, dock, field, reloaded, rows};
    use crate::posted::Posted;

    #[test]
    fn the_docked_save_announces_ctrl_s_for_the_keys_module() {
        let html = dock(&Locale::default(), String::from("Save"), &Standing::Changed);
        let html = html.as_str();
        assert!(html.contains("aria-keyshortcuts=\"Control+S\""), "{html}");
        assert_eq!(1, html.matches("aria-keyshortcuts").count(), "{html}");
    }

    #[test]
    fn rows_come_back_in_the_order_the_form_drew_them() {
        let posted = Posted::parse(b"concept.k2.code=B&concept.k1.code=A&concept.k2.display=Bee&other=x&concept.new-1.code=");
        let read = rows(&posted, "concept");
        let keys: Vec<&str> = read.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(vec!["k2", "k1", "new-1"], keys);
        assert_eq!("Bee", field(&read[0].1, "display"));
        assert_eq!("", field(&read[1].1, "display"));
    }

    #[test]
    fn the_reload_after_a_conflict_is_marked_apart_from_the_page_it_is_on() {
        assert_eq!(
            "/codesystem?fhir=r4b&system=x&reload=current",
            reloaded("/codesystem?fhir=r4b&system=x")
        );
        assert_eq!("/compose?reload=current", reloaded("/compose"));
    }
}
