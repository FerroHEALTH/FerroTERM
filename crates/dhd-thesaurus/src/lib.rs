// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The DHD Diagnosethesaurus and Verrichtingenthesaurus deliveries
//! ("Uitleverformaat 5.0", <https://www.dhd.nl/assets/uploads/Uitleverformaat-Thesauri-5.0-v1.0.pdf>).
//!
//! A delivery is a set of UTF-8 CSV tables. `ThesaurusConcept` gives the
//! concepts (`ConceptID`, `TypeConcept`, the flags, the validity dates, a
//! LOINC code for laboratory procedures), `ThesaurusTerm` their terms by type
//! and language (the preferred term, synonyms, the patient-friendly terms, the
//! fully specified names with their SNOMED CT identifier, search terms),
//! `ThesaurusConceptRelaties` replacements and splits, `ThesaurusConceptRol`
//! the roles per specialism group, `Parapluterm` umbrella terms, and the
//! `Afleiding*` and `CodeMapping` tables the derivations to ICD-10, DBC, ZA,
//! and other code systems. The reader turns a delivery into a flat
//! [`Classification`] (no hierarchy; the relations are properties) and the
//! SNOMED CT and ICD-10 links into pairs the build writes as concept maps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ::classification::{Class, Classification, PREFERRED, Rubric, collapse};

/// The system URI of the thesauri: the DHD concept identifier OID
/// (<https://www.dhd.nl/producten-diensten/diagnosethesaurus>).
pub const SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.4.3.120.5.1";
/// The Dutch language tag the terms carry.
pub const DUTCH: &str = "nl-NL";
/// The rubric kinds that are designations, the preferred term first
/// (`TypeTerm` values of the delivery, in the reader's spelling).
pub const DESIGNATION_KINDS: [&str; 6] = [PREFERRED, "synonym", "pvo", "pvt", "fsn", "search"];
/// The property that carries the SNOMED CT identifier of a concept.
pub const SNOMED: &str = "snomed";
/// The property that carries an ICD-10 derivation.
pub const ICD10: &str = "icd10";
/// The property that carries a DBC derivation (`DBC_ID (specialism)`).
pub const DBC: &str = "dbc";
/// The property that carries a ZA derivation.
pub const ZA: &str = "za";
/// The property that carries a role (`name=value [group]`).
pub const ROLE: &str = "role";
/// The property that carries a code mapping (`system: code`).
pub const MAPPING: &str = "mapping";
/// The property naming the concepts that replace an ended one.
pub const REPLACED_BY: &str = "replaced-by";
/// The property naming the concepts an ended one was split into.
pub const SPLIT_INTO: &str = "split-into";
/// The property naming the umbrella term a concept falls under.
pub const UMBRELLA: &str = "umbrella";
/// The end date that means "no end" in a delivery.
const NO_END: &str = "2099";

/// A failure to read a delivery.
#[derive(Debug, thiserror::Error)]
pub enum DhdError {
    /// A directory or file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A table the reader needs is not in the delivery.
    #[error("no `{table}` table under {root}")]
    Missing {
        /// The delivery directory.
        root: PathBuf,
        /// The table name.
        table: &'static str,
    },
    /// A table does not parse.
    #[error("cannot parse {path}")]
    Csv {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: csv::Error,
    },
    /// A table lacks a column.
    #[error("{path} has no column `{column}`")]
    Column {
        /// The path.
        path: PathBuf,
        /// The column.
        column: &'static str,
    },
}

/// A delivery: its tables, found by name under a directory.
#[derive(Debug, Clone)]
pub struct Delivery {
    root: PathBuf,
    files: Vec<PathBuf>,
    /// The date the file names carry (`YYYYMMDD`), when they do.
    date: Option<String>,
    /// The thesaurus version the zip or directory name carries (`2.27`).
    version: Option<String>,
}

/// One row of a table, by column name.
#[derive(Debug, Clone, Default)]
pub struct Row {
    fields: BTreeMap<String, String>,
}

