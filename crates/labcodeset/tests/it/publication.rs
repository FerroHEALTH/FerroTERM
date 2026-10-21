// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reader over the `labconcepts` document.

use ::labcodeset::{
    ConceptStatus, DUTCH, LabcodesetError, LoincStatus, Outcome, SNOMED_OID, parse, read,
};
use ferroterm_testkit::labcodeset::{
    CULTURE, EFFECTIVE_DATE, GLUCOSE, OLD_SODIUM, ORDINAL_OID, REFSET, RELEASE, SERUM, SODIUM,
    document, write_publication,
};

#[test]
fn the_document_reads_into_the_model() {
    let dir = tempfile::tempdir().expect("tempdir");
    let document_path = write_publication(dir.path()).expect("writes");
    // The document, or the release directory holding it.
    let publication = read(&document_path).expect("reads the document");
    assert_eq!(read(dir.path()).expect("reads the directory"), publication);
    assert_eq!(publication.effective_date, EFFECTIVE_DATE);
    assert_eq!(publication.release(), RELEASE);
    assert_eq!(
        publication.description,
        "01-01-2026: Synthetic Labcodeset publication"
    );
    assert_eq!(publication.concepts.len(), 3);
    let glucose = &publication.concepts[0];
    assert_eq!(glucose.status, ConceptStatus::Active);
    assert_eq!(glucose.loinc.code, GLUCOSE);
    assert_eq!(glucose.loinc.status, LoincStatus::Active);
    assert_eq!(glucose.loinc.axes.component, "Glucose");
    assert_eq!(glucose.loinc.axes.method, None);
    assert_eq!(glucose.loinc.class, "CHEM");
    assert_eq!(glucose.loinc.order_observation.as_deref(), Some("Both"));
    let translation = glucose.loinc.translation.as_ref().expect("translated");
    assert_eq!(translation.language, DUTCH);
    assert_eq!(
        translation.long_name.as_deref(),
        Some("glucose [massa/volume] in serum of plasma")
    );
    assert_eq!(translation.axes.scale.as_deref(), Some("kwantitatief"));
    assert_eq!(glucose.materials.len(), 1);
    assert_eq!(glucose.materials[0].code, SERUM);
    assert_eq!(
        glucose.outcome,
        Some(Outcome::ValueSet(String::from(ORDINAL_OID)))
    );
    assert_eq!(glucose.units, ["1"]);
    assert_eq!(publication.unit("1").expect("unit").ucum, "mmol/L");
    let sodium = &publication.concepts[1];
    assert_eq!(sodium.status, ConceptStatus::Retired);
    assert_eq!(sodium.loinc.code, OLD_SODIUM);
    assert_eq!(sodium.loinc.status, LoincStatus::Deprecated);
    assert!(sodium.loinc.translation.is_none());
    let replacement = sodium.loinc.replacement.as_ref().expect("replacement");
    assert_eq!(
        (replacement.from.as_str(), replacement.to.as_str()),
        (OLD_SODIUM, SODIUM)
    );
    assert_eq!(replacement.comment, "use the serum or plasma term");
    assert_eq!(
        sodium.retired_reason.as_deref(),
        Some("Afgeraden voor gebruik")
    );
    assert_eq!(sodium.retired_replacement.as_deref(), Some(SODIUM));
    assert_eq!(
        sodium.release_note.as_deref(),
        Some("Vervangen in januari 2026")
    );
    let culture = &publication.concepts[2];
    assert_eq!(culture.loinc.code, CULTURE);
    assert_eq!(culture.loinc.panel_type.as_deref(), Some("Panel"));
    assert_eq!(culture.loinc.axes.method.as_deref(), Some("Culture"));
    match &culture.outcome {
        Some(Outcome::Refset(refset)) => {
            assert_eq!(refset.concept_id, REFSET);
            assert_eq!(refset.preferred_term, "referentieset voor micro-organismen");
        }
        other => panic!("a refset outcome, not {other:?}"),
    }
    assert_eq!(publication.materials.len(), 2);
    assert_eq!(publication.materials[0].system.as_deref(), Some("Ser"));
    assert_eq!(publication.units.len(), 2);
    assert_eq!(publication.units[1].status.as_deref(), Some("retired"));
    assert_eq!(publication.units[1].dutch_name, "milligram per deciliter");
    assert_eq!(publication.ordinals.len(), 1);
    let ordinal = &publication.ordinals[0];
    assert_eq!(ordinal.id, ORDINAL_OID);
    assert_eq!(ordinal.display_name, "Ordinale uitslagenlijst");
    assert_eq!(ordinal.status.as_deref(), Some("final"));
    assert_eq!(ordinal.concepts.len(), 2);
    assert_eq!(ordinal.concepts[0].code_system, SNOMED_OID);
    assert_eq!(
        ordinal.concepts[0].descriptions,
        [(Some(String::from(DUTCH)), String::from("Aangetoond"))]
    );
    assert_eq!(
        ordinal.concepts[1].code_system_name.as_deref(),
        Some("SNOMED CT")
    );
    assert_eq!(publication.nominals.len(), 1);
    assert_eq!(publication.nominals[0].concept_id, REFSET);
}

#[test]
fn an_element_the_schema_does_not_define_is_refused() {
    let path = std::path::Path::new("labconcepts-test.xml");
    let unknown = document().replace(
        "<orderObs>Both</orderObs>",
        "<orderObs>Both</orderObs><colour>red</colour>",
    );
    let error = parse(&unknown, path).expect_err("refused");
    assert!(
        matches!(
            &error,
            LabcodesetError::Unexpected { parent, element, .. }
                if parent == "loincConcept" && element == "colour"
        ),
        "{error}"
    );
    let missing = document().replace(r#" loinc_num="1000-1""#, "");
    let error = parse(&missing, path).expect_err("refused");
    assert!(
        matches!(&error, LabcodesetError::MissingAttribute { attribute, .. } if *attribute == "loinc_num"),
        "{error}"
    );
    let truncated = document().replace("</publication>", "");
    assert!(matches!(
        parse(&truncated, path).expect_err("refused"),
        LabcodesetError::Truncated { .. } | LabcodesetError::Xml { .. }
    ));
    let empty_dir = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        read(empty_dir.path()).expect_err("refused"),
        LabcodesetError::NoDocument { .. }
    ));
}
