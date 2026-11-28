// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 provider over the testkit's API-shaped artifacts, asserting
//! what the HL7 terminology ecosystem test cases for ICD-11 assert.

use ferroterm_testkit::icd11::{
    BLOCK, CHOLERA, CLASSICAL, ELTOR, OTHER_VIBRIO, RELEASE, RESIDUAL, SEPSIS, SEPTIC_SHOCK,
    SEVERE, VIBRIO, write_artifacts,
};
use fhir_terminology::icd11::{Icd11Provider, OpenError};
use fhir_terminology::provider::declaration::{Compositional, HierarchyMeaning};
use fhir_terminology::provider::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{
    CodeSystemProvider, Concept, PropertyValue, error::ProviderError,
};

const MMS: &str = "http://id.who.int/icd/release/11/mms";

fn providers() -> (
    tempfile::TempDir,
    Icd11Provider,
    Icd11Provider,
    Icd11Provider,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mms = Icd11Provider::open(&dir.path().join("mms")).expect("opens mms");
    let icf = Icd11Provider::open(&dir.path().join("icf")).expect("opens icf");
    let foundation = Icd11Provider::open(&dir.path().join("entity")).expect("opens entity");
    (dir, mms, icf, foundation)
}

fn located(provider: &Icd11Provider, code: &str) -> Concept {
    provider
        .locate(code)
        .expect("reads")
        .expect("a concept with the code")
        .concept
}

/// A value as text, a `Coding` as `system|code (display)`.
fn text(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Coding {
            system,
            code,
            display,
        } => match display {
            Some(display) => format!("{system}|{code} ({display})"),
            None => format!("{system}|{code}"),
        },
        other => other.as_text(),
    }
}

/// Each property as `code=value [sub=value; ...]`, or `code [...]` with no value.
fn props(provider: &Icd11Provider, concept: Concept) -> Vec<String> {
    provider
        .properties(concept)
        .expect("reads")
        .into_iter()
        .map(|p| {
            let head = match &p.value {
                Some(value) => format!("{}={}", p.code, text(value)),
                None => p.code.clone(),
            };
            let parts: Vec<String> = p
                .subproperties
                .iter()
                .map(|s| format!("{}={}", s.code, text(&s.value)))
                .collect();
            if parts.is_empty() {
                head
            } else {
                format!("{head} [{}]", parts.join("; "))
            }
        })
        .collect()
}

// NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, ICD-11 Codes: the entity id is
// the code and the normal form, the classification code an alternative
// spelling, and the full entity uri no code in MMS or ICF.
#[test]
fn the_entity_id_is_the_code_and_the_classification_code_spells_it_too() {
    let (_dir, mms, _icf, _foundation) = providers();
    assert_eq!(mms.identity().url, MMS);
    assert_eq!(mms.identity().version, RELEASE);
    assert_eq!(
        mms.identity().title.as_deref(),
        Some("ICD-11 for Mortality and Morbidity Statistics")
    );
    assert_eq!(
        mms.declaration().hierarchy_meaning,
        Some(HierarchyMeaning::ClassifiedWith)
    );
    assert!(mms.declaration().case_sensitive);
    assert_eq!(mms.declaration().languages, ["en", "fr"]);
    let by_id = located(&mms, CHOLERA);
    let by_code = located(&mms, "1A00");
    assert_eq!(by_id, by_code);
    assert_eq!(
        mms.locate(CHOLERA).expect("reads").expect("located").code,
        CHOLERA,
        "the id is its own normal form"
    );
    assert_eq!(
        mms.locate("1A00").expect("reads").expect("located").code,
        CHOLERA,
        "the classification code's normal form is the id"
    );
    assert_eq!(mms.code(by_code).expect("reads").as_deref(), Some(CHOLERA));
    for uri in [
        format!("{MMS}/{CHOLERA}"),
        format!("http://id.who.int/icd/release/11/{RELEASE}/mms/{CHOLERA}"),
    ] {
        assert!(
            mms.locate(&uri).expect("reads").is_none(),
            "a full entity uri is no MMS code: {uri}"
        );
    }
    assert!(
        mms.locate("1a00").expect("reads").is_none(),
        "case sensitive"
    );
    assert!(mms.locate("XXXX9").expect("reads").is_none());
    assert!(
        mms.locate("01002").expect("reads").is_none(),
        "an id is matched as the release spells it"
    );
    assert_eq!(
        mms.display(by_code, None).expect("reads").as_deref(),
        Some("Cholera")
    );
    assert_eq!(
        mms.display(by_code, Some("fr")).expect("reads").as_deref(),
        Some("Choléra")
    );
    assert_eq!(
        mms.display(by_code, Some("de")).expect("reads").as_deref(),
        Some("Cholera"),
        "English when the language is missing"
    );
    let p = props(&mms, by_code);
    assert!(p.contains(&format!("code={CHOLERA}")), "{p:?}");
    assert!(
        p.contains(&String::from("classificationCode=1A00")),
        "{p:?}"
    );
    assert!(!p.iter().any(|x| x.starts_with("id=")), "{p:?}");
    assert!(p.contains(&format!("parent={BLOCK}")), "{p:?}");
    assert!(p.contains(&String::from("classKind=category")));
    assert!(p.contains(&String::from("exclusion=Vibrio vulnificus infection")));
    // NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, Properties: "A plain (not
    // postcoordinated) concept therefore lists its axes only when
    // post-coordination-metadata=true".
    assert!(
        !p.iter().any(|x| x.starts_with("postcoordination")),
        "{p:?}"
    );
    assert!(!p.iter().any(|x| x.starts_with("notSelectable")));
    assert_eq!(
        mms.definition(by_code).expect("reads").as_deref(),
        Some("An infection of the intestine by Vibrio cholerae.")
    );
}