impl Row {
    /// The trimmed value of `column`, `None` when absent or empty.
    #[must_use]
    pub fn get(&self, column: &str) -> Option<&str> {
        self.fields
            .get(&column.to_lowercase())
            .map(String::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    }

    /// Whether the row is current at `date`: its `Einddatum` is absent, in
    /// 2099, or not before `date`.
    #[must_use]
    pub fn current_at(&self, date: Option<&str>) -> bool {
        match (self.get("Einddatum"), date) {
            (Some(end), Some(date)) if !end.starts_with(NO_END) => end >= date,
            _ => true,
        }
    }
}

/// What a delivery yields: the thesaurus and its outward links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thesaurus {
    /// The concepts with their terms and properties.
    pub classification: Classification,
    /// `(concept id, SNOMED CT id)` from the fully specified names.
    pub snomed: Vec<(String, String)>,
    /// `(concept id, ICD-10 codes in order)` from the ICD-10 derivations.
    pub icd10: Vec<(String, Vec<String>)>,
}

fn version_from_name(name: &str) -> Option<String> {
    let mut parts = name.split('_');
    while let Some(part) = parts.next() {
        if part.ends_with("thesaurus") {
            let version = parts.next()?;
            return (version.chars().all(|c| c.is_ascii_digit() || c == '.')
                && !version.is_empty())
            .then(|| version.to_owned());
        }
    }
    None
}

fn date_from_name(name: &str) -> Option<String> {
    let head = name.split('_').next()?;
    (head.len() == 8 && head.bytes().all(|b| b.is_ascii_digit())).then(|| head.to_owned())
}

impl Delivery {
    /// Opens the delivery under `root` (the unpacked zip).
    ///
    /// # Errors
    ///
    /// Returns [`DhdError`] when the directory does not read or holds no
    /// `ThesaurusConcept` table.
    pub fn open(root: &Path) -> Result<Self, DhdError> {
        let mut files = Vec::new();
        walk(root, &mut files)?;
        files.sort();
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| {
                p.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .map(str::to_owned)
            })
            .collect();
        let date = names.iter().find_map(|n| date_from_name(n));
        let version = root
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(version_from_name);
        let delivery = Self {
            root: root.to_path_buf(),
            files,
            date,
            version,
        };
        delivery.table("ThesaurusConcept")?;
        Ok(delivery)
    }

    /// The delivery date the file names carry.
    #[must_use]
    pub fn date(&self) -> Option<&str> {
        self.date.as_deref()
    }

    /// The thesaurus version the directory name carries.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// The file of `table` (`…_ThesaurusConcept.csv`).
    ///
    /// # Errors
    ///
    /// Returns [`DhdError::Missing`] when the delivery has no such table.
    pub fn table(&self, table: &'static str) -> Result<PathBuf, DhdError> {
        self.optional(table).ok_or_else(|| DhdError::Missing {
            root: self.root.clone(),
            table,
        })
    }

    /// The file of `table` when the delivery has it.
    #[must_use]
    pub fn optional(&self, table: &str) -> Option<PathBuf> {
        let suffix = format!("_{}.csv", table.to_lowercase());
        self.files
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|n| n.to_lowercase().ends_with(&suffix))
            })
            .cloned()
    }

    /// The rows of `table`, or none when the delivery lacks it.
    ///
    /// # Errors
    ///
    /// Returns [`DhdError`] when the file does not read or parse.
    pub fn rows(&self, table: &str) -> Result<Vec<Row>, DhdError> {
        let Some(path) = self.optional(table) else {
            return Ok(Vec::new());
        };
        read_rows(&path)
    }
}

fn walk(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), DhdError> {
    let io = |source| DhdError::Io {
        path: root.to_path_buf(),
        source,
    };
    for entry in std::fs::read_dir(root).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Reads a table: UTF-8, a header row, quoted fields.
///
/// # Errors
///
/// Returns [`DhdError`] when the file does not read or parse.
pub fn read_rows(path: &Path) -> Result<Vec<Row>, DhdError> {
    let text = std::fs::read_to_string(path).map_err(|source| DhdError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let delimiter = text
        .lines()
        .next()
        .and_then(|h| b",;\t".iter().copied().find(|d| h.contains(char::from(*d))))
        .unwrap_or(b',');
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());
    let csv_error = |source| DhdError::Csv {
        path: path.to_path_buf(),
        source,
    };
    let headers: Vec<String> = reader
        .headers()
        .map_err(csv_error)?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(csv_error)?;
        let mut fields = BTreeMap::new();
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = record.get(i) {
                fields.insert(header.clone(), value.to_owned());
            }
        }
        rows.push(Row { fields });
    }
    Ok(rows)
}

