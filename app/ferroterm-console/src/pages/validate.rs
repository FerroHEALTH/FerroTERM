// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Check: whether a code is in a value set, with `ValueSet/$validate-code`
//! (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>), or how
//! two codes of one system relate, with `CodeSystem/$subsumes`
//! (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).

use std::collections::BTreeMap;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::refusal::{Issue, Severity, issue_list};
use ferrohealth_ui::view::segmented::segmented_links;
use ferrohealth_ui::view::surface::{SectionHead, panel_section};
use leptos::prelude::*;

use crate::fhir::validation::{
    SubsumesRequest, ValidateOn, ValidateRequest, Validation, subsumption_sentence,
};
use crate::fhir::{CODE_SYSTEM, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::lookup::CODE_PARAM;
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::{failure, loaded, name_of};
use crate::request::{Request, request};
use crate::routes::{SYSTEM_PARAM, ui_link};

/// The address parameter naming what to check.
const MODE_PARAM: &str = "mode";

/// The mode that checks how two codes relate.
const RELATION: &str = "relation";

/// The address parameter carrying the value set's canonical.
const URL_PARAM: &str = "url";

/// The address parameter carrying the first code of a relation.
const CODE_A_PARAM: &str = "codeA";

/// The address parameter carrying the second code of a relation.
const CODE_B_PARAM: &str = "codeB";

/// The Check tab.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn ValidatePage() -> impl IntoView {
    let request = request();
    loaded(async move {
        let relation = request.text(MODE_PARAM) == RELATION;
        let systems_named = request
            .client
            .published_names(request.version, CODE_SYSTEM)
            .await
            .ok();
        let systems = choices(systems_named.as_ref());
        let system_names = systems_named
            .as_ref()
            .map(crate::fhir::named::NamedSearch::names)
            .unwrap_or_default();
        let system = request.text(SYSTEM_PARAM);
        let mode = mode(&request, relation, &system);
        let (form, answer) = if relation {
            relation_tab(&request, mode, &systems, system).await
        } else {
            membership_tab(&request, mode, &systems, &system_names, system).await
        };
        frame(&request, Op::Check, form, answer)
    })
}

