// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic ICD-11 cache, shaped like the entity JSON a local ICD-API
//! deployment serves.
//!
//! A small MMS, a smaller ICF, and the Foundation behind them, with English
//! titles throughout and a French title on one entity. The entity
//! identifiers and titles are invented; the shape (the `@id` URIs, `code`,
//! `classKind`, `parent`, `child`, `title`, `indexTerm`,
//! `postcoordinationScale`, the residual `/other` and `/unspecified`
//! entities, a block without a code) is the API's.

use std::path::Path;

use serde_json::{Value, json};

/// The release the cache claims.
pub const RELEASE: &str = "2099-01";
/// The MMS chapter `01`.
pub const CHAPTER: &str = "1000";
/// The block under the chapter (no short code).
pub const BLOCK: &str = "1001";
/// The category `1A00` with an infectious-agent scale and an associated-with scale.
pub const CHOLERA: &str = "1002";
/// The category `1A01` with a required causing-condition scale and a manifestation scale.
pub const OTHER_VIBRIO: &str = "1003";
/// The residual `1A0Y` under the block.
pub const RESIDUAL: &str = "1001/other";
/// The extension chapter `X`.
pub const EXTENSION_CHAPTER: &str = "9000";
/// The extension code `XN7N1` (Vibrio cholerae) with three children.
pub const VIBRIO: &str = "2000";
/// `XN8P1`.
pub const CLASSICAL: &str = "2001";
/// `XN62R`.
pub const ELTOR: &str = "2002";
/// `XN8KD`.
pub const O139: &str = "2003";
/// `1G40`, sepsis without septic shock.
pub const SEPSIS: &str = "3000";
/// `1G41`, sepsis with septic shock.
pub const SEPTIC_SHOCK: &str = "3001";
/// The ICF chapter `d`.
pub const ICF_CHAPTER: &str = "5000";
/// The ICF category `d540`.
pub const DRESSING: &str = "5001";
/// The ICF residual `d5409` with a performance scale.
pub const DRESSING_UNSPECIFIED: &str = "5001/unspecified";
/// The ICF qualifier block (no code).
pub const QUALIFIERS: &str = "5100";
/// The ICF qualifier `qp3`.
pub const SEVERE: &str = "5101";

const MMS: &str = "http://id.who.int/icd/release/11/2099-01/mms";
const ICF: &str = "http://id.who.int/icd/release/11/2099-01/icf";
const ENTITY: &str = "http://id.who.int/icd/entity";
const SCHEMA: &str = "http://id.who.int/icd/schema/";

fn text(language: &str, value: &str) -> Value {
    json!({"@language": language, "@value": value})
}

fn labels(language: &str, values: &[&str]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|v| json!({"label": text(language, v)}))
            .collect(),
    )
}

fn uris(base: &str, ids: &[&str]) -> Value {
    Value::Array(ids.iter().map(|id| json!(format!("{base}/{id}"))).collect())
}

fn scale(base: &str, axis: &str, required: bool, multiple: &str, entities: &[&str]) -> Value {
    json!({
        "@id": format!("{base}/scale/{axis}"),
        "axisName": format!("{SCHEMA}{axis}"),
        "requiredPostcoordination": if required { "true" } else { "false" },
        "allowMultipleValues": multiple,
        "scaleEntity": uris(base, entities),
    })
}

/// One entity of the fixture: the API's fields, positional for brevity.
struct Row<'a> {
    base: &'a str,
    id: &'a str,
    code: &'a str,
    kind: &'a str,
    parent: Option<&'a str>,
    children: &'a [&'a str],
    language: &'a str,
    title: &'a str,
    extra: Value,
}

impl Row<'_> {
    fn json(self) -> Value {
        let parent = match self.parent {
            Some(p) => format!("{}/{p}", self.base),
            None => self.base.to_owned(),
        };
        let mut value = json!({
            "@id": format!("{}/{}", self.base, self.id),
            "parent": [parent],
            "child": uris(self.base, self.children),
            "code": self.code,
            "classKind": self.kind,
            "title": text(self.language, self.title),
            "browserUrl": format!("https://icd.who.int/browse/2099-01/mms/{}#{}", self.language, self.id.replace('/', "%2F")),
        });
        if let (Some(object), Value::Object(more)) = (value.as_object_mut(), self.extra) {
            for (k, v) in more {
                object.insert(k, v);
            }
        }
        value
    }
}

fn write(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)
}

fn file(root: &Path, linearization: &str, language: &str, id: &str) -> std::path::PathBuf {
    root.join(linearization)
        .join(language)
        .join(format!("{}.json", id.replace('/', "~")))
}

fn mms(
    id: &'static str,
    code: &'static str,
    kind: &'static str,
    parent: Option<&'static str>,
    children: &'static [&'static str],
    title: &'static str,
    extra: Value,
) -> (&'static str, Value) {
    (
        id,
        Row {
            base: MMS,
            id,
            code,
            kind,
            parent,
            children,
            language: "en",
            title,
            extra,
        }
        .json(),
    )
}

