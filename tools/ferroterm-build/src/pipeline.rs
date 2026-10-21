// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pipeline: read the Snapshot, number everything, write the artifacts.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use concept_graph::attributes::{self, Attributes, AttributesError};
use concept_graph::closure::{Closure, ClosureError};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::identifiers::{Identifiers, IdentifiersError};
use concept_graph::members::{MembersError, Memberships};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_graph::refsets::{self, MemberRow, RefsetMembers, RefsetsError};
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::record;
use concept_store::store::Vocabulary;
use concept_store::tables;
use designation_index::index::{IndexBuilder, Input};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use rf2::component::{
    AlternateIdentifier, Concept, ConcreteRelationship, ConcreteValue, Description, Relationship,
    Rows,
};
use rf2::constants;
use rf2::edition::{Edition, EditionError};
use rf2::file::{ContentType, FieldKind, Release, ReleaseError, ReleaseFile, ReleaseType};
use rf2::id::{ConceptId, DescriptionId, ModuleId, RefsetId};
use rf2::reader::Rf2Error;
use rf2::refset::{
    FieldValue, LanguageMember, Members, ModuleDependencyMember, RefsetKind, ViewError,
};
use rf2::time::EffectiveTime;
use serde_json::json;

/// The store file name inside the output directory.
pub const STORE_FILE: &str = "store.redb";
/// The manifest file name inside the output directory.
pub const MANIFEST_FILE: &str = concept_store::MANIFEST_FILE;
/// The manifest layout this tool writes.
pub const MANIFEST_VERSION: u32 = 2;
/// The hierarchy artifact (`concept-graph`), beside the store.
pub const HIERARCHY_FILE: &str = "hierarchy.bin";
/// The designation index (`designation-index`), beside the store.
pub const TEXT_FILE: &str = "text.bin";
/// The reference set memberships file beside the store.
pub const REFSETS_FILE: &str = "refsets.bin";
/// The attribute relationships with their role groups and concrete values.
pub const ATTRIBUTES_FILE: &str = "attributes.bin";
/// The reference set member rows with their fields.
pub const MEMBERS_FILE: &str = "members.bin";
/// The alternate identifiers.
pub const IDENTIFIERS_FILE: &str = "identifiers.bin";

/// The fixed property keys the SNOMED loader writes, by ordinal; every
/// attribute type found in the release follows, keyed by its SCTID.
///
/// `parent` follows the FHIR `$lookup` property of that name
/// (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>); the others
/// are the RF2 concept columns. The attribute properties are the SNOMED CT on
/// FHIR rule that every concept-model attribute is a property keyed by the
/// attribute's concept id (<https://hl7.org/fhir/R4B/snomedct.html>).
pub const PROPERTY_KEYS: [&str; 3] = ["parent", "definitionStatus", "module"];

/// The designation-use ordinals: FSN, synonym, definition, in that order.
pub const DESIGNATION_USES: [ConceptId; 3] = [
    constants::FULLY_SPECIFIED_NAME,
    constants::SYNONYM,
    constants::DEFINITION,
];

/// The acceptability ordinals: preferred, acceptable, in that order.
pub const ACCEPTABILITIES: [ConceptId; 2] = [constants::PREFERRED, constants::ACCEPTABLE];

