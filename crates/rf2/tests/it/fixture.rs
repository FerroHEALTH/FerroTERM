// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! A synthetic RF2 Snapshot release built in a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};

use rf2::id::with_check_digit;

/// The invented namespace every extension identifier here carries.
pub(crate) const NAMESPACE: &str = "1234567";
/// The release date every file carries.
pub(crate) const DATE: &str = "20260101";

/// A concept identifier in the invented namespace, from a small item number.
pub(crate) fn concept(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}10"))
}

/// A description identifier in the invented namespace.
pub(crate) fn description(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}11"))
}

/// A relationship identifier in the invented namespace.
pub(crate) fn relationship(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}12"))
}

/// A member UUID from a small item number.
pub(crate) fn member(item: u32) -> String {
    format!("00000000-0000-4000-8000-{item:012}")
}

pub(crate) const IS_A: &str = "116680003";
pub(crate) const CORE_MODULE: &str = "900000000000207008";
pub(crate) const MODEL_MODULE: &str = "900000000000012004";
pub(crate) const PRIMITIVE: &str = "900000000000074008";
pub(crate) const DEFINED: &str = "900000000000073002";
pub(crate) const FSN: &str = "900000000000003001";
pub(crate) const SYNONYM: &str = "900000000000013009";
pub(crate) const CASE_INSENSITIVE: &str = "900000000000448009";
pub(crate) const INFERRED: &str = "900000000000011006";
pub(crate) const EXISTENTIAL: &str = "900000000000451002";
pub(crate) const PREFERRED: &str = "900000000000548007";
pub(crate) const ACCEPTABLE: &str = "900000000000549004";
pub(crate) const MODULE_DEPENDENCY_REFSET: &str = "900000000000534007";
pub(crate) const GB_LANGUAGE_REFSET: &str = "900000000000508004";

/// The synthetic release on disk.
pub(crate) struct Release {
    pub(crate) dir: tempfile::TempDir,
}

impl Release {
    /// The release root (the directory holding `Snapshot/`).
    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, relative: &str, header: &[&str], rows: &[Vec<String>]) -> PathBuf {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let mut text = header.join("\t");
        text.push_str("\r\n");
        for row in rows {
            text.push_str(&row.join("\t"));
            text.push_str("\r\n");
        }
        fs::write(&path, text).expect("write");
        path
    }
}

/// The extension module concept of the synthetic edition.
pub(crate) fn extension_module() -> String {
    concept(9)
}

/// Builds the standard synthetic release: a root concept, a body-structure
/// child, a finding grandchild, descriptions in two languages, inferred is-a
/// relationships, a language refset, a simple refset, an association, and the
/// module dependency rows that make the extension module the edition root.
pub(crate) fn standard() -> Release {
    let release = Release {
        dir: tempfile::tempdir().expect("tempdir"),
    };
    let module = extension_module();
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let (root, child, grandchild) = (concept(1), concept(2), concept(3));
    release.write(
        &format!("Snapshot/Terminology/sct2_Concept_Snapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "definitionStatusId",
        ],
        &[
            s(&[&root, DATE, "1", &module, PRIMITIVE]),
            s(&[&child, DATE, "1", &module, DEFINED]),
            s(&[&grandchild, DATE, "0", &module, PRIMITIVE]),
            s(&[&module, DATE, "1", &module, PRIMITIVE]),
        ],
    );
    release.write(
        &format!("Snapshot/Terminology/sct2_Description_Snapshot-en_XX{NAMESPACE}_{DATE}.txt"),
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
        &[
            s(&[
                &description(1),
                DATE,
                "1",
                &module,
                &root,
                "en",
                FSN,
                "Synthetic root (synthetic)",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(2),
                DATE,
                "1",
                &module,
                &root,
                "en",
                SYNONYM,
                "Synthetic root",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(3),
                DATE,
                "1",
                &module,
                &child,
                "en",
                FSN,
                "Synthetic child (synthetic)",
                CASE_INSENSITIVE,
            ]),
        ],
    );
    release.write(
        &format!("Snapshot/Terminology/sct2_Description_Snapshot-nl_XX{NAMESPACE}_{DATE}.txt"),
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
        &[s(&[
            &description(4),
            DATE,
            "1",
            &module,
            &child,
            "nl",
            SYNONYM,
            "Synthetisch kind",
            CASE_INSENSITIVE,
        ])],
    );
    release.write(
        &format!("Snapshot/Terminology/sct2_Relationship_Snapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "sourceId",
            "destinationId",
            "relationshipGroup",
            "typeId",
            "characteristicTypeId",
            "modifierId",
        ],
        &[
            s(&[
                &relationship(1),
                DATE,
                "1",
                &module,
                &child,
                &root,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
            s(&[
                &relationship(2),
                DATE,
                "1",
                &module,
                &grandchild,
                &child,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
        ],
    );
    release.write(
        &format!(
            "Snapshot/Terminology/sct2_RelationshipConcreteValues_Snapshot_XX{NAMESPACE}_{DATE}.txt"
        ),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "sourceId",
            "value",
            "relationshipGroup",
            "typeId",
            "characteristicTypeId",
            "modifierId",
        ],
        &[s(&[
            &relationship(3),
            DATE,
            "1",
            &module,
            &child,
            "#3",
            "1",
            &concept(7),
            INFERRED,
            EXISTENTIAL,
        ])],
    );
    release.write(
        &format!(
            "Snapshot/Refset/Language/der2_cRefset_LanguageSnapshot-en_XX{NAMESPACE}_{DATE}.txt"
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
        &[
            s(&[
                &member(1),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(1),
                PREFERRED,
            ]),
            s(&[
                &member(2),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(2),
                PREFERRED,
            ]),
            s(&[
                &member(3),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(3),
                ACCEPTABLE,
            ]),
        ],
    );
    release.write(
        &format!("Snapshot/Refset/Content/der2_Refset_SimpleSnapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
        ],
        &[s(&[&member(4), DATE, "1", &module, &concept(8), &child])],
    );
    release.write(
        &format!(
            "Snapshot/Refset/Content/der2_cRefset_AssociationSnapshot_XX{NAMESPACE}_{DATE}.txt"
        ),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
            "targetComponentId",
        ],
        &[s(&[
            &member(5),
            DATE,
            "1",
            &module,
            "900000000000526001",
            &grandchild,
            &child,
        ])],
    );
    release.write(
        &format!("Snapshot/Refset/Metadata/der2_ssRefset_ModuleDependencySnapshot_XX{NAMESPACE}_{DATE}.txt"),
        &["id", "effectiveTime", "active", "moduleId", "refsetId", "referencedComponentId", "sourceEffectiveTime", "targetEffectiveTime"],
        &[
            s(&[&member(6), DATE, "1", &module, MODULE_DEPENDENCY_REFSET, CORE_MODULE, DATE, "20251201"]),
            s(&[&member(7), DATE, "1", &module, MODULE_DEPENDENCY_REFSET, MODEL_MODULE, DATE, "20251201"]),
            s(&[&member(8), "20251201", "1", CORE_MODULE, MODULE_DEPENDENCY_REFSET, MODEL_MODULE, "20251201", "20251201"]),
        ],
    );
    release.write("Snapshot/Readme_en_20260101.txt", &["not an RF2 file"], &[]);
    release
}
