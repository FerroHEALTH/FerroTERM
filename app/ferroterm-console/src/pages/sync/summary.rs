// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What one synchronisation run did, fact by fact: the sources it read, the
//! releases it took, what it left behind, and its activation and
//! revalidation.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::surface::{SectionHead, definitions, panel_section};
use ferrohealth_ui::view::time::timestamp;

use crate::fhir::sync::RunRecord;
use crate::locale::{Msg, tr, tr_with};

use super::{count, trigger_words};

/// Each source's line: what its feed offered, and its FHIR API when it has
/// one.
pub(super) fn source_lines(locale: &Locale, record: &RunRecord) -> Vec<String> {
    record
        .sources
        .iter()
        .map(|source| match &source.fhir_api_url {
            Some(_) => tr_with(
                locale,
                Msg::SyncSourceLineApi,
                &[
                    ("source", source.source.clone().into()),
                    ("seen", source.entries_seen.into()),
                    ("listed", source.api_entries_seen.into()),
                ],
            ),
            None => tr_with(
                locale,
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
pub(super) fn taken_lines(locale: &Locale, record: &RunRecord) -> Vec<String> {
    record
        .sources
        .iter()
        .flat_map(|source| source.taken.iter())
        .map(|taken| match &taken.error {
            Some(error) => tr_with(
                locale,
                Msg::SyncTakenFailed,
                &[
                    ("title", taken.title.clone().into()),
                    ("version", taken.version.clone().into()),
                    ("error", error.clone().into()),
                ],
            ),
            None => tr_with(
                locale,
                Msg::SyncTakenEntry,
                &[
                    ("title", taken.title.clone().into()),
                    ("version", taken.version.clone().into()),
                ],
            ),
        })
        .collect()
}

/// What the run left behind, for another reason and outside the
/// subscription.
pub(super) fn left_line(locale: &Locale, record: &RunRecord) -> String {
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
        locale,
        Msg::SyncLeftLine,
        &[
            ("skipped", skipped.into()),
            ("unsubscribed", unsubscribed.into()),
        ],
    )
}

/// How the activation ended.
pub(super) fn activation_line(locale: &Locale, record: &RunRecord) -> String {
    let activation = &record.activation;
    let activated = count(activation.activated.len());
    match &activation.error {
        Some(error) => tr_with(
            locale,
            Msg::SyncActivationFailed,
            &[
                ("activated", activated.into()),
                ("error", error.clone().into()),
            ],
        ),
        None => tr_with(
            locale,
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
pub(super) fn revalidation_line(locale: &Locale, record: &RunRecord) -> String {
    let revalidation = &record.revalidation;
    match &revalidation.skipped {
        Some(reason) => tr_with(
            locale,
            Msg::SyncRevalidationSkipped,
            &[("reason", reason.clone().into())],
        ),
        None => tr_with(
            locale,
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
fn list(locale: &Locale, items: Vec<String>) -> Html {
    if items.is_empty() {
        Html::text(tr(locale, Msg::SyncNothing))
    } else {
        Html::raw(format!(
            "<ul>{}</ul>",
            Html::join(
                items
                    .into_iter()
                    .map(|item| Html::raw(format!("<li>{}</li>", Html::text(item))))
            )
        ))
    }
}

/// What one run did, fact by fact.
pub(super) fn summary_section(locale: &Locale, record: &RunRecord) -> Html {
    let facts = definitions(vec![
        (
            tr(locale, Msg::SyncFactTrigger),
            Html::text(trigger_words(locale, &record.trigger)),
        ),
        (
            tr(locale, Msg::SyncFactStarted),
            timestamp(locale, &record.started),
        ),
        (
            tr(locale, Msg::SyncFactEnded),
            timestamp(locale, &record.finished),
        ),
        (
            tr(locale, Msg::SyncFactSources),
            list(locale, source_lines(locale, record)),
        ),
        (
            tr(locale, Msg::SyncFactTaken),
            list(locale, taken_lines(locale, record)),
        ),
        (
            tr(locale, Msg::SyncFactLeft),
            Html::text(left_line(locale, record)),
        ),
        (
            tr(locale, Msg::SyncFactActivation),
            Html::text(activation_line(locale, record)),
        ),
        (
            tr(locale, Msg::SyncFactRevalidation),
            Html::text(revalidation_line(locale, record)),
        ),
        (
            tr(locale, Msg::SyncFactErrors),
            list(locale, record.errors.clone()),
        ),
    ]);
    panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::SyncWhat),
            count: None,
            actions: None,
        }),
        facts,
    )
}