#[test]
fn a_grouper_and_a_residual_are_named_by_their_ids() {
    let (_dir, mms, _icf, _foundation) = providers();
    let block = located(&mms, BLOCK);
    let block_props = props(&mms, block);
    assert!(block_props.contains(&String::from("notSelectable=true")));
    assert!(block_props.contains(&format!("code={BLOCK}")));
    assert!(
        !block_props
            .iter()
            .any(|x| x.starts_with("classificationCode=")),
        "a grouper has no classification code"
    );
    assert_eq!(
        mms.display(block, None).expect("reads").as_deref(),
        Some("Bacterial intestinal infections")
    );
    let residual = located(&mms, "1A0Y");
    assert_eq!(
        located(&mms, RESIDUAL),
        residual,
        "a residual's id holds a `/`"
    );
    assert!(props(&mms, residual).contains(&format!("code={RESIDUAL}")));
    assert_eq!(
        mms.locate("1A0Y").expect("reads").expect("located").code,
        RESIDUAL
    );
}

#[test]
fn the_foundation_code_is_the_entity_uri() {
    let (_dir, _mms, _icf, foundation) = providers();
    let entity = located(
        &foundation,
        &format!("http://id.who.int/icd/entity/{CHOLERA}"),
    );
    assert_eq!(
        foundation.display(entity, None).expect("reads").as_deref(),
        Some("Cholera")
    );
    assert!(
        props(&foundation, entity)
            .contains(&format!("parent=http://id.who.int/icd/entity/{BLOCK}"))
    );
    assert!(
        foundation.locate(CHOLERA).expect("reads").is_none(),
        "a bare number is not a Foundation code"
    );
    assert!(
        foundation
            .locate("http://id.who.int/icd/entity/1001/other")
            .expect("reads")
            .is_none(),
        "the Foundation has no residuals"
    );
    assert!(
        foundation.declaration().implicit_forms.is_empty(),
        "the Foundation defines no postcoordination scale"
    );
}