/// The rubric kind of a `TypeTerm` value.
fn term_kind(type_term: &str) -> Option<&'static str> {
    match type_term.to_lowercase().as_str() {
        "voorkeursterm" => Some(PREFERRED),
        "synoniem" => Some("synonym"),
        "pvo" => Some("pvo"),
        "pvt" => Some("pvt"),
        "fsn" => Some("fsn"),
        "zoekterm" => Some("search"),
        _ => None,
    }
}

fn required<'a>(row: &'a Row, column: &'static str, path: &Path) -> Result<&'a str, DhdError> {
    row.get(column).ok_or_else(|| DhdError::Column {
        path: path.to_path_buf(),
        column,
    })
}

/// The concepts of a delivery: the classes by identifier and the concept
/// kinds in the order the table names them.
struct Concepts {
    classes: BTreeMap<String, Class>,
    kinds: Vec<String>,
}

/// The ICD-10 derivations of a concept: the codes with their order number.
type Icd10Derivations = BTreeMap<String, Vec<(u32, String)>>;

/// Reads `ThesaurusConcept`: one class per concept, with the flag columns as
/// rubrics and the validity as the active flag.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read or lacks a column.
fn read_concepts(delivery: &Delivery, date: Option<&str>) -> Result<Concepts, DhdError> {
    let concept_table = delivery.table("ThesaurusConcept")?;
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    let mut kinds: Vec<String> = Vec::new();
    for row in read_rows(&concept_table)? {
        let id = required(&row, "ConceptID", &concept_table)?.to_owned();
        let kind = required(&row, "TypeConcept", &concept_table)?.to_owned();
        if !kinds.contains(&kind) {
            kinds.push(kind.clone());
        }
        let mut rubrics = Vec::new();
        for (column, name) in [
            ("Complicatie", "complication"),
            ("GebruiktImplantaat", "implant"),
            ("Gradatie", "grading"),
            ("Lateraliteit", "laterality"),
            ("LOINCCode", "loinc"),
        ] {
            if let Some(value) = row.get(column) {
                rubrics.push(Rubric {
                    kind: name.to_owned(),
                    language: DUTCH.to_owned(),
                    text: value.to_owned(),
                });
            }
        }
        classes.insert(
            id.clone(),
            Class {
                code: id,
                kind,
                parent: None,
                usage: None,
                valid: None,
                active: row.current_at(date),
                rubrics,
            },
        );
    }
    Ok(Concepts { classes, kinds })
}

/// Reads `ThesaurusTerm` into the classes and returns the SNOMED CT
/// identifier of every fully specified name that carries one.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_terms(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<BTreeMap<String, String>, DhdError> {
    let mut snomed: BTreeMap<String, String> = BTreeMap::new();
    for row in delivery.rows("ThesaurusTerm")? {
        if !row.current_at(date) {
            continue;
        }
        let (Some(id), Some(text), Some(type_term)) = (
            row.get("ConceptID"),
            row.get("Omschrijving"),
            row.get("TypeTerm"),
        ) else {
            continue;
        };
        let Some(class) = classes.get_mut(id) else {
            continue;
        };
        let Some(kind) = term_kind(type_term) else {
            continue;
        };
        class.rubrics.push(Rubric {
            kind: kind.to_owned(),
            language: row.get("TaalCode").unwrap_or(DUTCH).to_owned(),
            text: collapse(text),
        });
        if kind == "fsn"
            && let Some(sctid) = row.get("SnomedID")
        {
            snomed
                .entry(id.to_owned())
                .or_insert_with(|| sctid.to_owned());
            if !class
                .rubrics
                .iter()
                .any(|r| r.kind == SNOMED && r.text == sctid)
            {
                class.rubrics.push(Rubric {
                    kind: SNOMED.to_owned(),
                    language: DUTCH.to_owned(),
                    text: sctid.to_owned(),
                });
            }
        }
    }
    Ok(snomed)
}

