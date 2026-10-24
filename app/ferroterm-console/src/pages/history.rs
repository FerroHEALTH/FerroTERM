// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The History tab of a stored resource: its revisions, what two of them
//! differ by, and putting an earlier one back.
//!
//! The list is read with whichever interaction the root declares,
//! `history-instance` or `vread` (<https://hl7.org/fhir/R4B/http.html#history>,
//! <https://hl7.org/fhir/R4B/http.html#vread>). A restore is a plain form
//! post: the console sends an update of the whole earlier resource with
//! `If-Match` of the revision the reader saw as current
//! (<https://hl7.org/fhir/R4B/http.html#concurrency>), so an edit made
//! elsewhere since is refused with `412` and nothing changes. The difference
//! between two revisions is our own design.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::change::{Change, change_table};
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::form::{Field, SELECT, action_field, field_flow};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::refusal::refusal;
use ferrohealth_ui::view::surface::{SectionHead, panel_section, panel_sections, section_intro};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;
use serde_json::Value;

use crate::auth::scopes::Letter;
use crate::authoring::history::{Comparison, Opened, Subject, Support, open, restore};
use crate::fhir::error::FhirError;
use crate::fhir::write::Version;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::detail::{Head, Kind, Links, Pane, header, line, tabbed};
use crate::pages::parts::{Source, failure, heading, loaded, refusal_of, relative};
use crate::posted::{Posted, posted};
use crate::request::{Request, request};
use crate::routes::{HISTORY_ROUTE, ID_PARAM, TYPE_PARAM, editing_link, history_link, screen_link};

/// The query parameter naming the earlier revision of a comparison.
const FROM_PARAM: &str = "from";

/// The query parameter naming the later revision of a comparison.
const TO_PARAM: &str = "to";

/// The query parameter naming the revision a restore put back.
const RESTORED_PARAM: &str = "restored";

/// The query parameter naming the revision a restore wrote.
const AS_PARAM: &str = "as";

/// The intent of a restore form.
const RESTORE: &str = "restore";

/// The posted field naming the revision to put back.
const REVISION_FIELD: &str = "revision";

/// The posted field naming the revision the reader saw as current.
const CURRENT_FIELD: &str = "current";

/// What a posted restore came to, when the page was posted one.
enum Restored {
    /// The server took the update: the revision put back, and the one the
    /// write made.
    Written {
        /// The revision the reader chose.
        revision: String,
        /// The revision the server wrote.
        written: String,
    },
    /// The server refused the update.
    Refused(FhirError),
    /// The chosen revision holds no resource to put back.
    Empty,
}

/// The History tab of the resource the address names.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn HistoryPage() -> impl IntoView {
    let request = request();
    let subject = Subject {
        resource_type: request.text(TYPE_PARAM),
        id: request.text(ID_PARAM),
    };
    let Some(kind) = Kind::of(&subject.resource_type).filter(|_| !subject.id.is_empty()) else {
        return crate::pages::errors::not_found_page();
    };
    let posted = posted().filter(|form| form.intent() == RESTORE);
    loaded(async move {
        let support = match request.client.capability_statement(request.version).await {
            Ok(statement) => Support::of(&statement, &subject.resource_type),
            Err(error) => return refused_page(&subject, &error),
        };
        let restored = match &posted {
            Some(form) => send_restore(&request, support, &subject, form).await,
            None => None,
        };
        match open(&request.client, request.version, support, subject.clone()).await {
            Ok(opened) => page(&request, kind, &opened, restored.as_ref()),
            Err(error) => refused_page(&subject, &error),
        }
    })
}

/// The page of a resource whose revisions did not read.
fn refused_page(subject: &Subject, error: &FhirError) -> AnyView {
    view! {
        {heading(
            format!("{}/{}", subject.resource_type, subject.id),
            None,
            Vec::new(),
            ActionGroup::none(),
        )}
        {failure(error)}
    }
    .into_any()
}