/// The disease chapter of the MMS fixture.
fn mms_diseases() -> Vec<(&'static str, Value)> {
    vec![
        mms(
            CHAPTER,
            "01",
            "chapter",
            None,
            &[BLOCK, SEPSIS, SEPTIC_SHOCK],
            "Certain infectious or parasitic diseases",
            json!({"definition": text("en", "Conditions caused by pathogenic organisms.")}),
        ),
        mms(
            BLOCK,
            "",
            "block",
            Some(CHAPTER),
            &[CHOLERA, OTHER_VIBRIO, RESIDUAL],
            "Bacterial intestinal infections",
            json!({}),
        ),
        mms(
            CHOLERA,
            "1A00",
            "category",
            Some(BLOCK),
            &[],
            "Cholera",
            json!({
                "definition": text("en", "An infection of the intestine by Vibrio cholerae."),
                "fullySpecifiedName": text("en", "Intestinal infection due to Vibrio cholerae"),
                "inclusion": labels("en", &["cholera syndrome"]),
                "exclusion": labels("en", &["Vibrio vulnificus infection"]),
                "indexTerm": labels("en", &["Cholera", "asiatic cholera"]),
                "source": format!("{ENTITY}/{CHOLERA}"),
                "postcoordinationScale": [
                    scale(MMS, "infectiousAgent", false, "AllowAlways", &[VIBRIO]),
                    scale(MMS, "associatedWith", false, "AllowAlways", &[SEPSIS]),
                ],
            }),
        ),
        mms(
            OTHER_VIBRIO,
            "1A01",
            "category",
            Some(BLOCK),
            &[],
            "Intestinal infection due to other Vibrio",
            json!({
                "indexTerm": labels("en", &["Intestinal infection due to other Vibrio"]),
                "postcoordinationScale": [
                    scale(MMS, "hasManifestation", false, "AllowAlways", &[CHAPTER]),
                    scale(MMS, "hasCausingCondition", true, "AllowAlways", &[CHAPTER]),
                ],
            }),
        ),
        mms(
            RESIDUAL,
            "1A0Y",
            "category",
            Some(BLOCK),
            &[],
            "Other specified bacterial intestinal infections",
            json!({}),
        ),
        mms(
            SEPSIS,
            "1G40",
            "category",
            Some(CHAPTER),
            &[],
            "Sepsis without septic shock",
            json!({}),
        ),
        mms(
            SEPTIC_SHOCK,
            "1G41",
            "category",
            Some(CHAPTER),
            &[],
            "Sepsis with septic shock",
            json!({}),
        ),
    ]
}

/// The extension chapter of the MMS fixture.
fn mms_extension() -> Vec<(&'static str, Value)> {
    vec![
        mms(
            EXTENSION_CHAPTER,
            "X",
            "chapter",
            None,
            &[VIBRIO],
            "Extension Codes",
            json!({}),
        ),
        mms(
            VIBRIO,
            "XN7N1",
            "category",
            Some(EXTENSION_CHAPTER),
            &[CLASSICAL, ELTOR, O139],
            "Vibrio cholerae",
            json!({}),
        ),
        mms(
            CLASSICAL,
            "XN8P1",
            "category",
            Some(VIBRIO),
            &[],
            "Vibrio cholerae O1, biovar cholerae",
            json!({}),
        ),
        mms(
            ELTOR,
            "XN62R",
            "category",
            Some(VIBRIO),
            &[],
            "Vibrio cholerae O1, biovar eltor",
            json!({}),
        ),
        mms(
            O139,
            "XN8KD",
            "category",
            Some(VIBRIO),
            &[],
            "Vibrio cholerae O139",
            json!({}),
        ),
    ]
}

fn root(id: &str, language: &str, title: &str, release: bool, children: Value) -> Value {
    let mut value = json!({
        "@id": id,
        "title": text(language, title),
        "availableLanguages": ["en", "fr"],
    });
    if let Some(object) = value.as_object_mut() {
        object.insert(String::from("child"), children);
        if release {
            object.insert(String::from("releaseId"), json!(RELEASE));
            object.insert(String::from("releaseDate"), json!("2099-01-17"));
        }
    }
    value
}

