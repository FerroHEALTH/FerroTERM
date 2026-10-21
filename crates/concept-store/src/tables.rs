// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The table set of one artifact.
//!
//! Every value is a byte string decoded by `record`, so a damaged artifact is
//! a typed error at read time rather than a panic inside the database. Keys
//! are dense ordinals; the vocabulary tables give each property key,
//! designation use, and language reference set its ordinal and name.

use redb::TableDefinition;

/// Artifact-level facts: system URI, version, edition, layout version.
pub const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
/// Native code to concept ordinal.
pub const CODES: TableDefinition<&str, u32> = TableDefinition::new("codes");
/// The columns of an artifact, in the order the store reads them.
///
/// Each is a [`crate::column::Column`] in its own file beside the database,
/// named by [`crate::column::Column::file`]: a column held in the database
/// arrived as the value `redb` materialized and again as the copy the store
/// serves from (#641).
pub const COLUMNS: [&str; 4] = [
    COLUMN_CONCEPTS,
    COLUMN_DISPLAYS,
    COLUMN_PROPERTIES,
    COLUMN_ACCEPTABILITY,
];
/// The concept column: the encoded [`crate::record::Concept`] of every ordinal.
pub const COLUMN_CONCEPTS: &str = "concepts";
/// The preferred-designation column, the displays the build chose per concept.
pub const COLUMN_DISPLAYS: &str = "displays";
/// The property column, one encoded [`crate::record::Properties`] per concept.
pub const COLUMN_PROPERTIES: &str = "properties";
/// The acceptability column, one encoded [`crate::record::Acceptability`] per
/// concept.
pub const COLUMN_ACCEPTABILITY: &str = "acceptability";
/// Concept ordinal to its encoded [`crate::record::Designations`].
///
/// The designation text is the largest thing an artifact holds, so it stays in
/// the database and is point-read per concept; the columns the store reads
/// into memory carry only what a request path walks (#338).
pub const DESIGNATIONS: TableDefinition<u32, &[u8]> = TableDefinition::new("designations");
/// Property key ordinal to its name.
pub const PROPERTY_KEYS: TableDefinition<u32, &str> = TableDefinition::new("property_keys");
/// Designation use ordinal to its code (for SNOMED, the description type SCTID).
pub const DESIGNATION_USES: TableDefinition<u32, &str> = TableDefinition::new("designation_uses");
/// Language reference set ordinal to its code.
pub const LANGUAGE_REFSETS: TableDefinition<u32, &str> = TableDefinition::new("language_refsets");
/// Acceptability ordinal to its code.
pub const ACCEPTABILITIES: TableDefinition<u32, &str> = TableDefinition::new("acceptabilities");

/// The `META` key of the layout version.
pub const META_LAYOUT: &str = "layout";
/// The layout version this build writes and reads.
///
/// An artifact of another layout is refused either way round: an older server
/// refuses one whose columns are side files, and this one refuses an older
/// artifact whose columns are still in the database, rather than reading
/// either as garbage.
pub const LAYOUT_VERSION: &str = "7";
/// The `META` key of the code system URI.
pub const META_SYSTEM: &str = "system";
/// The `META` key of the code system version string.
pub const META_VERSION: &str = "version";
/// The `META` key of the number of concepts.
pub const META_CONCEPTS: &str = "concepts";
/// The `META` key of the acceptability ordinal that marks a designation
/// preferred in its language reference set.
pub const META_PREFERRED: &str = "preferred";