#[test]
fn postcoordination_expressions_validate_against_the_axes() {
    let (_dir, mms, icf, _foundation) = providers();
    let simple = located(&mms, "1A00&XN8P1");
    assert_eq!(
        mms.display(simple, None).expect("reads").as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae]")
    );
    let id_form = format!("{CHOLERA}&{CLASSICAL}");
    assert_eq!(
        mms.code(simple).expect("reads").as_deref(),
        Some(id_form.as_str()),
        "the expression's code is its id form"
    );
    assert_eq!(located(&mms, &id_form), simple);
    let p = props(&mms, simple);
    assert!(p.contains(&format!("code={id_form}")), "{p:?}");
    assert!(
        p.contains(&String::from("classificationCode=1A00&XN8P1")),
        "{p:?}"
    );
    assert!(!p.iter().any(|x| x.starts_with("id=")), "{p:?}");
    assert!(
        p.contains(&format!("stem={MMS}|{CHOLERA} (Cholera)")),
        "{p:?}"
    );
    assert!(
        p.contains(&format!(
            "postcoordination [axis=http://id.who.int/icd/schema|infectiousAgent (Infectious agent); value={MMS}|{CLASSICAL} (Vibrio cholerae O1, biovar cholerae)]"
        )),
        "{p:?}"
    );
    assert_eq!(
        p.iter()
            .filter(|x| x.starts_with("postcoordination"))
            .count(),
        1,
        "only the axis with a value: {p:?}"
    );
    assert!(
        mms.locate(&format!("{MMS}/{CHOLERA} & {MMS}/{CLASSICAL}"))
            .expect("reads")
            .is_none(),
        "an expression of full entity uris is no MMS code"
    );
    assert!(
        mms.locate("1A00&XN8P1&XN62R").expect("reads").is_some(),
        "a second value on an AllowAlways axis"
    );
    assert!(matches!(
        mms.locate("1A00&1G41"),
        Err(ProviderError::InvalidCode { ref reason, .. }) if reason.contains("1G41")
    ));
    assert!(
        mms.locate("1A00&XXXX9").is_err(),
        "an unknown value is an error"
    );
    assert!(
        mms.locate("XXXX9&XN8P1").expect("reads").is_none(),
        "an unknown stem is not a code"
    );
    let two_stems = located(&mms, "1A00&XN8P1/XN62R");
    assert_eq!(
        mms.display(two_stems, None).expect("reads").as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae] / Vibrio cholerae O1, biovar eltor"),
        "the display renders the syntax"
    );
    let dot = located(&icf, "d5409.qp3");
    assert_eq!(
        icf.display(dot, None).expect("reads").as_deref(),
        Some("Dressing, unspecified [SEVERE performance difficulty (high, extreme,...) 50-95 %]")
    );
    assert!(props(&icf, dot).iter().any(|x| {
        x.starts_with(
            "postcoordination [axis=http://id.who.int/icd/schema|performance (Performance); value=",
        ) && x.contains(SEVERE)
    }));
    assert!(
        icf.locate("d5409.3").expect("reads").is_none(),
        "the pre-2026 qualifier syntax names no code"
    );
    assert_eq!(located(&icf, "d540"), located(&icf, "d540"));
}

/// The code `provider` locates `text` at: its normal form.
fn normal(provider: &Icd11Provider, text: &str) -> String {
    provider
        .locate(text)
        .expect("reads")
        .expect("a concept with the code")
        .code
}

// NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, Post Coordination Expressions: the normal form
// is the root, each qualifier group in its code's axis order and by entity id within an axis,
// the members likewise against the root's axes and those on none last, duplicates deleted.
#[test]
fn the_normal_form_orders_each_group_by_axis_then_entity_id() {
    let (_dir, mms, icf, _foundation) = providers();
    let ids = |codes: &[&str]| codes.join("&");
    assert_eq!(
        normal(&mms, &ids(&[CHOLERA, ELTOR, CLASSICAL])),
        ids(&[CHOLERA, CLASSICAL, ELTOR]),
        "two values on one axis, by entity id"
    );
    assert_eq!(
        normal(&mms, &ids(&[CHOLERA, SEPSIS, CLASSICAL])),
        ids(&[CHOLERA, CLASSICAL, SEPSIS]),
        "infectiousAgent before associatedWith, as the stem declares them"
    );
    let duplicate = ids(&[CHOLERA, CLASSICAL, CLASSICAL]);
    assert_eq!(normal(&mms, &duplicate), ids(&[CHOLERA, CLASSICAL]));
    assert_eq!(
        mms.spelled_display(located(&mms, &duplicate), &duplicate, None)
            .expect("reads")
            .as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae] [Vibrio cholerae O1, biovar cholerae]"),
        "the display keeps what was written"
    );
    assert_eq!(
        mms.display(located(&mms, &duplicate), None)
            .expect("reads")
            .as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae]"),
        "with no spelling, the display renders the normal form"
    );
    let members = format!("{CHOLERA}/{OTHER_VIBRIO}/{SEPSIS}");
    assert_eq!(
        normal(&mms, &members),
        format!("{CHOLERA}/{SEPSIS}/{OTHER_VIBRIO}"),
        "a member on an axis of the root before one on none"
    );
    assert_eq!(
        mms.spelled_display(located(&mms, &members), &members, None)
            .expect("reads")
            .as_deref(),
        Some("Cholera / Intestinal infection due to other Vibrio / Sepsis without septic shock")
    );
    assert_eq!(
        normal(&mms, &format!("{CHOLERA}/{SEPSIS}/{SEPSIS}")),
        format!("{CHOLERA}/{SEPSIS}")
    );
    assert_eq!(
        normal(
            &mms,
            &format!("{CHOLERA}/{OTHER_VIBRIO}&{SEPTIC_SHOCK}/{OTHER_VIBRIO}&{SEPSIS}")
        ),
        format!("{CHOLERA}/{OTHER_VIBRIO}&{SEPSIS}/{OTHER_VIBRIO}&{SEPTIC_SHOCK}"),
        "the same member with other qualifiers is no duplicate"
    );
    let kept = format!("{CHOLERA}/{CLASSICAL}");
    assert_eq!(normal(&mms, &kept), kept, "a `/` stays a `/`");
    assert_eq!(
        normal(&mms, "1A00&XN62R&XN8P1"),
        ids(&[CHOLERA, CLASSICAL, ELTOR]),
        "short codes normalize to entity ids"
    );
    assert!(
        props(&mms, located(&mms, &ids(&[CHOLERA, ELTOR, CLASSICAL])))
            .contains(&String::from("classificationCode=1A00&XN8P1&XN62R")),
        "the classification code is the normal form's"
    );
    assert_eq!(
        normal(&mms, &format!("{CHOLERA}/{RESIDUAL}")),
        format!("{CHOLERA}/{RESIDUAL}"),
        "a residual's `/other` is part of its id"
    );
    let dotted = format!(
        "{}.{SEVERE}",
        ferroterm_testkit::icd11::DRESSING_UNSPECIFIED
    );
    assert_eq!(normal(&icf, &dotted), dotted, "ICF's `.` stays a `.`");
    assert_eq!(normal(&icf, "d5409.qp3"), dotted);
}

