// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synchronisation (P1 to P4, P8): the runs the synchronisation service
//! made, one run's record with what it did and what it found, and Run now
//! and Activate.
//!
//! Every screen reads the service's admin listener through the console's
//! server ([`crate::fhir::sync`]), so it is drawn only for a reader whose
//! token carries the listener's product scope on a console that reaches a
//! listener; anyone else gets the not-found page, as for an address the
//! console does not serve. Run now and Activate are plain form posts to this
//! screen's own address behind a confirmation, so they work with no
//! script: the listener answers once the run has finished, and the
//! screen redirects to the list with the finished run named over it. No
//! specification governs these screens: our own design.

mod summary;

use std::collections::BTreeMap;

use base64::Engine;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Shape, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{Confirmation, Stakes, confirm_dialog};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{
    Container, Crumb, Header, SectionHead, panel_section, section_intro,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::time::timestamp;

use crate::fhir::error::FhirError;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::sync::{Finding, RunOutcome, RunRecord, RunSummary, Subject};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, p};
use crate::pages::parts::{failure, headed, heading};
use crate::posted::INTENT;
use crate::request::Request;
use crate::routes::{SYNC_PATH, SYNC_ROUTE, screen_link, ui_path};
use summary::summary_section;

/// The intent of the Run now form.
const RUN: &str = "run";

/// The intent of the Activate form.
const ACTIVATE: &str = "activate";

/// The id of the Run now confirmation.
const RUN_DIALOG: &str = "sync-run-now";

/// The id of the Activate confirmation.
const ACTIVATE_DIALOG: &str = "sync-activate";

/// The query parameter naming the run a post just finished.
const RAN_PARAM: &str = "ran";

/// The query parameter saying how that run ended.
const OUTCOME_PARAM: &str = "outcome";

/// The address of one run's screen, on its `findings` tab when asked.
fn run_link(id: &str, findings: bool) -> String {
    let base = format!("{}/{}", ui_path(SYNC_PATH), percent(id));
    if findings {
        format!("{base}/findings")
    } else {
        base
    }
}

