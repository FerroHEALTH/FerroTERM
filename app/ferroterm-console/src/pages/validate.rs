// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Check: whether a code is in a value set, with `ValueSet/$validate-code`
//! (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>), or how
//! two codes of one system relate, with `CodeSystem/$subsumes`
//! (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).

use std::collections::BTreeMap;

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::load_report::Outcome;
use ferrohealth_ui::view::refusal::{Issue, Severity, issue_list};
use ferrohealth_ui::view::segmented::segmented_links;
use ferrohealth_ui::view::surface::{SectionHead, panel_section};

use crate::fhir::validation::{
    SubsumesRequest, ValidateOn, ValidateRequest, Validation, subsumption_sentence,
};
use crate::fhir::{CODE_SYSTEM, VALUE_SET};
use crate::locale::{Msg, tr, tr_with};
use crate::markup::{code, h, p, strong, t};
use crate::pages::lookup::CODE_PARAM;
use crate::pages::operations::{
    Op, before, choices, frame, options, run, select, sent, text, verdict,
};
use crate::pages::parts::{failure, name_of};
use crate::request::Request;
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
pub(crate) async fn page(request: Request) -> Html {
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
}

/// The choice of what to check, two links that keep the code system.
fn mode(request: &Request, relation: bool, system: &str) -> Html {
    let locale = &request.locale;
    let base = ui_link(Op::Check.path(), request.version);
    let keep = if system.is_empty() {
        String::new()
    } else {
        format!("&{SYSTEM_PARAM}={}", percent(system))
    };
    let links = segmented_links(
        tr(locale, Msg::CheckWhat),
        vec![
            (format!("{base}{keep}"), tr(locale, Msg::CheckMembership)),
            (
                format!("{base}&{MODE_PARAM}={RELATION}{keep}"),
                tr(locale, Msg::CheckRelation),
            ),
        ],
        usize::from(relation),
    );
    h(format!(
        r#"<div class="fui-field"><span class="fui-label" aria-hidden="true">{}</span>{links}</div>"#,
        t(tr(locale, Msg::CheckWhat))
    ))
}

