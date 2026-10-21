// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The LOINC pipeline: read the release, number everything, write the
//! artifacts (<https://hl7.org/fhir/R4B/loinc.html> for what the provider
//! needs of them).
//!
//! Terms, parts, answer lists, and answers are the concepts, in that order,
//! each group sorted by code; the multiaxial hierarchy is the graph; every
//! `Loinc.csv` column is a property under its own name; the long common
//! name, short name, consumer name, part names, answer texts, and the
//! linguistic variants are the designations, indexed for search. No FHIR or
//! LOINC specification governs the artifact layout: our own design, shared
//! with the SNOMED CT build.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use ::loinc::release::{Release, ReleaseError};
use ::loinc::{answer, part, term, variant};
use concept_graph::closure::{Closure, ClosureError};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::store::Vocabulary;
use concept_store::tables;
use serde_json::json;

use crate::common::{self, LanguageKey, PipelineError, Placed, PropertyKeys};
use crate::pipeline::{HIERARCHY_FILE, MANIFEST_FILE, MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

/// The system URI.
pub const SYSTEM: &str = "http://loinc.org";

/// The language of the release's own tables.
///
/// A release carries its names in `LoincTable/Loinc.csv` and its translations
/// beside them in `AccessoryFiles/LinguisticVariants`, so the table itself is
/// the English one. No FHIR/SNOMED spec governs this: our own design.
pub const BASE_LANGUAGE: &str = "en";

/// The designation uses, by ordinal: the `DESIGNATION_USES` vocabulary.
pub const DESIGNATION_USES: [&str; 5] = [
    "LONG_COMMON_NAME",
    "SHORTNAME",
    "CONSUMER_NAME",
    "DISPLAY",
    "LinguisticVariantDisplayName",
];

/// The property keys the build adds beside the `Loinc.csv` columns.
pub const COPYRIGHT_KEY: &str = "copyright";
/// The answer lists a term is linked to (`LL…` codes).
pub const ANSWER_LIST_KEY: &str = "answer-list";
/// The answers of an answer list (`LA…` codes).
pub const ANSWERS_KEY: &str = "answers";
/// The kind of code: `term`, `part`, `answer-list`, or `answer`.
pub const KIND_KEY: &str = "kind";

/// What the build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The LOINC version recorded.
    pub version: String,
    /// The store file.
    pub store: PathBuf,
    /// Terms written.
    pub terms: u64,
    /// Parts written.
    pub parts: u64,
    /// Answer lists written.
    pub answer_lists: u64,
    /// Designations written.
    pub designations: u64,
    /// Words indexed.
    pub words: u64,
}