/// A value percent-encoded as one path segment or query component.
fn percent(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// The words of what started a run.
fn trigger_words(locale: &Locale, trigger: &str) -> String {
    match trigger {
        "schedule" => tr(locale, Msg::SyncTriggerSchedule),
        "manual" => tr(locale, Msg::SyncTriggerManual),
        "run-once" => tr(locale, Msg::SyncTriggerRunOnce),
        "activate" => tr(locale, Msg::SyncTriggerActivate),
        other => other.to_owned(),
    }
}

/// How a run ended, as a state mark with its word.
fn outcome_view(locale: &Locale, outcome: RunOutcome) -> Html {
    match outcome {
        RunOutcome::Ok => state(Mark::Ok, tr(locale, Msg::SyncOutcomeOk), None),
        RunOutcome::Failed => state(Mark::Bad, tr(locale, Msg::SyncOutcomeFailed), None),
        RunOutcome::Other => state(Mark::Neutral, tr(locale, Msg::SyncOutcomeOther), None),
    }
}

/// How long `ms` milliseconds is, in minutes and seconds.
fn duration(locale: &Locale, ms: u64) -> String {
    let seconds = ms.div_euclid(1000);
    let (minutes, seconds) = (seconds.div_euclid(60), seconds.rem_euclid(60));
    if minutes == 0 {
        tr_with(
            locale,
            Msg::SyncDurationSeconds,
            &[("seconds", seconds.into())],
        )
    } else {
        tr_with(
            locale,
            Msg::SyncDurationMinutes,
            &[("minutes", minutes.into()), ("seconds", seconds.into())],
        )
    }
}

/// What a run did, in a few words.
fn did(locale: &Locale, summary: &RunSummary) -> String {
    if summary.entries_taken == 0 && summary.activated == 0 && summary.errors.is_empty() {
        tr(locale, Msg::SyncDidNothing)
    } else if let (RunOutcome::Failed, Some(first)) = (summary.outcome, summary.errors.first()) {
        first.clone()
    } else {
        tr_with(
            locale,
            Msg::SyncDid,
            &[
                ("taken", summary.entries_taken.into()),
                ("activated", summary.activated.into()),
            ],
        )
    }
}

/// What a posted Run now or Activate came to, when one was refused.
fn posted_failure(locale: &Locale, error: Option<&FhirError>) -> Html {
    error
        .map(|error| failure(locale, error))
        .unwrap_or_default()
}

/// Synchronisation: the runs, newest first, with Run now and Activate.
pub(crate) async fn page(request: Request) -> Html {
    let locale = &request.locale;
    if !request.reads_sync() {
        return crate::pages::errors::not_found_page(&request);
    }
    let form = request
        .posted
        .clone()
        .filter(|form| matches!(form.intent(), RUN | ACTIVATE));
    let refused = match form {
        Some(form) => {
            let answered = if form.intent() == RUN {
                request.client.run_now().await
            } else {
                request.client.activate().await
            };
            match answered {
                Ok(summary) => {
                    let outcome = match summary.outcome {
                        RunOutcome::Ok => "ok",
                        RunOutcome::Failed | RunOutcome::Other => "failed",
                    };
                    request.answering.redirect(&format!(
                        "{SYNC_ROUTE}?{RAN_PARAM}={}&{OUTCOME_PARAM}={outcome}",
                        percent(&summary.id)
                    ));
                    return Html::empty();
                }
                Err(error) => Some(error),
            }
        }
        None => None,
    };
    let header = heading(
        &request,
        tr(locale, Msg::ShellSynchronisation),
        Some(tr(locale, Msg::SyncSubtitle)),
        Vec::new(),
        actions(locale),
    );
    let body = match request.client.runs().await {
        Ok(runs) => runs_section(locale, &runs),
        Err(error) => failure(locale, &error),
    };
    Html::join([
        header,
        confirmations(locale),
        posted_failure(locale, refused.as_ref()),
        ran_notice(&request).unwrap_or_default(),
        Container::PanelSections.wrap(body),
    ])
}

/// The title row's actions: Run now, and Activate in More, each opening its
/// confirmation.
fn actions(locale: &Locale) -> ActionGroup {
    ActionGroup::primary(Action::new(
        Some(Glyph::REFRESH_CW),
        tr(locale, Msg::SyncRunNow),
        Act::Dialog(String::from(RUN_DIALOG)),
    ))
    .more(Menu::new(
        "sync-more",
        tr(locale, Msg::SyncMore),
        vec![Item::new(Action::new(
            Some(Glyph::PLAY),
            tr(locale, Msg::SyncActivate),
            Act::Dialog(String::from(ACTIVATE_DIALOG)),
        ))],
    ))
}

/// The two confirmations the title row's actions open, each posting its
/// intent to this screen.
fn confirmations(locale: &Locale) -> Html {
    let dialog = |id: &'static str, glyph, words: Msg, question: Msg, message: Msg, intent| {
        let confirm = action_button(
            locale,
            Action::new(
                Some(glyph),
                tr(locale, words),
                Act::Post {
                    action: String::from(SYNC_ROUTE),
                    fields: vec![(String::from(INTENT), String::from(intent))],
                },
            ),
            Variant::Primary,
        );
        confirm_dialog(
            locale,
            Confirmation {
                id: id.into(),
                title: tr(locale, question).into(),
                message: tr(locale, message).into(),
            },
            Stakes::Plain,
            None,
            confirm,
        )
    };
    Html::join([
        dialog(
            RUN_DIALOG,
            Glyph::REFRESH_CW,
            Msg::SyncRunNow,
            Msg::SyncRunNowQuestion,
            Msg::SyncRunNowMessage,
            RUN,
        ),
        dialog(
            ACTIVATE_DIALOG,
            Glyph::PLAY,
            Msg::SyncActivate,
            Msg::SyncActivateQuestion,
            Msg::SyncActivateMessage,
            ACTIVATE,
        ),
    ])
}

/// The run a post just finished, named over the list with a link to it.
fn ran_notice(request: &Request) -> Option<Html> {
    let locale = &request.locale;
    let id = request.param(RAN_PARAM).filter(|id| !id.is_empty())?;
    let ok = request.text(OUTCOME_PARAM) == "ok";
    let (tone, msg) = if ok {
        (Tone::Ok, Msg::SyncRanOk)
    } else {
        (Tone::Bad, Msg::SyncRanFailed)
    };
    let open = action_button(
        locale,
        Action::new(
            None,
            tr(locale, Msg::SyncOpenRun),
            Act::Link(run_link(&id, false)),
        ),
        Variant::Secondary,
    );
    Some(Html::join([
        notice(
            tone,
            Announce::Status,
            None,
            Html::text(tr_with(locale, msg, &[("id", id.clone().into())])),
        ),
        p(open),
    ]))
}

