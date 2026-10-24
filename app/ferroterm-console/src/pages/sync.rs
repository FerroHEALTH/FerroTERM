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
//! WebAssembly: the listener answers once the run has finished, and the
//! screen redirects to the list with the finished run named over it. No
//! specification governs these screens: our own design.

use std::collections::BTreeMap;

use base64::Engine;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::{ActionGroup, action_button};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{Confirmation, confirm_dialog};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{
    Crumb, Header, SectionHead, definitions, panel_section, panel_sections, section_intro,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::view::time::timestamp;
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::fhir::error::FhirError;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::sync::{Finding, RunOutcome, RunRecord, RunSummary, Subject};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::parts::{failure, headed, heading, loaded};
use crate::posted::{INTENT, posted};
use crate::request::{Request, request};
use crate::routes::{SYNC_PATH, SYNC_ROUTE, screen_link, ui_path};

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
fn trigger_words(trigger: &str) -> String {
    match trigger {
        "schedule" => tr(Msg::SyncTriggerSchedule),
        "manual" => tr(Msg::SyncTriggerManual),
        "run-once" => tr(Msg::SyncTriggerRunOnce),
        "activate" => tr(Msg::SyncTriggerActivate),
        other => other.to_owned(),
    }
}

/// How a run ended, as a state mark with its word.
fn outcome_view(outcome: RunOutcome) -> AnyView {
    match outcome {
        RunOutcome::Ok => state(Mark::Ok, tr(Msg::SyncOutcomeOk), None),
        RunOutcome::Failed => state(Mark::Bad, tr(Msg::SyncOutcomeFailed), None),
        RunOutcome::Other => state(Mark::Neutral, tr(Msg::SyncOutcomeOther), None),
    }
}

/// How long `ms` milliseconds is, in minutes and seconds.
fn duration(ms: u64) -> String {
    let seconds = ms.div_euclid(1000);
    let (minutes, seconds) = (seconds.div_euclid(60), seconds.rem_euclid(60));
    if minutes == 0 {
        tr_with(Msg::SyncDurationSeconds, &[("seconds", seconds.into())])
    } else {
        tr_with(
            Msg::SyncDurationMinutes,
            &[("minutes", minutes.into()), ("seconds", seconds.into())],
        )
    }
}

/// What a run did, in a few words.
fn did(summary: &RunSummary) -> String {
    if summary.entries_taken == 0 && summary.activated == 0 && summary.errors.is_empty() {
        tr(Msg::SyncDidNothing)
    } else if let (RunOutcome::Failed, Some(first)) = (summary.outcome, summary.errors.first()) {
        first.clone()
    } else {
        tr_with(
            Msg::SyncDid,
            &[
                ("taken", summary.entries_taken.into()),
                ("activated", summary.activated.into()),
            ],
        )
    }
}

/// The not-found page, for a reader the screens are not drawn for.
fn hidden() -> AnyView {
    crate::pages::errors::not_found_page()
}

/// What a posted Run now or Activate came to, when one was refused.
fn posted_failure(error: Option<&FhirError>) -> Option<AnyView> {
    error.map(failure)
}

/// Synchronisation: the runs, newest first, with Run now and Activate.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn SyncPage() -> impl IntoView {
    let request = request();
    if !request.reads_sync() {
        return hidden();
    }
    let form = posted().filter(|form| matches!(form.intent(), RUN | ACTIVATE));
    loaded(async move {
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
                        leptos_axum::redirect(&format!(
                            "{SYNC_ROUTE}?{RAN_PARAM}={}&{OUTCOME_PARAM}={outcome}",
                            percent(&summary.id)
                        ));
                        return ().into_any();
                    }
                    Err(error) => Some(error),
                }
            }
            None => None,
        };
        let header = heading(
            tr(Msg::ShellSynchronisation),
            Some(tr(Msg::SyncSubtitle)),
            Vec::new(),
            actions(),
        );
        let body = match request.client.runs().await {
            Ok(runs) => runs_section(&runs),
            Err(error) => failure(&error),
        };
        view! {
            {header}
            {confirmations()}
            {posted_failure(refused.as_ref())}
            {ran_notice(&request)}
            {panel_sections(body)}
        }
        .into_any()
    })
}