// NOTE: tx-ecosystem `tests/icd-11/doco.txt` at 9c2bbce, "How an expression is read": the axis
// a value binds to "is decided in entity id order, so neither the binding nor the normal form
// depends on the order written".
#[test]
fn members_bind_to_the_roots_axes_in_entity_id_order() {
    let (_dir, mms, _icf, _foundation) = providers();
    let one = located(&mms, &format!("{OTHER_VIBRIO}/{SEPTIC_SHOCK}/{SEPSIS}"));
    let other = located(&mms, &format!("{OTHER_VIBRIO}/{SEPSIS}/{SEPTIC_SHOCK}"));
    assert_eq!(one, other);
    assert_eq!(
        mms.code(one).expect("reads").as_deref(),
        Some(format!("{OTHER_VIBRIO}/{SEPTIC_SHOCK}/{SEPSIS}").as_str()),
        "hasManifestation before hasCausingCondition, as the root declares them"
    );
    let p = props(&mms, one);
    assert!(
        p.contains(&format!(
            "postcoordination [axis=http://id.who.int/icd/schema|hasCausingCondition (Has causing condition); value={MMS}|{SEPSIS} (Sepsis without septic shock)]"
        )),
        "the lower entity id takes the required axis first: {p:?}"
    );
    assert!(
        p.contains(&format!(
            "postcoordination [axis=http://id.who.int/icd/schema|hasManifestation (Has manifestation); value={MMS}|{SEPTIC_SHOCK} (Sepsis with septic shock)]"
        )),
        "{p:?}"
    );
    let off = props(
        &mms,
        located(
            &mms,
            &format!("{CHOLERA}&{CLASSICAL}/{SEPSIS}/{OTHER_VIBRIO}"),
        ),
    );
    assert!(
        off.contains(&format!(
            "postcoordination [value={MMS}|{OTHER_VIBRIO} (Intestinal infection due to other Vibrio)]"
        )),
        "a member on no axis of the root has a property of its own, with no axis: {off:?}"
    );
    assert!(
        off.contains(&format!(
            "postcoordination [axis=http://id.who.int/icd/schema|associatedWith (Associated with); value={MMS}|{SEPSIS} (Sepsis without septic shock)]"
        )),
        "{off:?}"
    );
}

