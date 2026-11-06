// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pipeline: read the Snapshot, number everything, write the artifacts.

mod read;
mod write;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use concept_graph::attributes::{self, Attributes, AttributesError};
use concept_graph::closure::ClosureError;
use concept_graph::csr::CsrError;
use concept_graph::identifiers::{Identifiers, IdentifiersError};
use concept_graph::members::{MembersError, Memberships};
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{self, MemberRow, RefsetMembers, RefsetsError};
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::record;
use concept_store::tables;
use read::{
    place_acceptabilities, read_concepts, read_designations, read_identifiers, read_refsets,
    read_relationships, refset_of,
};
use rf2::component::Concept;
use rf2::constants;
use rf2::edition::{Edition, EditionError};
use rf2::file::{ContentType, Release, ReleaseError, ReleaseType};
use rf2::id::{ConceptId, DescriptionId, ModuleId, RefsetId};
use rf2::reader::Rf2Error;
use rf2::refset::{Members, ModuleDependencyMember, ViewError, kind::RefsetKind};
use rf2::time::EffectiveTime;
use serde_json::json;
use write::{
    build_hierarchy, build_text, write_concepts, write_designations, write_ecl_files,
    write_vocabularies,
};

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
/// writes its own name into it, so the columns decide (`rf2::refset::kind::read`).
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

/// One designation, placed under its concept.
#[derive(Debug)]
struct Placed {
    id: DescriptionId,
    ordinal: Ordinal,
    index: u32,
    record: record::Designation,
}

type Acceptabilities = BTreeMap<(Ordinal, u32), Vec<(u32, u32)>>;

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
