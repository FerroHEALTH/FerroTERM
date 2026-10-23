// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parts the screens share: reading on the server while the page
//! renders, the page's title and heading, where a resource comes from, a
//! relative time, and the refusal of a request the server answered with a
//! failure.

use std::future::Future;

use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::refusal::{Issue, Refusal, Severity, refusal};
use ferrohealth_ui::view::surface::{Crumb, Header, page_heading};
use ferrohealth_ui::view::tag::Tag;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::fhir::error::FhirError;
use crate::fhir::named::Listed;
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, tr, tr_with};

/// Draws what `read` resolves to, once the server has read it.
///
/// The future runs while the server renders the page: every route renders
/// whole (`SsrMode::Async`), so the page arrives with its content and no
/// loading state, and in islands mode nothing it read reaches the browser
/// but as HTML (the Leptos book, "Guide: Islands", reduced data
/// serialization).
pub(crate) fn loaded<F>(read: F) -> AnyView
where
    F: Future<Output = AnyView> + Send + 'static,
{
    view! { <Suspense>{Suspend::new(read)}</Suspense> }.into_any()
}

/// The page's title in the browser tab, and its heading with `subtitle`,
/// the trail `crumbs` and `actions`.
pub(crate) fn heading(
    title: String,
    subtitle: Option<String>,
    crumbs: Vec<Crumb>,
    actions: ActionGroup,
) -> AnyView {
    headed(
        Header {
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            crumbs,
            mono: false,
            badge: None,
        },
        actions,
    )
}

/// The page's title in the browser tab, from `header`, and the heading
/// `header` draws with `actions`.
pub(crate) fn headed(header: Header, actions: ActionGroup) -> AnyView {
    let tab = header.title.text().to_owned();
    view! {
        <Title text=tab />
        {page_heading(header, actions)}
    }
    .into_any()
}

/// Where a published resource comes from, as the listings say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// Written through the FHIR API and kept in the server's store: it can
    /// be edited and has a history.
    Stored,
    /// Served from an index built offline.
    Index,
    /// Read from a resource directory, or carried by the server, when it
    /// starts.
    Loaded,
}

impl Source {
    /// Where the code system `card` comes from: an index when a served
    /// version names the artifact it was read from, stored when a
    /// `CodeSystem` of its canonical was written through the API, and loaded
    /// otherwise.
    #[must_use]
    pub(crate) fn of_system(card: &SystemCard, listed: &[Listed]) -> Self {
        if card
            .versions
            .iter()
            .any(|version| version.artifact.is_some())
        {
            Self::Index
        } else if listed
            .iter()
            .any(|resource| resource.url == card.url && resource.stored.is_some())
        {
            Self::Stored
        } else {
            Self::Loaded
        }
    }

    /// Where the value set or concept map `resource` comes from.
    #[must_use]
    pub(crate) fn of_resource(resource: &Listed) -> Self {
        Self::of_stored(resource.stored.as_deref())
    }

    /// Where a resource comes from, by the version the server `stored` it
    /// at: written through the API when it states one, loaded otherwise.
    #[must_use]
    pub(crate) const fn of_stored(stored: Option<&str>) -> Self {
        if stored.is_some() {
            Self::Stored
        } else {
            Self::Loaded
        }
    }

    /// The key a filter of the listings names this source by.
    #[must_use]
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Index => "index",
            Self::Loaded => "loaded",
        }
    }

    /// The source a filter's `key` names.
    #[must_use]
    pub(crate) fn parse(key: &str) -> Option<Self> {
        [Self::Stored, Self::Index, Self::Loaded]
            .into_iter()
            .find(|source| source.key() == key)
    }

    /// The source's words, with a capital.
    #[must_use]
    pub(crate) fn word(self) -> String {
        tr(match self {
            Self::Stored => Msg::SourceStored,
            Self::Index => Msg::SourceIndex,
            Self::Loaded => Msg::SourceLoaded,
        })
    }

    /// The source as a tag (R2): a plain tag for a stored resource, and the
    /// provenance tag (F4) for one the console cannot change, its owner in
    /// its tooltip.
    #[must_use]
    pub(crate) fn tag(self) -> AnyView {
        match self {
            Self::Stored => Tag::new(self.word()).title(tr(Msg::SourceStoredTip)).view(),
            Self::Index => Tag::new(self.word()).locked(tr(Msg::SourceIndexTip)).view(),
            Self::Loaded => Tag::new(self.word())
                .locked(tr(Msg::SourceLoadedTip))
                .view(),
        }
    }
}

