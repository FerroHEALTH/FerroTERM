// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic classification releases: a `ClaML` document shaped like a WHO
//! ICD-10 translation and an ICD-10-CM release shaped like the NCHS files.
//!
//! The codes are ICD-shaped and the titles invented; the shape (class
//! kinds, rubric kinds, a modifier with an exclusion, a reference in
//! brackets, a one-category section, seventh-character codes in the order
//! file) is the releases'.

use std::path::Path;

/// The system URI the `ClaML` fixture is served under in tests.
pub const CLAML_SYSTEM: &str = "http://hl7.org/fhir/sid/icd-10-nl";
/// The version the `ClaML` title states.
pub const CLAML_VERSION: &str = "2021";
/// The chapter.
pub const CHAPTER: &str = "II";
/// The block under the chapter.
pub const BLOCK: &str = "C00-C97";
/// A category with two subcategories and an exclusion referencing another code.
pub const LIVER: &str = "C22";
/// A subcategory.
pub const LIVER_CELL: &str = "C22.0";
/// A subcategory marked with the dagger.
pub const BILE_DUCT: &str = "C22.1";
/// The injury chapter.
pub const INJURY_CHAPTER: &str = "XIX";
/// The injury block, modified by the open/closed modifier.
pub const INJURY_BLOCK: &str = "S00-S09";
/// The skull fracture category.
pub const SKULL: &str = "S02";
/// The vault subcategory, which the modifier expands.
pub const VAULT: &str = "S02.0";
/// The closed vault fracture the expansion produces.
pub const VAULT_CLOSED: &str = "S02.00";
/// The open vault fracture the expansion produces.
pub const VAULT_OPEN: &str = "S02.01";
/// The base subcategory, which excludes the modifier.
pub const BASE: &str = "S02.1";
/// The modifier.
pub const MODIFIER: &str = "S5";
/// The ICD-10 category whose subcategory shares four characters with a
/// morphology class.
pub const DEFORMITY: &str = "M95";
/// The explicit ICD-10 subcategory, spelled with its period in the document.
pub const NECK_DEFORMITY: &str = "M95.3";
/// The root of the ICD-O morphology axis.
pub const MORPHOLOGY: &str = "M";
/// The morphology class spelled `M953` in the document, which keeps its
/// spelling: its ancestor is `M`, not `M95`.
pub const MESOTHELIOMAS: &str = "M953";
/// A morphology code with a behaviour suffix.
pub const MALIGNANT_MESOTHELIOMA: &str = "M9530/3";

/// The version the ICD-10-CM fixture states.
pub const CM_VERSION: &str = "2099";
/// The first chapter's code, its range.
pub const CM_CHAPTER: &str = "A00-B99";
/// The intestinal block.
pub const CM_BLOCK: &str = "A00-A09";
/// A category with an `excludes1` note.
pub const CHOLERA: &str = "A00";
/// A subcategory with an inclusion term.
pub const CLASSICAL: &str = "A00.0";
/// A header subcategory (not valid for use).
pub const UNSPECIFIED: &str = "A00.9";
/// A category whose section shares its code.
pub const HERPES: &str = "B10";
/// The injury chapter's code.
pub const CM_INJURY: &str = "S00-T88";
/// The skull fracture subcategory with a seventh-character definition.
pub const CM_VAULT: &str = "S02.0";
/// A seventh-character code the order file adds.
pub const CM_VAULT_INITIAL: &str = "S02.0XXA";