/// The choice of what to check, two links that keep the code system.
fn mode(request: &Request, relation: bool, system: &str) -> AnyView {
    let base = ui_link(Op::Check.path(), request.version);
    let keep = if system.is_empty() {
        String::new()
    } else {
        format!("&{SYSTEM_PARAM}={}", percent(system))
    };
    let links = segmented_links(
        tr(Msg::CheckWhat),
        vec![
            (format!("{base}{keep}"), tr(Msg::CheckMembership)),
            (
                format!("{base}&{MODE_PARAM}={RELATION}{keep}"),
                tr(Msg::CheckRelation),
            ),
        ],
        usize::from(relation),
    );
    view! {
        <div class="fui-field">
            <span class="fui-label" aria-hidden="true">
                {tr(Msg::CheckWhat)}
            </span>
            {links}
        </div>
    }
    .into_any()
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// Whether a code is in a value set: the options and the answer.
async fn membership_tab(
    request: &Request,
    mode: AnyView,
    systems: &[(String, String)],
    system_names: &BTreeMap<String, String>,
    system: String,
) -> (AnyView, AnyView) {
    let sets_named = request
        .client
        .published_names(request.version, VALUE_SET)
        .await
        .ok();
    let sets = choices(sets_named.as_ref());
    let set_names = sets_named
        .as_ref()
        .map(crate::fhir::named::NamedSearch::names)
        .unwrap_or_default();
    let url = request.text(URL_PARAM);
    let url = if url.is_empty() {
        sets.first()
            .map(|(value, _)| value.clone())
            .unwrap_or_default()
    } else {
        url
    };
    let code = request.text(CODE_PARAM);
    let fields = view! {
        {mode}
        {select("check-url", URL_PARAM, tr(Msg::CheckValueSet), &sets, &url, None)}
        {text("check-code", CODE_PARAM, tr(Msg::LookupCode), &code, tr(Msg::LookupCodeHint))}
        {select(
            "check-system",
            SYSTEM_PARAM,
            tr(Msg::CheckItsSystem),
            systems,
            &system,
            Some(tr(Msg::CheckChooseSystem)),
        )}
    }
    .into_any();
    let form = options(
        request,
        &[],
        fields,
        run(
            "check-run",
            tr(Msg::OpsCheck),
            Glyph::SHIELD_CHECK,
            "$validate-code",
        ),
    );
    let asked = ValidateRequest {
        on: ValidateOn::ValueSet,
        url: url.clone(),
        system: system.clone(),
        code: code.clone(),
        ..ValidateRequest::default()
    };
    if code.is_empty() {
        return (form, before(tr(Msg::CheckBefore)));
    }
    if !asked.runnable() {
        return (form, before(tr(Msg::CheckNeedsSystem)));
    }
    let sent_url = request.client.validate_code_url(request.version, &asked);
    let answer = match request.client.validate_code(request.version, &asked).await {
        Err(error) => view! {
            {panel_section(None, failure(&error))}
            {sent(&sent_url, None)}
        }
        .into_any(),
        Ok(document) => {
            let validation = document.value.validation();
            let set = name_of(&set_names, &url);
            view! {
                {verdict(outcome(&validation), membership_title(&validation, &code, &set), None)}
                {concept(&validation, &code, system_names)}
                {issues(&validation)}
                {sent(&sent_url, Some((tr(Msg::CheckRaw), document.text)))}
            }
            .into_any()
        }
    };
    (form, answer)
}

/// How a membership answer went: valid, valid but inactive, or not valid.
fn outcome(validation: &Validation) -> Outcome {
    match (validation.result, validation.inactive) {
        (Some(true), Some(true)) => Outcome::Warn,
        (Some(true), _) => Outcome::Ok,
        _ => Outcome::Bad,
    }
}

/// The verdict's sentence about `code` in the value set named `set`.
fn membership_title(validation: &Validation, code: &str, set: &str) -> AnyView {
    let display = validation.display.clone().map(|display| {
        view! {
            <strong>{display}</strong>
            " · "
        }
    });
    let code = view! { <code>{code.to_owned()}</code> };
    match validation.result {
        Some(true) => view! {
            {tr(Msg::CheckValid)}
            " "
            {display}
            {code}
            " "
            {tr_with(Msg::CheckIsIn, &[("set", set.to_owned().into())])}
        }
        .into_any(),
        Some(false) => view! {
            {tr(Msg::CheckNotValid)}
            " "
            {display}
            {code}
            " "
            {tr_with(Msg::CheckIsNotIn, &[("set", set.to_owned().into())])}
        }
        .into_any(),
        None => tr(Msg::CheckNoResult).into_any(),
    }
}

/// The concept the answer names: its display, code, system and version.
fn concept(
    validation: &Validation,
    code: &str,
    names: &BTreeMap<String, String>,
) -> Option<AnyView> {
    validation.display.as_ref()?;
    let system = validation
        .system
        .as_deref()
        .map(|system| name_of(names, system))
        .unwrap_or_default();
    let place = match &validation.version {
        Some(version) => tr_with(
            Msg::CheckSystemVersion,
            &[
                ("system", system.into()),
                ("version", version.clone().into()),
            ],
        ),
        None => system,
    };
    let inactive = (validation.inactive == Some(true)).then(|| {
        view! {
            " "
            {tr(Msg::CheckInactive)}
        }
    });
    Some(panel_section(
        Some(SectionHead {
            title: tr(Msg::CheckConcept),
            count: None,
            actions: None,
        }),
        view! {
            <p>{validation.display.clone()}</p>
            <p class="fui-small fui-muted">
                <code>{validation.code.clone().unwrap_or_else(|| code.to_owned())}</code>
                " "
                {place}
                {inactive}
            </p>
        },
    ))
}

/// The message and the issues the server answered with, in its own words.
fn issues(validation: &Validation) -> Option<AnyView> {
    if validation.message.is_none() && validation.issues.is_empty() {
        return None;
    }
    let message = validation
        .message
        .clone()
        .map(|message| view! { <p>{message}</p> });
    let listed: Vec<Issue> = validation
        .issues
        .iter()
        .map(|issue| {
            let severity = match issue.severity.as_str() {
                "fatal" => Severity::Fatal,
                "warning" => Severity::Warning,
                "information" => Severity::Information,
                _ => Severity::Error,
            };
            Issue::new(severity, issue.issue_code.clone())
                .text(issue.text.clone())
                .place(issue.expressions.join(", "))
        })
        .collect();
    Some(panel_section(
        Some(SectionHead {
            title: tr(Msg::CheckServerSays),
            count: None,
            actions: None,
        }),
        view! {
            {message}
            {issue_list(&listed)}
        },
    ))
}

/// How two codes of one system relate: the options and the answer.
async fn relation_tab(
    request: &Request,
    mode: AnyView,
    systems: &[(String, String)],
    system: String,
) -> (AnyView, AnyView) {
    let system = if system.is_empty() {
        systems
            .first()
            .map(|(value, _)| value.clone())
            .unwrap_or_default()
    } else {
        system
    };
    let code_a = request.text(CODE_A_PARAM);
    let code_b = request.text(CODE_B_PARAM);
    let fields = view! {
        {mode}
        {select("check-system", SYSTEM_PARAM, tr(Msg::LookupSystem), systems, &system, None)}
        {text("check-code-a", CODE_A_PARAM, tr(Msg::CheckCodeA), &code_a, tr(Msg::LookupCodeHint))}
        {text("check-code-b", CODE_B_PARAM, tr(Msg::CheckCodeB), &code_b, tr(Msg::LookupCodeHint))}
    }
    .into_any();
    let form = options(
        request,
        &[(MODE_PARAM, String::from(RELATION))],
        fields,
        run(
            "check-run",
            tr(Msg::CheckTheRelation),
            Glyph::GIT_BRANCH,
            "$subsumes",
        ),
    );
    let asked = SubsumesRequest {
        system: system.clone(),
        code_a: code_a.clone(),
        code_b: code_b.clone(),
        ..SubsumesRequest::default()
    };
    if !asked.runnable() {
        return (form, before(tr(Msg::CheckRelationBefore)));
    }
    let sent_url = request.client.subsumes_url(request.version, &asked);
    let answer = match request.client.subsumes(request.version, &asked).await {
        Err(error) => view! {
            {panel_section(None, failure(&error))}
            {sent(&sent_url, None)}
        }
        .into_any(),
        Ok(document) => {
            let said = document.value.subsumption().map_or_else(
                || tr(Msg::CheckNoOutcome),
                |outcome| subsumption_sentence(&outcome, &code_a, &code_b),
            );
            view! {
                {verdict(Outcome::Ok, view! { {said} }.into_any(), None)}
                {sent(&sent_url, Some((tr(Msg::CheckRaw), document.text)))}
            }
            .into_any()
        }
    };
    (form, answer)
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::load_report::Outcome;

    use super::outcome;
    use crate::fhir::validation::Validation;

    #[test]
    fn an_inactive_member_is_a_warning_and_a_stranger_is_bad() {
        let valid = Validation {
            result: Some(true),
            ..Validation::default()
        };
        assert_eq!(Outcome::Ok, outcome(&valid));
        let inactive = Validation {
            result: Some(true),
            inactive: Some(true),
            ..Validation::default()
        };
        assert_eq!(Outcome::Warn, outcome(&inactive));
        let refused = Validation {
            result: Some(false),
            ..Validation::default()
        };
        assert_eq!(Outcome::Bad, outcome(&refused));
        assert_eq!(Outcome::Bad, outcome(&Validation::default()));
    }
}