/// A build failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The release directory does not read as an RF2 Snapshot.
    #[error("cannot open the RF2 release")]
    Release(#[from] ReleaseError),
    /// A file the release needs is missing.
    #[error("the release has no {0} file")]
    MissingFile(&'static str),
    /// An RF2 row does not parse.
    #[error("cannot read an RF2 row")]
    Rf2(#[from] Rf2Error),
    /// A reference set member does not fit its typed view.
    #[error("a reference set member does not fit its view")]
    View(#[from] ViewError),
    /// A language reference set member refers to something other than a description.
    #[error("language reference set member {member} refers to {component}, not a description")]
    NotADescription {
        /// The member.
        member: String,
        /// The referenced component.
        component: String,
    },
    /// The module dependency reference set does not identify one edition.
    #[error("cannot identify the edition")]
    Edition(#[from] EditionError),
    /// A layered package names a module the edition does not carry.
    #[error("{package}: module {module} is not in the edition")]
    UnknownModule {
        /// The package.
        package: PathBuf,
        /// The module it depends on.
        module: String,
    },
    /// A layered package needs a module version the edition predates.
    #[error("{package}: module {module} is needed at {required}, the edition is {edition}")]
    UnmetDependency {
        /// The package.
        package: PathBuf,
        /// The module it depends on.
        module: String,
        /// The `targetEffectiveTime` the dependency asks for.
        required: String,
        /// The edition's release date.
        edition: String,
    },
    /// A relationship names a concept the release does not define.
    #[error("relationship {relationship} names unknown concept {concept}")]
    UnknownConcept {
        /// The relationship.
        relationship: String,
        /// The concept it names.
        concept: ConceptId,
    },
    /// The is-a graph has a cycle.
    #[error("the inferred is-a hierarchy is not acyclic")]
    Closure(#[from] ClosureError),
    /// An edge is out of range (cannot happen after numbering; kept as a typed error).
    #[error("cannot build the adjacency")]
    Csr(#[from] CsrError),
    /// The store cannot be written.
    #[error("cannot write the store")]
    Store(#[from] BuildError),
    /// The hierarchy cannot be serialized.
    #[error("cannot serialize the hierarchy")]
    Graph(#[from] concept_graph::persist::PersistError),
    /// The text index cannot be built or serialized.
    #[error("cannot build the designation index")]
    TextBuild(#[from] designation_index::index::BuildError),
    /// The text index cannot be serialized.
    #[error("cannot serialize the designation index")]
    TextPersist(#[from] designation_index::persist::PersistError),
    /// The reference set memberships cannot be serialized.
    #[error("cannot serialize the reference set memberships")]
    Members(#[from] MembersError),
    /// The attribute relationships cannot be built or serialized.
    #[error("cannot build the attribute relationships")]
    Attributes(#[from] AttributesError),
    /// The reference set member rows cannot be built or serialized.
    #[error("cannot build the reference set member rows")]
    Refsets(#[from] RefsetsError),
    /// The alternate identifiers cannot be serialized.
    #[error("cannot serialize the alternate identifiers")]
    Identifiers(#[from] IdentifiersError),
    /// The output directory or manifest cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// More components than a `u32` ordinal addresses.
    #[error("the release has more {0} than the artifact can number")]
    TooMany(&'static str),
}

/// What a build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The SNOMED edition URI (`http://snomed.info/sct/{module}`).
    pub edition_uri: String,
    /// The edition version URI.
    pub version_uri: String,
    /// The store file.
    pub store: PathBuf,
    /// The hierarchy file.
    pub hierarchy: PathBuf,
    /// The designation index file.
    pub text: PathBuf,
    /// The manifest file.
    pub manifest: PathBuf,
    /// Concepts written, active and inactive.
    pub concepts: u64,
    /// Designations written.
    pub designations: u64,
    /// Active inferred is-a edges.
    pub is_a_edges: u64,
    /// Reference sets with at least one active concept member.
    pub refsets: u64,
    /// Active attribute relationships (every inferred relationship that is not is-a).
    pub attributes: u64,
    /// Relationship rows left out, those outside the inferred view.
    pub skipped_relationships: u64,
    /// Active reference set member rows kept with their fields.
    pub member_rows: u64,
    /// Alternate identifiers.
    pub identifiers: u64,
    /// Distinct words in the designation index.
    pub words: u64,
    /// The designation languages present (RF2 `languageCode`), sorted.
    pub languages: Vec<String>,
}

fn ordinal_of(count: usize, what: &'static str) -> Result<u32, Error> {
    // A count past u32::MAX is the whole message; the conversion error adds nothing.
    let Ok(count) = u32::try_from(count) else {
        return Err(Error::TooMany(what));
    };
    Ok(count)
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Builds the artifacts for the Snapshot under `rf2`, with every
/// refset-only package under `refsets` layered onto it, into `out`.
///
/// # Errors
///
/// Returns [`Error`] when a release does not read, the edition cannot be
/// identified, a layered package's module dependency is unmet, the hierarchy
/// has a cycle, or an artifact cannot be written.
pub fn build(rf2: &Path, refsets: &[PathBuf], out: &Path) -> Result<Report, Error> {
    let release = Release::open(rf2, ReleaseType::Snapshot)?;
    let edition = identify_edition(&release)?;
    let release_date = release.date();
    let (releases, layered) = open_layers(release, &edition, refsets)?;
    std::fs::create_dir_all(out).map_err(io_error(out))?;

    let loaded = read_release(&releases)?;
    let attribute_graph = loaded
        .relationships
        .graph(ordinal_of(loaded.concepts.len(), "concepts")?)?;
    let version_uri = edition.version_uri();
    let written = write_artifacts(&loaded, &attribute_graph, out, &version_uri)?;

    let manifest_path = out.join(MANIFEST_FILE);
    let mut manifest = json!({
        "manifest": MANIFEST_VERSION,
        "system": "http://snomed.info/sct",
        "edition": edition.edition_uri(),
        "version": version_uri,
        "releaseDate": release_date.to_string(),
        "store": STORE_FILE,
        "storeLayout": tables::LAYOUT_VERSION,
        "hierarchy": HIERARCHY_FILE,
        "text": TEXT_FILE,
        "refsets": REFSETS_FILE,
        "attributes": ATTRIBUTES_FILE,
        "members": MEMBERS_FILE,
        "identifiers": IDENTIFIERS_FILE,
        "concepts": loaded.concepts.len(),
        "designations": written.designations,
        "isAEdges": written.is_a_edges,
        "referenceSets": loaded.memberships.len(),
        "memberships": loaded.memberships.total(),
        "attributeRows": attribute_graph.edges(),
        "memberRows": loaded.member_tables.total(),
        "alternateIdentifiers": loaded.identifiers.len(),
        "words": written.words,
        "languages": written.languages,
    });
    if let (Some(object), false) = (manifest.as_object_mut(), layered.is_empty()) {
        object.insert(String::from("layered"), layers_manifest(&layered));
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|source| Error::Io {
        path: manifest_path.clone(),
        source: io::Error::other(source),
    })?;
    std::fs::write(&manifest_path, format!("{text}\n")).map_err(io_error(&manifest_path))?;

    Ok(Report {
        edition_uri: edition.edition_uri(),
        version_uri,
        store: out.join(STORE_FILE),
        hierarchy: out.join(HIERARCHY_FILE),
        text: out.join(TEXT_FILE),
        manifest: manifest_path,
        concepts: u64::try_from(loaded.concepts.len()).unwrap_or(u64::MAX),
        designations: written.designations,
        is_a_edges: written.is_a_edges,
        refsets: u64::try_from(loaded.memberships.len()).unwrap_or(u64::MAX),
        attributes: u64::try_from(attribute_graph.edges()).unwrap_or(u64::MAX),
        skipped_relationships: loaded.relationships.skipped,
        member_rows: loaded.member_tables.total(),
        identifiers: u64::try_from(loaded.identifiers.len()).unwrap_or(u64::MAX),
        words: written.words,
        languages: written.languages,
    })
}

/// Everything the release yields, numbered and joined.
struct Loaded {
    concepts: Vec<Concept>,
    ordinals: BTreeMap<ConceptId, Ordinal>,
    relationships: Relationships,
    designations: Vec<Placed>,
    refsets: Vec<RefsetId>,
    acceptabilities: Acceptabilities,
    memberships: Memberships,
    member_tables: RefsetMembers,
    identifiers: Identifiers,
}

/// What the artifact writers counted.
struct Written {
    is_a_edges: u64,
    designations: u64,
    words: u64,
    languages: Vec<String>,
}

/// Reads every release into its numbered rows.
///
/// The concepts are numbered first because everything else is keyed by the
/// ordinal that numbering gives. The four reads that follow cover disjoint
/// parts of the release, so they run together; each collects its files in
/// path order, so what it returns does not depend on which worker won.
fn read_release(releases: &[Release]) -> Result<Loaded, Error> {
    let concepts = read_concepts(releases)?;
    let ordinals: BTreeMap<ConceptId, Ordinal> = concepts
        .iter()
        .enumerate()
        .map(|(i, concept)| Ok((concept.id, Ordinal::new(ordinal_of(i, "concepts")?))))
        .collect::<Result<_, Error>>()?;
    let (relationships, (designations, (refset_pass, identifiers))) = rayon::join(
        || read_relationships(releases, &ordinals),
        || {
            rayon::join(
                || read_designations(releases, &ordinals),
                || {
                    rayon::join(
                        || read_refsets(releases, &ordinals),
                        || read_identifiers(releases, &ordinals),
                    )
                },
            )
        },
    );
    let designations = designations?;
    let refset_pass = refset_pass?;
    let (refsets, acceptabilities) = place_acceptabilities(refset_pass.language, &designations)?;
    Ok(Loaded {
        concepts,
        ordinals,
        relationships: relationships?,
        designations,
        refsets,
        acceptabilities,
        memberships: refset_pass.memberships,
        member_tables: refset_pass.member_tables,
        identifiers: identifiers?,
    })
}

/// Writes the store, the designation index, and the graph files into `out`.
///
/// The three are outputs over the same rows, each written from its own inputs
/// alone, so they are built together and every file's bytes stay a function of
/// those inputs.
fn write_artifacts(
    loaded: &Loaded,
    attribute_graph: &Attributes,
    out: &Path,
    version_uri: &str,
) -> Result<Written, Error> {
    let store_path = out.join(STORE_FILE);
    let hierarchy_path = out.join(HIERARCHY_FILE);
    let text_path = out.join(TEXT_FILE);
    let refsets_path = out.join(REFSETS_FILE);
    let (store, (text, graph)) = rayon::join(
        || -> Result<(u64, u64), Error> {
            let mut builder =
                StoreBuilder::create(&store_path, "http://snomed.info/sct", version_uri)?;
            write_vocabularies(
                &mut builder,
                &loaded.refsets,
                &loaded.relationships.attribute_types,
            )?;
            let is_a_edges = write_concepts(
                &mut builder,
                &loaded.concepts,
                &loaded.ordinals,
                &loaded.relationships,
            )?;
            let designations = write_designations(&mut builder, &loaded.designations)?;
            for ((ordinal, index), memberships) in &loaded.acceptabilities {
                for (refset, acceptability) in memberships {
                    builder.acceptability(*ordinal, *index, *refset, *acceptability)?;
                }
            }
            // The display of a SNOMED concept is the preferred synonym of a
            // language reference set (`.claude/rules/snomed-terminology.md`,
            // [S-RF2-5]), so only that use goes into the display column. The
            // position comes from the same array the vocabulary was written
            // from, so the two cannot drift apart.
            let synonym = DESIGNATION_USES
                .iter()
                .position(|use_id| *use_id == constants::SYNONYM)
                .map(|p| ordinal_of(p, "designation uses"))
                .transpose()?;
            builder.finish(&PreferredRule {
                preferred: 0,
                display_use: synonym,
            })?;
            Ok((is_a_edges, designations))
        },
        || {
            rayon::join(
                || -> Result<u64, Error> {
                    let (bytes, words) = build_text(&loaded.designations, &loaded.acceptabilities)?;
                    std::fs::write(&text_path, &bytes).map_err(io_error(&text_path))?;
                    Ok(words)
                },
                || -> Result<(), Error> {
                    let hierarchy = build_hierarchy(&loaded.concepts, &loaded.relationships.is_a)?;
                    let mut bytes = Vec::new();
                    hierarchy.write_to(&mut bytes)?;
                    std::fs::write(&hierarchy_path, &bytes).map_err(io_error(&hierarchy_path))?;
                    bytes.clear();
                    loaded.memberships.write_to(&mut bytes)?;
                    std::fs::write(&refsets_path, &bytes).map_err(io_error(&refsets_path))?;
                    write_ecl_files(
                        out,
                        attribute_graph,
                        &loaded.member_tables,
                        &loaded.identifiers,
                    )
                },
            )
        },
    );
    let (is_a_edges, designations) = store?;
    let words = text?;
    graph?;
    let mut languages: Vec<String> = loaded
        .designations
        .iter()
        .map(|placed| placed.record.language.clone())
        .collect();
    languages.sort();
    languages.dedup();
    Ok(Written {
        is_a_edges,
        designations,
        words,
        languages,
    })
}

fn identify_edition(release: &Release) -> Result<Edition, Error> {
    let members = module_dependencies(release)?;
    Ok(Edition::identify(&members, release.date())?)
}

/// The module dependency reference set members of one release.
///
/// The file is found by the columns its header names and the rows by their
/// `refsetId`. A file name's summary is free-form and a derivative package
/// writes its own name into it, so the columns decide (`rf2::refset::kind`).
fn module_dependencies(release: &Release) -> Result<Vec<ModuleDependencyMember>, Error> {
    let file = refset_of(release, RefsetKind::ModuleDependency)?
        .ok_or(Error::MissingFile("module dependency reference set"))?;
    let ContentType::Refset(kinds) = &file.name.content_type else {
        return Err(Error::MissingFile("module dependency reference set"));
    };
    let mut members = Vec::new();
    for member in Members::open(&file.path, kinds)? {
        let member = member?;
        if member.refset_id != constants::MODULE_DEPENDENCY_REFSET {
            continue;
        }
        members.push(ModuleDependencyMember::try_from(member)?);
    }
    Ok(members)
}

/// Opens every layered package, checks it against `edition`, and returns the
/// releases to read (the edition first) with what each package layers.
fn open_layers(
    edition_release: Release,
    edition: &Edition,
    refsets: &[PathBuf],
) -> Result<(Vec<Release>, Vec<Layered>), Error> {
    let release_date = edition_release.date();
    let mut releases = Vec::with_capacity(refsets.len().saturating_add(1));
    releases.push(edition_release);
    let mut layered = Vec::with_capacity(refsets.len());
    for path in refsets {
        let package = Release::open(path, ReleaseType::Snapshot)?;
        layered.push(layer(path, &package, edition, release_date)?);
        releases.push(package);
    }
    layered.sort();
    Ok((releases, layered))
}

/// The manifest's `layered` array: one object per package, module then version.
fn layers_manifest(layered: &[Layered]) -> serde_json::Value {
    let packages: Vec<serde_json::Value> = layered
        .iter()
        .map(|package| {
            json!({
                "module": package.module.to_string(),
                "version": package.version.compact(),
            })
        })
        .collect();
    json!(packages)
}

/// The module and version one layered package contributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Layered {
    module: ModuleId,
    version: EffectiveTime,
}

/// Checks a layered package against `edition` and reports what it layers.
///
/// Every active module dependency row states the version of a module the
/// package needs. SNOMED CT assembles an edition from its modules at
/// compatible versions
/// (<https://docs.snomed.org/snomed-ct-practical-guides/snomed-ct-extension-guide/4-logical-design/4.4-editions>),
/// so a package whose dependency the edition does not meet is refused rather
/// than layered into a partial edition. The specifications give no
/// consumer-side rule for an edition NEWER than a dependency asks for;
/// counting it as met is our own design.
///
/// # Errors
///
/// Returns [`Error::UnknownModule`] when a row names a module outside the
/// edition, and [`Error::UnmetDependency`] when the edition predates the
/// version a row asks for.
fn layer(
    path: &Path,
    package: &Release,
    edition: &Edition,
    release_date: EffectiveTime,
) -> Result<Layered, Error> {
    let members = module_dependencies(package)?;
    for member in members.iter().filter(|m| m.member.active) {
        // The error names the offending target; the id error adds nothing.
        let Ok(module) =
            ConceptId::try_from(member.member.referenced_component_id).map(ModuleId::from)
        else {
            return Err(Error::UnknownModule {
                package: path.to_path_buf(),
                module: member.member.referenced_component_id.to_string(),
            });
        };
        if !edition.modules.contains_key(&module) {
            return Err(Error::UnknownModule {
                package: path.to_path_buf(),
                module: module.to_string(),
            });
        }
        // NOTE: an edition at or after the required version meets the
        // dependency; the ICNP package asks for 20260101 while the
        // International Edition in use is 20260901.
        if release_date < member.target_effective_time {
            return Err(Error::UnmetDependency {
                package: path.to_path_buf(),
                module: module.to_string(),
                required: member.target_effective_time.compact(),
                edition: release_date.compact(),
            });
        }
    }
    let identified = Edition::identify(&members, package.date())?;
    Ok(Layered {
        module: identified.module,
        version: identified.effective_time,
    })
}

/// Every concept row of every release, sorted by identifier so ordinals are
/// stable; a concept a layered package restates keeps the edition's row,
/// which the stable sort leaves first.
fn read_concepts(releases: &[Release]) -> Result<Vec<Concept>, Error> {
    let paths: Vec<&Path> = releases
        .iter()
        .map(|release| {
            release
                .of_type(&ContentType::Concept)
                .next()
                .map(|file| file.path.as_path())
                .ok_or(Error::MissingFile("concept"))
        })
        .collect::<Result<_, Error>>()?;
    let mut concepts = concat(read_files(&paths, component_rows::<Concept>)?);
    // The sort is stable and the release order is fixed, so a concept a
    // layered package restates keeps the edition's row, which stays first.
    concepts.par_sort_by_key(|concept| concept.id);
    concepts.dedup_by_key(|concept| concept.id);
    Ok(concepts)
}

/// Reads every path in parallel and returns what each one yielded, in the
/// order the paths were given.
fn read_files<T: Send>(
    paths: &[&Path],
    read: impl Fn(&Path) -> Result<T, Error> + Sync,
) -> Result<Vec<T>, Error> {
    paths.par_iter().map(|path| read(path)).collect()
}

/// Every row of one component file.
fn component_rows<T: rf2::component::Component>(path: &Path) -> Result<Vec<T>, Error> {
    let mut out = Vec::new();
    for row in Rows::<_, T>::open(path)? {
        out.push(row?);
    }
    Ok(out)
}

/// The parts of a per-file read, joined in file order.
fn concat<T>(parts: Vec<Vec<T>>) -> Vec<T> {
    let mut out = Vec::with_capacity(parts.iter().map(Vec::len).sum());
    for part in parts {
        out.extend(part);
    }
    out
}

/// The active inferred relationships: is-a edges as (child, parent) ordinals,
/// sorted, and every other attribute as a property value per (source, type).
#[derive(Default)]
struct Relationships {
    is_a: Vec<(Ordinal, Ordinal)>,
    /// Attribute type SCTIDs, sorted; the position is the key ordinal offset.
    attribute_types: Vec<ConceptId>,
    /// Per source ordinal, per attribute type, the values in file order.
    attributes: BTreeMap<(Ordinal, ConceptId), Vec<record::PropertyValue>>,
    /// Every attribute row with its role group, for the graph.
    edges: Vec<(Ordinal, u32, ConceptId, attributes::Value)>,
    /// Rows the inferred view left out, whatever their characteristic type.
    skipped: u64,
}

impl Relationships {
    /// The attribute graph: the rows keyed by the sorted type list.
    fn graph(&self, nodes: u32) -> Result<Attributes, Error> {
        let types: Vec<u64> = self.attribute_types.iter().map(|t| t.value()).collect();
        let mut edges = Vec::with_capacity(self.edges.len());
        for (source, group, type_id, value) in &self.edges {
            let Ok(kind) = self.attribute_types.binary_search(type_id) else {
                return Err(Error::TooMany("attribute types"));
            };
            edges.push(attributes::Edge {
                source: *source,
                group: *group,
                kind: ordinal_of(kind, "attribute types")?,
                value: value.clone(),
            });
        }
        Ok(Attributes::build(nodes, types, edges)?)
    }
}

/// The ordinal of `concept`, named by the relationship that references it.
fn ordinal_of_concept(
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    relationship: &str,
    concept: ConceptId,
) -> Result<Ordinal, Error> {
    ordinals
        .get(&concept)
        .copied()
        .ok_or_else(|| Error::UnknownConcept {
            relationship: relationship.to_owned(),
            concept,
        })
}

/// Reads one RF2 relationship file: an active inferred is-a row becomes a
/// hierarchy edge, every other one an attribute value on its source.
fn read_relationship_file(
    path: &Path,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    out: &mut Relationships,
) -> Result<(), Error> {
    let mut rows = Rows::<_, Relationship>::open(path)?.inferred();
    for relationship in rows.by_ref() {
        let relationship = relationship?;
        if !relationship.base.active {
            continue;
        }
        let id = relationship.id.to_string();
        let source = ordinal_of_concept(ordinals, &id, relationship.source_id)?;
        let destination = ordinal_of_concept(ordinals, &id, relationship.destination_id)?;
        if relationship.type_id == constants::IS_A {
            out.is_a.push((source, destination));
        } else {
            // NOTE: the FHIR SNOMED CT page says a concept model attribute
            // relationship becomes a property and is silent on
            // characteristicTypeId, so the inferred view is our own design.
            out.attributes
                .entry((source, relationship.type_id))
                .or_default()
                .push(record::PropertyValue::Concept(destination));
            out.edges.push((
                source,
                relationship.relationship_group,
                relationship.type_id,
                attributes::Value::Concept(destination),
            ));
        }
    }
    out.skipped = out.skipped.saturating_add(rows.skipped());
    Ok(())
}

/// Reads one RF2 concrete-value relationship file: every active inferred row
/// is an attribute whose value is a number or a string.
fn read_concrete_relationship_file(
    path: &Path,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    out: &mut Relationships,
) -> Result<(), Error> {
    let mut rows = Rows::<_, ConcreteRelationship>::open(path)?.inferred();
    for relationship in rows.by_ref() {
        let relationship = relationship?;
        if !relationship.base.active {
            continue;
        }
        let id = relationship.id.to_string();
        let source = ordinal_of_concept(ordinals, &id, relationship.source_id)?;
        // NOTE: RF2 spells a numeric concrete value with a `#` prefix and a
        // string one in quotes; the reader strips both and keeps the kind.
        let (value, edge) = match relationship.value {
            ConcreteValue::Number(number) => (
                record::PropertyValue::Decimal(number.clone()),
                attributes::Value::Number(number),
            ),
            ConcreteValue::String(text) => (
                record::PropertyValue::String(text.clone()),
                attributes::Value::String(text),
            ),
        };
        out.attributes
            .entry((source, relationship.type_id))
            .or_default()
            .push(value);
        out.edges.push((
            source,
            relationship.relationship_group,
            relationship.type_id,
            edge,
        ));
    }
    out.skipped = out.skipped.saturating_add(rows.skipped());
    Ok(())
}

fn read_relationships(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Relationships, Error> {
    let mut files: Vec<(&Path, bool)> = Vec::new();
    for release in releases {
        files.extend(
            release
                .of_type(&ContentType::Relationship)
                .map(|file| (file.path.as_path(), false)),
        );
        files.extend(
            release
                .of_type(&ContentType::RelationshipConcreteValues)
                .map(|file| (file.path.as_path(), true)),
        );
    }
    let parts: Vec<Relationships> = files
        .par_iter()
        .map(|(path, concrete)| {
            let mut part = Relationships::default();
            if *concrete {
                read_concrete_relationship_file(path, ordinals, &mut part)?;
            } else {
                read_relationship_file(path, ordinals, &mut part)?;
            }
            Ok(part)
        })
        .collect::<Result<_, Error>>()?;
    let mut out = Relationships::default();
    for part in parts {
        out.is_a.extend(part.is_a);
        for (key, values) in part.attributes {
            out.attributes.entry(key).or_default().extend(values);
        }
        out.edges.extend(part.edges);
        out.skipped = out.skipped.saturating_add(part.skipped);
    }
    out.is_a.par_sort_unstable();
    out.is_a.dedup();
    let mut attribute_types: Vec<ConceptId> = out.attributes.keys().map(|(_, t)| *t).collect();
    attribute_types.sort_unstable();
    attribute_types.dedup();
    out.attribute_types = attribute_types;
    Ok(out)
}

/// One designation, placed under its concept.
#[derive(Debug)]
struct Placed {
    id: DescriptionId,
    ordinal: Ordinal,
    index: u32,
    record: record::Designation,
}

/// Every description and text definition of every release, numbered per
/// concept in identifier order.
fn read_designations(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Vec<Placed>, Error> {
    let described = [ContentType::Description, ContentType::TextDefinition];
    let mut paths: Vec<&Path> = Vec::new();
    for release in releases {
        for content in &described {
            paths.extend(release.of_type(content).map(|file| file.path.as_path()));
        }
    }
    let mut rows: Vec<Description> = concat(read_files(&paths, |path| {
        component_rows::<Description>(path)
    })?);
    rows.par_sort_by_key(|row| row.id);
    let mut per_concept: BTreeMap<Ordinal, u32> = BTreeMap::new();
    let mut placed = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(ordinal) = ordinals.get(&row.concept_id).copied() else {
            // NOTE: a description of a concept outside the release is not an
            // error in RF2 (the concept lives in a dependency the release omits);
            // it has no home in this store and is skipped.
            continue;
        };
        let index = per_concept.entry(ordinal).or_insert(0);
        let use_ordinal = DESIGNATION_USES
            .iter()
            .position(|use_id| *use_id == row.type_id)
            .map(|p| ordinal_of(p, "designation uses"))
            .transpose()?
            .unwrap_or(1);
        placed.push(Placed {
            id: row.id,
            ordinal,
            index: *index,
            record: record::Designation {
                id: Some(row.id.to_string()),
                term: row.term,
                language: row.language_code,
                use_ordinal,
                active: row.base.active,
            },
        });
        *index = index.checked_add(1).ok_or(Error::TooMany("designations"))?;
    }
    Ok(placed)
}

/// The first reference set file of `release` of `wanted`, `None` when it has
/// none.
///
/// # Errors
///
/// Returns [`Error`] when a reference set file cannot be read.
fn refset_of(release: &Release, wanted: RefsetKind) -> Result<Option<&ReleaseFile>, Error> {
    for file in release.refsets() {
        if rf2::refset::kind(&file.path)? == wanted {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

/// The kind of one reference set file, from the columns its header names.
///
/// # Errors
///
/// Returns [`Error`] when the file cannot be read.
fn refset_kind(file: &ReleaseFile) -> Result<RefsetKind, Error> {
    Ok(rf2::refset::kind(&file.path)?)
}

type Acceptabilities = BTreeMap<(Ordinal, u32), Vec<(u32, u32)>>;

/// The language reference sets (by ordinal) and, per designation, its
/// (refset ordinal, acceptability ordinal) memberships.
fn place_acceptabilities(
    mut members: Vec<(RefsetId, DescriptionId, u32)>,
    designations: &[Placed],
) -> Result<(Vec<RefsetId>, Acceptabilities), Error> {
    let by_id: BTreeMap<DescriptionId, (Ordinal, u32)> = designations
        .iter()
        .map(|d| (d.id, (d.ordinal, d.index)))
        .collect();
    members.par_sort_unstable();
    let mut refsets: Vec<RefsetId> = members.iter().map(|m| m.0).collect();
    refsets.dedup();
    let mut acceptabilities: Acceptabilities = BTreeMap::new();
    for (refset, description, acceptability) in members {
        let Some(place) = by_id.get(&description).copied() else {
            continue;
        };
        let Ok(position) = refsets.binary_search(&refset) else {
            return Err(Error::TooMany("language reference sets"));
        };
        let refset_ordinal = ordinal_of(position, "language reference sets")?;
        acceptabilities
            .entry(place)
            .or_default()
            .push((refset_ordinal, acceptability));
    }
    Ok((refsets, acceptabilities))
}

/// The fields and rows of one reference set before they become a table.
type PendingTable = (Vec<(String, refsets::FieldKind)>, Vec<MemberRow>);

/// What one pass over the reference set files of a release yields.
struct RefsetPass {
    /// The active concept members of every reference set that references
    /// concepts: the simple, association, attribute value, and map reference
    /// sets, whatever their content.
    memberships: Memberships,
    /// The active members of every content reference set, with their fields;
    /// the OWL axiom reference sets carry text no ECL filter reads.
    member_tables: RefsetMembers,
    /// The active language reference set members, in file order.
    language: Vec<(RefsetId, DescriptionId, u32)>,
}

/// What one reference set file yields, before the files are joined.
#[derive(Default)]
struct RefsetFile {
    memberships: Vec<(u64, Ordinal)>,
    tables: BTreeMap<u64, PendingTable>,
    language: Vec<(RefsetId, DescriptionId, u32)>,
}

/// Reads every reference set file of every release, once.
///
/// The concept memberships, the member tables of the content reference sets,
/// and the language reference set members come from the same rows, so one
/// pass yields all three. The files are read together and joined in path
/// order, so a row's place in its table is the file's, never a worker's.
fn read_refsets(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<RefsetPass, Error> {
    let files: Vec<&ReleaseFile> = releases.iter().flat_map(Release::refsets).collect();
    let parts: Vec<RefsetFile> = files
        .par_iter()
        .map(|file| read_refset_file(file, ordinals))
        .collect::<Result<_, Error>>()?;
    let mut memberships = Memberships::new();
    let mut tables: BTreeMap<u64, PendingTable> = BTreeMap::new();
    let mut language = Vec::new();
    for part in parts {
        for (refset, ordinal) in part.memberships {
            memberships.insert(refset, ordinal);
        }
        for (refset, (fields, rows)) in part.tables {
            tables
                .entry(refset)
                .or_insert_with(|| (fields, Vec::new()))
                .1
                .extend(rows);
        }
        language.extend(part.language);
    }
    let mut member_tables = RefsetMembers::new();
    for (refset, (fields, rows)) in tables {
        member_tables.insert(refset, &fields, rows)?;
    }
    Ok(RefsetPass {
        memberships,
        member_tables,
        language,
    })
}

/// Reads one reference set file: its language members when it is a language
/// reference set, its concept memberships otherwise, and its member rows with
/// their fields when it is a content reference set.
fn read_refset_file(
    file: &ReleaseFile,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<RefsetFile, Error> {
    let kind = refset_kind(file)?;
    let mut out = RefsetFile::default();
    let ContentType::Refset(kinds) = &file.name.content_type else {
        return Ok(out);
    };
    for member in Members::open(&file.path, kinds)? {
        let member = member?;
        if kind == RefsetKind::Language {
            read_language_member(member, &mut out.language)?;
            continue;
        }
        if !member.active {
            continue;
        }
        // NOTE: every reference set of the edition is served, the metadata ones
        // included (<https://hl7.org/fhir/R4B/snomedct.html>, #272).
        // NOTE: a member referencing a description or a relationship is not a
        // concept membership; that is "absent" here, not a defect of the file.
        let Ok(concept) = ConceptId::try_from(member.referenced_component_id) else {
            continue;
        };
        let Some(&ordinal) = ordinals.get(&concept) else {
            continue;
        };
        let refset = member.refset_id.concept().value();
        out.memberships.push((refset, ordinal));
        if kind != RefsetKind::Content {
            continue;
        }
        let entry = out
            .tables
            .entry(refset)
            .or_insert_with(|| (field_columns(&member.fields, kinds), Vec::new()));
        entry.1.push(MemberRow {
            concept: ordinal,
            effective_time: compact_time(member.effective_time),
            module: member.module_id.concept().value(),
            values: field_values(member.fields, ordinals),
        });
    }
    Ok(out)
}

/// Reads one language reference set member: the reference set, the
/// description it places, and the acceptability ordinal.
fn read_language_member(
    member: rf2::refset::Member,
    out: &mut Vec<(RefsetId, DescriptionId, u32)>,
) -> Result<(), Error> {
    let member = LanguageMember::try_from(member)?;
    if !member.member.active {
        return Ok(());
    }
    // The error names the member and component; the id error adds nothing.
    let Ok(description) = DescriptionId::try_from(member.member.referenced_component_id) else {
        return Err(Error::NotADescription {
            member: member.member.id.to_string(),
            component: member.member.referenced_component_id.to_string(),
        });
    };
    let acceptability = ACCEPTABILITIES
        .iter()
        .position(|a| *a == member.acceptability_id)
        .map(|p| ordinal_of(p, "acceptabilities"))
        .transpose()?
        .unwrap_or(1);
    out.push((member.member.refset_id, description, acceptability));
    Ok(())
}

/// The `YYYYMMDD` of an effective time as a number.
fn compact_time(time: EffectiveTime) -> u32 {
    time.compact().parse().unwrap_or_default()
}

/// The stored columns of a reference set: the member's field names under the
/// kinds the file header declares.
fn field_columns(
    fields: &[(String, FieldValue)],
    kinds: &[FieldKind],
) -> Vec<(String, refsets::FieldKind)> {
    fields
        .iter()
        .zip(kinds)
        .map(|((name, _), kind)| {
            (
                name.clone(),
                match kind {
                    FieldKind::Component => refsets::FieldKind::Component,
                    FieldKind::Integer => refsets::FieldKind::Integer,
                    FieldKind::String => refsets::FieldKind::String,
                },
            )
        })
        .collect()
}

/// The stored values of one member's fields; a component field that names a
/// loaded concept is stored as its ordinal.
fn field_values(
    fields: Vec<(String, FieldValue)>,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Vec<refsets::FieldValue> {
    fields
        .into_iter()
        .map(|(_, value)| match value {
            FieldValue::Component(id) => ConceptId::try_from(id)
                .ok()
                .and_then(|c| ordinals.get(&c))
                .map_or(refsets::FieldValue::Component(id.value()), |o| {
                    refsets::FieldValue::Concept(*o)
                }),
            FieldValue::Integer(value) => refsets::FieldValue::Integer(value),
            FieldValue::String(text) => refsets::FieldValue::String(text),
        })
        .collect()
}

/// The active alternate identifiers of every release's concepts.
fn read_identifiers(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Identifiers, Error> {
    let mut entries = Vec::new();
    for release in releases {
        for file in release.of_type(&ContentType::Identifier) {
            for identifier in Rows::<_, AlternateIdentifier>::open(&file.path)? {
                let identifier = identifier?;
                if !identifier.base.active {
                    continue;
                }
                let Ok(concept) = ConceptId::try_from(identifier.referenced_component_id) else {
                    continue;
                };
                if let Some(&ordinal) = ordinals.get(&concept) {
                    entries.push((
                        identifier.identifier_scheme_id.value(),
                        identifier.alternate_identifier,
                        ordinal,
                    ));
                }
            }
        }
    }
    Ok(Identifiers::new(entries))
}

fn write_vocabularies(
    builder: &mut StoreBuilder,
    refsets: &[RefsetId],
    attribute_types: &[ConceptId],
) -> Result<(), Error> {
    for (i, key) in PROPERTY_KEYS.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::PropertyKeys,
            ordinal_of(i, "property keys")?,
            key,
        )?;
    }
    for (i, attribute) in attribute_types.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::PropertyKeys,
            ordinal_of(PROPERTY_KEYS.len().saturating_add(i), "property keys")?,
            &attribute.to_string(),
        )?;
    }
    for (i, use_id) in DESIGNATION_USES.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::DesignationUses,
            ordinal_of(i, "designation uses")?,
            &use_id.to_string(),
        )?;
    }
    for (i, acceptability) in ACCEPTABILITIES.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::Acceptabilities,
            ordinal_of(i, "acceptabilities")?,
            &acceptability.to_string(),
        )?;
    }
    for (i, refset) in refsets.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::LanguageRefsets,
            ordinal_of(i, "language reference sets")?,
            &refset.to_string(),
        )?;
    }
    Ok(())
}

/// Writes every concept and its properties; returns the is-a edge count.
fn write_concepts(
    builder: &mut StoreBuilder,
    concepts: &[Concept],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    relationships: &Relationships,
) -> Result<u64, Error> {
    let mut parents: BTreeMap<Ordinal, Vec<record::PropertyValue>> = BTreeMap::new();
    for (child, parent) in &relationships.is_a {
        parents
            .entry(*child)
            .or_default()
            .push(record::PropertyValue::Concept(*parent));
    }
    for (i, concept) in concepts.iter().enumerate() {
        let ordinal = Ordinal::new(ordinal_of(i, "concepts")?);
        let module = ordinals.get(&concept.base.module_id.concept()).copied();
        builder.concept(
            ordinal,
            &record::Concept {
                code: concept.id.to_string(),
                active: concept.base.active,
                effective_time: Some(concept.base.effective_time.compact()),
                module,
            },
        )?;
        if let Some(values) = parents.get(&ordinal) {
            builder.properties(ordinal, 0, values)?;
        }
        builder.properties(
            ordinal,
            1,
            &[record::PropertyValue::Code(
                concept.definition_status_id.to_string(),
            )],
        )?;
        builder.properties(
            ordinal,
            2,
            &[record::PropertyValue::Code(
                concept.base.module_id.to_string(),
            )],
        )?;
        for ((_, attribute), values) in relationships
            .attributes
            .range((ordinal, ConceptId::published(0))..)
            .take_while(|((source, _), _)| *source == ordinal)
        {
            let Ok(position) = relationships.attribute_types.binary_search(attribute) else {
                return Err(Error::TooMany("attribute types"));
            };
            let key = ordinal_of(
                PROPERTY_KEYS.len().saturating_add(position),
                "property keys",
            )?;
            builder.properties(ordinal, key, values)?;
        }
    }
    Ok(u64::try_from(relationships.is_a.len()).unwrap_or(u64::MAX))
}

fn write_designations(builder: &mut StoreBuilder, designations: &[Placed]) -> Result<u64, Error> {
    for placed in designations {
        builder.designation(placed.ordinal, placed.index, &placed.record)?;
    }
    Ok(u64::try_from(designations.len()).unwrap_or(u64::MAX))
}

/// Writes the files the ECL evaluator reads beside the store.
fn write_ecl_files(
    out: &Path,
    attributes: &Attributes,
    members: &RefsetMembers,
    identifiers: &Identifiers,
) -> Result<(), Error> {
    let mut bytes = Vec::new();
    attributes.write_to(&mut bytes)?;
    let path = out.join(ATTRIBUTES_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    bytes.clear();
    members.write_to(&mut bytes)?;
    let path = out.join(MEMBERS_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    bytes.clear();
    identifiers.write_to(&mut bytes)?;
    let path = out.join(IDENTIFIERS_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    Ok(())
}

fn build_hierarchy(concepts: &[Concept], is_a: &[(Ordinal, Ordinal)]) -> Result<Hierarchy, Error> {
    let nodes = ordinal_of(concepts.len(), "concepts")?;
    let csr = Csr::build(nodes, is_a.iter().copied())?;
    let closure = Closure::compute(&csr)?;
    Ok(Hierarchy { is_a: csr, closure })
}

fn build_text(
    designations: &[Placed],
    acceptabilities: &Acceptabilities,
) -> Result<(Vec<u8>, u64), Error> {
    let mut index = IndexBuilder::new();
    for placed in designations {
        let refsets: Vec<u32> = acceptabilities
            .get(&(placed.ordinal, placed.index))
            .map(|m| m.iter().map(|(refset, _)| *refset).collect())
            .unwrap_or_default();
        index.add(&Input {
            concept: placed.ordinal,
            index: placed.index,
            term: &placed.record.term,
            language: &placed.record.language,
            use_ordinal: placed.record.use_ordinal,
            active: placed.record.active,
            refsets: &refsets,
        })?;
    }
    let index = index.build()?;
    let mut bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut bytes)?;
    Ok((bytes, u64::try_from(index.words()).unwrap_or(u64::MAX)))
}