/// The title row's actions: Run now, and Activate in More, each opening its
/// confirmation.
fn actions() -> ActionGroup {
    ActionGroup::primary(Action::new(
        Some(Glyph::REFRESH_CW),
        tr(Msg::SyncRunNow),
        Act::Dialog(String::from(RUN_DIALOG)),
    ))
    .more(Menu::new(
        "sync-more",
        tr(Msg::SyncMore),
        vec![Item::from(Action::new(
            Some(Glyph::PLAY),
            tr(Msg::SyncActivate),
            Act::Dialog(String::from(ACTIVATE_DIALOG)),
        ))],
    ))
}

/// The two confirmations the title row's actions open, each posting its
/// intent to this screen.
fn confirmations() -> AnyView {
    let dialog = |id: &'static str, glyph, words: Msg, question: Msg, message: Msg, intent| {
        let confirm = action_button(
            Action::new(
                Some(glyph),
                tr(words),
                Act::Post {
                    action: String::from(SYNC_ROUTE),
                    fields: vec![(String::from(INTENT), String::from(intent))],
                },
            ),
            Variant::Primary,
        );
        confirm_dialog(
            Confirmation {
                id: id.into(),
                title: tr(question).into(),
                message: tr(message).into(),
            },
            confirm,
        )
    };
    view! {
        {dialog(
            RUN_DIALOG,
            Glyph::REFRESH_CW,
            Msg::SyncRunNow,
            Msg::SyncRunNowQuestion,
            Msg::SyncRunNowMessage,
            RUN,
        )}
        {dialog(
            ACTIVATE_DIALOG,
            Glyph::PLAY,
            Msg::SyncActivate,
            Msg::SyncActivateQuestion,
            Msg::SyncActivateMessage,
            ACTIVATE,
        )}
    }
    .into_any()
}

/// The run a post just finished, named over the list with a link to it.
fn ran_notice(request: &Request) -> Option<AnyView> {
    let id = request.param(RAN_PARAM).filter(|id| !id.is_empty())?;
    let ok = request.text(OUTCOME_PARAM) == "ok";
    let (tone, msg) = if ok {
        (Tone::Ok, Msg::SyncRanOk)
    } else {
        (Tone::Bad, Msg::SyncRanFailed)
    };
    let open = action_button(
        Action::new(None, tr(Msg::SyncOpenRun), Act::Link(run_link(&id, false))),
        Variant::Secondary,
    );
    Some(
        view! {
            {notice(tone, Announce::Status, None, tr_with(msg, &[("id", id.clone().into())]))}
            <p>{open}</p>
        }
        .into_any(),
    )
}

/// The runs, newest first, as a table.
fn runs_section(runs: &[RunSummary]) -> AnyView {
    let columns = [
        Column::text("run", tr(Msg::SyncColumnRun)).role(Role::Title),
        Column::text("trigger", tr(Msg::SyncColumnTrigger)),
        Column::text("outcome", tr(Msg::SyncColumnOutcome)),
        Column::text("did", tr(Msg::SyncColumnDid)).role(Role::Wide),
        Column::text("findings", tr(Msg::SyncColumnFindings)),
    ];
    let [_, trigger, outcome, what, findings] = &columns;
    let rows = runs
        .iter()
        .map(|run| {
            let href = run_link(&run.id, false);
            view! {
                <tr>
                    {title_cell(Some(&href), timestamp(&run.started), None)}
                    {cell(trigger, trigger_words(&run.trigger))}
                    {cell(outcome, outcome_view(run.outcome))} {cell(what, did(run))}
                    {cell(
                        findings,
                        tr_with(
                            Msg::SyncFindingsCount,
                            &[("count", run.revalidation_findings.into())],
                        ),
                    )}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::SyncRuns);
    let body = if runs.is_empty() {
        Body::Empty(section_intro(tr(Msg::SyncNoRuns)))
    } else {
        Body::Rows(rows)
    };
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count(runs.len()))),
            actions: None,
        }),
        data_table(DataTable {
            caption: &caption,
            columns: &columns,
            sorting: Sorting::default(),
            body,
        }),
    )
    .into_any()
}