/// Sends the restore `form` asks for and, when the server takes it,
/// redirects to the history saying so.
///
/// The page drawn in answer to the post says what was written too, so the
/// outcome stands whichever of the two pages the browser ends on.
async fn send_restore(
    request: &Request,
    support: Support,
    subject: &Subject,
    form: &Posted,
) -> Option<Restored> {
    let revision = form.text(REVISION_FIELD);
    let current = form.text(CURRENT_FIELD);
    let opened = match open(&request.client, request.version, support, subject.clone()).await {
        Ok(opened) => opened,
        Err(error) => return Some(Restored::Refused(error)),
    };
    let Some(chosen) = opened.resource_at(&revision) else {
        return Some(Restored::Empty);
    };
    let sent = restore(chosen, &subject.id, &current);
    let stated = Some(sent.version_id.as_str()).filter(|stated| !stated.is_empty());
    match request
        .client
        .update(
            request.version,
            &subject.resource_type,
            &subject.id,
            &sent.body,
            stated,
        )
        .await
    {
        Ok(written) => {
            let written = written.version_id().unwrap_or_default().to_owned();
            let back = format!(
                "{}&{RESTORED_PARAM}={}&{AS_PARAM}={}",
                history_link(&subject.resource_type, &subject.id, request.version),
                percent(&revision),
                percent(&written)
            );
            leptos_axum::redirect(&back);
            Some(Restored::Written { revision, written })
        }
        Err(error) => Some(Restored::Refused(error)),
    }
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// A string element of `resource`, when it states one.
fn element<'a>(resource: Option<&'a Value>, name: &str) -> Option<&'a str> {
    resource?
        .get(name)?
        .as_str()
        .filter(|text| !text.is_empty())
}

/// The page of `opened`, after a restore that came to `restored`.
fn page(request: &Request, kind: Kind, opened: &Opened, restored: Option<&Restored>) -> AnyView {
    let current = opened.current.as_ref();
    let subject = &opened.subject;
    let canonical = opened.canonical();
    let title = element(current, "title")
        .or_else(|| element(current, "name"))
        .unwrap_or(&subject.id)
        .to_owned();
    let writable = request.can(&subject.resource_type, Letter::Update);
    let actions = editing_link(
        &subject.resource_type,
        &canonical,
        &subject.id,
        request.version,
    )
    .filter(|_| writable)
    .map_or_else(ActionGroup::none, |href| {
        ActionGroup::primary(Action::new(
            Some(Glyph::PENCIL),
            tr(Msg::CodeSystemEdit),
            Act::Link(href),
        ))
    });
    let head = header(
        Head {
            kind,
            title,
            status: element(current, "status").map(str::to_owned),
            canonical: canonical.clone(),
            line: line(element(current, "version"), Source::Stored),
            actions,
        },
        request.version,
    );
    let page_link = screen_link(
        &subject.resource_type,
        &canonical,
        &subject.id,
        request.version,
    )
    .map(|(href, _)| href)
    .unwrap_or_default();
    let links = Links {
        page: page_link,
        history: Some(subject.id.clone()),
    };
    let panel = view! {
        {outcome(request, opened, restored)}
        {panel_sections(
            view! {
                {revisions(request, opened, writable)}
                {comparison(request, opened)}
            },
        )}
    }
    .into_any();
    view! {
        {head}
        {tabbed(kind, Pane::History, &links, request.version, panel)}
    }
    .into_any()
}

/// What the last restore came to: the revision it wrote, the refusal with
/// the way to read the revisions again, or nothing.
fn outcome(request: &Request, opened: &Opened, restored: Option<&Restored>) -> Option<AnyView> {
    let subject = &opened.subject;
    match restored {
        Some(Restored::Written { revision, written }) => {
            Some(restored_notice(revision.clone(), written.clone()))
        }
        Some(Restored::Refused(error)) => {
            let mut refused = refusal_of(error);
            if error.status() == Some(http::StatusCode::PRECONDITION_FAILED) {
                refused.summary = tr(Msg::HistoryConflict);
            }
            let reload = action_button(
                Action::new(
                    Some(Glyph::REFRESH_CW),
                    tr(Msg::HistoryReload),
                    Act::Link(history_link(
                        &subject.resource_type,
                        &subject.id,
                        request.version,
                    )),
                ),
                Variant::Primary,
            );
            Some(
                view! {
                    {refusal(&refused)}
                    <p>{reload}</p>
                }
                .into_any(),
            )
        }
        Some(Restored::Empty) => Some(
            notice(
                Tone::Bad,
                Announce::Alert,
                None,
                tr(Msg::VersionCarriesNoResourcePutBack),
            )
            .into_any(),
        ),
        None => {
            let restored = request.param(RESTORED_PARAM)?;
            Some(restored_notice(restored, request.text(AS_PARAM)))
        }
    }
}

