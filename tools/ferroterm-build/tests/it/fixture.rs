// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic RF2 Snapshot in a temporary directory: invented concepts in an
//! invented namespace, valid check digits, published metadata identifiers only.

use std::fs;
use std::path::Path;

use rf2::id::with_check_digit;

const NAMESPACE: &str = "1234567";
pub(crate) const DATE: &str = "20260101";

pub(crate) fn concept(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}10"))
}

fn description(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}11"))
}

fn relationship(item: u32) -> String {
    with_check_digit(&format!("{item}{NAMESPACE}12"))
}

fn member(item: u32) -> String {
    format!("00000000-0000-4000-8000-{item:012}")
}

const IS_A: &str = "116680003";
const CORE_MODULE: &str = "900000000000207008";
const MODEL_MODULE: &str = "900000000000012004";
const PRIMITIVE: &str = "900000000000074008";
const DEFINED: &str = "900000000000073002";
const FSN: &str = "900000000000003001";
const SYNONYM: &str = "900000000000013009";
const CASE_INSENSITIVE: &str = "900000000000448009";
const INFERRED: &str = "900000000000011006";
const QUALIFYING: &str = "900000000000225001";
const ADDITIONAL: &str = "900000000000227009";
const EXISTENTIAL: &str = "900000000000451002";
const PREFERRED: &str = "900000000000548007";
const ACCEPTABLE: &str = "900000000000549004";
const MODULE_DEPENDENCY_REFSET: &str = "900000000000534007";
pub(crate) const GB_LANGUAGE_REFSET: &str = "900000000000508004";
pub(crate) const NL_LANGUAGE_REFSET: &str = "31000146106";

/// The extension module concept, which is also the edition root.
pub(crate) fn module() -> String {
    concept(9)
}

fn write(root: &Path, relative: &str, header: &[&str], rows: &[Vec<String>]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let mut text = header.join("\t");
    text.push_str("\r\n");
    for row in rows {
        text.push_str(&row.join("\t"));
        text.push_str("\r\n");
    }
    fs::write(&path, text).expect("write");
}