/// One run's screen: its Summary or its Findings.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn RunPage() -> impl IntoView {
    let request = request();
    if !request.reads_sync() {
        return hidden();
    }
    let params = use_params_map();
    let (id, tab) = params.with_untracked(|params| (params.get("id"), params.get("tab")));
    let id = id.unwrap_or_default();
    let findings = match tab.as_deref() {
        None => false,
        Some("findings") => true,
        Some(_) => return hidden(),
    };
    loaded(async move {
        match request.client.run_record(&id).await {
            Ok(record) => {
                let ids = if findings {
                    resolve(&request, &record.revalidation.findings).await
                } else {
                    BTreeMap::new()
                };
                run_page(&request, &record, findings, &ids)
            }
            Err(error) => {
                let not_found = error.status() == Some(http::StatusCode::NOT_FOUND);
                view! {
                    {heading(
                        tr_with(Msg::SyncRunTitle, &[("when", id.clone().into())]),
                        None,
                        vec![crumb()],
                        ActionGroup::none(),
                    )}
                    {not_found
                        .then(|| {
                            notice(
                                Tone::Bad,
                                Announce::Alert,
                                None,
                                tr_with(Msg::SyncNotFound, &[("id", id.clone().into())]),
                            )
                        })}
                    {(!not_found).then(|| failure(&error))}
                }
                .into_any()
            }
        }
    })
}

/// The trail's step back to the runs.
fn crumb() -> Crumb {
    Crumb::new(tr(Msg::ShellSynchronisation), ui_path(SYNC_PATH))
}

/// The page of `record`, on its Findings tab when `findings`.
fn run_page(
    request: &Request,
    record: &RunRecord,
    findings: bool,
    ids: &BTreeMap<String, String>,
) -> AnyView {
    let when = ferrohealth_ui::view::time::reading(&record.started);
    let title = tr_with(Msg::SyncRunTitle, &[("when", when.into())]);
    let encoded = base64::engine::general_purpose::STANDARD.encode(record.text.as_bytes());
    let download = view! {
        <a
            class="fui-button fui-button-primary"
            href=format!("data:application/json;base64,{encoded}")
            download=format!("run-{}.json", record.id)
        >
            {ferrohealth_ui::icon::icon(Glyph::DOWNLOAD, "16")}
            {tr(Msg::SyncDownload)}
        </a>
    };
    let header = headed(
        Header {
            title: title.into(),
            subtitle: Some(
                tr_with(
                    Msg::SyncTook,
                    &[("duration", duration(record.duration_ms).into())],
                )
                .into(),
            ),
            subtitle_view: None,
            crumbs: vec![crumb()],
            mono: false,
            badge: Some(outcome_view(record.outcome)),
        },
        ActionGroup::none(),
    );
    let tabs = vec![
        Tab::new(
            "summary",
            run_link(&record.id, false),
            tr(Msg::SyncTabSummary),
        ),
        Tab::new(
            "findings",
            run_link(&record.id, true),
            tr(Msg::SyncTabFindings),
        ),
    ];
    let panel = if findings {
        findings_section(request, &record.revalidation.findings, ids)
    } else {
        summary_section(record)
    };
    view! {
        {header}
        <p>{download}</p>
        {binder(
            tr(Msg::SyncTabs),
            tabs,
            if findings { "findings" } else { "summary" },
            panel_sections(panel),
        )}
    }
    .into_any()
}

/// Each source's line: what its feed offered, and its FHIR API when it has
/// one.
fn source_lines(record: &RunRecord) -> Vec<String> {
    record
        .sources
        .iter()
        .map(|source| match &source.fhir_api_url {
            Some(_) => tr_with(
                Msg::SyncSourceLineApi,
                &[
                    ("source", source.source.clone().into()),
                    ("seen", source.entries_seen.into()),
                    ("listed", source.api_entries_seen.into()),
                ],
            ),
            None => tr_with(
                Msg::SyncSourceLine,
                &[
                    ("source", source.source.clone().into()),
                    ("seen", source.entries_seen.into()),
                ],
            ),
        })
        .collect()
}

/// Each entry the run took, with why taking it failed when it did.
fn taken_lines(record: &RunRecord) -> Vec<String> {
    record
        .sources
        .iter()
        .flat_map(|source| source.taken.iter())
        .map(|taken| match &taken.error {
            Some(error) => tr_with(
                Msg::SyncTakenFailed,
                &[
                    ("title", taken.title.clone().into()),
                    ("version", taken.version.clone().into()),
                    ("error", error.clone().into()),
                ],
            ),
            None => tr_with(
                Msg::SyncTakenEntry,
                &[
                    ("title", taken.title.clone().into()),
                    ("version", taken.version.clone().into()),
                ],
            ),
        })
        .collect()
}

/// A count as the catalog takes it, saturating past `u64::MAX`.
fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

