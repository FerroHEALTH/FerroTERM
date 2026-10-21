// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic Labcodeset-shaped publication: the `labconcepts` document with
//! invented LOINC, SNOMED CT, and UCUM content.

use std::path::{Path, PathBuf};

/// The publication's effective date attribute.
pub const EFFECTIVE_DATE: &str = "20260101-090000000";
/// The release the build records.
pub const RELEASE: &str = "20260101";
/// An active concept with a material, units, and an ordinal outcome list.
pub const GLUCOSE: &str = "1000-1";
/// A retired concept with a LOINC replacement and no translation.
pub const OLD_SODIUM: &str = "2000-2";
/// The concept replacing it.
pub const SODIUM: &str = "2001-0";
/// An active panel concept with a nominal (refset) outcome list.
pub const CULTURE: &str = "3000-3";
/// The SNOMED CT material of the glucose concept.
pub const SERUM: &str = "119364003";
/// The ordinal value set's OID.
pub const ORDINAL_OID: &str = "2.16.840.1.113883.2.4.3.46.99.7.11.1.6";
/// The nominal reference set's concept identifier.
pub const REFSET: &str = "2581000146104";

/// The document text.
#[must_use]
pub fn document() -> String {
    format!(
        "{}{}{}{}{}",
        header(),
        glucose(),
        old_sodium(),
        culture(),
        tables()
    )
}

/// The publication's head, up to the concept list.
fn header() -> String {
    format!(
        r#"<publication type="simple" effectiveDate="{EFFECTIVE_DATE}" user="fixture"><!--Synthetic content: no LOINC, SNOMED CT, or UCUM release was used.-->
    <desc>01-01-2026: Synthetic Labcodeset publication</desc>
    <lab_concepts>
"#
    )
}

/// The active glucose concept.
fn glucose() -> String {
    format!(
        r#"        <lab_concept status="active">
            <loincConcept loinc_num="{GLUCOSE}" status="ACTIVE">
                <component>Glucose</component>
                <property>MCnc</property>
                <timing>Pt</timing>
                <system>Ser/Plas</system>
                <scale>Qn</scale>
                <class>CHEM</class>
                <orderObs>Both</orderObs>
                <longName>Glucose [Mass/volume] in Serum or Plasma</longName>
                <translation language="nl-NL">
                    <component>glucose</component>
                    <property>massaconcentratie</property>
                    <timing>moment</timing>
                    <system>serum of plasma</system>
                    <scale>kwantitatief</scale>
                    <class>chemie</class>
                    <longName>glucose [massa/volume] in serum of plasma</longName>
                </translation>
                <references>
                    <a href="https://example.org/loinc/{GLUCOSE}">https://example.org/loinc/{GLUCOSE}</a>
                </references>
            </loincConcept>
            <materials>
                <material code="{SERUM}" displayName="Serum specimen (specimen)"/>
            </materials>
            <outcomes>
                <valueSet ref="{ORDINAL_OID}"/>
            </outcomes>
            <units>
                <unit ref="1"/>
            </units>
        </lab_concept>
"#
    )
}

/// The retired sodium concept.
fn old_sodium() -> String {
    format!(
        r#"        <lab_concept status="retired">
            <loincConcept loinc_num="{OLD_SODIUM}" status="DEPRECATED">
                <component>Sodium</component>
                <property>SCnc</property>
                <timing>Pt</timing>
                <system>Ser</system>
                <scale>Qn</scale>
                <class>CHEM</class>
                <orderObs>Observation</orderObs>
                <longName>Deprecated Sodium [Moles/volume] in Serum</longName>
                <map from="{OLD_SODIUM}" to="{SODIUM}" comment="use the serum or plasma term"/>
                <references>
                    <a href="https://example.org/loinc/{OLD_SODIUM}">https://example.org/loinc/{OLD_SODIUM}</a>
                </references>
            </loincConcept>
            <materials>
                <material code="{SERUM}" displayName="Serum specimen (specimen)"/>
            </materials>
            <units>
                <unit ref="2"/>
            </units>
            <retired-reason>Afgeraden voor gebruik</retired-reason>
            <retired-replacement>{SODIUM}</retired-replacement>
            <releasenote>Vervangen in januari 2026</releasenote>
        </lab_concept>
"#
    )
}