/// The `ClaML` document.
const CLAML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ClaML version="2.0.0">
  <Meta name="lang" value="nl"/>
  <Identifier authority="WHO" uid="synthetic"/>
  <Title date="2021-01-01" name="ICD-10-NL" version="2021">ICD-10 Nederlandse vertaling (synthetisch)</Title>
  <ClassKinds>
    <ClassKind name="chapter"/>
    <ClassKind name="block"/>
    <ClassKind name="category"/>
    <ClassKind name="mblock"/>
    <ClassKind name="mdigit4"/>
    <ClassKind name="mdigit6"/>
  </ClassKinds>
  <UsageKinds>
    <UsageKind mark="*" name="aster"/>
    <UsageKind mark="+" name="dagger"/>
  </UsageKinds>
  <RubricKinds>
    <RubricKind inherited="false" name="preferred"/>
    <RubricKind inherited="false" name="preferredLong"/>
    <RubricKind inherited="false" name="inclusion"/>
    <RubricKind inherited="false" name="exclusion"/>
    <RubricKind inherited="false" name="note"/>
  </RubricKinds>
  <Modifier code="S5">
    <SubClass code="0"/>
    <SubClass code="1"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Open of gesloten</Label></Rubric>
  </Modifier>
  <ModifierClass modifier="S5" code="0">
    <SuperClass code="S5"/>
    <Rubric kind="preferred"><Label xml:lang="nl">gesloten</Label><Label xml:lang="en">closed</Label></Rubric>
  </ModifierClass>
  <ModifierClass modifier="S5" code="1">
    <SuperClass code="S5"/>
    <Rubric kind="preferred"><Label xml:lang="nl">open</Label><Label xml:lang="en">open</Label></Rubric>
  </ModifierClass>
  <Class code="II" kind="chapter">
    <SubClass code="C00-C97"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Nieuwvormingen</Label><Label xml:lang="en">Neoplasms</Label></Rubric>
  </Class>
  <Class code="C00-C97" kind="block">
    <SuperClass code="II"/>
    <SubClass code="C22"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Maligne nieuwvormingen</Label></Rubric>
  </Class>
  <Class code="C22" kind="category">
    <SuperClass code="C00-C97"/>
    <SubClass code="C22.0"/>
    <SubClass code="C22.1"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Maligne nieuwvorming van lever en intrahepatische galwegen</Label><Label xml:lang="en">Malignant neoplasm of liver and intrahepatic bile ducts</Label></Rubric>
    <Rubric kind="exclusion"><Label xml:lang="nl">secundaire maligne nieuwvorming van lever <Reference class="in brackets">C78.7</Reference></Label></Rubric>
    <Rubric kind="note"><Label xml:lang="nl">Een &amp; ander <Fragment>in fragmenten</Fragment></Label></Rubric>
  </Class>
  <Class code="C22.0" kind="category">
    <SuperClass code="C22"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Levercelcarcinoom</Label><Label xml:lang="en">Liver cell carcinoma</Label></Rubric>
    <Rubric kind="inclusion"><Label xml:lang="nl">hepatocellulair carcinoom</Label></Rubric>
  </Class>
  <Class code="C221" kind="category" usage="dagger">
    <SuperClass code="C22"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Intrahepatisch galwegcarcinoom</Label></Rubric>
  </Class>
  <Class code="XIX" kind="chapter">
    <SubClass code="S00-S09"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Letsel</Label></Rubric>
  </Class>
  <Class code="S00-S09" kind="block">
    <SuperClass code="XIX"/>
    <SubClass code="S02"/>
    <ModifiedBy code="S5" all="true" position="5"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Letsel van hoofd</Label></Rubric>
  </Class>
  <Class code="S02" kind="category">
    <SuperClass code="S00-S09"/>
    <SubClass code="S02.0"/>
    <SubClass code="S02.1"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Fractuur van schedel en aangezichtsbeenderen</Label></Rubric>
  </Class>
  <Class code="S02.0" kind="category">
    <SuperClass code="S02"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Fractuur van schedeldak</Label><Label xml:lang="en">Fracture of vault of skull</Label></Rubric>
  </Class>
  <Class code="S02.1" kind="category">
    <SuperClass code="S02"/>
    <ExcludeModifier code="S5"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Fractuur van schedelbasis</Label></Rubric>
  </Class>
  <Class code="XIII" kind="chapter">
    <SubClass code="M95-M99"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Ziekten van botspierstelsel en bindweefsel</Label></Rubric>
  </Class>
  <Class code="M95-M99" kind="block">
    <SuperClass code="XIII"/>
    <SubClass code="M95"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Overige aandoeningen van botspierstelsel en bindweefsel</Label></Rubric>
  </Class>
  <Class code="M95" kind="category">
    <SuperClass code="M95-M99"/>
    <SubClass code="M95.3"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Overige verworven misvormingen van botspierstelsel en bindweefsel</Label></Rubric>
  </Class>
  <Class code="M95.3" kind="category">
    <SuperClass code="M95"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Verworven misvorming van hals</Label></Rubric>
  </Class>
  <Class code="M" kind="mblock">
    <SubClass code="M953"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Morfologie van neoplasmata</Label></Rubric>
  </Class>
  <Class code="M953" kind="mdigit4">
    <SuperClass code="M"/>
    <SubClass code="M9530/3"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Mesotheliomen</Label></Rubric>
  </Class>
  <Class code="M9530/3" kind="mdigit6">
    <SuperClass code="M953"/>
    <Rubric kind="preferred"><Label xml:lang="nl">Mesothelioom, maligne</Label></Rubric>
  </Class>
</ClaML>
"#;

/// The `ClaML` document.
#[must_use]
pub fn claml() -> String {
    CLAML.to_owned()
}

/// Writes the `ClaML` document to `path`.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be written.
pub fn write_claml(path: &Path) -> std::io::Result<()> {
    std::fs::write(path, claml())
}