/// What the run left behind, for another reason and outside the
/// subscription.
fn left_line(record: &RunRecord) -> String {
    let skipped = record
        .sources
        .iter()
        .map(|source| count(source.skipped.len()))
        .fold(0_u64, u64::saturating_add);
    let unsubscribed = record
        .sources
        .iter()
        .flat_map(|source| source.not_subscribed.iter())
        .map(|counted| counted.entries)
        .fold(0_u64, u64::saturating_add);
    tr_with(
        Msg::SyncLeftLine,
        &[
            ("skipped", skipped.into()),
            ("unsubscribed", unsubscribed.into()),
        ],
    )
}

/// How the activation ended.
fn activation_line(record: &RunRecord) -> String {
    let activation = &record.activation;
    let activated = count(activation.activated.len());
    match &activation.error {
        Some(error) => tr_with(
            Msg::SyncActivationFailed,
            &[
                ("activated", activated.into()),
                ("error", error.clone().into()),
            ],
        ),
        None => tr_with(
            Msg::SyncActivationLine,
            &[
                ("activated", activated.into()),
                ("staged", activation.staged.into()),
                ("reloads", count(activation.reloads.len()).into()),
            ],
        ),
    }
}

/// What revalidating the deployment's own resources found, or why it did
/// not run.
fn revalidation_line(record: &RunRecord) -> String {
    let revalidation = &record.revalidation;
    match &revalidation.skipped {
        Some(reason) => tr_with(
            Msg::SyncRevalidationSkipped,
            &[("reason", reason.clone().into())],
        ),
        None => tr_with(
            Msg::SyncRevalidationLine,
            &[
                ("resources", revalidation.resources.into()),
                ("checked", revalidation.checked.into()),
                ("statement", revalidation.statement.clone().into()),
            ],
        ),
    }
}

/// `items` as a list, or Nothing when there is none.
fn list(items: Vec<String>) -> AnyView {
    if items.is_empty() {
        tr(Msg::SyncNothing).into_any()
    } else {
        view! { <ul>{items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}</ul> }
            .into_any()
    }
}

/// What one run did, fact by fact.
fn summary_section(record: &RunRecord) -> AnyView {
    let facts = definitions(vec![
        (
            tr(Msg::SyncFactTrigger),
            trigger_words(&record.trigger).into_any(),
        ),
        (
            tr(Msg::SyncFactStarted),
            timestamp(&record.started).into_any(),
        ),
        (
            tr(Msg::SyncFactEnded),
            timestamp(&record.finished).into_any(),
        ),
        (tr(Msg::SyncFactSources), list(source_lines(record))),
        (tr(Msg::SyncFactTaken), list(taken_lines(record))),
        (tr(Msg::SyncFactLeft), left_line(record).into_any()),
        (
            tr(Msg::SyncFactActivation),
            activation_line(record).into_any(),
        ),
        (
            tr(Msg::SyncFactRevalidation),
            revalidation_line(record).into_any(),
        ),
        (tr(Msg::SyncFactErrors), list(record.errors.clone())),
    ]);
    panel_section(
        Some(SectionHead {
            title: tr(Msg::SyncWhat),
            count: None,
            actions: None,
        }),
        facts,
    )
    .into_any()
}