/// Reads `ThesaurusConceptRelaties`: the replacements and splits of the ended
/// concepts, as properties of the concept they leave.
///
/// The table is read whole, where every other table drops a row that is not
/// current at the delivery date. A relation exists because the concept it
/// leaves has ended, and that concept is served (inactive), so filtering the
/// relation by the same date would drop exactly the successor a client of the
/// inactive concept has to follow. No published DHD documentation states this,
/// so it is our own design, pinned by a test (#269).
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_relations(
    delivery: &Delivery,
    classes: &mut BTreeMap<String, Class>,
) -> Result<(), DhdError> {
    for row in delivery.rows("ThesaurusConceptRelaties")? {
        let (Some(first), Some(second), Some(kind)) = (
            row.get("ConceptID1"),
            row.get("ConceptID2"),
            row.get("TypeRelatie"),
        ) else {
            continue;
        };
        let name = match kind.to_lowercase().as_str() {
            "vervanging" => REPLACED_BY,
            "splitsing" => SPLIT_INTO,
            _ => continue,
        };
        if let Some(class) = classes.get_mut(first) {
            class.rubrics.push(Rubric {
                kind: name.to_owned(),
                language: DUTCH.to_owned(),
                text: second.to_owned(),
            });
        }
    }
    Ok(())
}

/// Reads `Parapluterm`: the umbrella term a concept falls under.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_umbrella_terms(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<(), DhdError> {
    for row in delivery.rows("Parapluterm")? {
        if !row.current_at(date) {
            continue;
        }
        if let (Some(umbrella), Some(member)) = (row.get("ConceptID1"), row.get("ConceptID2"))
            && let Some(class) = classes.get_mut(member)
        {
            class.rubrics.push(Rubric {
                kind: UMBRELLA.to_owned(),
                language: DUTCH.to_owned(),
                text: umbrella.to_owned(),
            });
        }
    }
    Ok(())
}

/// Reads `ThesaurusConceptRol`: the roles per specialism group.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_roles(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<(), DhdError> {
    for row in delivery.rows("ThesaurusConceptRol")? {
        if !row.current_at(date) {
            continue;
        }
        if let (Some(id), Some(name), Some(value)) = (
            row.get("ConceptID"),
            row.get("Rolnaam"),
            row.get("Rolwaarde"),
        ) && let Some(class) = classes.get_mut(id)
        {
            let group = row
                .get("SpecialismeGroepCode")
                .map(|g| format!(" [{g}]"))
                .unwrap_or_default();
            class.rubrics.push(Rubric {
                kind: ROLE.to_owned(),
                language: DUTCH.to_owned(),
                text: format!("{name}={value}{group}"),
            });
        }
    }
    Ok(())
}

/// Reads `AfleidingICD10` into the classes and returns the derivations with
/// their order numbers.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_icd10(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<Icd10Derivations, DhdError> {
    let mut icd10: Icd10Derivations = BTreeMap::new();
    for row in delivery.rows("AfleidingICD10")? {
        if !row.current_at(date) {
            continue;
        }
        if let (Some(id), Some(code)) = (row.get("ConceptID"), row.get("ICD10"))
            && let Some(class) = classes.get_mut(id)
        {
            let order = row
                .get("Volgnummer")
                .and_then(|v| v.parse().ok())
                .unwrap_or(u32::MAX);
            icd10
                .entry(id.to_owned())
                .or_default()
                .push((order, code.to_owned()));
            class.rubrics.push(Rubric {
                kind: ICD10.to_owned(),
                language: DUTCH.to_owned(),
                text: code.to_owned(),
            });
        }
    }
    Ok(icd10)
}