/// A value percent-encoded as one query component.
fn percent(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// Whether a code is in a value set: the options and the answer.
async fn membership_tab(
    request: &Request,
    mode: Html,
    systems: &[(String, String)],
    system_names: &BTreeMap<String, String>,
    system: String,
) -> (Html, Html) {
    let locale = &request.locale;
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
    let fields = Html::join([
        mode,
        select(
            "check-url",
            URL_PARAM,
            tr(locale, Msg::CheckValueSet),
            &sets,
            &url,
            None,
        ),
        text(
            "check-code",
            CODE_PARAM,
            tr(locale, Msg::LookupCode),
            &code,
            tr(locale, Msg::LookupCodeHint),
        ),
        select(
            "check-system",
            SYSTEM_PARAM,
            tr(locale, Msg::CheckItsSystem),
            systems,
            &system,
            Some(tr(locale, Msg::CheckChooseSystem)),
        ),
    ]);
    let form = options(
        request,
        &[],
        fields,
        run(
            locale,
            "check-run",
            tr(locale, Msg::OpsCheck),
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
        return (form, before(locale, tr(locale, Msg::CheckBefore)));
    }
    if !asked.runnable() {
        return (form, before(locale, tr(locale, Msg::CheckNeedsSystem)));
    }
    let sent_url = request.client.validate_code_url(request.version, &asked);
    let answer = match request.client.validate_code(request.version, &asked).await {
        Err(error) => Html::join([
            panel_section(locale, None, failure(locale, &error)),
            sent(locale, &sent_url, None),
        ]),
        Ok(document) => {
            let validation = document.value.validation(locale);
            let set = name_of(&set_names, &url);
            Html::join([
                verdict(
                    locale,
                    outcome(&validation),
                    membership_title(locale, &validation, &code, &set),
                    None,
                ),
                concept(locale, &validation, &code, system_names).unwrap_or_default(),
                issues(locale, &validation).unwrap_or_default(),
                sent(
                    locale,
                    &sent_url,
                    Some((tr(locale, Msg::CheckRaw), document.text)),
                ),
            ])
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
fn membership_title(locale: &Locale, validation: &Validation, code_typed: &str, set: &str) -> Html {
    let display = validation
        .display
        .as_ref()
        .map(|display| h(format!("{} · ", strong(display))))
        .unwrap_or_default();
    let typed = code(code_typed);
    match validation.result {
        Some(true) => h(format!(
            "{} {display}{typed} {}",
            t(tr(locale, Msg::CheckValid)),
            t(tr_with(
                locale,
                Msg::CheckIsIn,
                &[("set", set.to_owned().into())]
            ))
        )),
        Some(false) => h(format!(
            "{} {display}{typed} {}",
            t(tr(locale, Msg::CheckNotValid)),
            t(tr_with(
                locale,
                Msg::CheckIsNotIn,
                &[("set", set.to_owned().into())]
            ))
        )),
        None => t(tr(locale, Msg::CheckNoResult)),
    }
}

/// The concept the answer names: its display, code, system and version.
fn concept(
    locale: &Locale,
    validation: &Validation,
    code_typed: &str,
    names: &BTreeMap<String, String>,
) -> Option<Html> {
    let display = validation.display.as_ref()?;
    let system = validation
        .system
        .as_deref()
        .map(|system| name_of(names, system))
        .unwrap_or_default();
    let place = match &validation.version {
        Some(version) => tr_with(
            locale,
            Msg::CheckSystemVersion,
            &[
                ("system", system.into()),
                ("version", version.clone().into()),
            ],
        ),
        None => system,
    };
    let inactive = if validation.inactive == Some(true) {
        format!(" {}", t(tr(locale, Msg::CheckInactive)))
    } else {
        String::new()
    };
    Some(panel_section(
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::CheckConcept),
            count: None,
            actions: None,
        }),
        h(format!(
            r#"{}<p class="fui-small fui-muted">{} {}{inactive}</p>"#,
            p(t(display)),
            code(
                validation
                    .code
                    .clone()
                    .unwrap_or_else(|| code_typed.to_owned())
            ),
            t(place),
        )),
    ))
}

/// The message and the issues the server answered with, in its own words.
fn issues(locale: &Locale, validation: &Validation) -> Option<Html> {
    if validation.message.is_none() && validation.issues.is_empty() {
        return None;
    }
    let message = validation
        .message
        .as_ref()
        .map(|message| p(t(message)))
        .unwrap_or_default();
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
        locale,
        Some(SectionHead {
            title: tr(locale, Msg::CheckServerSays),
            count: None,
            actions: None,
        }),
        Html::join([message, issue_list(locale, &listed)]),
    ))
}

/// How two codes of one system relate: the options and the answer.
async fn relation_tab(
    request: &Request,
    mode: Html,
    systems: &[(String, String)],
    system: String,
) -> (Html, Html) {
    let locale = &request.locale;
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
    let fields = Html::join([
        mode,
        select(
            "check-system",
            SYSTEM_PARAM,
            tr(locale, Msg::LookupSystem),
            systems,
            &system,
            None,
        ),
        text(
            "check-code-a",
            CODE_A_PARAM,
            tr(locale, Msg::CheckCodeA),
            &code_a,
            tr(locale, Msg::LookupCodeHint),
        ),
        text(
            "check-code-b",
            CODE_B_PARAM,
            tr(locale, Msg::CheckCodeB),
            &code_b,
            tr(locale, Msg::LookupCodeHint),
        ),
    ]);
    let form = options(
        request,
        &[(MODE_PARAM, String::from(RELATION))],
        fields,
        run(
            locale,
            "check-run",
            tr(locale, Msg::CheckTheRelation),
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
        return (form, before(locale, tr(locale, Msg::CheckRelationBefore)));
    }
    let sent_url = request.client.subsumes_url(request.version, &asked);
    let answer = match request.client.subsumes(request.version, &asked).await {
        Err(error) => Html::join([
            panel_section(locale, None, failure(locale, &error)),
            sent(locale, &sent_url, None),
        ]),
        Ok(document) => {
            let said = document.value.subsumption().map_or_else(
                || tr(locale, Msg::CheckNoOutcome),
                |outcome| subsumption_sentence(locale, &outcome, &code_a, &code_b),
            );
            Html::join([
                verdict(locale, Outcome::Ok, t(said), None),
                sent(
                    locale,
                    &sent_url,
                    Some((tr(locale, Msg::CheckRaw), document.text)),
                ),
            ])
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