fn write_mms(root_dir: &Path) -> std::io::Result<()> {
    let children = uris(MMS, &[CHAPTER, EXTENSION_CHAPTER]);
    write(
        &root_dir.join("mms/en/_root.json"),
        &root(
            MMS,
            "en",
            "ICD-11 for Mortality and Morbidity Statistics",
            true,
            children.clone(),
        ),
    )?;
    for (id, value) in mms_diseases().into_iter().chain(mms_extension()) {
        write(&file(root_dir, "mms", "en", id), &value)?;
    }
    write(
        &root_dir.join("mms/fr/_root.json"),
        &root(
            MMS,
            "fr",
            "CIM-11 pour les statistiques de mortalité et de morbidité",
            true,
            children,
        ),
    )?;
    write(
        &file(root_dir, "mms", "fr", CHOLERA),
        &Row {
            base: MMS,
            id: CHOLERA,
            code: "1A00",
            kind: "category",
            parent: Some(BLOCK),
            children: &[],
            language: "fr",
            title: "Choléra",
            extra: json!({"indexTerm": labels("fr", &["Choléra"])}),
        }
        .json(),
    )
}

/// An ICF fixture row: id, code, kind, parent, children, title, extra fields.
type IcfRow<'a> = (
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    &'a [&'a str],
    &'a str,
    Value,
);

fn write_icf(root_dir: &Path) -> std::io::Result<()> {
    write(
        &root_dir.join("icf/en/_root.json"),
        &root(
            ICF,
            "en",
            "International Classification of Functioning, Disability and Health (ICF)",
            true,
            uris(ICF, &[ICF_CHAPTER]),
        ),
    )?;
    let rows: [IcfRow; 5] = [
        (
            ICF_CHAPTER,
            "d",
            "chapter",
            None,
            &[DRESSING, QUALIFIERS],
            "Activities and Participation",
            json!({}),
        ),
        (
            DRESSING,
            "d540",
            "category",
            Some(ICF_CHAPTER),
            &[DRESSING_UNSPECIFIED],
            "Dressing",
            json!({}),
        ),
        (
            DRESSING_UNSPECIFIED,
            "d5409",
            "category",
            Some(DRESSING),
            &[],
            "Dressing, unspecified",
            json!({
                "postcoordinationScale": [scale(ICF, "performance", false, "NotAllowed", &[QUALIFIERS])],
            }),
        ),
        (
            QUALIFIERS,
            "",
            "block",
            Some(ICF_CHAPTER),
            &[SEVERE],
            "Performance qualifiers",
            json!({}),
        ),
        (
            SEVERE,
            "qp3",
            "category",
            Some(QUALIFIERS),
            &[],
            "SEVERE performance difficulty (high, extreme,...) 50-95 %",
            json!({}),
        ),
    ];
    for (id, code, kind, parent, children, title, extra) in rows {
        let row = Row {
            base: ICF,
            id,
            code,
            kind,
            parent,
            children,
            language: "en",
            title,
            extra,
        };
        write(&file(root_dir, "icf", "en", id), &row.json())?;
    }
    Ok(())
}

/// A Foundation fixture row: id, parent, children, title, extra fields.
type FoundationRow<'a> = (&'a str, Option<&'a str>, &'a [&'a str], &'a str, Value);

fn write_foundation(root_dir: &Path) -> std::io::Result<()> {
    write(
        &root_dir.join("entity/en/_root.json"),
        &root(
            ENTITY,
            "en",
            "WHO Family of International Classifications Foundation",
            false,
            uris(ENTITY, &[CHAPTER]),
        ),
    )?;
    let rows: [FoundationRow; 3] = [
        (
            CHAPTER,
            None,
            &[BLOCK],
            "Certain infectious or parasitic diseases",
            json!({}),
        ),
        (
            BLOCK,
            Some(CHAPTER),
            &[CHOLERA],
            "Bacterial intestinal infections",
            json!({}),
        ),
        (
            CHOLERA,
            Some(BLOCK),
            &[],
            "Cholera",
            json!({"synonym": labels("en", &["asiatic cholera"])}),
        ),
    ];
    for (id, parent, children, title, extra) in rows {
        let parent = match parent {
            Some(p) => format!("{ENTITY}/{p}"),
            None => ENTITY.to_owned(),
        };
        let mut value = json!({
            "@id": format!("{ENTITY}/{id}"),
            "parent": [parent],
            "child": uris(ENTITY, children),
            "title": text("en", title),
        });
        if let (Some(object), Value::Object(more)) = (value.as_object_mut(), extra) {
            for (k, v) in more {
                object.insert(k, v);
            }
        }
        write(&file(root_dir, "entity", "en", id), &value)?;
    }
    Ok(())
}

/// Writes the cache under `root`: `mms/{en,fr}`, `icf/en`, and `entity/en`.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_cache(root_dir: &Path) -> std::io::Result<()> {
    write_mms(root_dir)?;
    write_icf(root_dir)?;
    write_foundation(root_dir)
}

/// Builds the cache into `out/mms`, `out/icf`, and `out/entity`.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_artifacts(out: &Path) -> std::io::Result<()> {
    let cache = tempfile::tempdir()?;
    write_cache(cache.path())?;
    ferroterm_build::icd11::build_all(cache.path(), None, out)
        .map(|_| ())
        .map_err(std::io::Error::other)
}