// NOTE: tx-ecosystem `cs-validate-nf-axis-not-multiple` at 9c2bbce: several values on an axis
// that takes one is valid and in normal form, with a warning naming the axis.
#[test]
fn several_values_on_an_axis_that_takes_one_is_a_warning() {
    let (_dir, mms, _icf, _foundation) = providers();
    let crowded = format!("{RESIDUAL}&{CLASSICAL}&{ELTOR}");
    assert_eq!(normal(&mms, &crowded), crowded);
    let advisories = mms.advisories(located(&mms, &crowded)).expect("reads");
    assert_eq!(advisories.len(), 1, "{advisories:?}");
    assert!(advisories[0].contains("laterality"), "{advisories:?}");
    assert_eq!(
        mms.advisories(located(&mms, &format!("{RESIDUAL}&{CLASSICAL}")))
            .expect("reads"),
        Vec::<String>::new()
    );
    assert_eq!(
        mms.advisories(located(&mms, &format!("{CHOLERA}&{CLASSICAL}&{ELTOR}")))
            .expect("reads"),
        Vec::<String>::new(),
        "an AllowAlways axis takes several"
    );
}

#[test]
fn scales_are_implicit_value_sets_and_the_tree_answers_filters() {
    let (_dir, mms, _icf, _foundation) = providers();
    let url = format!("{MMS}/{CHOLERA}/postcoordinationScale/infectiousAgent");
    let compose = mms
        .implicit_value_set(&url)
        .expect("implicit")
        .expect("compose");
    assert_eq!(compose.include.len(), 1);
    assert_eq!(compose.include[0].filters[0].op, FilterOperator::IsA);
    assert_eq!(compose.include[0].filters[0].value, VIBRIO);
    assert_eq!(
        compose.include[0]
            .system
            .as_ref()
            .and_then(|s| s.version.as_deref()),
        Some(RELEASE)
    );
    assert!(
        mms.implicit_value_set(&format!(
            "http://id.who.int/icd/release/11/{RELEASE}/mms/{CHOLERA}/postcoordinationScale/infectiousAgent"
        ))
        .is_some(),
        "the versioned form"
    );
    // The scale's value set carries the release as version and date, and a
    // name and title of its own (the ecosystem's `icd-11` expansions).
    let metadata = mms.implicit_metadata(&url);
    assert_eq!(metadata.version.as_deref(), Some(RELEASE));
    assert_eq!(metadata.date.as_deref(), Some(RELEASE));
    assert_eq!(metadata.experimental, Some(false));
    assert_eq!(
        metadata.name.as_deref(),
        Some(format!("PostcoordinationScale_{CHOLERA}_infectiousAgent").as_str())
    );
    assert!(metadata.title.is_some());
    assert_eq!(
        mms.implicit_metadata(&format!("{MMS}/{CHOLERA}")),
        fhir_terminology::provider::ImplicitMetadata::default(),
        "a plain entity URI is no implicit value set"
    );
    assert!(
        mms.implicit_value_set(&format!(
            "{MMS}/999999999/postcoordinationScale/infectiousAgent"
        ))
        .is_none(),
        "an unknown entity is an unknown value set"
    );
    assert!(
        mms.implicit_value_set(&format!("{MMS}/{CHOLERA}/postcoordinationScale/laterality"))
            .is_none()
    );
    let under = mms
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::IsA,
            value: String::from(VIBRIO),
        })
        .expect("filters");
    assert_eq!(under.len(), 4);
    let by_code = mms
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::IsA,
            value: String::from("XN7N1"),
        })
        .expect("filters");
    assert_eq!(under, by_code);
    let hierarchy = mms.hierarchy().expect("tree");
    let chapter = located(&mms, "01");
    let cholera = located(&mms, "1A00");
    assert!(hierarchy.ancestors(cholera).contains(chapter.index()));
    assert!(
        hierarchy.ancestors(located(&mms, "1A00&XN8P1")).is_empty(),
        "an expression has no place in the tree"
    );
    assert_eq!(mms.all().expect("all").len(), 12);
    assert_eq!(mms.search("chol", Some("fr")).expect("searches").len(), 1);
    assert_eq!(mms.search("asiatic", None).expect("searches").len(), 1);
    // NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, Filter Properties: the text
    // filter matches the start of the classification code.
    let xn = mms.search("XN", None).expect("searches");
    for id in [VIBRIO, CLASSICAL] {
        assert!(xn.contains(located(&mms, id).index()), "{id}");
    }
    assert!(!xn.contains(located(&mms, CHOLERA).index()));
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_artifact(dir.path()).expect("builds");
    assert!(matches!(
        Icd11Provider::open(dir.path()),
        Err(OpenError::NotIcd11(_))
    ));
    assert_eq!(PropertyValue::Uri(String::from("u")).as_text(), "u");
}

