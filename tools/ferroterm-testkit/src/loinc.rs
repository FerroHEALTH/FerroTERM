// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic LOINC-shaped release: the files `loinc` reads, laid
//! out as the release archive lays them out, with invented codes carrying
//! valid check digits.
//!
//! The codes are not LOINC codes and the names are invented; the shape (file
//! names, column names, the multiaxial hierarchy, an answer list, a Dutch
//! linguistic variant) is the release's.

use std::path::Path;

use ::loinc::id::with_check_digit;

/// The release version the fixture claims.
pub const VERSION: &str = "2.99";
/// A term: glucose in blood, active, with a Dutch translation and an answer list.
pub const GLUCOSE: &str = "90001";
/// A term: sodium in serum, active, third-party copyright.
pub const SODIUM: &str = "90002";
/// A deprecated term.
pub const OLD_GLUCOSE: &str = "90003";
/// A survey term linked to the answer list.
pub const SURVEY: &str = "90004";
/// The root part of the fixture hierarchy.
pub const ROOT_PART: &str = "LP70001";
/// The chemistry part under the root.
pub const CHEMISTRY_PART: &str = "LP70002";
/// The glucose component part under chemistry.
pub const GLUCOSE_PART: &str = "LP70003";
/// A class part only the hierarchy names (`Sodium | Serum or Plasma | Chemistry`).
pub const SODIUM_CLASS: &str = "LP70005";
/// The answer list.
pub const ANSWER_LIST: &str = "LL70001";
/// The first answer.
pub const YES: &str = "LA70001";
/// The second answer.
pub const NO: &str = "LA70002";

/// `body` with its LOINC check digit (`-0` when `body` is not digits after
/// its letter prefix, which no fixture constant is).
#[must_use]
pub fn code(body: &str) -> String {
    let digits = body.trim_start_matches(|c: char| c.is_ascii_uppercase());
    let prefix = body.strip_suffix(digits).unwrap_or_default();
    let checked = with_check_digit(digits).unwrap_or_else(|| format!("{digits}-0"));
    format!("{prefix}{checked}")
}

fn csv(header: &[&str], rows: &[Vec<String>]) -> String {
    let quote = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
    let mut text = header
        .iter()
        .map(|h| quote(h))
        .collect::<Vec<_>>()
        .join(",");
    text.push('\n');
    for row in rows {
        text.push_str(&row.iter().map(|v| quote(v)).collect::<Vec<_>>().join(","));
        text.push('\n');
    }
    text
}

/// One row of `Loinc.csv`: the columns the fixture varies per term.
struct Term<'a> {
    /// `LOINC_NUM`, without the check digit the fixture adds.
    num: &'a str,
    /// `COMPONENT`.
    component: &'a str,
    /// `CLASS`.
    class: &'a str,
    /// `STATUS`.
    status: &'a str,
    /// `CONSUMER_NAME`.
    consumer: &'a str,
    /// `SHORTNAME`.
    short: &'a str,
    /// `ORDER_OBS`.
    order_obs: &'a str,
    /// `EXTERNAL_COPYRIGHT_NOTICE`.
    copyright: &'a str,
    /// `LONG_COMMON_NAME`.
    long: &'a str,
    /// `CLASSTYPE`.
    class_type: &'a str,
}