/// Reads `AfleidingDBC` and `AfleidingZA`: the derivations that carry a
/// specialism code.
///
/// # Errors
///
/// Returns [`DhdError`] when a table does not read.
fn read_derivations(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<(), DhdError> {
    for (table, kind, code_column) in [
        ("AfleidingDBC", DBC, "DBC_ID"),
        ("AfleidingZA", ZA, "ZA_Code"),
    ] {
        for row in delivery.rows(table)? {
            if !row.current_at(date) {
                continue;
            }
            if let (Some(id), Some(code)) = (row.get("ConceptID"), row.get(code_column))
                && let Some(class) = classes.get_mut(id)
            {
                let specialism = row
                    .get("SpecialismeCode")
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                class.rubrics.push(Rubric {
                    kind: kind.to_owned(),
                    language: DUTCH.to_owned(),
                    text: format!("{code}{specialism}"),
                });
            }
        }
    }
    Ok(())
}

/// Reads `CodeMapping`: the codes of other systems a concept maps to.
///
/// # Errors
///
/// Returns [`DhdError`] when the table does not read.
fn read_mappings(
    delivery: &Delivery,
    date: Option<&str>,
    classes: &mut BTreeMap<String, Class>,
) -> Result<(), DhdError> {
    for row in delivery.rows("CodeMapping")? {
        if !row.current_at(date) {
            continue;
        }
        if let (Some(id), Some(system), Some(code)) = (
            row.get("ConceptID"),
            row.get("Codestelsel"),
            row.get("Code"),
        ) && let Some(class) = classes.get_mut(id)
        {
            class.rubrics.push(Rubric {
                kind: MAPPING.to_owned(),
                language: DUTCH.to_owned(),
                text: format!("{system}: {code}"),
            });
        }
    }
    Ok(())
}

/// Reads the thesaurus of the delivery under `root`.
///
/// Concepts and terms ended before the delivery date are skipped as terms
/// and served inactive as concepts; the relations and derivations become
/// properties; the SNOMED CT identifiers of the fully specified names and the
/// ICD-10 derivations are also returned as pairs for the concept maps.
///
/// # Errors
///
/// Returns [`DhdError`] when a table does not read or lacks a column.
pub fn read(root: &Path, version: Option<&str>) -> Result<Thesaurus, DhdError> {
    let delivery = Delivery::open(root)?;
    let date = delivery.date();
    let Concepts { mut classes, kinds } = read_concepts(&delivery, date)?;
    let snomed = read_terms(&delivery, date, &mut classes)?;
    read_relations(&delivery, &mut classes)?;
    read_umbrella_terms(&delivery, date, &mut classes)?;
    read_roles(&delivery, date, &mut classes)?;
    let icd10 = read_icd10(&delivery, date, &mut classes)?;
    read_derivations(&delivery, date, &mut classes)?;
    read_mappings(&delivery, date, &mut classes)?;
    let icd10 = icd10
        .into_iter()
        .map(|(id, mut codes)| {
            codes.sort();
            (id, codes.into_iter().map(|(_, c)| c).collect())
        })
        .collect();
    let version = version
        .map(str::to_owned)
        .or_else(|| delivery.version().map(str::to_owned))
        .or_else(|| date.map(str::to_owned));
    let procedures = kinds.iter().any(|k| k.eq_ignore_ascii_case("Verrichting"));
    Ok(Thesaurus {
        classification: Classification {
            name: String::from(if procedures {
                "Verrichtingenthesaurus"
            } else {
                "Diagnosethesaurus"
            }),
            title: String::from(if procedures {
                "DHD Verrichtingenthesaurus"
            } else {
                "DHD Diagnosethesaurus"
            }),
            version,
            language: String::from("nl"),
            kinds,
            classes: classes.into_values().collect(),
            designation_kinds: DESIGNATION_KINDS.iter().map(|k| (*k).to_owned()).collect(),
            hierarchy: None,
        },
        snomed: snomed.into_iter().collect(),
        icd10,
    })
}

#[cfg(test)]
mod tests {
    use super::{Row, date_from_name, term_kind, version_from_name};

    #[test]
    fn names_carry_the_date_and_the_version() {
        assert_eq!(
            version_from_name("20170824_113250_Diagnosethesaurus_2.27_uitleverformaat_5.0")
                .as_deref(),
            Some("2.27")
        );
        assert_eq!(version_from_name("random"), None);
        assert_eq!(
            date_from_name("20221212_140720_uitleverformaat5.0_ThesaurusConcept.csv").as_deref(),
            Some("20221212")
        );
        assert_eq!(term_kind("Voorkeursterm"), Some("preferred"));
        assert_eq!(term_kind("Zoekterm"), Some("search"));
        assert_eq!(term_kind("other"), None);
        let mut row = Row::default();
        row.fields
            .insert(String::from("einddatum"), String::from("20990101"));
        assert!(row.current_at(Some("20250101")));
        row.fields
            .insert(String::from("einddatum"), String::from("20240101"));
        assert!(!row.current_at(Some("20250101")));
        assert!(row.current_at(None));
    }
}
