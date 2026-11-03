// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `RxNorm` pipeline: read the release, number the concepts, write the
//! artifacts (<https://hl7.org/fhir/R4B/rxnorm.html> for what the provider
//! needs of them).
//!
//! The codes are the `RXCUI`s that have an atom from the source `RXNORM`
//! (the FHIR page: "only with SAB=RXNORM"), sorted numerically; every atom of
//! those concepts is a designation with its term type as the use, the
//! display atom first; `TTY`, `SAB`, the semantic types when the release has
//! them, and every `RXNORM` attribute (`NDC`, `RXN_AVAILABLE_STRENGTH`, ...)
//! are properties under their own names; the `RXNORM` relationships are typed
//! edges (`REL` and `RELA`) in `relations.bin`, and the atom identifiers sit
//! in `atoms.bin` for the `AUI:` filter form. No FHIR or `RxNorm`
//! specification governs the artifact layout: our own design, shared with the
//! other builds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use concept_graph::ordinal::Ordinal;
use concept_graph::relations::{Relations, RelationsError};
use concept_store::builder::StoreBuilder;
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::tables;
use rxnorm_rrf::row::Atom;
use rxnorm_rrf::{Release, RrfError};
use serde_json::json;

use crate::artifact::{self, LanguageKey, Placed, PropertyKeys};
use crate::pipeline::{MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

/// The system URI.
pub const SYSTEM: &str = "http://www.nlm.nih.gov/research/umls/rxnorm";
/// The source whose atoms are the codes.
pub const RXNORM: &str = "RXNORM";
/// The sources whose content the UMLS licence leaves unrestricted.
///
/// The build keeps the atoms of these unless told which others to include
/// (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/prescribe.html>).
pub const UNRESTRICTED_SOURCES: [&str; 2] = ["RXNORM", "MTHSPL"];
/// The typed relationships file beside the store.
pub const RELATIONS_FILE: &str = "relations.bin";
/// The atom table beside the store.
pub const ATOMS_FILE: &str = "atoms.bin";
/// The property that lists a concept's `RXNORM` term types.
pub const TTY_KEY: &str = "TTY";
/// The property that lists the sources with an atom for the concept.
pub const SAB_KEY: &str = "SAB";
/// The property that lists the semantic types (the full release only).
pub const STY_KEY: &str = "STY";
/// The term types whose string is the display, most preferred first
/// (the FHIR page names `SCD` and `SBD`; the rest is our own order for the
/// concepts that have neither).
pub const DISPLAY_TTYS: [&str; 16] = [
    "SCD", "SBD", "GPCK", "BPCK", "SCDG", "SBDG", "SCDF", "SBDF", "SCDC", "SBDC", "IN", "PIN",
    "MIN", "BN", "DF", "DFG",
];

/// What the build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The release date recorded as the version.
    pub version: String,
    /// The store file.
    pub store: PathBuf,
    /// Concepts written.
    pub concepts: u64,
    /// Atoms (designations) written.
    pub atoms: u64,
    /// Typed edges written.
    pub relationships: u64,
    /// Words indexed.
    pub words: u64,
}