/// The ICD-10-CM tabular list.
#[must_use]
pub fn tabular() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<ICD10CM.tabular>
  <version>2099</version>
  <introduction>
    <introSection type="title"><title>ICD-10-CM TABULAR LIST (synthetic)</title></introSection>
  </introduction>
  <chapter>
    <name>1</name>
    <desc>Certain infectious and parasitic diseases (A00-B99)</desc>
    <includes><note>diseases generally recognized as communicable</note></includes>
    <sectionIndex>
      <sectionRef first="A00" last="A09" id="A00-A09">Intestinal infectious diseases</sectionRef>
      <sectionRef first="B10" last="B10" id="B10">Other human herpesviruses</sectionRef>
    </sectionIndex>
    <section id="A00-A09">
      <desc>Intestinal infectious diseases (A00-A09)</desc>
      <diag>
        <name>A00</name>
        <desc>Cholera</desc>
        <excludes1><note>cholera-like illness (A09)</note></excludes1>
        <diag>
          <name>A00.0</name>
          <desc>Cholera due to Vibrio cholerae 01, biovar cholerae</desc>
          <inclusionTerm><note>Classical cholera</note></inclusionTerm>
        </diag>
        <diag>
          <name>A00.9</name>
          <desc>Cholera, unspecified</desc>
        </diag>
      </diag>
    </section>
    <section id="B10">
      <desc>Other human herpesviruses (B10)</desc>
      <diag>
        <name>B10</name>
        <desc>Other human herpesviruses</desc>
        <excludes2><note>cytomegalovirus (B25.9)</note></excludes2>
        <useAdditionalCode><note>code to identify resistance (Z16.-)</note></useAdditionalCode>
      </diag>
    </section>
  </chapter>
  <chapter>
    <name>19</name>
    <desc>Injury, poisoning and certain other consequences of external causes (S00-T88)</desc>
    <sectionIndex>
      <sectionRef first="S00" last="S09" id="S00-S09">Injuries to the head</sectionRef>
    </sectionIndex>
    <section id="S00-S09">
      <desc>Injuries to the head (S00-S09)</desc>
      <diag>
        <name>S02</name>
        <desc>Fracture of skull and facial bones</desc>
        <codeAlso><note>any associated intracranial injury (S06.-)</note></codeAlso>
        <sevenChrNote><note>The appropriate 7th character is to be added to each code from category S02</note></sevenChrNote>
        <sevenChrDef>
          <extension char="A">initial encounter for closed fracture</extension>
          <extension char="B">initial encounter for open fracture</extension>
        </sevenChrDef>
        <diag>
          <name>S02.0</name>
          <desc>Fracture of vault of skull</desc>
        </diag>
      </diag>
    </section>
  </chapter>
</ICD10CM.tabular>
"#
    .to_owned()
}

/// The ICD-10-CM order file, in the CMS fixed columns.
#[must_use]
pub fn order() -> String {
    let lines: [(&str, u8, &str, &str); 8] = [
        ("A00", 0, "Cholera", "Cholera"),
        (
            "A000",
            1,
            "Cholera due to Vibrio cholerae 01, biovar cholerae",
            "Cholera due to Vibrio cholerae 01, biovar cholerae",
        ),
        ("A009", 0, "Cholera, unspecified", "Cholera, unspecified"),
        (
            "B10",
            1,
            "Other human herpesviruses",
            "Other human herpesviruses",
        ),
        (
            "S02",
            0,
            "Fracture of skull and facial bones",
            "Fracture of skull and facial bones",
        ),
        (
            "S020",
            0,
            "Fracture of vault of skull",
            "Fracture of vault of skull",
        ),
        (
            "S020XXA",
            1,
            "Fracture of vault of skull, init for clos fx",
            "Fracture of vault of skull, initial encounter for closed fracture",
        ),
        (
            "S020XXB",
            1,
            "Fracture of vault of skull, init for opn fx",
            "Fracture of vault of skull, initial encounter for open fracture",
        ),
    ];
    lines
        .iter()
        .enumerate()
        .map(|(i, (code, flag, short, long))| {
            format!("{:05} {:<7} {} {:<60} {}\n", i + 1, code, flag, short, long)
        })
        .collect::<Vec<String>>()
        .concat()
}

/// Writes the ICD-10-CM release under `root`: `Table and Index/icd10cm_tabular_2099.xml`
/// and `icd10cm_order_2099.txt`.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_icd10cm(root: &Path) -> std::io::Result<()> {
    let tables = root.join("Table and Index");
    std::fs::create_dir_all(&tables)?;
    std::fs::write(tables.join("icd10cm_tabular_2099.xml"), tabular())?;
    std::fs::write(root.join("icd10cm_order_2099.txt"), order())
}

/// Builds the `ClaML` fixture into an artifact directory under [`CLAML_SYSTEM`].
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_claml_artifact(dir: &Path) -> std::io::Result<()> {
    let classification = ::classification::claml::read(&claml()).map_err(std::io::Error::other)?;
    ferroterm_build::classification::build(&classification, CLAML_SYSTEM, None, dir)
        .map(|_| ())
        .map_err(std::io::Error::other)
}

/// Builds the ICD-10-CM fixture into an artifact directory.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_icd10cm_artifact(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    write_icd10cm(release.path())?;
    let files = ::classification::icd10cm::locate(&[release.path().to_path_buf()])
        .map_err(std::io::Error::other)?;
    let classification = ::classification::icd10cm::read(&files).map_err(std::io::Error::other)?;
    ferroterm_build::classification::build(
        &classification,
        ferroterm_build::classification::ICD10CM_SYSTEM,
        None,
        dir,
    )
    .map(|_| ())
    .map_err(std::io::Error::other)
}
