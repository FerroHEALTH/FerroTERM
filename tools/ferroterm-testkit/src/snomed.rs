// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic SNOMED CT edition as an artifact directory, and a refset-only
//! RF2 package to layer onto one.
//!
//! Written with the store, graph, and text writers the way `ferroterm-build`
//! lays it out: invented concepts in an invented namespace with valid check
//! digits, and no SNOMED content.

mod shape;

use std::path::Path;

use concept_graph::ordinal::Ordinal;
use concept_store::builder::StoreBuilder;
use concept_store::record::{Concept, Designation, PropertyValue};
use designation_index::index::{IndexBuilder, Input};
use rf2::constants;
use rf2::id::with_check_digit;

use crate::edition::ord;
use shape::write_shape;

const NAMESPACE: &str = "1234567";
/// The edition URI of the synthetic edition.
pub const EDITION: &str = "http://snomed.info/sct/91234567105";
/// The edition version URI (the `version` the provider serves).
pub const VERSION: &str = "http://snomed.info/sct/91234567105/version/20260101";
/// The item number of the second edition's module.
const SECOND_MODULE: u32 = 90;
/// The release date of the second synthetic edition, as an RF2 `effectiveTime`.
pub const SECOND_DATE: &str = "20260201";
/// The release date of the later release of the first synthetic edition.
pub const LATER_DATE: &str = "20260301";
/// The GB English language reference set (a published SCTID, metadata only).
pub const GB_REFSET: &str = "900000000000508004";
/// The Dutch language reference set (a published SCTID, metadata only).
pub const NL_REFSET: &str = "31000146106";

/// An item number in the invented namespace, what [`sctid`] turns into a code.
///
/// [`item`] derives one from an ordinal, [`Item::raw`] names one directly (a
/// module or a reference set the fixture numbers by hand), so a bare ordinal
/// can never reach [`sctid`] by mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item(u32);

impl Item {
    /// The item with this number, as the fixture numbers it.
    #[must_use]
    pub const fn raw(number: u32) -> Self {
        Self(number)
    }
}

/// A concept identifier in the invented namespace.
#[must_use]
pub fn sctid(item: Item) -> String {
    with_check_digit(&format!("{}{NAMESPACE}10", item.0))
}