/// The words of what a release did to a code.
fn kind_words(kind: &str) -> String {
    match kind {
        "inactive" => tr(Msg::SyncKindInactive),
        "absent" => tr(Msg::SyncKindAbsent),
        "outside-value-set" => tr(Msg::SyncKindOutsideValueSet),
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
    screen_link(resource_type, canonical, id, request.version).map(|(href, _)| href)
}

/// What the release did to the deployment's own content, linked to each
/// resource.
fn findings_section(
    request: &Request,
    findings: &[Finding],
    ids: &BTreeMap<String, String>,
) -> AnyView {
    let columns = [
        Column::text("resource", tr(Msg::SyncColumnResource)).role(Role::Title),
        Column::text("code", tr(Msg::SyncColumnCode)),
        Column::text("change", tr(Msg::SyncColumnChange)).role(Role::Wide),
        Column::text("release", tr(Msg::SyncColumnRelease)),
    ];
    let [_, code, change, release] = &columns;
    let rows = findings
        .iter()
        .map(|finding| {
            let href = finding_link(request, finding, ids);
            view! {
                <tr>
                    {title_cell(href.as_deref(), finding.resource.clone(), None)}
                    {cell(
                        code,
                        view! {
                            <code>{finding.code.clone()}</code>
                            " "
                            <span class="fui-muted">{finding.system.clone()}</span>
                        },
                    )} {cell(change, kind_words(&finding.kind))}
                    {cell(
                        release,
                        finding.release.clone().unwrap_or_else(|| tr(Msg::SyncReleaseNone)),
                    )}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let caption = tr(Msg::SyncEffect);
    let body = if findings.is_empty() {
        Body::Empty(section_intro(tr(Msg::SyncNoFindings)))
    } else {
        Body::Rows(rows)
    };
    panel_section(
        Some(SectionHead {
            title: caption.clone(),
            count: Some(HeadingCount::All(count(findings.len()))),
            actions: None,
        }),
        view! {
            {section_intro(tr(Msg::SyncEffectIntro))}
            {data_table(DataTable {
                caption: &caption,
                columns: &columns,
                sorting: Sorting::default(),
                body,
            })}
        },
    )
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_reads_in_minutes_and_seconds() {
        assert_eq!(duration(252_000), "4 min 12 s");
        assert_eq!(duration(9_400), "9 s");
        assert_eq!(duration(0), "0 s");
    }

    #[test]
    fn a_run_says_what_it_did_or_why_it_failed() {
        let quiet = RunSummary {
            outcome: RunOutcome::Ok,
            ..RunSummary::default()
        };
        assert_eq!(did(&quiet), "Nothing new");
        let took = RunSummary {
            outcome: RunOutcome::Ok,
            entries_taken: 2,
            activated: 1,
            ..RunSummary::default()
        };
        assert_eq!(did(&took), "2 releases taken, 1 activated");
        let failed = RunSummary {
            outcome: RunOutcome::Failed,
            errors: vec![String::from("the feed answered 503")],
            ..RunSummary::default()
        };
        assert_eq!(did(&failed), "the feed answered 503");
    }

    #[test]
    fn every_kind_the_service_records_reads_as_a_sentence() {
        assert_eq!(
            kind_words("inactive"),
            "The release made this code inactive"
        );
        assert_eq!(
            kind_words("absent"),
            "The release no longer carries this code"
        );
        assert_eq!(
            kind_words("outside-value-set"),
            "The code no longer falls inside the value set it was included through"
        );
        assert_eq!(
            kind_words("something-new"),
            "something-new",
            "a kind this console predates is shown as the service wrote it"
        );
    }

    #[test]
    fn a_record_reads_as_its_summary_lines() {
        let record: RunRecord = serde_json::from_str(
            r#"{"sources":[{"source":"nts","entries_seen":412,"fhir_api_url":"https://api.example",
                "api_entries_seen":1920,"taken":[{"title":"Findings","version":"2026-10"},
                {"title":"Labs","version":"7","error":"the digest did not match"}],
                "skipped":[{},{}],"not_subscribed":[{"entries":388},{"entries":1}]}],
                "activation":{"activated":[{}],"staged":0,"reloads":[{"status":200}]},
                "revalidation":{"resources":12,"checked":214,"statement":"1 finding"}}"#,
        )
        .expect("the record parses");
        assert_eq!(
            source_lines(&record),
            ["nts: the feed offered 412 entries, and its FHIR API listed 1,920 resources"]
        );
        assert_eq!(
            taken_lines(&record),
            ["Findings 2026-10", "Labs 7: the digest did not match"]
        );
        assert_eq!(
            left_line(&record),
            "2 entries for another reason, 389 outside your subscription"
        );
        assert_eq!(
            activation_line(&record),
            "1 activated, 0 still staged, 1 reloads asked of the server"
        );
        assert_eq!(
            revalidation_line(&record),
            "12 locally written resources read, 214 codes checked: 1 finding"
        );
    }

    #[test]
    fn a_failed_activation_and_a_skipped_revalidation_say_why() {
        let record: RunRecord = serde_json::from_str(
            r#"{"activation":{"activated":[],"error":"the server refused the reload"},
                "revalidation":{"skipped":"no resource store"}}"#,
        )
        .expect("the record parses");
        assert_eq!(
            activation_line(&record),
            "0 activated, then it failed: the server refused the reload"
        );
        assert_eq!(revalidation_line(&record), "Not run: no resource store");
    }

    #[test]
    fn a_run_id_is_one_path_segment() {
        assert_eq!(
            run_link("20261021T030000Z", false),
            "/sync/20261021T030000Z"
        );
        assert_eq!(run_link("a/b", true), "/sync/a%2Fb/findings");
    }
}
