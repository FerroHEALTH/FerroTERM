// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parts the screens share: a redirect, the page's title and heading,
//! where a resource comes from, a relative time, and the refusal of a
//! request the server answered with a failure.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::refusal::{Issue, Refusal, Severity, refusal};
use ferrohealth_ui::view::surface::{Crumb, Header, page_heading};
use ferrohealth_ui::view::tag::Tag;

use crate::fhir::error::FhirError;
use crate::fhir::named::Listed;
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::problem::Problem;
use crate::fhir::terminology::SystemCard;
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{h, t};
use crate::request::Request;

/// The page's title in the browser tab, and its heading with `subtitle`,
/// the trail `crumbs` and `actions`.
pub(crate) fn heading(
    request: &Request,
    title: String,
    subtitle: Option<String>,
    crumbs: Vec<Crumb>,
    actions: ActionGroup,
) -> Html {
    headed(
        request,
        Header {
            title: title.into(),
            subtitle: subtitle.map(Into::into),
            subtitle_view: None,
            crumbs,
            mono: false,
            badge: None,
        },
        actions,
    )
}

/// The page's title in the browser tab, from `header`, and the heading
/// `header` draws with `actions`.
pub(crate) fn headed(request: &Request, header: Header, actions: ActionGroup) -> Html {
    request.answering.title(header.title.text());
    page_heading(&request.locale, header, actions)
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
    pub(crate) fn word(self, locale: &Locale) -> String {
        tr(
            locale,
            match self {
                Self::Stored => Msg::SourceStored,
                Self::Index => Msg::SourceIndex,
                Self::Loaded => Msg::SourceLoaded,
            },
        )
    }

    /// The source as a tag (R2): a plain tag for a stored resource, and the
    /// provenance tag (F4) for one the console cannot change, its owner in
    /// its tooltip.
    #[must_use]
    pub(crate) fn tag(self, locale: &Locale) -> Html {
        match self {
            Self::Stored => Tag::new(self.word(locale))
                .title(tr(locale, Msg::SourceStoredTip))
                .view(),
            Self::Index => Tag::new(self.word(locale))
                .locked(tr(locale, Msg::SourceIndexTip))
                .view(),
            Self::Loaded => Tag::new(self.word(locale))
                .locked(tr(locale, Msg::SourceLoadedTip))
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
pub(crate) fn versions_word(locale: &Locale, versions: &[String]) -> String {
    match versions {
        [] => tr(locale, Msg::VersionsNone),
        [one] => tr_with(locale, Msg::VersionsOne, &[("version", one.clone().into())]),
        many => tr_with(
            locale,
            Msg::VersionsMany,
            &[("versions", many.join(", ").into())],
        ),
    }
}

/// How long ago `raw`, an instant the server wrote, was, read against
/// `now`: in seconds, minutes, hours, yesterday or days (FerroUI's family
/// rule 11), the full timestamp in its tooltip; a value that is no instant
/// is drawn as it came.
pub(crate) fn relative(locale: &Locale, raw: &str, now: jiff::Timestamp) -> Html {
    let reading = ferrohealth_ui::view::time::reading(locale, raw);
    let Ok(then) = raw.parse::<jiff::Timestamp>() else {
        return h(format!(r#"<span class="fui-time">{}</span>"#, t(raw)));
    };
    let words = ago(locale, now.as_second().saturating_sub(then.as_second()));
    h(format!(
        r#"<time class="fui-time" datetime="{}" title="{}">{}</time>"#,
        t(raw),
        t(reading),
        t(words)
    ))
}

/// The words of a moment `seconds` ago.
#[must_use]
pub(crate) fn ago(locale: &Locale, seconds: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let seconds = seconds.max(0);
    if seconds < MINUTE {
        tr_with(
            locale,
            Msg::AgoSeconds,
            &[("count", seconds.to_string().into())],
        )
    } else if seconds < HOUR {
        tr_with(
            locale,
            Msg::AgoMinutes,
            &[("count", seconds.div_euclid(MINUTE).to_string().into())],
        )
    } else if seconds < DAY {
        tr_with(
            locale,
            Msg::AgoHours,
            &[("count", seconds.div_euclid(HOUR).to_string().into())],
        )
    } else if seconds < 2 * DAY {
        tr(locale, Msg::AgoYesterday)
    } else {
        tr_with(
            locale,
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
pub(crate) fn issues(locale: &Locale, outcome: &OperationOutcome) -> Vec<Issue> {
    outcome
        .lines(locale)
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

/// The RFC 9457 problem an admin route answered `error` with, if its body
/// is one.
fn problem_of(error: &FhirError) -> Option<Problem> {
    match error {
        FhirError::Status { body, .. } => Problem::read(body),
        FhirError::Refused { .. } | FhirError::Transport { .. } | FhirError::Decode { .. } => None,
    }
}

/// The issue a problem draws as: its type's fragment as the code, its detail
/// as the text, else its title.
fn problem_issue(problem: &Problem) -> Issue {
    Issue {
        severity: Severity::Error,
        code: problem.code(),
        text: problem.detail.clone().or_else(|| problem.title.clone()),
        place: String::new(),
        details: Vec::new(),
    }
}

/// The refusal `error` is, as FerroUI draws one.
pub(crate) fn refusal_of(locale: &Locale, error: &FhirError) -> Refusal {
    Refusal {
        status: error.status().map(|status| status.as_u16()),
        summary: match error {
            FhirError::Refused { .. } | FhirError::Status { .. } => {
                tr(locale, Msg::ServerRefusedRequest)
            }
            FhirError::Transport { .. } => tr(locale, Msg::RequestGotNoAnswer),
            FhirError::Decode { .. } => tr(locale, Msg::AnswerDidNotRead),
        },
        issues: error
            .outcome()
            .map(|outcome| issues(locale, outcome))
            .or_else(|| problem_of(error).map(|problem| vec![problem_issue(&problem)]))
            .unwrap_or_default(),
        request: Some(error.url().to_owned()),
        evidence: match error {
            FhirError::Status { .. } if problem_of(error).is_some() => None,
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
pub(crate) fn failure(locale: &Locale, error: &FhirError) -> Html {
    refusal(locale, &refusal_of(locale, error))
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;
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
        let refused = refusal_of(
            &Locale::english(),
            &FhirError::Refused {
                url: String::from("http://x/r4/ValueSet/$expand"),
                status: StatusCode::NOT_FOUND,
                outcome,
            },
        );
        assert_eq!(Some(404), refused.status);
        assert_eq!(2, refused.issues.len());
        assert_eq!(Severity::Error, refused.issues[0].severity);
        assert_eq!(Some("no such value set"), refused.issues[0].text.as_deref());
        assert_eq!(Severity::Warning, refused.issues[1].severity);
    }

    #[test]
    fn a_request_with_no_answer_has_no_status() {
        let refused = refusal_of(
            &Locale::english(),
            &FhirError::Transport {
                url: String::from("http://x/health"),
                message: String::from("connection refused"),
            },
        );
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
        let english = Locale::english();
        assert_eq!("13 s ago", ago(&english, 13));
        assert_eq!("13 min ago", ago(&english, 13 * 60 + 5));
        assert_eq!("1 h ago", ago(&english, 3_700));
        assert_eq!("yesterday", ago(&english, 25 * 3_600));
        assert_eq!("7 days ago", ago(&english, 7 * 86_400 + 10));
        assert_eq!(
            "0 s ago",
            ago(&english, -5),
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

    #[test]
    fn a_problem_an_admin_route_answers_is_drawn_as_its_issue_in_place_of_the_body() {
        let refused = refusal_of(
            &Locale::english(),
            &FhirError::Status {
                url: String::from("http://sync:8181/runs/x"),
                status: StatusCode::NOT_FOUND,
                body: String::from(
                    r#"{"type":"https://ferroterm.eu/docs/operate/problems.html#run-not-found","title":"The sync service holds no such run","status":404,"detail":"no run of this service has that id"}"#,
                ),
            },
        );
        assert_eq!(Some(404), refused.status);
        assert_eq!(1, refused.issues.len());
        assert_eq!("run-not-found", refused.issues[0].code);
        assert_eq!(
            Some("no run of this service has that id"),
            refused.issues[0].text.as_deref()
        );
        assert_eq!(
            None, refused.evidence,
            "the problem is drawn, not its raw body"
        );
    }
}