/// The concept identifier of the concept at `ordinal`: `sctid(item(ordinal))`.
#[must_use]
pub fn code(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// The root concept.
pub const TOP: u32 = 0;
/// The animal, under the root.
pub const ANIMAL: u32 = 1;
/// The cat, under the animal; defined; has fur and four legs.
pub const CAT: u32 = 2;
/// The dog, under the animal; defined.
pub const DOG: u32 = 3;
/// The inactive fish, unclassified.
pub const FISH: u32 = 4;
/// The fur, target of the covering attribute.
pub const FUR: u32 = 5;
/// The covering attribute type.
pub const COVERING: u32 = 6;
/// The leg-count attribute type (concrete values).
pub const LEGS: u32 = 7;
/// A reference set concept whose members are the cat and the dog.
pub const PETS: u32 = 8;
/// A map reference set (`mapGroup`, `mapTarget`) whose members are the cat and the dog.
pub const CODES_MAP: u32 = 9;
/// The historical association reference set root (a published SCTID, metadata only).
pub const HISTORICAL: u32 = 10;
/// The SAME AS association reference set (a published SCTID); the fish is SAME AS the cat.
pub const SAME_AS: u32 = 11;
/// An identifier scheme whose alias is `ZOO`; the cat is `ZOO#cat-1`, the dog `ZOO#dog-1`.
pub const SCHEME: u32 = 12;
/// The REPLACED BY association reference set (a published SCTID); the fish is REPLACED BY the dog.
pub const REPLACED_BY: u32 = 13;
/// The POSSIBLY EQUIVALENT TO association reference set (a published SCTID); the fish points at the dog.
pub const POSSIBLY_EQUIVALENT_TO: u32 = 14;
/// The ALTERNATIVE association reference set (a published SCTID); the fish points at the cat.
pub const ALTERNATIVE: u32 = 15;
/// The Module Dependency reference set (a published SCTID); its member is the
/// edition module, the way a real edition states what it was built on.
pub const MODULE_DEPENDENCY: u32 = 16;
/// The edition module, a concept and the member of the Module Dependency set.
pub const MODULE_CONCEPT: u32 = 17;
/// The ICD-10 extended map reference set (a published SCTID); the cat maps to
/// the ICD-10 code `C01`.
pub const ICD10_MAP: u32 = 18;
/// The GB English language reference set as a concept of the edition.
pub const GB_LANGUAGE_REFSET: u32 = 19;
/// The Dutch language reference set as a concept of the edition.
pub const NL_LANGUAGE_REFSET: u32 = 20;
/// The bird, under the animal and a member of the pets reference set; only the
/// second edition ([`write_second`]) holds it.
pub const BIRD: u32 = 21;
/// The published SCTID of the ICD-10 extended map reference set.
pub const ICD10_MAP_SCTID: &str = "447562003";
/// The code system the ICD-10 extended map reference set maps to.
pub const ICD10_SYSTEM: &str = "http://hl7.org/fhir/sid/icd-10";
/// The published SCTID of the Module Dependency reference set.
pub const MODULE_DEPENDENCY_SCTID: &str = "900000000000534007";
/// The published SCTID of the historical association reference set root.
pub const HISTORICAL_SCTID: &str = "900000000000522004";
/// The published SCTID of the SAME AS association reference set.
pub const SAME_AS_SCTID: &str = "900000000000527005";
/// The published SCTID of the REPLACED BY association reference set.
pub const REPLACED_BY_SCTID: &str = "900000000000526001";
/// The published SCTID of the POSSIBLY EQUIVALENT TO association reference set.
pub const POSSIBLY_EQUIVALENT_TO_SCTID: &str = "900000000000523009";
/// The published SCTID of the ALTERNATIVE association reference set.
pub const ALTERNATIVE_SCTID: &str = "900000000000530003";

/// The item behind each ordinal, so `sctid(item(CAT))` is the cat's code.
#[must_use]
pub const fn item(ordinal: u32) -> Item {
    Item(ordinal + 1)
}

/// (refset ordinal, acceptability ordinal).
type LanguageMemberships = Vec<(u32, u32)>;
/// (term, language, use ordinal, active, memberships).
type DesignationRow = (&'static str, &'static str, u32, bool, LanguageMemberships);

struct Row {
    ordinal: u32,
    /// A published code instead of the invented one.
    code: Option<&'static str>,
    active: bool,
    defined: bool,
    designations: Vec<DesignationRow>,
}

/// The code of `row`: the published one it names, else the invented SCTID.
fn code_of(row: &Row) -> String {
    row.code
        .map_or_else(|| sctid(item(row.ordinal)), str::to_owned)
}

/// Writes every concept of `rows` with its parents, definition status, module,
/// and designations, into the store and the text index.
fn write_concepts(
    builder: &mut StoreBuilder,
    text: &mut IndexBuilder,
    rows: &[Row],
    is_a: &[(u32, u32)],
    module: &str,
) -> Result<(), FixtureError> {
    for row in rows {
        let ordinal = Ordinal::new(row.ordinal);
        builder.concept(
            ordinal,
            &Concept {
                code: code_of(row),
                active: row.active,
                effective_time: Some(String::from("20260101")),
                module: None,
            },
        )?;
        let parents: Vec<PropertyValue> = is_a
            .iter()
            .filter(|(child, _)| *child == row.ordinal)
            .map(|(_, parent)| PropertyValue::Concept(Ordinal::new(*parent)))
            .collect();
        if !parents.is_empty() {
            builder.properties(ordinal, 0, &parents)?;
        }
        let status = if row.defined {
            constants::DEFINED
        } else {
            constants::PRIMITIVE
        };
        builder.properties(ordinal, 1, &[PropertyValue::Code(status.to_string())])?;
        builder.properties(ordinal, 2, &[PropertyValue::Code(module.to_owned())])?;
        for (index, (term, language, use_ordinal, active, memberships)) in
            row.designations.iter().enumerate()
        {
            let index = ord(index);
            builder.designation(
                ordinal,
                index,
                &Designation {
                    id: None,
                    term: (*term).to_owned(),
                    language: (*language).to_owned(),
                    use_ordinal: *use_ordinal,
                    active: *active,
                },
            )?;
            for (refset, acceptability) in memberships {
                builder.acceptability(ordinal, index, *refset, *acceptability)?;
            }
            let refsets: Vec<u32> = memberships.iter().map(|(r, _)| *r).collect();
            text.add(&Input {
                concept: ordinal,
                index,
                term,
                language,
                use_ordinal: *use_ordinal,
                active: *active,
                refsets: &refsets,
            })
            .map_err(|e| FixtureError::Text(e.to_string()))?;
        }
    }
    Ok(())
}

/// A failure to write the fixture.
#[derive(Debug)]
pub enum FixtureError {
    /// The store could not be written.
    Store(concept_store::builder::BuildError),
    /// The hierarchy could not be built or serialized.
    Graph(String),
    /// The text index could not be built or serialized.
    Text(String),
    /// The manifest could not be written.
    Io(std::io::Error),
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store: {e}"),
            Self::Graph(e) => write!(f, "graph: {e}"),
            Self::Text(e) => write!(f, "text: {e}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<concept_store::builder::BuildError> for FixtureError {
    fn from(e: concept_store::builder::BuildError) -> Self {
        Self::Store(e)
    }
}

impl From<std::io::Error> for FixtureError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// What tells one synthetic edition from another.
struct Shape<'a> {
    edition: &'a str,
    version: &'a str,
    release_date: &'a str,
    /// Whether the edition holds [`BIRD`].
    bird: bool,
}

/// The edition URI of the second synthetic edition ([`write_second`]).
#[must_use]
pub fn second_edition() -> String {
    format!("http://snomed.info/sct/{}", sctid(Item::raw(SECOND_MODULE)))
}

/// The edition version URI of the second synthetic edition.
#[must_use]
pub fn second_version() -> String {
    format!("{}/version/{SECOND_DATE}", second_edition())
}

/// Writes the edition under `dir` (`store.redb`, `hierarchy.bin`, `text.bin`, `refsets.bin`, `attributes.bin`, `members.bin`, `identifiers.bin`, and `manifest.json`).
///
/// # Errors
///
/// Returns [`FixtureError`] when a writer fails; the fixture is fixed content,
/// so a failure means the writers or the directory are at fault.
pub fn write(dir: &Path) -> Result<(), FixtureError> {
    write_shape(
        dir,
        &Shape {
            edition: EDITION,
            version: VERSION,
            release_date: DATE,
            bird: false,
        },
    )
}

/// The edition version URI of the later release of the first edition.
#[must_use]
pub fn later_version() -> String {
    format!("{EDITION}/version/{LATER_DATE}")
}

/// Writes a later release of the FIRST synthetic edition under `dir`, so a
/// test can load two versions of one edition at once.
///
/// It carries the same edition URI at a later release date, and the concept
/// the first release lacks ([`BIRD`]), so a test can tell which release
/// answered.
///
/// # Errors
///
/// Returns [`FixtureError`] when a writer fails; the fixture is fixed content,
/// so a failure means the writers or the directory are at fault.
pub fn write_later(dir: &Path) -> Result<(), FixtureError> {
    write_shape(
        dir,
        &Shape {
            edition: EDITION,
            version: &later_version(),
            release_date: LATER_DATE,
            bird: true,
        },
    )
}

/// Writes a second synthetic edition under `dir`, so a test can load two
/// editions of SNOMED CT at once.
///
/// It carries another module and release date, and one concept the first
/// edition lacks ([`BIRD`]), so the same implicit value set answers a
/// different membership from each edition.
///
/// # Errors
///
/// Returns [`FixtureError`] when a writer fails; the fixture is fixed content,
/// so a failure means the writers or the directory are at fault.
pub fn write_second(dir: &Path) -> Result<(), FixtureError> {
    write_shape(
        dir,
        &Shape {
            edition: &second_edition(),
            version: &second_version(),
            release_date: SECOND_DATE,
            bird: true,
        },
    )
}

/// The item number of the refset-only package's module concept.
pub const PACKAGE_MODULE: u32 = 200;
/// The item number of the refset-only package's reference set concept.
pub const PACKAGE_REFSET: u32 = 201;
/// The release date the fixture writes, as an RF2 `effectiveTime`.
pub const DATE: &str = "20260101";

/// Case insensitive (`900000000000448009`), the case significance every
/// fixture description carries.
const CASE_INSENSITIVE: &str = "900000000000448009";

/// A description identifier in the invented namespace.
fn description_id(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}11"))
}

