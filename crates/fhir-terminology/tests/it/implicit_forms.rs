// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit value set forms a provider declares, instantiated against it.
//!
//! No FHIR/SNOMED spec governs the declaration: our own design. Each declared
//! form is a URL template the system's page defines; a test fills every
//! placeholder with a fixture argument and asserts the provider resolves it.

use fhir_terminology::provider::CodeSystemProvider;

/// Replaces the bracketed placeholders of `pattern`, in order, with `arguments`.
pub(crate) fn instantiate(pattern: &str, arguments: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    let mut arguments = arguments.iter();
    while let Some((before, after)) = rest.split_once('[') {
        let (_, tail) = after.split_once(']').expect("the placeholder closes");
        out.push_str(before);
        out.push_str(arguments.next().expect("an argument per placeholder"));
        rest = tail;
    }
    assert!(
        arguments.next().is_none(),
        "every argument fills a placeholder"
    );
    out.push_str(rest);
    out
}

/// Asserts `provider` declares exactly the patterns of `forms` and resolves
/// each, instantiated with its arguments, to a compose.
pub(crate) fn every_form_resolves(provider: &dyn CodeSystemProvider, forms: &[(&str, &[&str])]) {
    let declared: Vec<&str> = provider
        .declaration()
        .implicit_forms
        .iter()
        .map(|form| form.pattern.as_str())
        .collect();
    let tested: Vec<&str> = forms.iter().map(|(pattern, _)| *pattern).collect();
    assert_eq!(
        declared, tested,
        "the test instantiates every declared form"
    );
    for (pattern, arguments) in forms {
        let url = instantiate(pattern, arguments);
        let resolved = provider.implicit_value_set(&url);
        assert!(
            matches!(resolved, Some(Ok(_))),
            "{pattern} instantiated as {url} resolves: {resolved:?}"
        );
    }
}

/// Asserts `provider` declares no form and resolves nothing under `base`.
pub(crate) fn declares_none(provider: &dyn CodeSystemProvider, base: &str) {
    assert!(
        provider.declaration().implicit_forms.is_empty(),
        "{base} declares no implicit value set form"
    );
    for url in [
        base.to_owned(),
        format!("{base}/vs"),
        format!("{base}?fhir_vs"),
    ] {
        assert!(
            provider.implicit_value_set(&url).is_none(),
            "{url} is no implicit value set"
        );
    }
}

#[test]
fn a_pattern_takes_its_arguments_in_order() {
    assert_eq!(
        instantiate("http://example.org/vs", &[]),
        "http://example.org/vs"
    );
    assert_eq!(
        instantiate("http://example.org?fhir_vs=isa/[code]", &["cat"]),
        "http://example.org?fhir_vs=isa/cat"
    );
    assert_eq!(
        instantiate("[entity]/scale/[axis]", &["http://example.org/1", "size"]),
        "http://example.org/1/scale/size"
    );
}
