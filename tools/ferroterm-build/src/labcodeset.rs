// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nederlandse Labcodeset as FHIR resources over the LOINC, SNOMED CT, and
//! UCUM providers.
//!
//! A publication becomes a `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` directory of FHIR R4B
//! resources: the value set of the active concepts over LOINC with their Dutch
//! displays, a LOINC supplement carrying the Dutch names and the publication's
//! properties (materials, units, outcome lists, statuses), and one value set
//! per ordinal outcome list (<https://hl7.org/fhir/R4B/valueset.html>,
//! <https://hl7.org/fhir/R4B/codesystem.html#supplements>). The Labcodeset
//! has no canonical URL of its own on the FHIR wire; the canonicals below are
//! this server's, and the ordinal lists keep Nictiz's OIDs as `urn:oid:` URIs
//! (<https://hl7.org/fhir/R4B/datatypes.html#uri>). No spec governs the
//! resource layout: our own design.

use std::io;
use std::path::{Path, PathBuf};

use ::labcodeset::{ConceptStatus, DUTCH, LabConcept, Outcome, Publication, SNOMED_OID};
use fhir_types::codec::Json;
use fhir_types::r4b::code_system::{
    CodeSystem, CodeSystemConcept, CodeSystemConceptDesignation, CodeSystemConceptProperty,
    CodeSystemConceptPropertyValue, CodeSystemProperty,
};
use fhir_types::r4b::coding::Coding;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeConceptDesignation,
};

/// The directory under `--out` the resources are written to.
pub const DIR: &str = "labcodeset";
/// The canonical of the Labcodeset value set (our own).
pub const VALUE_SET_URL: &str = "https://ferroterm.eu/fhir/ValueSet/nl-labcodeset";
/// The canonical of the LOINC supplement (our own).
pub const SUPPLEMENT_URL: &str = "https://ferroterm.eu/fhir/CodeSystem/nl-labcodeset-loinc";
/// LOINC.
pub const LOINC: &str = "http://loinc.org";
/// SNOMED CT.
pub const SNOMED: &str = "http://snomed.info/sct";
/// UCUM.
pub const UCUM: &str = "http://unitsofmeasure.org";
/// The FHIR version the resources are written in.
const FHIR_VERSION: &str = "4.3.0";

/// A failure to write the resources.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// A file or directory cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// A resource cannot be encoded.
    #[error("cannot encode a resource")]
    Encode(#[from] fhir_types::codec::EncodeError),
}

/// What the build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The resource directory.
    pub dir: PathBuf,
    /// The release (`20260818`).
    pub release: String,
    /// The active concepts in the value set.
    pub active: usize,
    /// The retired concepts, in the supplement only.
    pub retired: usize,
    /// The ordinal value sets.
    pub ordinals: usize,
}

/// Writes the publication as FHIR resources under `<out>/labcodeset/`.
///
/// # Errors
///
/// Returns [`WriteError`] when a file cannot be written or encoded.
pub fn build(publication: &Publication, out: &Path) -> Result<Report, WriteError> {
    let dir = out.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|source| WriteError::Io {
        path: dir.clone(),
        source,
    })?;
    let release = publication.release();
    let manifest = serde_json::json!({
        "name": "nl.nictiz.labcodeset",
        "version": release,
        "fhirVersions": [FHIR_VERSION],
    });
    write_text(&dir.join("package.json"), &manifest)?;
    write(
        &dir,
        "ValueSet-nl-labcodeset",
        &value_set(publication, &release).to_json()?,
    )?;
    write(
        &dir,
        "CodeSystem-nl-labcodeset-loinc",
        &supplement(publication, &release).to_json()?,
    )?;
    for ordinal in &publication.ordinals {
        write(
            &dir,
            &format!("ValueSet-{}", ordinal.id.replace('.', "-")),
            &ordinal_value_set(ordinal).to_json()?,
        )?;
    }
    let active = publication
        .concepts
        .iter()
        .filter(|c| c.status == ConceptStatus::Active)
        .count();
    Ok(Report {
        dir,
        release,
        active,
        retired: publication.concepts.len() - active,
        ordinals: publication.ordinals.len(),
    })
}

fn write(dir: &Path, name: &str, object: &fhir_types::codec::Object) -> Result<(), WriteError> {
    write_text(
        &dir.join(format!("{name}.json")),
        &fhir_types::codec::Value::Object(object.clone()),
    )
}

fn write_text<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), WriteError> {
    let io = |source| WriteError::Io {
        path: path.to_path_buf(),
        source,
    };
    let text = serde_json::to_string_pretty(value).map_err(|e| io(io::Error::other(e)))?;
    std::fs::write(path, format!("{text}\n")).map_err(io)
}

/// `20260818` as the FHIR date `2026-08-18`; the release as given otherwise.
fn date_of(release: &str) -> String {
    if release.len() == 8 && release.bytes().all(|b| b.is_ascii_digit()) {
        let (year, rest) = release.split_at(4);
        let (month, day) = rest.split_at(2);
        return format!("{year}-{month}-{day}");
    }
    release.to_owned()
}