/// A reference set member identifier (RF2 spells it as a UUID).
fn member_id(item: u32) -> String {
    format!("00000000-0000-4000-8000-{item:012}")
}

/// What a refset-only package depends on and holds.
#[derive(Debug, Clone, Copy)]
pub struct Package<'a> {
    /// The edition module the package's module dependency row names.
    pub depends_on: &'a str,
    /// The version that row asks of that module (`YYYYMMDD`).
    pub target: &'a str,
    /// The edition concepts the package's simple reference set holds.
    pub members: &'a [String],
}

/// Writes one RF2 file: a tab-separated header and rows, CRLF-terminated.
fn write_rf2(
    dir: &Path,
    relative: &str,
    header: &[&str],
    rows: &[Vec<String>],
) -> std::io::Result<()> {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = header.join("\t");
    text.push_str("\r\n");
    for row in rows {
        text.push_str(&row.join("\t"));
        text.push_str("\r\n");
    }
    std::fs::write(&path, text)
}

/// Writes a refset-only RF2 Snapshot package under `dir`, the shape SNOMED
/// International publishes for a package dependent on an edition.
///
/// The package carries its own module and one simple reference set, both as
/// concepts with a fully specified name and a synonym, the language reference
/// set rows those descriptions need, the reference set's members over the
/// edition's concepts, and the module dependency row stating what it needs of
/// the edition. Its file names carry the package's own name the way a
/// derivative package does, and in both positions a real package uses: before
/// the reference set type (`der2_Refset_ICNPSimpleSnapshot`) and after it
/// (`sct2_sRefset_OWLExpressionICNPFull`). One file names no reference set type
/// at all (`der2_cRefset_ZooTermsSnapshot-en` is a language reference set), so
/// a reader that matches the file summary reads this package wrong (#273).
///
/// # Errors
///
/// Returns [`FixtureError`] when a file cannot be written; the content is
/// fixed, so a failure means the directory is at fault.
pub fn write_refset_package(dir: &Path, package: &Package<'_>) -> Result<(), FixtureError> {
    write_package_terminology(dir)?;
    write_package_refsets(dir, package)?;
    Ok(())
}