/// The active culture panel concept.
fn culture() -> String {
    format!(
        r#"        <lab_concept status="active">
            <loincConcept loinc_num="{CULTURE}" status="ACTIVE">
                <component>Bacteria identified</component>
                <property>Prid</property>
                <timing>Pt</timing>
                <system>XXX</system>
                <scale>Nom</scale>
                <method>Culture</method>
                <class>MICRO</class>
                <orderObs>Both</orderObs>
                <panelType>Panel</panelType>
                <longName>Bacteria identified in Specimen by Culture</longName>
                <translation language="nl-NL">
                    <component>bacterie</component>
                    <property>identificator</property>
                    <timing>moment</timing>
                    <system>XXX</system>
                    <scale>nominaal</scale>
                    <method>kweek</method>
                    <class>microbiologie</class>
                    <longName>bacterie [identificator] d.m.v. kweek</longName>
                </translation>
                <references>
                    <a href="https://example.org/loinc/{CULTURE}">https://example.org/loinc/{CULTURE}</a>
                </references>
            </loincConcept>
            <materials>
                <material code="123038009" displayName="Specimen (specimen)"/>
            </materials>
            <outcomes>
                <refset conceptId="{REFSET}" preferredTerm="referentieset voor micro-organismen" src="https://example.org/refset/{REFSET}"/>
            </outcomes>
        </lab_concept>
"#
    )
}

/// The material and unit tables, the ordinal and nominal lists, and the
/// document's end.
fn tables() -> String {
    format!(
        r#"    </lab_concepts>
    <map>
        <material code="{SERUM}" displayName="Serum specimen (specimen)" system="Ser"/>
        <material code="123038009" displayName="Specimen (specimen)" system="XXX"/>
    </map>
    <units>
        <unit id="1" status="active">
            <rm>mmol/L</rm>
            <name>millimole per liter</name>
            <nlname>millimol per liter</nlname>
        </unit>
        <unit id="2" status="retired">
            <rm>mg/dL</rm>
            <name>milligram per deciliter</name>
            <nlname>milligram per deciliter</nlname>
        </unit>
    </units>
    <ordinals>
        <valueSet id="{ORDINAL_OID}" effectiveDate="2016-09-30T12:00:00" name="ordinale-uitslag" displayName="Ordinale uitslagenlijst" statusCode="final">
            <conceptList>
                <concept code="260373001" codeSystem="2.16.840.1.113883.6.96" displayName="Detected (qualifier value)" level="0" type="L">
                    <desc language="nl-NL">Aangetoond</desc>
                </concept>
                <concept code="260415000" codeSystem="2.16.840.1.113883.6.96" codeSystemName="SNOMED CT" displayName="Not detected (qualifier value)" level="0" type="L">
                    <desc language="nl-NL">Niet aangetoond</desc>
                </concept>
            </conceptList>
        </valueSet>
    </ordinals>
    <nominals>
        <refset conceptId="{REFSET}" preferredTerm="referentieset voor micro-organismen" src="https://example.org/refset/{REFSET}"/>
    </nominals>
</publication>
"#
    )
}

/// Writes the publication as a release directory holding the document;
/// returns the document's path.
///
/// # Errors
///
/// Returns the I/O error of the write.
pub fn write_publication(dir: &Path) -> std::io::Result<PathBuf> {
    let root = dir.join("Labcodeset v2026-01");
    std::fs::create_dir_all(&root)?;
    let document = root.join(format!("labconcepts-{RELEASE}.xml"));
    std::fs::write(&document, document_text())?;
    Ok(document)
}

fn document_text() -> String {
    document()
}

/// Builds the publication into a FHIR resource directory under `dir`.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_resources(dir: &Path) -> std::io::Result<PathBuf> {
    let release = tempfile::tempdir()?;
    let document = write_publication(release.path())?;
    let publication = ::labcodeset::read(&document).map_err(std::io::Error::other)?;
    ferroterm_build::labcodeset::build(&publication, dir)
        .map(|report| report.dir)
        .map_err(std::io::Error::other)
}