/// The Dutch long name of a concept, when translated.
fn dutch_name(concept: &LabConcept) -> Option<&str> {
    concept
        .loinc
        .translation
        .as_ref()
        .and_then(|t| t.long_name.as_deref())
}

/// The value set of the active concepts over LOINC: the Dutch long name as
/// the display, the English one as a designation.
fn value_set(publication: &Publication, release: &str) -> ValueSet {
    let concepts = publication
        .concepts
        .iter()
        .filter(|c| c.status == ConceptStatus::Active)
        .map(|c| ValueSetComposeIncludeConcept {
            code: c.loinc.code.as_str().into(),
            display: Some(dutch_name(c).unwrap_or(&c.loinc.long_name).into()),
            designation: vec![ValueSetComposeIncludeConceptDesignation {
                language: Some("en".into()),
                value: c.loinc.long_name.as_str().into(),
                ..Default::default()
            }],
            ..Default::default()
        })
        .collect();
    ValueSet {
        url: Some(VALUE_SET_URL.into()),
        version: Some(release.into()),
        language: Some(DUTCH.into()),
        name: Some("NlLabcodeset".into()),
        title: Some("Nederlandse Labcodeset".into()),
        status: "active".into(),
        experimental: Some(false.into()),
        date: Some(date_of(release).as_str().into()),
        publisher: Some("Nictiz".into()),
        description: Some(publication.description.as_str().into()),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(LOINC.into()),
                concept: concepts,
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The LOINC supplement: every concept, active or retired, with its Dutch
/// long name as a designation and the publication's facts as properties.
fn supplement(publication: &Publication, release: &str) -> CodeSystem {
    let property = |code: &str, kind: &str, description: &str| CodeSystemProperty {
        code: code.into(),
        description: Some(description.into()),
        r#type: kind.into(),
        ..Default::default()
    };
    let properties = vec![
        property(
            "labcodeset-status",
            "code",
            "active or retired in the Labcodeset",
        ),
        property(
            "loinc-status",
            "code",
            "LOINC's status of the code: ACTIVE, DEPRECATED, or DISCOURAGED",
        ),
        property(
            "material",
            "Coding",
            "A SNOMED CT specimen the concept is observed on",
        ),
        property("unit", "Coding", "A UCUM unit the concept is reported in"),
        property(
            "outcome-refset",
            "Coding",
            "The SNOMED CT reference set of the nominal outcomes",
        ),
        property(
            "outcome-valueset",
            "string",
            "The value set of the ordinal outcomes",
        ),
        property(
            "replaced-by",
            "code",
            "The LOINC code replacing a deprecated one",
        ),
        property(
            "retired-reason",
            "string",
            "Why the concept was retired from the Labcodeset",
        ),
        property(
            "retired-replacement",
            "string",
            "The concepts replacing a retired one",
        ),
        property("release-note", "string", "The publication's release note"),
        property("nl-component", "string", "The component axis in Dutch"),
        property("nl-property", "string", "The property axis in Dutch"),
        property("nl-timing", "string", "The timing axis in Dutch"),
        property("nl-system", "string", "The system axis in Dutch"),
        property("nl-scale", "string", "The scale axis in Dutch"),
        property("nl-method", "string", "The method axis in Dutch"),
        property("nl-class", "string", "The LOINC class in Dutch"),
    ];
    let concepts = publication
        .concepts
        .iter()
        .map(|c| concept_entry(publication, c))
        .collect();
    CodeSystem {
        url: Some(SUPPLEMENT_URL.into()),
        version: Some(release.into()),
        language: Some(DUTCH.into()),
        name: Some("NlLabcodesetLoincSupplement".into()),
        title: Some("Nederlandse Labcodeset: LOINC supplement".into()),
        status: "active".into(),
        experimental: Some(false.into()),
        date: Some(date_of(release).as_str().into()),
        publisher: Some("Nictiz".into()),
        description: Some(publication.description.as_str().into()),
        content: "supplement".into(),
        supplements: Some(LOINC.into()),
        property: properties,
        concept: concepts,
        ..Default::default()
    }
}

fn coding(system: &str, code: &str, display: Option<&str>) -> CodeSystemConceptPropertyValue {
    CodeSystemConceptPropertyValue::Coding(Box::new(Coding {
        system: Some(system.into()),
        code: Some(code.into()),
        display: display.map(Into::into),
        ..Default::default()
    }))
}

/// The Dutch axis names and class of a concept's LOINC translation.
fn translation_properties(
    translation: &::labcodeset::Translation,
    properties: &mut Vec<CodeSystemConceptProperty>,
) {
    let mut push = |code: &str, value: CodeSystemConceptPropertyValue| {
        properties.push(CodeSystemConceptProperty {
            id: None,
            extension: Vec::new(),
            modifier_extension: Vec::new(),
            code: code.into(),
            value,
        });
    };
    let text = |s: &str| CodeSystemConceptPropertyValue::String(s.into());
    let axes = &translation.axes;
    push("nl-component", text(&axes.component));
    for (name, value) in [
        ("nl-property", &axes.property),
        ("nl-timing", &axes.timing),
        ("nl-system", &axes.system),
        ("nl-scale", &axes.scale),
        ("nl-method", &axes.method),
        ("nl-class", &translation.class),
    ] {
        if let Some(value) = value {
            push(name, text(value));
        }
    }
}

/// The retirement notes and the release note a concept states.
fn note_properties(concept: &LabConcept, properties: &mut Vec<CodeSystemConceptProperty>) {
    for (name, value) in [
        ("retired-reason", &concept.retired_reason),
        ("retired-replacement", &concept.retired_replacement),
        ("release-note", &concept.release_note),
    ] {
        if let Some(value) = value {
            properties.push(CodeSystemConceptProperty {
                id: None,
                extension: Vec::new(),
                modifier_extension: Vec::new(),
                code: name.into(),
                value: CodeSystemConceptPropertyValue::String(value.as_str().into()),
            });
        }
    }
}

fn concept_entry(publication: &Publication, concept: &LabConcept) -> CodeSystemConcept {
    let mut properties = Vec::new();
    let mut push = |code: &str, value: CodeSystemConceptPropertyValue| {
        properties.push(CodeSystemConceptProperty {
            id: None,
            extension: Vec::new(),
            modifier_extension: Vec::new(),
            code: code.into(),
            value,
        });
    };
    let text = |s: &str| CodeSystemConceptPropertyValue::String(s.into());
    let code = |s: &str| CodeSystemConceptPropertyValue::Code(s.into());
    push(
        "labcodeset-status",
        code(match concept.status {
            ConceptStatus::Active => "active",
            ConceptStatus::Retired => "retired",
        }),
    );
    push(
        "loinc-status",
        code(match concept.loinc.status {
            ::labcodeset::LoincStatus::Active => "ACTIVE",
            ::labcodeset::LoincStatus::Deprecated => "DEPRECATED",
            ::labcodeset::LoincStatus::Discouraged => "DISCOURAGED",
        }),
    );
    for material in &concept.materials {
        push(
            "material",
            coding(SNOMED, &material.code, Some(&material.display_name)),
        );
    }
    for id in &concept.units {
        if let Some(unit) = publication.unit(id) {
            push("unit", coding(UCUM, &unit.ucum, Some(&unit.dutch_name)));
        }
    }
    match &concept.outcome {
        Some(Outcome::Refset(refset)) => push(
            "outcome-refset",
            coding(SNOMED, &refset.concept_id, Some(&refset.preferred_term)),
        ),
        Some(Outcome::ValueSet(oid)) => push("outcome-valueset", text(&format!("urn:oid:{oid}"))),
        None => {}
    }
    if let Some(replacement) = &concept.loinc.replacement {
        push("replaced-by", code(&replacement.to));
    }
    note_properties(concept, &mut properties);
    if let Some(translation) = &concept.loinc.translation {
        translation_properties(translation, &mut properties);
    }
    CodeSystemConcept {
        code: concept.loinc.code.as_str().into(),
        designation: dutch_name(concept)
            .map(|name| CodeSystemConceptDesignation {
                language: Some(DUTCH.into()),
                value: name.into(),
                ..Default::default()
            })
            .into_iter()
            .collect(),
        property: properties,
        ..Default::default()
    }
}

/// The code system URI of an OID the ordinal lists name.
fn system_of(oid: &str) -> String {
    if oid == SNOMED_OID {
        return String::from(SNOMED);
    }
    format!("urn:oid:{oid}")
}

/// One ordinal outcome list as a value set under its OID.
fn ordinal_value_set(ordinal: &::labcodeset::OrdinalValueSet) -> ValueSet {
    let mut includes: Vec<ValueSetComposeInclude> = Vec::new();
    for concept in &ordinal.concepts {
        let system = system_of(&concept.code_system);
        let entry = ValueSetComposeIncludeConcept {
            code: concept.code.as_str().into(),
            display: Some(concept.display_name.as_str().into()),
            designation: concept
                .descriptions
                .iter()
                .map(
                    |(language, text)| ValueSetComposeIncludeConceptDesignation {
                        language: language.as_deref().map(Into::into),
                        value: text.as_str().into(),
                        ..Default::default()
                    },
                )
                .collect(),
            ..Default::default()
        };
        match includes
            .iter_mut()
            .find(|i| i.system.as_ref().and_then(|s| s.value.as_deref()) == Some(system.as_str()))
        {
            Some(include) => include.concept.push(entry),
            None => includes.push(ValueSetComposeInclude {
                system: Some(system.as_str().into()),
                concept: vec![entry],
                ..Default::default()
            }),
        }
    }
    ValueSet {
        url: Some(format!("urn:oid:{}", ordinal.id).into()),
        version: ordinal
            .version_label
            .as_deref()
            .or(ordinal.effective_date.as_deref())
            .map(Into::into),
        language: Some(DUTCH.into()),
        name: ordinal.name.as_deref().map(Into::into),
        title: Some(ordinal.display_name.as_str().into()),
        status: "active".into(),
        publisher: Some("Nictiz".into()),
        compose: Some(ValueSetCompose {
            include: includes,
            ..Default::default()
        }),
        ..Default::default()
    }
}