/// The package's two concepts and their descriptions.
fn write_package_terminology(dir: &Path) -> Result<(), FixtureError> {
    let module = sctid(Item::raw(PACKAGE_MODULE));
    let refset = sctid(Item::raw(PACKAGE_REFSET));
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let fsn = constants::FULLY_SPECIFIED_NAME.to_string();
    let synonym = constants::SYNONYM.to_string();
    write_rf2(
        dir,
        &format!("Snapshot/Terminology/sct2_Concept_ZooSnapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "definitionStatusId",
        ],
        &[
            s(&[
                &module,
                DATE,
                "1",
                &module,
                &constants::PRIMITIVE.to_string(),
            ]),
            s(&[
                &refset,
                DATE,
                "1",
                &module,
                &constants::PRIMITIVE.to_string(),
            ]),
        ],
    )?;
    let describe = |item: u32, concept: &str, kind: &str, term: &str| {
        s(&[
            &description_id(item),
            DATE,
            "1",
            &module,
            concept,
            "en",
            kind,
            term,
            CASE_INSENSITIVE,
        ])
    };
    write_rf2(
        dir,
        &format!("Snapshot/Terminology/sct2_Description_ZooSnapshot-en_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "conceptId",
            "languageCode",
            "typeId",
            "term",
            "caseSignificanceId",
        ],
        // The four descriptions are items 200 to 203: the module's fully
        // specified name and synonym, then the reference set's.
        &[
            describe(
                200,
                &module,
                &fsn,
                "Zoo nursing module (core metadata concept)",
            ),
            describe(201, &module, &synonym, "Zoo nursing module"),
            describe(
                202,
                &refset,
                &fsn,
                "Zoo nursing reference set (foundation metadata concept)",
            ),
            describe(203, &refset, &synonym, "Zoo nursing reference set"),
        ],
    )?;
    Ok(())
}

/// The package's language reference set, simple reference set, and module
/// dependency rows.
fn write_package_refsets(dir: &Path, package: &Package<'_>) -> Result<(), FixtureError> {
    let module = sctid(Item::raw(PACKAGE_MODULE));
    let refset = sctid(Item::raw(PACKAGE_REFSET));
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let accept = |item: u32| {
        s(&[
            &member_id(item),
            DATE,
            "1",
            &module,
            GB_REFSET,
            &description_id(item),
            &constants::PREFERRED.to_string(),
        ])
    };
    write_rf2(
        dir,
        &format!(
            "Snapshot/Refset/Language/der2_cRefset_ZooTermsSnapshot-en_XX{NAMESPACE}_{DATE}.txt"
        ),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
            "acceptabilityId",
        ],
        &[accept(200), accept(201), accept(202), accept(203)],
    )?;
    let members: Vec<Vec<String>> = package
        .members
        .iter()
        .enumerate()
        .map(|(i, member)| {
            s(&[
                &member_id(300 + ord(i)),
                DATE,
                "1",
                &module,
                &refset,
                member,
            ])
        })
        .collect();
    write_rf2(
        dir,
        &format!("Snapshot/Refset/Content/der2_Refset_ZooSimpleSnapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
        ],
        &members,
    )?;
    write_rf2(
        dir,
        &format!(
            "Snapshot/Refset/Metadata/der2_ssRefset_DependencyZooSnapshot_XX{NAMESPACE}_{DATE}.txt"
        ),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
            "sourceEffectiveTime",
            "targetEffectiveTime",
        ],
        &[s(&[
            &member_id(400),
            DATE,
            "1",
            &module,
            &constants::MODULE_DEPENDENCY_REFSET.to_string(),
            package.depends_on,
            DATE,
            package.target,
        ])],
    )?;
    Ok(())
}