/// The runs, newest first, as a table.
fn runs_section(locale: &Locale, runs: &[RunSummary]) -> Html {
    let columns = [
        Column::text("run", tr(locale, Msg::SyncColumnRun)).role(Role::Title),
        Column::text("trigger", tr(locale, Msg::SyncColumnTrigger)),
        Column::text("outcome", tr(locale, Msg::SyncColumnOutcome)),
        Column::text("did", tr(locale, Msg::SyncColumnDid)).role(Role::Wide),
        Column::text("findings", tr(locale, Msg::SyncColumnFindings)),
    ];
    let [_, trigger, outcome, what, findings] = &columns;
    let rows = Html::join(runs.iter().map(|run| {
        let href = run_link(&run.id, false);
        Html::raw(format!(
            "<tr>{}{}{}{}{}</tr>",
            title_cell(None, Some(&href), timestamp(locale, &run.started), None),
            cell(trigger, Html::text(trigger_words(locale, &run.trigger))),
            cell(outcome, outcome_view(locale, run.outcome)),
            cell(what, Html::text(did(locale, run))),
            cell(
                findings,
                Html::text(tr_with(
                    locale,
                    Msg::SyncFindingsCount,
                    &[("count", run.revalidation_findings.into())],
                )),
            ),
        ))
    }));
    let caption = tr(locale, Msg::SyncRuns);
    let body = if runs.is_empty() {
        Body::Empty(section_intro(tr(locale, Msg::SyncNoRuns)))
    } else {
        Body::Rows(rows)
    };
    panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count(runs.len()))),
            actions: None,
        }),
        data_table(DataTable {
            shape: Shape::Stacked,
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body,
        }),
    )
}

/// One run's screen: its Summary or its Findings.
pub(crate) async fn run_page(request: Request) -> Html {
    let locale = &request.locale;
    if !request.reads_sync() {
        return crate::pages::errors::not_found_page(&request);
    }
    let (id, tab) = (request.segment(1), request.segment(2));
    let id = id.unwrap_or_default();
    let findings = match tab.as_deref() {
        None => false,
        Some("findings") => true,
        Some(_) => return crate::pages::errors::not_found_page(&request),
    };
    match request.client.run_record(&id).await {
        Ok(record) => {
            let ids = if findings {
                resolve(&request, &record.revalidation.findings).await
            } else {
                BTreeMap::new()
            };
            run_screen(&request, &record, findings, &ids)
        }
        Err(error) => {
            let not_found = error.status() == Some(http::StatusCode::NOT_FOUND);
            Html::join([
                heading(
                    &request,
                    tr_with(locale, Msg::SyncRunTitle, &[("when", id.clone().into())]),
                    None,
                    vec![crumb(locale)],
                    ActionGroup::default(),
                ),
                if not_found {
                    notice(
                        Tone::Bad,
                        Announce::Alert,
                        None,
                        Html::text(tr_with(
                            locale,
                            Msg::SyncNotFound,
                            &[("id", id.clone().into())],
                        )),
                    )
                } else {
                    failure(locale, &error)
                },
            ])
        }
    }
}

/// The trail's step back to the runs.
fn crumb(locale: &Locale) -> Crumb {
    Crumb::new(tr(locale, Msg::ShellSynchronisation), ui_path(SYNC_PATH))
}

/// The page of `record`, on its Findings tab when `findings`.
fn run_screen(
    request: &Request,
    record: &RunRecord,
    findings: bool,
    ids: &BTreeMap<String, String>,
) -> Html {
    let locale = &request.locale;
    let when = ferrohealth_ui::view::time::reading(locale, &record.started);
    let title = tr_with(locale, Msg::SyncRunTitle, &[("when", when.into())]);
    let encoded = base64::engine::general_purpose::STANDARD.encode(record.text.as_bytes());
    let download = Html::raw(format!(
        r#"<a class="fui-button fui-button-primary" href="data:application/json;base64,{encoded}" download="{}">{}{}</a>"#,
        Html::text(format!("run-{}.json", record.id)),
        ferrohealth_ui::icon::icon(Glyph::DOWNLOAD, "16"),
        Html::text(tr(locale, Msg::SyncDownload)),
    ));
    let header = headed(
        request,
        Header {
            title: title.into(),
            subtitle: Some(
                tr_with(
                    locale,
                    Msg::SyncTook,
                    &[("duration", duration(locale, record.duration_ms).into())],
                )
                .into(),
            ),
            subtitle_view: None,
            crumbs: vec![crumb(locale)],
            mono: false,
            badge: Some(outcome_view(locale, record.outcome)),
        },
        ActionGroup::default(),
    );
    let tabs = vec![
        Tab::new(
            "summary",
            run_link(&record.id, false),
            tr(locale, Msg::SyncTabSummary),
        ),
        Tab::new(
            "findings",
            run_link(&record.id, true),
            tr(locale, Msg::SyncTabFindings),
        ),
    ];
    let panel = if findings {
        findings_section(request, &record.revalidation.findings, ids)
    } else {
        summary_section(locale, record)
    };
    Html::join([
        header,
        p(download),
        binder(
            locale,
            tr(locale, Msg::SyncTabs),
            tabs,
            if findings { "findings" } else { "summary" },
            Container::PanelSections.wrap(panel),
        ),
    ])
}