/// The status message that names the revision put back and the one written.
fn restored_notice(revision: String, written: String) -> AnyView {
    notice(
        Tone::Ok,
        Announce::Status,
        None,
        tr_with(
            Msg::HistoryRestored,
            &[("revision", revision.into()), ("written", written.into())],
        ),
    )
    .into_any()
}

/// The words of the interaction that made `version`.
fn interaction(version: &Version) -> String {
    tr(match version.method.as_deref() {
        Some("POST") => Msg::HistoryCreated,
        Some("DELETE") => Msg::HistoryDeleted,
        Some("PUT" | "PATCH") => Msg::HistoryUpdated,
        _ => return String::new(),
    })
}

/// The revisions, the current one marked, each earlier one with Restore.
fn revisions(request: &Request, opened: &Opened, writable: bool) -> AnyView {
    let current = opened.current_version_id();
    let now = jiff::Timestamp::now();
    let mut columns = vec![
        Column::text("revision", tr(Msg::HistoryRevision)).role(Role::Title),
        Column::text("changed", tr(Msg::HistoryChanged)),
        Column::text("interaction", tr(Msg::HistoryInteraction)),
    ];
    if writable {
        columns.push(Column::text("actions", tr(Msg::HistoryActions)).role(Role::Wide));
    }
    let changed = columns.get(1).cloned();
    let made = columns.get(2).cloned();
    let act = columns.get(3).cloned();
    let subject = &opened.subject;
    let rows = opened
        .versions
        .iter()
        .map(|version| {
            let is_current = version.id == current;
            let mark = is_current.then(
                || view! { <span class=Tone::Accent.badge()>{tr(Msg::HistoryCurrent)}</span> },
            );
            let name = view! {
                {tr_with(Msg::HistoryRevisionN, &[("revision", version.id.clone().into())])}
                " "
                {mark}
            };
            let when = version
                .last_updated
                .as_deref()
                .map(|raw| relative(raw, now));
            let restore = (!is_current && writable && version.resource.is_some())
                .then(|| restore_control(request, subject, version, &current));
            view! {
                <tr>
                    {title_cell(None, name, None)}
                    {changed.as_ref().map(|column| cell(column, when))}
                    {made.as_ref().map(|column| cell(column, interaction(version)))}
                    {act.as_ref().map(|column| cell(column, restore))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::HistoryRevisions);
    let count = u64::try_from(opened.versions.len()).unwrap_or(u64::MAX);
    let body = if opened.versions.is_empty() {
        Body::Empty(section_intro(opened.support.why_empty()))
    } else {
        Body::Rows(rows)
    };
    let intro = if opened.truncated {
        tr(Msg::HistoryTruncated)
    } else {
        tr(Msg::HistoryIntro)
    };
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count)),
            actions: None,
        }),
        view! {
            {section_intro(intro)}
            {data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body,
            })}
        },
    )
}

/// One row's Restore: the trigger and the dialog it opens, whose confirm
/// posts the restore.
fn restore_control(
    request: &Request,
    subject: &Subject,
    version: &Version,
    current: &str,
) -> AnyView {
    // NOTE: https://hl7.org/fhir/R4B/datatypes.html#id: `meta.versionId` is an
    // `id`, `[A-Za-z0-9\-\.]{1,64}`, so it names an HTML id as it stands.
    let id = format!("restore-{}", version.id);
    let trigger = action_button(
        Action::new(
            Some(Glyph::ROTATE_CCW),
            tr(Msg::HistoryRestore),
            Act::Dialog(id.clone()),
        ),
        Variant::Ghost,
    );
    let words = tr_with(
        Msg::HistoryRestoreN,
        &[("revision", version.id.clone().into())],
    );
    let confirm = action_button(
        Action::new(
            Some(Glyph::ROTATE_CCW),
            words.clone(),
            Act::Post {
                action: format!("{HISTORY_ROUTE}?{}", request.query),
                fields: vec![
                    (String::from(crate::posted::INTENT), String::from(RESTORE)),
                    (String::from(REVISION_FIELD), version.id.clone()),
                    (String::from(CURRENT_FIELD), current.to_owned()),
                ],
            },
        ),
        Variant::Primary,
    );
    let dialog = confirm_dialog(
        Confirmation {
            id: id.into(),
            title: tr_with(
                Msg::HistoryRestoreQuestion,
                &[("revision", version.id.clone().into())],
            )
            .into(),
            message: tr_with(
                Msg::HistoryRestoreMessage,
                &[
                    ("revision", version.id.clone().into()),
                    ("current", current.to_owned().into()),
                    ("kind", subject.resource_type.clone().into()),
                ],
            )
            .into(),
        },
        confirm,
    );
    view! {
        {trigger}
        {dialog}
    }
    .into_any()
}