/// A failure of the `RxNorm` build.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The release does not read.
    #[error(transparent)]
    Release(#[from] RrfError),
    /// The release names no date and none was given.
    #[error("the release names no date; pass `--rxnorm-version`")]
    NoVersion,
    /// The relationships cannot be built or written.
    #[error(transparent)]
    Relations(#[from] RelationsError),
    /// A part of the artifact cannot be built or written.
    #[error(transparent)]
    Artifact(#[from] artifact::Error),
    /// A relationship type the build asked for and never gathered.
    #[error("the relationship type `{name}` is not in the vocabulary")]
    UnknownRelationshipType {
        /// The type the build asked for.
        name: String,
    },
}

/// The language the release's own atoms are in.
///
/// The `RXNORM` atoms a release names its concepts by carry `LAT` `ENG`
/// (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html>). No
/// FHIR/SNOMED spec governs this: our own design.
const BASE_LANGUAGE: &str = "en";

/// The BCP 47 tag of an `RRF` language code.
fn language(lat: &str) -> String {
    match lat {
        "ENG" => String::from("en"),
        "SPA" => String::from("es"),
        "FRE" => String::from("fr"),
        "GER" => String::from("de"),
        "DUT" => String::from("nl"),
        other => other.to_ascii_lowercase(),
    }
}

/// The atoms of one concept, the display atom first.
fn order_atoms(atoms: &mut [Atom]) {
    let rank = |atom: &Atom| {
        let tty_rank = if atom.sab == RXNORM {
            DISPLAY_TTYS
                .iter()
                .position(|t| *t == atom.tty)
                .unwrap_or(DISPLAY_TTYS.len())
        } else {
            DISPLAY_TTYS.len().saturating_add(1)
        };
        (
            tty_rank,
            atom.sab.clone(),
            atom.tty.clone(),
            atom.name.clone(),
            atom.rxaui,
        )
    };
    atoms.sort_by_cached_key(rank);
}

/// The `RXNORM` attributes of the concepts, by `RXCUI` then attribute name.
type Attributes = BTreeMap<u64, BTreeMap<String, BTreeSet<String>>>;

/// The semantic types of the concepts, by `RXCUI`.
type SemanticTypes = BTreeMap<u64, BTreeSet<String>>;

/// The concepts of the release: their atoms, their ordinals, and the concept
/// each atom belongs to.
struct Numbered {
    by_concept: BTreeMap<u64, Vec<Atom>>,
    ordinals: BTreeMap<u64, Ordinal>,
    atom_concepts: BTreeMap<u64, u64>,
}

/// What writing the concepts gathers: the designations to index, the atom
/// identifiers, and the sources with an atom.
#[derive(Default)]
struct Gathered {
    placed: Vec<Placed>,
    atom_pairs: Vec<(u64, u32)>,
    sabs: BTreeSet<String>,
}

/// The counts the manifest and the report record.
struct Counts {
    concepts: usize,
    designations: usize,
    relationships: usize,
    words: usize,
}

/// Numbers the concepts: the atoms of the kept sources, grouped by `RXCUI`,
/// keeping only the concepts with an `RXNORM` atom.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read or there are more
/// concepts than an ordinal can number.
fn number(release: &Release, sources: &[String]) -> Result<Numbered, Error> {
    let kept = |sab: &str| UNRESTRICTED_SOURCES.contains(&sab) || sources.iter().any(|s| s == sab);
    let mut by_concept: BTreeMap<u64, Vec<Atom>> = BTreeMap::new();
    for atom in release.atoms()? {
        let atom = atom?;
        if !kept(&atom.sab) {
            continue;
        }
        by_concept.entry(atom.rxcui).or_default().push(atom);
    }
    by_concept.retain(|_, atoms| atoms.iter().any(|a| a.sab == RXNORM));
    let mut ordinals: BTreeMap<u64, Ordinal> = BTreeMap::new();
    for (i, rxcui) in by_concept.keys().enumerate() {
        ordinals.insert(*rxcui, artifact::ordinal(i)?);
    }
    let mut atom_concepts: BTreeMap<u64, u64> = BTreeMap::new();
    for (rxcui, atoms) in &mut by_concept {
        order_atoms(atoms);
        for atom in atoms.iter() {
            atom_concepts.insert(atom.rxaui, *rxcui);
        }
    }
    Ok(Numbered {
        by_concept,
        ordinals,
        atom_concepts,
    })
}

/// Reads the `RXNORM` attributes of the numbered concepts.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read.
fn read_attributes(release: &Release, numbered: &Numbered) -> Result<Attributes, Error> {
    let mut attributes: Attributes = BTreeMap::new();
    for attribute in release.attributes()? {
        let attribute = attribute?;
        if attribute.sab != RXNORM || !numbered.ordinals.contains_key(&attribute.rxcui) {
            continue;
        }
        attributes
            .entry(attribute.rxcui)
            .or_default()
            .entry(attribute.name)
            .or_default()
            .insert(attribute.value);
    }
    Ok(attributes)
}

/// Reads the semantic types of the numbered concepts, none when the release
/// carries no such file.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read.
fn read_semantic_types(release: &Release, numbered: &Numbered) -> Result<SemanticTypes, Error> {
    let mut semantic_types: SemanticTypes = BTreeMap::new();
    if let Some(rows) = release.semantic_types()? {
        for row in rows {
            let row = row?;
            if numbered.ordinals.contains_key(&row.rxcui) {
                semantic_types
                    .entry(row.rxcui)
                    .or_default()
                    .insert(row.name);
            }
        }
    }
    Ok(semantic_types)
}

/// Builds the typed edges from the `RXNORM` relationships.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read or the edges do not
/// build.
fn build_relations(release: &Release, numbered: &Numbered) -> Result<Relations, Error> {
    let mut types: BTreeSet<String> = BTreeSet::new();
    let mut raw_edges: Vec<(Ordinal, String, Ordinal)> = Vec::new();
    for relationship in release.relationships()? {
        let relationship = relationship?;
        if relationship.sab != RXNORM {
            continue;
        }
        let concept = |cui: Option<u64>, aui: Option<u64>| {
            cui.or_else(|| aui.and_then(|a| numbered.atom_concepts.get(&a).copied()))
                .and_then(|c| numbered.ordinals.get(&c).copied())
        };
        let (Some(first), Some(second)) = (
            concept(relationship.rxcui1, relationship.rxaui1),
            concept(relationship.rxcui2, relationship.rxaui2),
        ) else {
            continue;
        };
        // NOTE: an RRF row states that the second concept has the relationship
        // to the first (the UMLS convention the RxNorm documentation follows).
        types.insert(relationship.rel.clone());
        raw_edges.push((second, relationship.rel, first));
        if let Some(rela) = relationship.rela {
            types.insert(rela.clone());
            raw_edges.push((second, rela, first));
        }
    }
    let types: Vec<String> = types.into_iter().collect();
    let kind_of =
        |name: &str| -> Result<u32, Error> {
            let index = types.iter().position(|t| t == name).ok_or_else(|| {
                Error::UnknownRelationshipType {
                    name: name.to_owned(),
                }
            })?;
            // A count past `u32::MAX` is the whole message, as in `ordinal`.
            let Ok(kind) = u32::try_from(index) else {
                return Err(artifact::Error::TooMany.into());
            };
            Ok(kind)
        };
    let mut edges = Vec::with_capacity(raw_edges.len());
    for (source, kind, target) in raw_edges {
        edges.push((source, kind_of(&kind)?, target));
    }
    let count = artifact::ordinal(numbered.ordinals.len())?.index();
    Ok(Relations::build(count, types, edges)?)
}

/// The designation uses: the term types of the kept atoms, sorted.
fn term_types(numbered: &Numbered) -> Vec<&str> {
    let ttys: BTreeSet<&str> = numbered
        .by_concept
        .values()
        .flat_map(|atoms| atoms.iter().map(|a| a.tty.as_str()))
        .collect();
    ttys.into_iter().collect()
}

/// The property keys of one release, in the order the store numbers them: the
/// term type and source, the semantic type when the release has any, then
/// every attribute name it carries.
fn property_key_names(semantic_types: &SemanticTypes, attributes: &Attributes) -> Vec<String> {
    let mut names: Vec<String> = vec![TTY_KEY.to_owned(), SAB_KEY.to_owned()];
    if !semantic_types.is_empty() {
        names.push(STY_KEY.to_owned());
    }
    let attribute_names: BTreeSet<&str> = attributes
        .values()
        .flat_map(|by_name| by_name.keys().map(String::as_str))
        .collect();
    names.extend(attribute_names.into_iter().map(str::to_owned));
    names
}

/// Writes the concepts with their properties, gathering the designations, the
/// atom identifiers, and the sources.
///
/// # Errors
///
/// Returns [`artifact::Error`] when the store does not take a concept or a property, or
/// a key is unknown.
fn write_concepts(
    builder: &mut StoreBuilder,
    numbered: &Numbered,
    uses: &BTreeMap<&str, u32>,
    keys: &PropertyKeys,
    semantic_types: &SemanticTypes,
    attributes: &Attributes,
) -> Result<Gathered, artifact::Error> {
    let mut gathered = Gathered::default();
    for (rxcui, atoms) in &numbered.by_concept {
        let Some(&ordinal) = numbered.ordinals.get(rxcui) else {
            continue;
        };
        let active = atoms
            .iter()
            .any(|a| a.sab == RXNORM && !matches!(a.suppress.as_str(), "O" | "Y"));
        builder.concept(
            ordinal,
            &Concept {
                code: rxcui.to_string(),
                active,
                effective_time: None,
                module: None,
            },
        )?;
        let mut index = 0u32;
        let mut concept_ttys: BTreeSet<&str> = BTreeSet::new();
        let mut concept_sabs: BTreeSet<&str> = BTreeSet::new();
        for atom in atoms {
            gathered.atom_pairs.push((atom.rxaui, ordinal.index()));
            concept_sabs.insert(&atom.sab);
            if atom.sab == RXNORM {
                concept_ttys.insert(&atom.tty);
            }
            gathered.placed.push(Placed {
                ordinal,
                index,
                record: Designation {
                    id: Some(atom.rxaui.to_string()),
                    term: atom.name.clone(),
                    language: language(&atom.language),
                    use_ordinal: uses.get(atom.tty.as_str()).copied().unwrap_or_default(),
                    active: !matches!(atom.suppress.as_str(), "O" | "Y"),
                },
            });
            index = index.saturating_add(1);
        }
        gathered
            .sabs
            .extend(concept_sabs.iter().map(|s| (*s).to_owned()));
        let codes = |set: &BTreeSet<&str>| -> Vec<PropertyValue> {
            set.iter()
                .map(|s| PropertyValue::Code((*s).to_owned()))
                .collect()
        };
        builder.properties(ordinal, keys.key(TTY_KEY)?, &codes(&concept_ttys))?;
        builder.properties(ordinal, keys.key(SAB_KEY)?, &codes(&concept_sabs))?;
        if let Some(stys) = semantic_types.get(rxcui) {
            let values: Vec<PropertyValue> = stys
                .iter()
                .map(|s| PropertyValue::String(s.clone()))
                .collect();
            builder.properties(ordinal, keys.key(STY_KEY)?, &values)?;
        }
        if let Some(by_name) = attributes.get(rxcui) {
            for (name, values) in by_name {
                let values: Vec<PropertyValue> = values
                    .iter()
                    .map(|v| PropertyValue::String(v.clone()))
                    .collect();
                builder.properties(ordinal, keys.key(name)?, &values)?;
            }
        }
    }
    Ok(gathered)
}

/// Writes the typed relationships beside the store.
///
/// # Errors
///
/// Returns [`Error`] when the relationships or the file cannot be written.
fn write_relations(out: &Path, relations: &Relations) -> Result<(), Error> {
    let mut relation_bytes = Vec::new();
    relations.write_to(&mut relation_bytes)?;
    Ok(artifact::write_file(
        &out.join(RELATIONS_FILE),
        &relation_bytes,
    )?)
}

/// Writes the manifest beside the store.
///
/// # Errors
///
/// Returns [`artifact::Error`] when the manifest cannot be rendered or
/// written.
fn write_manifest(
    out: &Path,
    version: &str,
    sources: &BTreeSet<String>,
    counts: &Counts,
    semantic_types: bool,
) -> Result<(), artifact::Error> {
    artifact::write_manifest(
        out,
        &json!({
            "manifest": MANIFEST_VERSION,
            "system": SYSTEM,
            "edition": SYSTEM,
            "version": version,
            "store": STORE_FILE,
            "storeLayout": tables::LAYOUT_VERSION,
            "text": TEXT_FILE,
            "relations": RELATIONS_FILE,
            "atoms": ATOMS_FILE,
            "semanticTypes": semantic_types,
            "sources": sources,
            "concepts": counts.concepts,
            "designations": counts.designations,
            "relationships": counts.relationships,
            "words": counts.words,
            "language": BASE_LANGUAGE,
            "languages": [BASE_LANGUAGE],
        }),
    )
}

/// Builds the artifacts for the release under `root` into `out`.
///
/// `sources` names the sources (`SAB`) whose atoms are kept beside the
/// unrestricted ones ([`UNRESTRICTED_SOURCES`]), so a full release under a
/// UMLS licence needs an explicit list to serve the restricted sources it
/// carries.
///
/// # Errors
///
/// Returns [`Error`] when the release does not read, no date is known, or
/// an artifact cannot be written.
pub fn build(
    root: &Path,
    version: Option<&str>,
    sources: &[String],
    out: &Path,
) -> Result<Report, Error> {
    let release = Release::open(root)?;
    let version = version
        .map(str::to_owned)
        .or_else(|| release.version())
        .ok_or(Error::NoVersion)?;
    let numbered = number(&release, sources)?;
    let attributes = read_attributes(&release, &numbered)?;
    let semantic_types = read_semantic_types(&release, &numbered)?;
    let relations = build_relations(&release, &numbered)?;
    std::fs::create_dir_all(out).map_err(artifact::io_error(out))?;
    let store_path = out.join(STORE_FILE);
    let ttys = term_types(&numbered);
    let mut builder = artifact::open_store(&store_path, SYSTEM, &version, &ttys)?;
    let mut uses: BTreeMap<&str, u32> = BTreeMap::new();
    for (i, tty) in ttys.iter().enumerate() {
        uses.insert(tty, artifact::ordinal(i)?.index());
    }
    let keys = PropertyKeys::write(
        &mut builder,
        &property_key_names(&semantic_types, &attributes),
    )?;
    let gathered = write_concepts(
        &mut builder,
        &numbered,
        &uses,
        &keys,
        &semantic_types,
        &attributes,
    )?;
    artifact::write_designations(&mut builder, &gathered.placed)?;
    let words = artifact::write_text_index(out, &gathered.placed, LanguageKey::Whole)?;
    write_relations(out, &relations)?;
    let Gathered {
        placed,
        atom_pairs,
        sabs,
    } = gathered;
    let atoms = artifact::write_key_table(&out.join(ATOMS_FILE), atom_pairs)?;
    // NOTE: the display is the first designation of each concept (the RXNORM
    // atom of the most preferred term type), so the preferred rule names use 0
    // only as a formality.
    artifact::finish(builder)?;
    let counts = Counts {
        concepts: numbered.ordinals.len(),
        designations: placed.len(),
        relationships: relations.edges(),
        words,
    };
    write_manifest(out, &version, &sabs, &counts, !semantic_types.is_empty())?;
    Ok(Report {
        version,
        store: store_path,
        concepts: u64::try_from(counts.concepts).unwrap_or(u64::MAX),
        atoms: u64::try_from(atoms).unwrap_or(u64::MAX),
        relationships: u64::try_from(counts.relationships).unwrap_or(u64::MAX),
        words: u64::try_from(counts.words).unwrap_or(u64::MAX),
    })
}