/// The name each code system the root publishes goes by, by canonical; a
/// refused read names none, and a canonical then stands for itself.
pub(crate) async fn system_names(
    client: &crate::fhir::FhirClient,
    version: crate::fhir::version::FhirVersion,
) -> std::collections::BTreeMap<String, String> {
    client
        .published_names(version, crate::fhir::CODE_SYSTEM)
        .await
        .map(|named| named.names())
        .unwrap_or_default()
}

/// The name `canonical` goes by in `names`, else the canonical itself.
pub(crate) fn name_of(
    names: &std::collections::BTreeMap<String, String>,
    canonical: &str,
) -> String {
    names
        .get(canonical)
        .cloned()
        .unwrap_or_else(|| canonical.to_owned())
}

/// A code system's versions in words, so a phone card that drops the
/// column header still says what the figure is.
#[must_use]
pub(crate) fn versions_word(versions: &[String]) -> String {
    match versions {
        [] => tr(Msg::VersionsNone),
        [one] => tr_with(Msg::VersionsOne, &[("version", one.clone().into())]),
        many => tr_with(Msg::VersionsMany, &[("versions", many.join(", ").into())]),
    }
}

/// How long ago `raw`, an instant the server wrote, was, read against
/// `now`: in seconds, minutes, hours, yesterday or days (FerroUI's family
/// rule 11), the full timestamp in its tooltip; a value that is no instant
/// is drawn as it came.
pub(crate) fn relative(raw: &str, now: jiff::Timestamp) -> AnyView {
    let reading = ferrohealth_ui::view::time::reading(raw);
    let Ok(then) = raw.parse::<jiff::Timestamp>() else {
        return view! { <span class="fui-time">{raw.to_owned()}</span> }.into_any();
    };
    let words = ago(now.as_second().saturating_sub(then.as_second()));
    view! {
        <time class="fui-time" datetime=raw.to_owned() title=reading>
            {words}
        </time>
    }
    .into_any()
}

/// The words of a moment `seconds` ago.
#[must_use]
pub(crate) fn ago(seconds: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let seconds = seconds.max(0);
    if seconds < MINUTE {
        tr_with(Msg::AgoSeconds, &[("count", seconds.to_string().into())])
    } else if seconds < HOUR {
        tr_with(
            Msg::AgoMinutes,
            &[("count", seconds.div_euclid(MINUTE).to_string().into())],
        )
    } else if seconds < DAY {
        tr_with(
            Msg::AgoHours,
            &[("count", seconds.div_euclid(HOUR).to_string().into())],
        )
    } else if seconds < 2 * DAY {
        tr(Msg::AgoYesterday)
    } else {
        tr_with(
            Msg::AgoDays,
            &[("count", seconds.div_euclid(DAY).to_string().into())],
        )
    }
}

/// The severity a FHIR `issue.severity` code names (the `IssueSeverity` value
/// set, <https://hl7.org/fhir/R4B/valueset-issue-severity.html>); a code it
/// does not define is read as an error, never dropped.
fn severity(code: &str) -> Severity {
    match code {
        "fatal" => Severity::Fatal,
        "warning" => Severity::Warning,
        "information" => Severity::Information,
        _ => Severity::Error,
    }
}