/// Writes the release under `root`.
///
/// The files are `LoincTable/Loinc.csv`, `PartFile/Part.csv`,
/// `ComponentHierarchyBySystem/ComponentHierarchyBySystem.csv`,
/// `AnswerFile/AnswerList.csv` and `LoincAnswerListLink.csv`, and
/// `LinguisticVariants/nlNL22LinguisticVariant.csv`.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
#[expect(
    clippy::too_many_lines,
    reason = "one release file per block, read top to bottom"
)]
pub fn write_release(root: &Path) -> std::io::Result<()> {
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<String>>();
    let term_header = [
        "LOINC_NUM",
        "COMPONENT",
        "PROPERTY",
        "TIME_ASPCT",
        "SYSTEM",
        "SCALE_TYP",
        "METHOD_TYP",
        "CLASS",
        "VersionLastChanged",
        "CHNG_TYPE",
        "DefinitionDescription",
        "STATUS",
        "CONSUMER_NAME",
        "CLASSTYPE",
        "FORMULA",
        "EXMPL_ANSWERS",
        "SURVEY_QUEST_TEXT",
        "SURVEY_QUEST_SRC",
        "UNITSREQUIRED",
        "RELATEDNAMES2",
        "SHORTNAME",
        "ORDER_OBS",
        "HL7_FIELD_SUBFIELD_ID",
        "EXTERNAL_COPYRIGHT_NOTICE",
        "EXAMPLE_UNITS",
        "LONG_COMMON_NAME",
        "EXAMPLE_UCUM_UNITS",
        "STATUS_REASON",
        "STATUS_TEXT",
        "CHANGE_REASON_PUBLIC",
        "COMMON_TEST_RANK",
        "COMMON_ORDER_RANK",
        "HL7_ATTACHMENT_STRUCTURE",
        "EXTERNAL_COPYRIGHT_LINK",
        "PanelType",
        "AskAtOrderEntry",
        "AssociatedObservations",
        "VersionFirstReleased",
        "ValidHL7AttachmentRequest",
        "DisplayName",
    ];
    let term = |term: &Term<'_>| {
        let Term {
            num,
            component,
            class,
            status,
            consumer,
            short,
            order_obs,
            copyright,
            long,
            class_type,
        } = *term;
        let mut row = vec![String::new(); term_header.len()];
        let set = |row: &mut Vec<String>, name: &str, value: &str| {
            if let Some(slot) = term_header
                .iter()
                .position(|h| *h == name)
                .and_then(|i| row.get_mut(i))
            {
                value.clone_into(slot);
            }
        };
        set(&mut row, "LOINC_NUM", &code(num));
        set(&mut row, "COMPONENT", component);
        set(&mut row, "PROPERTY", "MCnc");
        set(&mut row, "TIME_ASPCT", "Pt");
        set(&mut row, "SYSTEM", "Bld");
        set(&mut row, "SCALE_TYP", "Qn");
        set(&mut row, "CLASS", class);
        set(&mut row, "VersionLastChanged", VERSION);
        set(&mut row, "STATUS", status);
        set(&mut row, "CONSUMER_NAME", consumer);
        set(&mut row, "CLASSTYPE", class_type);
        set(&mut row, "SHORTNAME", short);
        set(&mut row, "ORDER_OBS", order_obs);
        set(&mut row, "EXTERNAL_COPYRIGHT_NOTICE", copyright);
        set(&mut row, "LONG_COMMON_NAME", long);
        set(&mut row, "VersionFirstReleased", "1.0");
        row
    };
    let terms = vec![
        term(&Term {
            num: GLUCOSE,
            component: "Glucose",
            class: "CHEM",
            status: "ACTIVE",
            consumer: "Glucose",
            short: "Glucose Bld-mCnc",
            order_obs: "Both",
            copyright: "",
            long: "Glucose [Mass/volume] in Blood",
            class_type: "1",
        }),
        term(&Term {
            num: SODIUM,
            component: "Sodium",
            class: "CHEM",
            status: "ACTIVE",
            consumer: "Sodium",
            short: "Sodium SerPl-sCnc",
            order_obs: "Both",
            copyright: "Copyright example third party",
            long: "Sodium [Moles/volume] in Serum or Plasma",
            class_type: "1",
        }),
        term(&Term {
            num: OLD_GLUCOSE,
            component: "Glucose",
            class: "CHEM",
            status: "DEPRECATED",
            consumer: "",
            short: "Glucose Bld-mCnc old",
            order_obs: "Observation",
            copyright: "",
            long: "Glucose [Mass/volume] in Blood (superseded)",
            class_type: "1",
        }),
        term(&Term {
            num: SURVEY,
            component: "Fasting status",
            class: "PANEL.CHEM",
            status: "ACTIVE",
            consumer: "",
            short: "Fasting",
            order_obs: "Order",
            copyright: "",
            long: "Fasting status [Presence]",
            class_type: "4",
        }),
    ];
    let files: Vec<(&str, String)> = vec![
        ("LoincTable/Loinc.csv", csv(&term_header, &terms)),
        (
            "PartFile/Part.csv",
            csv(
                &[
                    "PartNumber",
                    "PartTypeName",
                    "PartName",
                    "PartDisplayName",
                    "Status",
                ],
                &[
                    s(&[
                        &code(ROOT_PART),
                        "CLASS",
                        "Laboratory",
                        "Laboratory",
                        "ACTIVE",
                    ]),
                    s(&[
                        &code(CHEMISTRY_PART),
                        "CLASS",
                        "Chemistry",
                        "Chemistry",
                        "ACTIVE",
                    ]),
                    s(&[
                        &code(GLUCOSE_PART),
                        "COMPONENT",
                        "Glucose",
                        "Glucose",
                        "ACTIVE",
                    ]),
                ],
            ),
        ),
        (
            "PartFile/LoincPartLink_Primary.csv",
            csv(
                &[
                    "LoincNumber",
                    "LongCommonName",
                    "PartNumber",
                    "PartName",
                    "PartCodeSystem",
                    "PartTypeName",
                    "LinkTypeName",
                    "Property",
                ],
                &[
                    s(&[
                        &code(GLUCOSE),
                        "Glucose [Mass/volume] in Blood",
                        &code(GLUCOSE_PART),
                        "Glucose",
                        "http://loinc.org",
                        "COMPONENT",
                        "Primary",
                        "http://loinc.org/property/COMPONENT",
                    ]),
                    s(&[
                        &code(OLD_GLUCOSE),
                        "Glucose [Mass/volume] in Blood (superseded)",
                        &code(GLUCOSE_PART),
                        "Glucose",
                        "http://loinc.org",
                        "COMPONENT",
                        "Primary",
                        "http://loinc.org/property/COMPONENT",
                    ]),
                ],
            ),
        ),
        (
            "ComponentHierarchyBySystem/ComponentHierarchyBySystem.csv",
            csv(
                &[
                    "PATH_TO_ROOT",
                    "SEQUENCE",
                    "IMMEDIATE_PARENT",
                    "CODE",
                    "CODE_TEXT",
                ],
                &[
                    s(&["", "1", "", &code(ROOT_PART), "Laboratory"]),
                    s(&[
                        &code(ROOT_PART),
                        "1",
                        &code(ROOT_PART),
                        &code(CHEMISTRY_PART),
                        "Chemistry",
                    ]),
                    s(&[
                        &format!("{}.{}", code(ROOT_PART), code(CHEMISTRY_PART)),
                        "1",
                        &code(CHEMISTRY_PART),
                        &code(GLUCOSE_PART),
                        "Glucose",
                    ]),
                    s(&[
                        &format!(
                            "{}.{}.{}",
                            code(ROOT_PART),
                            code(CHEMISTRY_PART),
                            code(GLUCOSE_PART)
                        ),
                        "1",
                        &code(GLUCOSE_PART),
                        &code(GLUCOSE),
                        "Glucose [Mass/volume] in Blood",
                    ]),
                    s(&[
                        &format!(
                            "{}.{}.{}",
                            code(ROOT_PART),
                            code(CHEMISTRY_PART),
                            code(GLUCOSE_PART)
                        ),
                        "2",
                        &code(GLUCOSE_PART),
                        &code(OLD_GLUCOSE),
                        "Glucose [Mass/volume] in Blood (superseded)",
                    ]),
                    s(&[
                        &format!("{}.{}", code(ROOT_PART), code(CHEMISTRY_PART)),
                        "2",
                        &code(CHEMISTRY_PART),
                        &code(SODIUM_CLASS),
                        "Sodium | Serum or Plasma | Chemistry",
                    ]),
                    s(&[
                        &format!(
                            "{}.{}.{}",
                            code(ROOT_PART),
                            code(CHEMISTRY_PART),
                            code(SODIUM_CLASS)
                        ),
                        "1",
                        &code(SODIUM_CLASS),
                        &code(SODIUM),
                        "Sodium [Moles/volume] in Serum or Plasma",
                    ]),
                ],
            ),
        ),
        (
            "AnswerFile/AnswerList.csv",
            csv(
                &[
                    "AnswerListId",
                    "AnswerListName",
                    "AnswerListOID",
                    "ExtDefinedYN",
                    "ExtDefinedAnswerListCodeSystem",
                    "ExtDefinedAnswerListLink",
                    "AnswerStringId",
                    "LocalAnswerCode",
                    "LocalAnswerCodeSystem",
                    "SequenceNumber",
                    "DisplayText",
                    "ExtCodeId",
                    "ExtCodeDisplayName",
                    "ExtCodeSystem",
                    "ExtCodeSystemVersion",
                    "ExtCodeSystemCopyrightNotice",
                ],
                &[
                    s(&[
                        &code(ANSWER_LIST),
                        "Yes or no",
                        "1.2.3",
                        "N",
                        "",
                        "",
                        &code(YES),
                        "",
                        "",
                        "1",
                        "Yes",
                        "",
                        "",
                        "",
                        "",
                        "",
                    ]),
                    s(&[
                        &code(ANSWER_LIST),
                        "Yes or no",
                        "1.2.3",
                        "N",
                        "",
                        "",
                        &code(NO),
                        "",
                        "",
                        "2",
                        "No",
                        "",
                        "",
                        "",
                        "",
                        "",
                    ]),
                ],
            ),
        ),
        (
            "AnswerFile/LoincAnswerListLink.csv",
            csv(
                &[
                    "LoincNumber",
                    "LongCommonName",
                    "AnswerListId",
                    "AnswerListName",
                    "AnswerListLinkType",
                    "ApplicableContext",
                ],
                &[s(&[
                    &code(SURVEY),
                    "Fasting status [Presence]",
                    &code(ANSWER_LIST),
                    "Yes or no",
                    "NORMATIVE",
                    "",
                ])],
            ),
        ),
        (
            "LinguisticVariants/nlNL22LinguisticVariant.csv",
            csv(
                &[
                    "LOINC_NUM",
                    "COMPONENT",
                    "PROPERTY",
                    "TIME_ASPCT",
                    "SYSTEM",
                    "SCALE_TYP",
                    "METHOD_TYP",
                    "CLASS",
                    "SHORTNAME",
                    "LONG_COMMON_NAME",
                    "RELATEDNAMES2",
                    "LinguisticVariantDisplayName",
                ],
                &[s(&[
                    &code(GLUCOSE),
                    "Glucose",
                    "MCnc",
                    "Mom",
                    "Bld",
                    "Kwn",
                    "",
                    "CHEM",
                    "Glucose Bld-mCnc",
                    "Glucose [massa/volume] in bloed",
                    "",
                    "Glucose in bloed",
                ])],
            ),
        ),
    ];
    for (relative, text) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(())
}

/// Writes the release to a temporary directory and builds the artifact the
/// provider opens under `dir`, the way `ferroterm-build --loinc` lays it out.
///
/// # Errors
///
/// Returns the I/O error when the release cannot be written, or the build
/// error as an I/O error.
pub fn write_artifact(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    write_release(release.path())?;
    ferroterm_build::loinc::build(release.path(), None, dir)
        .map(|_| ())
        .map_err(std::io::Error::other)
}