// NOTE: `ValueSet.expansion.contains.abstract` marks an entry the user cannot select as a
// value (<https://hl7.org/fhir/R5/valueset-definitions.html#ValueSet.expansion.contains.abstract>);
// the ecosystem's `notSelectable` cases refuse such a code when `abstract = false`.
#[test]
fn a_codeless_grouper_expands_abstract_and_is_refused_when_abstract_is_not_allowed() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
    use fhir_terminology::operations::{expand, sources::Sources, value_set_validate_code};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let block_id = String::from(BLOCK);
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(MMS.into()),
                concept: [block_id.as_str(), "1A00"]
                    .iter()
                    .map(|code| ValueSetComposeIncludeConcept {
                        code: (*code).into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let inline = fhir_terminology::valueset::convert::r4b::convert(&inline);
    let vs = expand::expand(
        &sources,
        &ExpandInput {
            inline_value_set: Some(inline.clone()),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let block = vs
        .contains
        .iter()
        .find(|c| c.code == block_id)
        .expect("the block");
    assert!(block.abstract_concept, "a codeless grouper is abstract");
    let cholera = vs
        .contains
        .iter()
        .find(|c| c.code == "1A00")
        .expect("cholera");
    assert!(!cholera.abstract_concept);

    let refused = value_set_validate_code::validate_code(
        &sources,
        &ValueSetValidateInput {
            inline_value_set: Some(inline.clone()),
            code: Some(block_id.clone()),
            system: Some(MMS.to_owned()),
            abstract_ok: Some(false),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(!refused.result, "abstract = false refuses the grouper");
    assert!(
        refused
            .issues
            .iter()
            .any(|i| i.kind == "code-rule" && i.text.contains("abstract")),
        "{:?}",
        refused.issues
    );
    let allowed = value_set_validate_code::validate_code(
        &sources,
        &ValueSetValidateInput {
            inline_value_set: Some(inline),
            code: Some(block_id),
            system: Some(MMS.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(allowed.result, "abstract defaults to allowed");
}

#[test]
fn a_codeless_grouper_is_abstract_and_codeless_and_a_coded_concept_is_neither() {
    let (_dir, mms, _icf, _foundation) = providers();
    // The codeless block is abstract for expansions and validation; `$lookup`
    // answers it `notSelectable` only (the ecosystem's `lookup-mms-no-code`).
    let status = mms.status(located(&mms, BLOCK)).expect("reads");
    assert!(status.abstract_concept);
    assert!(status.codeless);
    let status = mms.status(located(&mms, "1A00")).expect("reads");
    assert!(!status.abstract_concept);
    assert!(!status.codeless);
}

// NOTE: `excludePostCoordinated` leaves post-coordinated expressions out of an
// expansion (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
#[test]
fn the_exclude_flags_drop_a_postcoordinated_expression_and_a_codeless_grouper() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::{expand, sources::Sources};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let block_id = String::from(BLOCK);
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(MMS.into()),
                concept: [block_id.as_str(), "1A00", "1A00&XN8P1"]
                    .iter()
                    .map(|code| ValueSetComposeIncludeConcept {
                        code: (*code).into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let inline = fhir_terminology::valueset::convert::r4b::convert(&inline);
    let codes = |input: ExpandInput| -> Vec<String> {
        let vs = expand::expand(&sources, &input).expect("expands");
        vs.contains.iter().map(|c| c.code.clone()).collect()
    };
    let all = codes(ExpandInput {
        inline_value_set: Some(inline.clone()),
        ..ExpandInput::default()
    });
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all.contains(&String::from("1A00&XN8P1")));
    let no_expressions = codes(ExpandInput {
        inline_value_set: Some(inline.clone()),
        exclude_post_coordinated: Some(true),
        ..ExpandInput::default()
    });
    assert_eq!(no_expressions.len(), 2, "{no_expressions:?}");
    assert!(!no_expressions.iter().any(|c| c.contains('&')));
    assert!(no_expressions.contains(&block_id));
    let selectable = codes(ExpandInput {
        inline_value_set: Some(inline),
        exclude_not_for_ui: Some(true),
        exclude_post_coordinated: Some(true),
        ..ExpandInput::default()
    });
    assert_eq!(selectable, ["1A00"]);
}

// NOTE: no FHIR specification governs which spelling `expansion.contains.code` carries when
// a system admits several; the compose's spelling is kept, in every include and through an
// `include.valueSet` import (the ecosystem's icd-11 `expand-adhoc-enum-uri` pins the direct case).
#[test]
fn the_composes_spelling_survives_an_include_merge_and_a_value_set_import() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::{expand, sources::Sources};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let enumerated = |codes: &[&str]| ValueSetComposeInclude {
        system: Some(MMS.into()),
        concept: codes
            .iter()
            .map(|code| ValueSetComposeIncludeConcept {
                code: (*code).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let code_set = ValueSet {
        url: Some("http://example.org/icd11-code".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![enumerated(&["1A00"])],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut value_sets = ValueSetStore::default();
    value_sets
        .insert(fhir_terminology::valueset::convert::r4b::convert(&code_set).expect("converts"))
        .expect("stores");
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let codes = |value_set: ValueSet| -> Vec<String> {
        let vs = expand::expand(
            &sources,
            &ExpandInput {
                inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
                    &value_set,
                )),
                ..ExpandInput::default()
            },
        )
        .expect("expands");
        vs.contains.iter().map(|c| c.code.clone()).collect()
    };

    // Two includes over one system: the first by entity id, the second by
    // classification code.
    let merged = codes(ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![enumerated(&[CHOLERA]), enumerated(&["XN7N1"])],
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(merged.contains(&String::from(CHOLERA)), "{merged:?}");
    assert!(
        merged.contains(&String::from("XN7N1")),
        "the second include's classification code spelling: {merged:?}"
    );
    assert!(!merged.contains(&String::from(VIBRIO)), "{merged:?}");

    // An import through `include.valueSet` keeps the imported compose's spelling.
    let imported = codes(ValueSet {
        url: Some("http://example.org/inline-import".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                value_set: vec!["http://example.org/icd11-code".into()],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    });
    assert_eq!(
        imported,
        ["1A00"],
        "the imported classification code spelling"
    );
}

#[test]
fn a_linearizations_grammar_is_defined_and_supported_and_the_foundation_has_none() {
    let (_dir, mms, icf, foundation) = providers();
    // `CodeSystem.compositional` is "The code system defines a compositional
    // (post-coordination) grammar"
    // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.compositional>)
    // and `TerminologyCapabilities.codeSystem.version.compositional` is "If the
    // compositional grammar defined by the code system is supported"
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
    // A linearization defines postcoordination clusters and this provider
    // locates one, so both hold; the foundation defines no such grammar.
    for provider in [&mms, &icf] {
        assert_eq!(
            provider.declaration().compositional,
            Compositional::Supported
        );
        assert!(provider.declaration().compositional.defined());
        assert!(provider.declaration().compositional.supported());
    }
    assert_eq!(foundation.declaration().compositional, Compositional::None);
    assert!(!foundation.declaration().compositional.defined());
    assert!(!foundation.declaration().compositional.supported());
}

// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_display_in_the_base_language_does_not_satisfy_a_request_for_another_one() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
    use fhir_terminology::registry::Registry;

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let ask = |code: &str, display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(MMS.to_owned()),
                code: Some(code.to_owned()),
                display: Some(display.to_owned()),
                display_language: Some(String::from("fr")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // The fixture carries a French title for 1A00 alone, so the English title
    // is the wrong display for a French request and the French one is named.
    let wrong = ask(CHOLERA, "Cholera");
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(wrong.display.as_deref(), Some("Choléra"));
    let issue = wrong.issues.first().expect("an issue");
    assert_eq!(issue.severity, "error");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        Some("Display_Name_for__should_be_one_of__instead_of")
    );
    assert!(issue.text.contains("Choléra"), "{}", issue.text);
    assert_eq!(wrong.message.as_deref(), Some(issue.text.as_str()));

    // 1A01 has no French title at all, which is the shape of a linearization
    // built from the English bundle alone: the English display stands, and the
    // answer says the requested language has none.
    let none = ask(OTHER_VIBRIO, "Intestinal infection due to other Vibrio");
    assert!(none.result, "{none:?}");
    let issue = none.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        Some("NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK")
    );
    assert!(none.message.is_some(), "{none:?}");
}

#[test]
fn every_declared_implicit_value_set_form_resolves() {
    // No FHIR spec defines the scale form: the WHO ICD-API's URI
    // (<https://icd.who.int/icdapi>), with the entity and the axis as arguments.
    let (_dir, mms, _icf, foundation) = providers();
    let entity = format!("{MMS}/{CHOLERA}");
    crate::implicit_forms::every_form_resolves(
        &mms,
        &[(
            "[entity]/postcoordinationScale/[axis]",
            &[&entity, "infectiousAgent"],
        )],
    );
    crate::implicit_forms::declares_none(&foundation, &foundation.identity().url);
}

// NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, ICD-11 Codes: `$validate-code`
// reports a classification code "as valid but not in normal form, and returns
// the entity id as `normalized-code`".
#[test]
fn a_classification_code_validates_and_names_the_entity_id_as_its_normal_form() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
    use fhir_terminology::registry::Registry;

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let ask = |code: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(MMS.to_owned()),
                code: Some(code.to_owned()),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };
    let by_code = ask("1A00");
    assert!(by_code.result, "{by_code:?}");
    assert_eq!(by_code.normalized_code.as_deref(), Some(CHOLERA));
    assert!(
        by_code
            .issues
            .iter()
            .any(|i| i.severity == "information" && i.text.contains("normal form")),
        "{:?}",
        by_code.issues
    );
    let by_id = ask(CHOLERA);
    assert!(by_id.result, "{by_id:?}");
    assert_eq!(by_id.normalized_code, None, "the id is the normal form");
    let short_form = ask("1A00&XN8P1");
    assert!(short_form.result, "{short_form:?}");
    assert_eq!(
        short_form.normalized_code.as_deref(),
        Some(format!("{CHOLERA}&{CLASSICAL}").as_str())
    );
    let by_uri = ask(&format!("{MMS}/{CHOLERA}"));
    assert!(
        !by_uri.result,
        "a full entity uri is no MMS code: {by_uri:?}"
    );
}

// NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, Display Terms: an expression's display is built
// from its parts as written, so it follows the spelling the request located.
#[test]
fn two_spellings_of_one_expression_keep_their_own_displays() {
    let (_dir, mms, _icf, _foundation) = providers();
    let written = format!("{CHOLERA}&{ELTOR}&{CLASSICAL}");
    let normal_form = format!("{CHOLERA}&{CLASSICAL}&{ELTOR}");
    let first = located(&mms, &written);
    let second = located(&mms, &normal_form);
    assert_eq!(first, second, "one expression, one concept");
    let eltor_first =
        "Cholera [Vibrio cholerae O1, biovar eltor] [Vibrio cholerae O1, biovar cholerae]";
    let classical_first =
        "Cholera [Vibrio cholerae O1, biovar cholerae] [Vibrio cholerae O1, biovar eltor]";
    assert_eq!(
        mms.spelled_display(first, &written, None)
            .expect("reads")
            .as_deref(),
        Some(eltor_first),
        "the spelling located last does not decide another's display"
    );
    assert_eq!(
        mms.spelled_display(second, &normal_form, None)
            .expect("reads")
            .as_deref(),
        Some(classical_first)
    );
    assert_eq!(
        mms.spelled_display(first, "1A00&XN62R&XN8P1", None)
            .expect("reads")
            .as_deref(),
        Some(eltor_first),
        "short codes spell the same parts"
    );
    assert_eq!(
        mms.spelled_display(first, &format!("{CHOLERA}&{SEPSIS}"), None)
            .expect("reads")
            .as_deref(),
        Some(classical_first),
        "a spelling of another expression falls back to the normal form"
    );
}

// NOTE: every tx-ecosystem `$lookup` case at 9c2bbce echoes the code as sent (the icd-11
// `lookup-pc-pneumonia-cluster-no-metadata`); the normal form is the `code` property.
#[test]
fn lookup_answers_the_code_as_sent_and_the_normal_form_as_a_property() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::lookup::{LookupInput, lookup};
    use fhir_terminology::registry::Registry;

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    for (sent, normal) in [
        ("1A00&XN62R&XN8P1", format!("{CHOLERA}&{CLASSICAL}&{ELTOR}")),
        ("1A00", CHOLERA.to_owned()),
    ] {
        let outcome = lookup(
            &registry,
            &Invocation::Type,
            &LookupInput {
                system: Some(MMS.to_owned()),
                code: Some(sent.to_owned()),
                properties: vec![String::from("code")],
                ..LookupInput::default()
            },
        )
        .expect("looks up");
        assert_eq!(outcome.code, sent);
        assert!(
            outcome
                .properties
                .iter()
                .any(|p| p.code == "code" && p.value == Some(PropertyValue::Code(normal.clone()))),
            "{:?}",
            outcome.properties
        );
    }
}