/// The issues `outcome` carries, each with the text the server wrote and the
/// codes that classify it.
pub(crate) fn issues(outcome: &OperationOutcome) -> Vec<Issue> {
    outcome
        .lines()
        .into_iter()
        .map(|line| Issue {
            severity: severity(&line.severity),
            code: line.code,
            text: (!line.text.is_empty()).then_some(line.text),
            place: String::new(),
            details: line.details,
        })
        .collect()
}

/// The refusal `error` is, as FerroUI draws one.
pub(crate) fn refusal_of(error: &FhirError) -> Refusal {
    Refusal {
        status: error.status().map(|status| status.as_u16()),
        summary: match error {
            FhirError::Refused { .. } | FhirError::Status { .. } => tr(Msg::ServerRefusedRequest),
            FhirError::Transport { .. } => tr(Msg::RequestGotNoAnswer),
            FhirError::Decode { .. } => tr(Msg::AnswerDidNotRead),
        },
        issues: error.outcome().map(issues).unwrap_or_default(),
        request: Some(error.url().to_owned()),
        evidence: match error {
            FhirError::Status { body, .. } if !body.is_empty() => Some(body.clone()),
            FhirError::Transport { message, .. } | FhirError::Decode { message, .. } => {
                Some(message.clone())
            }
            FhirError::Status { .. } | FhirError::Refused { .. } => None,
        },
    }
}

/// Explains why a request failed, in the server's own words: its
/// `OperationOutcome` issue by issue, never paraphrased, or the start of a
/// body that carried none, and the address asked, so a reader can repeat it.
pub(crate) fn failure(error: &FhirError) -> AnyView {
    refusal(&refusal_of(error)).into_any()
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::refusal::Severity;
    use http::StatusCode;

    use super::{Source, ago, refusal_of};
    use crate::fhir::error::FhirError;
    use crate::fhir::outcome::OperationOutcome;

    #[test]
    fn a_refusal_keeps_the_status_and_every_issue_in_the_server_s_words() {
        let outcome: OperationOutcome = serde_json::from_str(
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"error","code":"not-found","diagnostics":"no such value set"},
                {"severity":"warning","code":"informational","details":{"text":"a hint"}}]}"#,
        )
        .expect("an outcome");
        let refused = refusal_of(&FhirError::Refused {
            url: String::from("http://x/r4/ValueSet/$expand"),
            status: StatusCode::NOT_FOUND,
            outcome,
        });
        assert_eq!(Some(404), refused.status);
        assert_eq!(2, refused.issues.len());
        assert_eq!(Severity::Error, refused.issues[0].severity);
        assert_eq!(Some("no such value set"), refused.issues[0].text.as_deref());
        assert_eq!(Severity::Warning, refused.issues[1].severity);
    }

    #[test]
    fn a_request_with_no_answer_has_no_status() {
        let refused = refusal_of(&FhirError::Transport {
            url: String::from("http://x/health"),
            message: String::from("connection refused"),
        });
        assert_eq!(None, refused.status);
        assert_eq!(
            refused.issues,
            Vec::<ferrohealth_ui::view::refusal::Issue>::new()
        );
        assert_eq!(Some("http://x/health"), refused.request.as_deref());
        assert_eq!(Some("connection refused"), refused.evidence.as_deref());
    }

    #[test]
    fn a_moment_reads_in_the_largest_whole_unit() {
        assert_eq!("13 s ago", ago(13));
        assert_eq!("13 min ago", ago(13 * 60 + 5));
        assert_eq!("1 h ago", ago(3_700));
        assert_eq!("yesterday", ago(25 * 3_600));
        assert_eq!("7 days ago", ago(7 * 86_400 + 10));
        assert_eq!(
            "0 s ago",
            ago(-5),
            "a clock ahead of the console reads as now"
        );
    }

    #[test]
    fn a_source_is_named_by_its_filter_key() {
        for source in [Source::Stored, Source::Index, Source::Loaded] {
            assert_eq!(Some(source), Source::parse(source.key()));
        }
        assert_eq!(None, Source::parse("written"));
    }
}