/// Writes the release under `root`: a root, an animal, a cat and a dog under
/// it, an inactive fish, two languages, a GB and an NL language refset.
#[expect(
    clippy::too_many_lines,
    reason = "one RF2 file per block, read top to bottom"
)]
pub(crate) fn write_release(root: &Path) {
    let module = module();
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let (top, animal, cat, dog, fish) =
        (concept(1), concept(2), concept(3), concept(4), concept(5));
    write(
        root,
        &format!("Snapshot/Terminology/sct2_Concept_Snapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "definitionStatusId",
        ],
        &[
            s(&[&fish, DATE, "0", &module, PRIMITIVE]),
            s(&[&top, DATE, "1", &module, PRIMITIVE]),
            s(&[&dog, DATE, "1", &module, DEFINED]),
            s(&[&animal, DATE, "1", &module, PRIMITIVE]),
            s(&[&cat, DATE, "1", &module, DEFINED]),
            s(&[&module, DATE, "1", &module, PRIMITIVE]),
            s(&[&concept(6), DATE, "1", &module, PRIMITIVE]),
            s(&[&concept(7), DATE, "1", &module, PRIMITIVE]),
            s(&[&concept(8), DATE, "1", &module, PRIMITIVE]),
        ],
    );
    let header = [
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "conceptId",
        "languageCode",
        "typeId",
        "term",
        "caseSignificanceId",
    ];
    write(
        root,
        &format!("Snapshot/Terminology/sct2_Description_Snapshot-en_XX{NAMESPACE}_{DATE}.txt"),
        &header,
        &[
            s(&[
                &description(1),
                DATE,
                "1",
                &module,
                &top,
                "en",
                FSN,
                "Living thing (synthetic)",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(2),
                DATE,
                "1",
                &module,
                &top,
                "en",
                SYNONYM,
                "Living thing",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(3),
                DATE,
                "1",
                &module,
                &animal,
                "en",
                FSN,
                "Animal (synthetic)",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(4),
                DATE,
                "1",
                &module,
                &animal,
                "en",
                SYNONYM,
                "Animal",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(5),
                DATE,
                "1",
                &module,
                &cat,
                "en",
                FSN,
                "Cat (synthetic)",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(6),
                DATE,
                "1",
                &module,
                &cat,
                "en",
                SYNONYM,
                "Cat",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(7),
                DATE,
                "1",
                &module,
                &dog,
                "en",
                FSN,
                "Dog (synthetic)",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(8),
                DATE,
                "1",
                &module,
                &dog,
                "en",
                SYNONYM,
                "Dog",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(9),
                DATE,
                "1",
                &module,
                &fish,
                "en",
                FSN,
                "Fish (synthetic)",
                CASE_INSENSITIVE,
            ]),
        ],
    );
    write(
        root,
        &format!("Snapshot/Terminology/sct2_Description_Snapshot-nl_XX{NAMESPACE}_{DATE}.txt"),
        &header,
        &[
            s(&[
                &description(10),
                DATE,
                "1",
                &module,
                &cat,
                "nl",
                SYNONYM,
                "Kat",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(11),
                DATE,
                "1",
                &module,
                &dog,
                "nl",
                SYNONYM,
                "Hond",
                CASE_INSENSITIVE,
            ]),
            s(&[
                &description(12),
                DATE,
                "1",
                &module,
                &cat,
                "nl",
                SYNONYM,
                "Poes",
                CASE_INSENSITIVE,
            ]),
        ],
    );
    write(
        root,
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
                &animal,
                &top,
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
                &cat,
                &animal,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
            s(&[
                &relationship(3),
                DATE,
                "1",
                &module,
                &dog,
                &animal,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
            // Inactive: the fish is no longer classified.
            s(&[
                &relationship(4),
                DATE,
                "0",
                &module,
                &fish,
                &animal,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
            s(&[
                &relationship(5),
                DATE,
                "1",
                &module,
                &module,
                &top,
                "0",
                IS_A,
                INFERRED,
                EXISTENTIAL,
            ]),
            // An attribute: the cat has covering fur (concept 6 is the attribute type).
            s(&[
                &relationship(6),
                DATE,
                "1",
                &module,
                &cat,
                &concept(7),
                "1",
                &concept(6),
                INFERRED,
                EXISTENTIAL,
            ]),
            // Inactive attribute: ignored.
            s(&[
                &relationship(7),
                DATE,
                "0",
                &module,
                &dog,
                &concept(7),
                "1",
                &concept(6),
                INFERRED,
                EXISTENTIAL,
            ]),
            // A qualifying is-a and an additional attribute: neither defines
            // the dog, so neither reaches the hierarchy or its properties.
            s(&[
                &relationship(9),
                DATE,
                "1",
                &module,
                &dog,
                &top,
                "0",
                IS_A,
                QUALIFYING,
                EXISTENTIAL,
            ]),
            s(&[
                &relationship(10),
                DATE,
                "1",
                &module,
                &dog,
                &concept(7),
                "1",
                &concept(6),
                ADDITIONAL,
                EXISTENTIAL,
            ]),
            // A qualifying row may name a concept the edition does not define,
            // since it is no part of the definition the edition carries.
            s(&[
                &relationship(12),
                DATE,
                "1",
                &module,
                &dog,
                &concept(50),
                "0",
                IS_A,
                QUALIFYING,
                EXISTENTIAL,
            ]),
        ],
    );
    write(
        root,
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
        // A concrete value: the cat has four legs (concept 8 is the attribute type).
        &[
            s(&[
                &relationship(8),
                DATE,
                "1",
                &module,
                &cat,
                "#4",
                "2",
                &concept(8),
                INFERRED,
                EXISTENTIAL,
            ]),
            // A qualifying concrete value: it does not define the dog either.
            s(&[
                &relationship(11),
                DATE,
                "1",
                &module,
                &dog,
                "#9",
                "2",
                &concept(8),
                QUALIFYING,
                EXISTENTIAL,
            ]),
        ],
    );
    let language_header = [
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "refsetId",
        "referencedComponentId",
        "acceptabilityId",
    ];
    write(
        root,
        &format!(
            "Snapshot/Refset/Language/der2_cRefset_LanguageSnapshot-en_XX{NAMESPACE}_{DATE}.txt"
        ),
        &language_header,
        &[
            s(&[
                &member(1),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(2),
                PREFERRED,
            ]),
            s(&[
                &member(2),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(4),
                PREFERRED,
            ]),
            s(&[
                &member(3),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(6),
                PREFERRED,
            ]),
            s(&[
                &member(4),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(8),
                PREFERRED,
            ]),
            s(&[
                &member(5),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(1),
                PREFERRED,
            ]),
            s(&[
                &member(13),
                DATE,
                "1",
                &module,
                GB_LANGUAGE_REFSET,
                &description(5),
                PREFERRED,
            ]),
        ],
    );
    write(
        root,
        &format!(
            "Snapshot/Refset/Language/der2_cRefset_LanguageSnapshot-nl_XX{NAMESPACE}_{DATE}.txt"
        ),
        &language_header,
        &[
            s(&[
                &member(6),
                DATE,
                "1",
                &module,
                NL_LANGUAGE_REFSET,
                &description(10),
                PREFERRED,
            ]),
            s(&[
                &member(7),
                DATE,
                "1",
                &module,
                NL_LANGUAGE_REFSET,
                &description(12),
                ACCEPTABLE,
            ]),
            s(&[
                &member(8),
                DATE,
                "1",
                &module,
                NL_LANGUAGE_REFSET,
                &description(11),
                PREFERRED,
            ]),
            // Inactive membership: ignored.
            s(&[
                &member(9),
                DATE,
                "0",
                &module,
                NL_LANGUAGE_REFSET,
                &description(6),
                PREFERRED,
            ]),
        ],
    );
    // A simple reference set (concept(8)) with the cat and the dog as active
    // members and the fish as an inactive one.
    write(
        root,
        &format!("Snapshot/Refset/Content/der2_Refset_SimpleSnapshot_XX{NAMESPACE}_{DATE}.txt"),
        &[
            "id",
            "effectiveTime",
            "active",
            "moduleId",
            "refsetId",
            "referencedComponentId",
        ],
        &[
            s(&[&member(20), DATE, "1", &module, &concept(8), &cat]),
            s(&[&member(21), DATE, "1", &module, &concept(8), &dog]),
            s(&[&member(22), DATE, "0", &module, &concept(8), &fish]),
        ],
    );
    write(
        root,
        &format!(
            "Snapshot/Refset/Metadata/der2_ssRefset_ModuleDependencySnapshot_XX{NAMESPACE}_{DATE}.txt"
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
        &[
            s(&[
                &member(10),
                DATE,
                "1",
                &module,
                MODULE_DEPENDENCY_REFSET,
                CORE_MODULE,
                DATE,
                "20251201",
            ]),
            s(&[
                &member(11),
                DATE,
                "1",
                &module,
                MODULE_DEPENDENCY_REFSET,
                MODEL_MODULE,
                DATE,
                "20251201",
            ]),
            s(&[
                &member(12),
                "20251201",
                "1",
                CORE_MODULE,
                MODULE_DEPENDENCY_REFSET,
                MODEL_MODULE,
                "20251201",
                "20251201",
            ]),
        ],
    );
    write(
        root,
        &format!("Snapshot/Readme_en_{DATE}.txt"),
        &["not an RF2 file"],
        &[],
    );
}