/// The revisions as the options of a select, `picked` chosen and `current`
/// named as the current one.
fn revision_options(opened: &Opened, current: &str, picked: &str) -> AnyView {
    opened
        .versions
        .iter()
        .map(|version| {
            let words = if version.id == current {
                tr_with(
                    Msg::HistoryRevisionCurrent,
                    &[("revision", version.id.clone().into())],
                )
            } else {
                tr_with(
                    Msg::HistoryRevisionN,
                    &[("revision", version.id.clone().into())],
                )
            };
            view! {
                <option value=version.id.clone() selected=version.id == picked>
                    {words}
                </option>
            }
        })
        .collect_view()
        .into_any()
}

/// The comparison of two revisions: a `GET` form choosing them and what
/// they differ by.
fn comparison(request: &Request, opened: &Opened) -> AnyView {
    let current = opened.current_version_id();
    let from = request.text(FROM_PARAM);
    let to = request.text(TO_PARAM);
    let earlier = opened
        .versions
        .get(1)
        .map(|version| version.id.clone())
        .unwrap_or_default();
    let picked_from = if from.is_empty() {
        earlier
    } else {
        from.clone()
    };
    let picked_to = if to.is_empty() {
        current.clone()
    } else {
        to.clone()
    };
    let subject = &opened.subject;
    let choose = view! {
        <form method="get" action=HISTORY_ROUTE>
            <input
                type="hidden"
                name=crate::routes::VERSION_PARAM
                value=request.version.segment()
            />
            <input type="hidden" name=TYPE_PARAM value=subject.resource_type.clone() />
            <input type="hidden" name=ID_PARAM value=subject.id.clone() />
            {field_flow(
                view! {
                    {Field::new("history-from", tr(Msg::HistoryEarlier))
                        .view(
                            view! {
                                <select id="history-from" name=FROM_PARAM class=SELECT>
                                    {revision_options(opened, &current, &picked_from)}
                                </select>
                            },
                        )}
                    {Field::new("history-to", tr(Msg::HistoryLater))
                        .view(
                            view! {
                                <select id="history-to" name=TO_PARAM class=SELECT>
                                    {revision_options(opened, &current, &picked_to)}
                                </select>
                            },
                        )}
                    {action_field(
                        view! {
                            <button type="submit" class=Variant::Secondary.class()>
                                {ferrohealth_ui::icon::icon(Glyph::GIT_COMPARE, "16")}
                                {tr(Msg::HistoryCompare)}
                            </button>
                        },
                        (),
                    )}
                },
            )}
        </form>
    };
    let compared = if from.is_empty() || to.is_empty() {
        Comparison::Unasked
    } else {
        Comparison::of(opened.resource_at(&from), opened.resource_at(&to))
    };
    let said = match &compared {
        Comparison::Unasked => tr(Msg::HistoryChooseTwo),
        Comparison::Unread => compared.sentence(),
        Comparison::Differences(found) => tr_with(
            Msg::HistoryDiffer,
            &[
                ("count", found.len().into()),
                ("from", from.clone().into()),
                ("to", to.clone().into()),
            ],
        ),
    };
    let changes = compared
        .rows()
        .iter()
        .map(|row| Change {
            key: row.path.clone(),
            from: Some(row.before.clone()).filter(|text| !text.is_empty()),
            to: Some(row.after.clone()).filter(|text| !text.is_empty()),
        })
        .collect::<Vec<_>>();
    let table = (!changes.is_empty()).then(|| change_table(tr(Msg::HistoryWhatChanged), &changes));
    panel_section(
        Some(SectionHead {
            title: tr(Msg::HistoryCompareTitle),
            count: None,
            actions: None,
        }),
        view! {
            {choose}
            {section_intro(said)}
            {table}
        },
    )
}