/// A failure of the LOINC build.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The release does not read.
    #[error(transparent)]
    Release(#[from] ReleaseError),
    /// The release names no version and none was given.
    #[error("the release names no version; pass `--loinc-version`")]
    NoVersion,
    /// The store cannot be written.
    #[error(transparent)]
    Store(#[from] BuildError),
    /// The hierarchy cannot be built.
    #[error(transparent)]
    Csr(#[from] CsrError),
    /// The hierarchy has a cycle.
    #[error(transparent)]
    Closure(#[from] ClosureError),
    /// The hierarchy cannot be written.
    #[error(transparent)]
    Graph(#[from] concept_graph::persist::PersistError),
    /// The text index cannot be built or written.
    #[error(transparent)]
    Text(#[from] designation_index::index::BuildError),
    /// The text index cannot be written.
    #[error(transparent)]
    TextPersist(#[from] designation_index::persist::PersistError),
    /// A file cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// More concepts than an ordinal can number.
    #[error("too many concepts")]
    TooMany,
    /// A property key the build asked for and never registered.
    #[error("the property key `{key}` is not in the vocabulary")]
    UnknownPropertyKey {
        /// The key the build asked for.
        key: String,
    },
}

/// The version in a release name such as `Loinc_2.82` or `Loinc_2.82.zip`.
#[must_use]
pub fn version_from_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let rest = name.strip_prefix("Loinc_")?;
    let rest = rest.strip_suffix(".zip").unwrap_or(rest);
    (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .then(|| rest.to_owned())
}

impl PipelineError for Error {
    fn too_many() -> Self {
        Self::TooMany
    }

    fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    fn unknown_property_key(key: &str) -> Self {
        Self::UnknownPropertyKey {
            key: key.to_owned(),
        }
    }
}

/// The ordinal of `index`, in this pipeline's error type.
fn ordinal(index: usize) -> Result<Ordinal, Error> {
    common::ordinal(index)
}

/// A closure naming `path` in this pipeline's I/O failure.
fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    common::io_error(path)
}

/// The designations gathered while the concepts are written, numbered per
/// concept.
#[derive(Default)]
struct Designations {
    placed: Vec<Placed>,
    index: u32,
}

impl Designations {
    /// Starts the designations of one concept, numbering them from zero.
    fn start(&mut self) {
        self.index = 0;
    }

    /// Records one designation of `ordinal`, skipping an empty term.
    fn place(&mut self, ordinal: Ordinal, term: &str, language: &str, use_ordinal: u32) {
        if term.is_empty() {
            return;
        }
        self.placed.push(Placed {
            ordinal,
            index: self.index,
            record: Designation {
                id: None,
                term: term.to_owned(),
                language: language.to_owned(),
                use_ordinal,
                active: true,
            },
        });
        self.index += 1;
    }
}

/// The numbered concepts of the release.
struct Numbered {
    ordinals: BTreeMap<String, Ordinal>,
    concepts: Vec<(Ordinal, Concept, &'static str)>,
    /// The class parts only the hierarchy names.
    hierarchy_only: BTreeSet<String>,
}

/// The release tables the build reads.
struct Tables {
    terms: term::Terms,
    parts: Vec<part::Part>,
    edges: Vec<part::Edge>,
    lists: BTreeMap<String, answer::AnswerList>,
    links: Vec<part::Link>,
    variants: Vec<variant::Variant>,
}

/// The counts the manifest and the report record.
struct Counts {
    concepts: usize,
    designations: usize,
    words: usize,
}

/// Per term code, per axis key, the linked part ordinals.
type Linked<'a> = BTreeMap<&'a str, BTreeMap<String, Vec<Ordinal>>>;

/// The column of `Loinc.csv` a link axis fills, or the axis name itself.
fn axis_key(axis: &str) -> &str {
    match axis {
        "TIME" => "TIME_ASPCT",
        "SCALE" => "SCALE_TYP",
        "METHOD" => "METHOD_TYP",
        other => other,
    }
}

/// Per term code, per axis key, the linked part ordinals.
fn link_parts<'a>(links: &'a [part::Link], numbered: &Numbered) -> Linked<'a> {
    let mut out: Linked<'a> = BTreeMap::new();
    for link in links {
        let Some(&part) = numbered.ordinals.get(&link.part.to_ascii_uppercase()) else {
            continue;
        };
        let parts = out
            .entry(link.code.as_str())
            .or_default()
            .entry(axis_key(&link.axis).to_owned())
            .or_default();
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    out
}

fn number(
    terms: &term::Terms,
    parts: &[part::Part],
    lists: &BTreeMap<String, answer::AnswerList>,
    edges: &[part::Edge],
) -> Result<Numbered, Error> {
    let mut codes: Vec<(String, bool, &'static str)> = Vec::new();
    let mut term_codes: Vec<_> = terms
        .rows
        .iter()
        .map(|t| (t.code.clone(), t.active(), "term"))
        .collect();
    term_codes.sort();
    codes.extend(term_codes);
    let mut part_codes: Vec<_> = parts
        .iter()
        .map(|p| {
            (
                p.code.clone(),
                !p.status.eq_ignore_ascii_case(term::DEPRECATED),
                "part",
            )
        })
        .collect();
    part_codes.sort();
    codes.extend(part_codes);
    for list in lists.values() {
        codes.push((list.code.clone(), true, "answer-list"));
    }
    let mut answers: Vec<_> = lists
        .values()
        .flat_map(|l| l.answers.iter().map(|a| (a.code.clone(), true, "answer")))
        .collect();
    answers.sort();
    answers.dedup();
    codes.extend(answers);
    let known: BTreeSet<String> = codes
        .iter()
        .map(|(c, _, _)| c.to_ascii_uppercase())
        .collect();
    let mut hierarchy_only: BTreeSet<String> = BTreeSet::new();
    for edge in edges {
        for code in std::iter::once(&edge.code).chain(edge.parent.iter()) {
            if !known.contains(&code.to_ascii_uppercase()) {
                hierarchy_only.insert(code.clone());
            }
        }
    }
    codes.extend(hierarchy_only.iter().map(|c| (c.clone(), true, "part")));
    let mut ordinals = BTreeMap::new();
    let mut concepts = Vec::with_capacity(codes.len());
    for (code, active, kind) in codes {
        if ordinals.contains_key(&code) {
            continue;
        }
        let ordinal = ordinal(concepts.len())?;
        ordinals.insert(code.to_ascii_uppercase(), ordinal);
        concepts.push((
            ordinal,
            Concept {
                code,
                active,
                effective_time: None,
                module: None,
            },
            kind,
        ));
    }
    Ok(Numbered {
        ordinals,
        concepts,
        hierarchy_only,
    })
}

/// Reads the tables of the release under `root`.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read.
fn read_tables(root: &Path) -> Result<Tables, Error> {
    let release = Release::open(root)?;
    let terms = term::read(&release)?;
    let parts = part::read_parts(&release)?;
    let edges = part::read_hierarchy(&release)?;
    let lists = answer::read(&release)?;
    let links = part::read_links(&release)?;
    let variants = variant::read(&release)?;
    Ok(Tables {
        terms,
        parts,
        edges,
        lists,
        links,
        variants,
    })
}

/// Resolves the version to record: the one given, else the latest the terms
/// name.
///
/// # Errors
///
/// Returns [`Error::NoVersion`] when neither names one.
fn resolve_version(version: Option<&str>, terms: &term::Terms) -> Result<String, Error> {
    match version {
        Some(version) => Ok(version.to_owned()),
        None => terms
            .rows
            .iter()
            .filter_map(|t| t.fields.get("VersionLastChanged"))
            .max()
            .cloned()
            .ok_or(Error::NoVersion),
    }
}

/// The property keys of one LOINC release, in the order the store numbers
/// them: every `Loinc.csv` column, the keys the build adds, then every link
/// axis the release names.
fn property_key_names(terms: &term::Terms, linked: &Linked<'_>) -> Vec<String> {
    let mut names: Vec<String> = terms.columns.clone();
    names.extend([COPYRIGHT_KEY, ANSWER_LIST_KEY, ANSWERS_KEY, KIND_KEY].map(str::to_owned));
    // The linked axes: the six of `Loinc.csv` under their column names, every
    // other link type under its own name.
    for axis in linked.values().flat_map(BTreeMap::keys) {
        if !names.iter().any(|k| k == axis) {
            names.push(axis.clone());
        }
    }
    names
}

/// Creates the store and writes the designation-use vocabulary.
///
/// # Errors
///
/// Returns [`Error`] when the store cannot be created or written.
fn open_store(path: &Path, version: &str) -> Result<StoreBuilder, Error> {
    let mut builder = StoreBuilder::create(path, SYSTEM, version)?;
    for (i, name) in DESIGNATION_USES.iter().enumerate() {
        builder.vocabulary(Vocabulary::DesignationUses, ordinal(i)?.index(), name)?;
    }
    Ok(builder)
}

/// Writes every concept with the kind of code it is.
///
/// # Errors
///
/// Returns [`Error`] when the store does not take a concept or its kind.
fn write_concepts(
    builder: &mut StoreBuilder,
    numbered: &Numbered,
    keys: &PropertyKeys<Error>,
) -> Result<(), Error> {
    for (ordinal, concept, kind) in &numbered.concepts {
        builder.concept(*ordinal, concept)?;
        builder.properties(
            *ordinal,
            keys.key(KIND_KEY)?,
            &[PropertyValue::Code((*kind).to_owned())],
        )?;
    }
    Ok(())
}

/// Writes the terms: their designations and linguistic variants, the
/// `Loinc.csv` columns, the linked parts, and the answer lists they use.
///
/// # Errors
///
/// Returns [`Error`] when the store does not take a property or a key is
/// unknown.
fn write_terms(
    builder: &mut StoreBuilder,
    tables: &Tables,
    numbered: &Numbered,
    linked: &Linked<'_>,
    keys: &PropertyKeys<Error>,
    designations: &mut Designations,
) -> Result<(), Error> {
    let answer_lists_of = answer_lists_by_term(tables);
    for term in &tables.terms.rows {
        let Some(&ordinal) = numbered.ordinals.get(&term.code.to_ascii_uppercase()) else {
            continue;
        };
        place_term_designations(designations, ordinal, term, &tables.variants);
        write_term_axes(builder, keys, ordinal, term, linked.get(term.code.as_str()))?;
        write_term_provenance(builder, keys, ordinal, term, &answer_lists_of)?;
    }
    Ok(())
}

/// The answer lists each term uses, keyed by the term's code.
fn answer_lists_by_term(tables: &Tables) -> BTreeMap<&str, Vec<String>> {
    let mut out: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for list in tables.lists.values() {
        for term_code in &list.terms {
            out.entry(term_code.as_str())
                .or_default()
                .push(list.code.clone());
        }
    }
    out
}

/// The English names of a term and every linguistic variant that carries it.
fn place_term_designations(
    designations: &mut Designations,
    ordinal: Ordinal,
    term: &term::Term,
    variants: &[variant::Variant],
) {
    designations.start();
    designations.place(ordinal, &term.long_common_name, BASE_LANGUAGE, 0);
    designations.place(ordinal, &term.short_name, BASE_LANGUAGE, 1);
    designations.place(ordinal, &term.consumer_name, BASE_LANGUAGE, 2);
    for variant in variants {
        let Some(translation) = variant.terms.get(&term.code) else {
            continue;
        };
        for (text, use_ordinal) in [
            (&translation.long_common_name, 0),
            (&translation.short_name, 1),
            (&translation.display_name, 4),
        ] {
            if let Some(text) = text {
                designations.place(ordinal, text, &variant.language, use_ordinal);
            }
        }
    }
}

/// A term's `Loinc.csv` columns and its linked axes, the linked ones as the
/// part they name.
fn write_term_axes(
    builder: &mut StoreBuilder,
    keys: &PropertyKeys<Error>,
    ordinal: Ordinal,
    term: &term::Term,
    term_links: Option<&BTreeMap<String, Vec<Ordinal>>>,
) -> Result<(), Error> {
    let concepts = |parts: &Vec<Ordinal>| -> Vec<PropertyValue> {
        parts.iter().map(|p| PropertyValue::Concept(*p)).collect()
    };
    for (column, value) in &term.fields {
        let values = match term_links.and_then(|by_axis| by_axis.get(column.as_str())) {
            // NOTE: the FHIR LOINC page types the six axes as Coding
            // (<https://terminology.hl7.org/LOINC.html>): the part is the value.
            Some(parts) => concepts(parts),
            None => vec![PropertyValue::String(value.clone())],
        };
        builder.properties(ordinal, keys.key(column)?, &values)?;
    }
    // Every other link type under its own name; the columns above already
    // carried the six axes `Loinc.csv` names.
    for (axis, parts) in term_links.into_iter().flatten() {
        if !term.fields.contains_key(axis.as_str()) {
            builder.properties(ordinal, keys.key(axis)?, &concepts(parts))?;
        }
    }
    Ok(())
}

/// A term's copyright holder and the answer lists it uses.
fn write_term_provenance(
    builder: &mut StoreBuilder,
    keys: &PropertyKeys<Error>,
    ordinal: Ordinal,
    term: &term::Term,
    answer_lists_of: &BTreeMap<&str, Vec<String>>,
) -> Result<(), Error> {
    let copyright = if term.external_copyright.is_some() {
        "3rdParty"
    } else {
        "LOINC"
    };
    builder.properties(
        ordinal,
        keys.key(COPYRIGHT_KEY)?,
        &[PropertyValue::Code(copyright.to_owned())],
    )?;
    if let Some(lists) = answer_lists_of.get(term.code.as_str()) {
        let values: Vec<PropertyValue> = lists
            .iter()
            .map(|l| PropertyValue::Code(l.clone()))
            .collect();
        builder.properties(ordinal, keys.key(ANSWER_LIST_KEY)?, &values)?;
    }
    Ok(())
}

/// Writes the parts with their names and status.
///
/// # Errors
///
/// Returns [`Error`] when the store does not take a property or a key is
/// unknown.
fn write_parts(
    builder: &mut StoreBuilder,
    parts: &[part::Part],
    numbered: &Numbered,
    keys: &PropertyKeys<Error>,
    designations: &mut Designations,
) -> Result<(), Error> {
    for part in parts {
        let Some(&ordinal) = numbered.ordinals.get(&part.code.to_ascii_uppercase()) else {
            continue;
        };
        designations.start();
        // NOTE: the FHIR LOINC page names no display for a part; the reference
        // servers show `PartName`, and `PartDisplayName` follows as a synonym.
        designations.place(ordinal, &part.name, BASE_LANGUAGE, 3);
        if !part.display_name.is_empty() && part.display_name != part.name {
            designations.place(ordinal, &part.display_name, BASE_LANGUAGE, 3);
        }
        builder.properties(
            ordinal,
            keys.key("STATUS")?,
            &[PropertyValue::String(part.status.clone())],
        )?;
        builder.properties(
            ordinal,
            keys.key(COPYRIGHT_KEY)?,
            &[PropertyValue::Code(String::from("LOINC"))],
        )?;
    }
    Ok(())
}

/// Writes the class parts only the hierarchy names, with its text as their
/// name.
///
/// # Errors
///
/// Returns [`Error`] when the store does not take a property or a key is
/// unknown.
fn write_class_parts(
    builder: &mut StoreBuilder,
    edges: &[part::Edge],
    numbered: &Numbered,
    keys: &PropertyKeys<Error>,
    designations: &mut Designations,
) -> Result<(), Error> {
    let mut named: BTreeSet<&str> = BTreeSet::new();
    for edge in edges {
        if numbered.hierarchy_only.contains(&edge.code)
            && named.insert(edge.code.as_str())
            && let Some(&ordinal) = numbered.ordinals.get(&edge.code.to_ascii_uppercase())
        {
            designations.start();
            designations.place(ordinal, &edge.text, BASE_LANGUAGE, 3);
            builder.properties(
                ordinal,
                keys.key(COPYRIGHT_KEY)?,
                &[PropertyValue::Code(String::from("LOINC"))],
            )?;
        }
    }
    Ok(())
}

/// Writes the answer lists with their answers.
///
/// # Errors
///
/// Returns [`Error`] when the store does not take a property or a key is
/// unknown.
fn write_answer_lists(
    builder: &mut StoreBuilder,
    lists: &BTreeMap<String, answer::AnswerList>,
    numbered: &Numbered,
    keys: &PropertyKeys<Error>,
    designations: &mut Designations,
) -> Result<(), Error> {
    for list in lists.values() {
        let Some(&ordinal) = numbered.ordinals.get(&list.code.to_ascii_uppercase()) else {
            continue;
        };
        designations.start();
        designations.place(ordinal, &list.name, BASE_LANGUAGE, 3);
        let values: Vec<PropertyValue> = list
            .answers
            .iter()
            .map(|a| PropertyValue::Code(a.code.clone()))
            .collect();
        builder.properties(ordinal, keys.key(ANSWERS_KEY)?, &values)?;
        builder.properties(
            ordinal,
            keys.key(COPYRIGHT_KEY)?,
            &[PropertyValue::Code(String::from("LOINC"))],
        )?;
        for answer in &list.answers {
            let Some(&answer_ordinal) = numbered.ordinals.get(&answer.code.to_ascii_uppercase())
            else {
                continue;
            };
            designations.start();
            designations.place(answer_ordinal, &answer.display, BASE_LANGUAGE, 3);
        }
    }
    Ok(())
}

/// Builds the multiaxial hierarchy and writes it beside the store.
///
/// # Errors
///
/// Returns [`Error`] when the hierarchy has a cycle or cannot be written.
fn write_hierarchy(out: &Path, numbered: &Numbered, edges: &[part::Edge]) -> Result<(), Error> {
    let count = ordinal(numbered.concepts.len())?.index();
    let mut is_a = Vec::new();
    for edge in edges {
        if let (Some(child), Some(parent)) = (
            numbered.ordinals.get(&edge.code.to_ascii_uppercase()),
            edge.parent
                .as_ref()
                .and_then(|p| numbered.ordinals.get(&p.to_ascii_uppercase())),
        ) {
            is_a.push((*child, *parent));
        }
    }
    let csr = Csr::build(count, is_a)?;
    let closure = Closure::compute(&csr)?;
    let hierarchy = Hierarchy { is_a: csr, closure };
    let hierarchy_path = out.join(HIERARCHY_FILE);
    let mut graph_bytes = Vec::new();
    hierarchy.write_to(&mut graph_bytes)?;
    std::fs::write(&hierarchy_path, &graph_bytes).map_err(io_error(&hierarchy_path))
}

/// Writes the manifest beside the store.
///
/// # Errors
///
/// Returns [`Error`] when the manifest cannot be rendered or written.
fn write_manifest(
    out: &Path,
    version: &str,
    variants: &[variant::Variant],
    counts: &Counts,
) -> Result<(), Error> {
    let mut languages: Vec<String> = variants.iter().map(|v| v.language.clone()).collect();
    languages.push(String::from(BASE_LANGUAGE));
    languages.sort();
    languages.dedup();
    let manifest = json!({
        "manifest": MANIFEST_VERSION,
        "system": SYSTEM,
        "edition": SYSTEM,
        "version": version,
        "language": BASE_LANGUAGE,
        "store": STORE_FILE,
        "storeLayout": tables::LAYOUT_VERSION,
        "hierarchy": HIERARCHY_FILE,
        "text": TEXT_FILE,
        "concepts": counts.concepts,
        "designations": counts.designations,
        "words": counts.words,
        "languages": languages,
    });
    let manifest_path = out.join(MANIFEST_FILE);
    let text = serde_json::to_string_pretty(&manifest).map_err(|source| Error::Io {
        path: manifest_path.clone(),
        source: io::Error::other(source),
    })?;
    std::fs::write(&manifest_path, format!("{text}\n")).map_err(io_error(&manifest_path))
}

/// Builds the artifacts for the release under `root` into `out`.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read, no version is known, the
/// hierarchy has a cycle, or an artifact cannot be written.
pub fn build(root: &Path, version: Option<&str>, out: &Path) -> Result<Report, Error> {
    let tables = read_tables(root)?;
    let version = resolve_version(version, &tables.terms)?;
    std::fs::create_dir_all(out).map_err(io_error(out))?;
    let numbered = number(&tables.terms, &tables.parts, &tables.lists, &tables.edges)?;
    let store_path = out.join(STORE_FILE);
    let mut builder = open_store(&store_path, &version)?;
    let linked = link_parts(&tables.links, &numbered);
    let keys: PropertyKeys<Error> =
        PropertyKeys::write(&mut builder, &property_key_names(&tables.terms, &linked))?;
    let mut designations = Designations::default();
    write_concepts(&mut builder, &numbered, &keys)?;
    write_terms(
        &mut builder,
        &tables,
        &numbered,
        &linked,
        &keys,
        &mut designations,
    )?;
    write_parts(
        &mut builder,
        &tables.parts,
        &numbered,
        &keys,
        &mut designations,
    )?;
    write_class_parts(
        &mut builder,
        &tables.edges,
        &numbered,
        &keys,
        &mut designations,
    )?;
    write_answer_lists(
        &mut builder,
        &tables.lists,
        &numbered,
        &keys,
        &mut designations,
    )?;
    common::write_designations::<Error>(&mut builder, &designations.placed)?;
    write_hierarchy(out, &numbered, &tables.edges)?;
    let words =
        common::write_text_index::<Error>(out, &designations.placed, LanguageKey::PrimarySubtag)?;
    builder.finish(&PreferredRule {
        preferred: 0,
        // Nothing reads a display column for this system; its provider
        // chooses a display from the designations it already reads.
        display_use: None,
    })?;
    let counts = Counts {
        concepts: numbered.concepts.len(),
        designations: designations.placed.len(),
        words,
    };
    write_manifest(out, &version, &tables.variants, &counts)?;
    Ok(Report {
        version,
        store: store_path,
        terms: u64::try_from(tables.terms.rows.len()).unwrap_or(u64::MAX),
        parts: u64::try_from(tables.parts.len()).unwrap_or(u64::MAX),
        answer_lists: u64::try_from(tables.lists.len()).unwrap_or(u64::MAX),
        designations: u64::try_from(counts.designations).unwrap_or(u64::MAX),
        words: u64::try_from(counts.words).unwrap_or(u64::MAX),
    })
}

#[cfg(test)]
mod tests {
    use concept_store::builder::StoreBuilder;

    use super::{Error, PropertyKeys};

    /// A key the vocabulary never received is a defect in the build, and says
    /// so: it is not the capacity limit `TooMany` reports.
    #[test]
    fn an_unregistered_property_key_names_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut builder =
            StoreBuilder::create(&dir.path().join("store.redb"), super::SYSTEM, "2.83")
                .expect("store");
        let keys: PropertyKeys<Error> =
            PropertyKeys::write(&mut builder, &[String::from("COMPONENT")]).expect("writes");
        assert_eq!(keys.key("COMPONENT").expect("registered"), 0);
        let error = keys.key("SCALE_TYP").expect_err("never registered");
        assert!(
            matches!(&error, Error::UnknownPropertyKey { key } if key == "SCALE_TYP"),
            "the key is named: {error:?}"
        );
        assert_eq!(
            error.to_string(),
            "the property key `SCALE_TYP` is not in the vocabulary"
        );
    }
}