/// A count as the catalog takes it, saturating past `u64::MAX`.
fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

/// The words of what a release did to a code.
fn kind_words(locale: &Locale, kind: &str) -> String {
    match kind {
        "inactive" => tr(locale, Msg::SyncKindInactive),
        "absent" => tr(locale, Msg::SyncKindAbsent),
        "outside-value-set" => tr(locale, Msg::SyncKindOutsideValueSet),
        other => other.to_owned(),
    }
}

/// The logical id of the stored value set or concept map each finding names
/// by its canonical, by `url` search, so its row links to its screen.
///
/// A canonical the server holds no resource of keeps its row unlinked.
async fn resolve(request: &Request, findings: &[Finding]) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for finding in findings {
        let (resource_type, Subject::Canonical(canonical)) = finding.subject() else {
            continue;
        };
        if ids.contains_key(&finding.resource) {
            continue;
        }
        let filter = SearchFilter {
            url: canonical.to_owned(),
            ..SearchFilter::default()
        };
        // NOTE: no specification governs this: our own design; a search that does
        // not answer leaves the row as the run wrote it, unlinked, and hides nothing.
        let id = match resource_type {
            crate::fhir::VALUE_SET => request
                .client
                .value_set_search(request.version, &filter)
                .await
                .ok()
                .and_then(|found| {
                    found
                        .found()
                        .first()
                        .and_then(|v| v.id())
                        .map(str::to_owned)
                }),
            crate::fhir::CONCEPT_MAP => request
                .client
                .concept_map_search(request.version, &filter)
                .await
                .ok()
                .and_then(|found| {
                    found
                        .found()
                        .first()
                        .and_then(|m| m.id())
                        .map(str::to_owned)
                }),
            _ => None,
        };
        if let Some(id) = id {
            ids.insert(finding.resource.clone(), id);
        }
    }
    ids
}

/// The link to the resource a finding names, when the console has a screen
/// for it: a code system by its canonical, a value set or concept map by its
/// logical id, given or resolved.
fn finding_link(
    request: &Request,
    finding: &Finding,
    ids: &BTreeMap<String, String>,
) -> Option<String> {
    let (resource_type, subject) = finding.subject();
    let (canonical, id) = match subject {
        Subject::Canonical(canonical) => (
            canonical,
            ids.get(&finding.resource).map_or("", String::as_str),
        ),
        Subject::Id(id) => ("", id),
    };
    screen_link(
        &request.locale,
        resource_type,
        canonical,
        id,
        request.version,
    )
    .map(|(href, _)| href)
}

/// What the release did to the deployment's own content, linked to each
/// resource.
fn findings_section(
    request: &Request,
    findings: &[Finding],
    ids: &BTreeMap<String, String>,
) -> Html {
    let locale = &request.locale;
    let columns = [
        Column::text("resource", tr(locale, Msg::SyncColumnResource)).role(Role::Title),
        Column::text("code", tr(locale, Msg::SyncColumnCode)),
        Column::text("change", tr(locale, Msg::SyncColumnChange)).role(Role::Wide),
        Column::text("release", tr(locale, Msg::SyncColumnRelease)),
    ];
    let [_, code_column, change, release] = &columns;
    let rows = Html::join(findings.iter().map(|finding| {
        let href = finding_link(request, finding, ids);
        Html::raw(format!(
            "<tr>{}{}{}{}</tr>",
            title_cell(None, href.as_deref(), Html::text(&finding.resource), None),
            cell(
                code_column,
                Html::raw(format!(
                    r#"{} <span class="fui-muted">{}</span>"#,
                    code(&finding.code),
                    Html::text(&finding.system)
                )),
            ),
            cell(change, Html::text(kind_words(locale, &finding.kind))),
            cell(
                release,
                Html::text(
                    finding
                        .release
                        .clone()
                        .unwrap_or_else(|| tr(locale, Msg::SyncReleaseNone))
                ),
            ),
        ))
    }));
    let caption = tr(locale, Msg::SyncEffect);
    let body = if findings.is_empty() {
        Body::Empty(section_intro(tr(locale, Msg::SyncNoFindings)))
    } else {
        Body::Rows(rows)
    };
    panel_section(
        locale,
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count(findings.len()))),
            actions: None,
        }),
        Html::join([
            section_intro(tr(locale, Msg::SyncEffectIntro)),
            data_table(DataTable {
                shape: Shape::Stacked,
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body,
            }),
        ]),
    )
}

#[cfg(test)]
mod tests;
