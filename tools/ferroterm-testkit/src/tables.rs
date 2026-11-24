// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic publisher releases, with invented codes and terms.
//!
//! Each is in its publisher's distribution format: NPU (the IFCC code file),
//! ICD-O-3 (the WHO topography and morphology tables), EMDN (the Commission's
//! workbook), ISCO-08 (the ILO structure workbook), Orphanet (the Orphadata
//! `product1` file), DICOM (NEMA's `dcm.owl`), and EDQM (a saved Standard
//! Terms API response). `write_xlsx` is a minimal Office Open XML workbook
//! writer.

use std::io::Write;
use std::path::Path;

/// The release every fixture here claims.
pub const VERSION: &str = "2099";

/// Writes a one-sheet workbook named `sheet` to `path`, the first row of
/// `rows` as shared strings and every other cell as an inline string.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be written.
pub fn write_xlsx(path: &Path, sheet: &str, rows: &[&[&str]]) -> std::io::Result<()> {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let column = |i: usize| char::from(b'A'.saturating_add(u8::try_from(i).unwrap_or(25)));
    let mut shared = Vec::new();
    let mut body: Vec<String> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let n = r.saturating_add(1);
        body.push(format!("<row r=\"{n}\">"));
        for (c, cell) in row.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            let at = format!("{}{n}", column(c));
            if r == 0 {
                body.push(format!("<c r=\"{at}\" t=\"s\"><v>{}</v></c>", shared.len()));
                shared.push(format!("<si><r><t>{}</t></r></si>", escape(cell)));
            } else {
                body.push(format!(
                    "<c r=\"{at}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                    escape(cell)
                ));
            }
        }
        body.push(String::from("</row>"));
    }
    let ns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let parts = [
        (
            "xl/workbook.xml",
            format!(
                "<workbook xmlns=\"{ns}\" xmlns:r=\"{rel}\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
                escape(sheet)
            ),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            String::from(
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>",
            ),
        ),
        (
            "xl/sharedStrings.xml",
            format!("<sst xmlns=\"{ns}\">{}</sst>", shared.concat()),
        ),
        (
            "xl/worksheets/sheet1.xml",
            format!(
                "<worksheet xmlns=\"{ns}\"><sheetData>{}</sheetData></worksheet>",
                body.concat()
            ),
        ),
    ];
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path)?);
    let options: zip::write::FileOptions<'_, '_, ()> = zip::write::FileOptions::default();
    for (name, text) in parts {
        writer
            .start_file(name, options)
            .map_err(std::io::Error::other)?;
        writer.write_all(text.as_bytes())?;
    }
    writer.finish().map_err(std::io::Error::other)?;
    Ok(())
}

/// NPU: an active code, a retired one replaced by it, and its full definition.
pub mod npu {
    /// The active code.
    pub const ACTIVE: &str = "NPU99001";
    /// The retired code.
    pub const RETIRED: &str = "NPU99002";
    /// The short definition of the active code.
    pub const SHORT: &str = "P\u{2014}Synthion; subst.c. = ? mmol/L";

    /// The code file, with the IFCC columns.
    #[must_use]
    pub fn csv() -> String {
        [
            "id,created_date,change_date,change_comment,npu_code,short_definition,system_short,sys_spec_short,component_short,comp_spec_short,kind_of_property_short,proc_short,unit_short,full_definition,system,sys_spec,component,kind_of_property,proc,unit,specialty,context_dependent,npu_group,scale_type,replaces,replaced_by,effective_from,effective_to,active,current_version",
            &format!(
                "1,\"2090-01-01 00:00:00\",\"2091-01-01 00:00:00\",\"Name changed\",{ACTIVE},\"{SHORT}\",P,,Synthion,,subst.c.,,mmol/L,\"Plasma\u{2014}Synthion; substance concentration = ? millimole per litre\",Plasma,,Synthion,\"substance concentration\",,\"millimole per litre\",CLC,,,Ratio,{RETIRED},,\"2090-01-01 00:00:00\",\"0000-00-00 00:00:00\",1,true"
            ),
            &format!(
                "2,\"2080-01-01 00:00:00\",\"2090-01-01 00:00:00\",\"Retired\",{RETIRED},\"P\u{2014}Synthion; mass c. = ? g/L\",P,,Synthion,,mass c.,,g/L,\"Plasma\u{2014}Synthion; mass concentration = ? gram per litre\",Plasma,,Synthion,\"mass concentration\",,\"gram per litre\",CLC,,,Ratio,,{ACTIVE},\"2080-01-01 00:00:00\",\"2090-01-01 00:00:00\",0,true"
            ),
        ]
        .join("\n")
    }
}

/// ICD-O-3: a topography category with a subcategory, and a morphology code
/// with a synonym and a related term.
pub mod icdo {
    /// The topography category.
    pub const CATEGORY: &str = "C99";
    /// The topography subcategory, written without its period in the table.
    pub const SUBCATEGORY: &str = "C99.1";
    /// The morphology code, written without its slash in the table.
    pub const MORPHOLOGY: &str = "9999/3";

    /// The topography table (`Kode`, `Lvl`, `Title`), tab-delimited.
    #[must_use]
    pub fn topography() -> String {
        [
            "Kode\tLvl\tTitle",
            "C99\t3\tSYNTHETIC ORGAN",
            "C991\t4\tUpper synthetic organ",
            "C991\tincl\tSynthetic apex",
        ]
        .join("\n")
    }

    /// The morphology table (`ICDO3.2`, `Level`, `Term`), as rows for a workbook.
    #[must_use]
    pub fn morphology() -> Vec<[&'static str; 3]> {
        vec![
            ["ICDO3.2", "Level", "Term"],
            ["99993", "Preferred", "Synthetic carcinoma, NOS"],
            ["99993", "Synonym", "Synthoma"],
            ["99993", "Related", "Invented neoplasm"],
        ]
    }
}

/// EMDN: a category with two levels below it, under a title row.
pub mod emdn {
    /// The category.
    pub const CATEGORY: &str = "Z";
    /// The group.
    pub const GROUP: &str = "Z01";
    /// The terminal type.
    pub const TYPE: &str = "Z0101";

    /// The workbook rows, a title row first as the Commission writes it.
    #[must_use]
    pub fn rows() -> Vec<[&'static str; 6]> {
        vec![
            ["", "", "EMDN_v.2099", "", "", ""],
            [
                "CATEGORY",
                "CATEGORY DESCRIPTION (v.2099)",
                "CODE",
                "TERM EMDN (v.2099)",
                "LEVEL",
                "TERMINAL LEVEL (YES/NO)",
            ],
            [
                "Z",
                "SYNTHETIC DEVICES",
                "Z",
                "SYNTHETIC DEVICES",
                "1",
                "NO",
            ],
            [
                "Z",
                "SYNTHETIC DEVICES",
                "Z01",
                "SYNTHETIC PROBES",
                "2",
                "NO",
            ],
            [
                "Z",
                "SYNTHETIC DEVICES",
                "Z0101",
                "SYNTHETIC PROBES, STERILE",
                "3",
                "YES",
            ],
        ]
    }
}

/// ISCO-08: a major group down to a unit group.
pub mod isco {
    /// The major group.
    pub const MAJOR: &str = "9";
    /// The unit group.
    pub const UNIT: &str = "9991";

    /// The workbook rows with the ILO columns.
    #[must_use]
    pub fn rows() -> Vec<[&'static str; 8]> {
        vec![
            [
                "Level",
                "ISCO 08 Code",
                "Title EN",
                "Definition",
                "Tasks include",
                "Included occupations",
                "Excluded occupations",
                "Notes",
            ],
            [
                "1",
                "9",
                "Synthetic Workers",
                "Workers who synthesize.",
                "",
                "",
                "",
                "",
            ],
            ["2", "99", "Synthesis Workers", "", "", "", "", ""],
            ["3", "999", "Synthesis Assistants", "", "", "", "", ""],
            [
                "4",
                "9991",
                "Synthesis Helpers",
                "Helpers assist synthesis.",
                "Tasks include stirring.",
                "Stirrer",
                "",
                "",
            ],
        ]
    }
}

/// Orphanet: an active disease with a synonym in two languages, and an
/// obsolete entity.
pub mod orpha {
    /// The active disease.
    pub const DISEASE: &str = "999901";
    /// The obsolete entity.
    pub const OBSOLETE: &str = "999902";

    fn disorder(code: &str, lang: &str, name: &str, synonym: &str, flags: &str) -> String {
        format!(
            "<Disorder id=\"1\"><OrphaCode>{code}</OrphaCode><Name lang=\"{lang}\">{name}</Name>\
             <DisorderFlagList count=\"1\">{flags}</DisorderFlagList>\
             <SynonymList count=\"1\"><Synonym lang=\"{lang}\">{synonym}</Synonym></SynonymList>\
             <DisorderType id=\"21394\"><Name lang=\"{lang}\">Disease</Name></DisorderType>\
             <DisorderGroup id=\"36547\"><Name lang=\"{lang}\">Disorder</Name></DisorderGroup>\
             <DisorderDisorderAssociationList count=\"0\"/></Disorder>"
        )
    }

    /// The `<lang>_product1.xml` document in `lang`, dated 2099-01-31.
    #[must_use]
    pub fn product(lang: &str) -> String {
        let active = "<DisorderFlag id=\"475\"><Value>1</Value><Label></Label></DisorderFlag>";
        let obsolete = "<DisorderFlag id=\"495\"><Value>8192</Value><Label>Inactive</Label></DisorderFlag>\
                        <DisorderFlag id=\"455\"><Value>16</Value><Label>Obsolete entity</Label></DisorderFlag>";
        let (name, synonym, old) = if lang == "fr" {
            ("Syndrome synthétique", "Synthose", "Ancien syndrome")
        } else {
            ("Synthetic syndrome", "Synthosis", "Former syndrome")
        };
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <JDBOR date=\"2099-01-31 07:53:50\" version=\"1.3.42\" copyright=\"Synthetic\">\
             <DisorderList count=\"2\">{}{}</DisorderList></JDBOR>",
            disorder(DISEASE, lang, name, synonym, active),
            disorder(OBSOLETE, lang, old, "Old synthosis", obsolete)
        )
    }
}

/// DICOM: a current code with a definition and a retired one replaced by it.
pub mod dicom {
    /// The edition the ontology names.
    pub const EDITION: &str = "2099a_20990101";
    /// The current code.
    pub const CURRENT: &str = "999001";
    /// The retired code.
    pub const RETIRED: &str = "ZZ";

    /// The canonical of the synthetic context group, in the shape of NEMA's
    /// PS3.16 context group value sets.
    pub const CONTEXT_GROUP: &str =
        "http://dicom.nema.org/medical/dicom/current/output/chtml/part16/sect_CID_99999.html";

    /// Writes a context group `ValueSet` over [`CURRENT`] into `dir`, shaped
    /// like the FHIR value sets NEMA publishes for PS3.16
    /// (<https://dicom.nema.org/medical/dicom/resources/valuesets/fhir/json/>).
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the file cannot be written.
    pub fn write_context_group(dir: &std::path::Path) -> std::io::Result<()> {
        let group = serde_json::json!({
            "resourceType": "ValueSet", "id": "dicom-cid-99999-SyntheticAcquisition",
            "url": CONTEXT_GROUP, "version": "20990101", "name": "SyntheticAcquisition",
            "status": "active", "experimental": false, "publisher": "Synthetic",
            "compose": {"include": [{"system": "http://dicom.nema.org/resources/ontology/DCM",
                "concept": [{"code": CURRENT, "display": "Synthetic acquisition"}]}]}
        });
        std::fs::write(
            dir.join("ValueSet-dicom-cid-99999-SyntheticAcquisition.json"),
            group.to_string(),
        )
    }

    /// The `dcm.owl` document.
    #[must_use]
    pub fn owl() -> String {
        let base = "http://dicom.nema.org/resources/ontology/DCM";
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <rdf:RDF xmlns:skos=\"http://www.w3.org/2004/02/skos/core#\" xmlns:owl=\"http://www.w3.org/2002/07/owl#\" \
             xmlns:rdfs=\"http://www.w3.org/2000/01/rdf-schema#\" xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"{base}/\"><rdf:type rdf:resource=\"http://www.w3.org/2002/07/owl#Ontology\"/>\
             <rdfs:label>DCM</rdfs:label><owl:versionInfo>{EDITION}</owl:versionInfo></rdf:Description>\
             <rdf:Description rdf:about=\"{base}/{CURRENT}\"><rdf:type rdf:resource=\"http://www.w3.org/2002/07/owl#Class\"/>\
             <skos:notation>{CURRENT}</skos:notation><skos:prefLabel xml:lang=\"en\">Synthetic acquisition</skos:prefLabel>\
             <skos:definition xml:lang=\"en\">An invented acquisition.</skos:definition></rdf:Description>\
             <rdf:Description rdf:about=\"{base}/{RETIRED}\"><rdf:type rdf:resource=\"http://www.w3.org/2002/07/owl#Class\"/>\
             <skos:notation>{RETIRED}</skos:notation><skos:prefLabel xml:lang=\"en\">Old synthetic modality</skos:prefLabel>\
             <owl:deprecated>true</owl:deprecated><skos:changeNote xml:lang=\"en\">Retired</skos:changeNote>\
             <owl:sameAs rdf:resource=\"{base}/{CURRENT}\"/></rdf:Description></rdf:RDF>"
        )
    }
}

/// EDQM: a current dose form linked to a basic dose form, and a deprecated
/// route, with French translations.
pub mod edqm {
    /// The current pharmaceutical dose form.
    pub const DOSE_FORM: &str = "99990000";
    /// The deprecated route.
    pub const ROUTE: &str = "99991000";
    /// The newest modification date, the version when none is given.
    pub const MODIFIED: &str = "2099-02-01";

    /// A saved `full_data_by_class` response with both terms.
    #[must_use]
    pub fn response() -> String {
        serde_json::json!({"content": [
            {"class": "PDF", "code": DOSE_FORM, "english": "Synthetic tablet", "comment": null,
             "status": "Current", "domain": "Human and Veterinary", "version": 1,
             "creation_date": "2098-01-01 00:00:00", "modification_date": "2099-02-01 10:00:00",
             "definition": "An invented solid dose form.",
             "translations": [{"language": "fr", "region": "", "term": "Comprimé synthétique"}],
             "links": {"BDF": [{"code": "99992000", "term": "Synthetic solid"}]}},
            {"class": "ROA", "code": ROUTE, "english": "Synthetic use", "status": "Deprecated",
             "domain": "Human", "version": 2, "creation_date": "2097-01-01 00:00:00",
             "modification_date": "2098-06-01 00:00:00", "translations": []}
        ]})
        .to_string()
    }
}

/// EMA SPOR: one RMS list with a current and a non-current term, an OMS
/// organisation, an SMS substance, and a PMS product, each a saved API response.
pub mod spor {
    /// The RMS list id.
    pub const LIST: &str = "999999000001";
    /// The current RMS term.
    pub const TERM: &str = "999999000011";
    /// The non-current RMS term.
    pub const OLD_TERM: &str = "999999000012";
    /// The OMS organisation.
    pub const ORGANISATION: &str = "ORG-999999001";
    /// The SMS substance.
    pub const SUBSTANCE: &str = "999999000101";
    /// The PMS product's MPID.
    pub const PRODUCT: &str = "EU-999-MPID-1";

    fn term(id: &str, en: &str, de: &str, status: &str, modified: &str) -> String {
        format!(
            "<controlled-term spor:rowid=\"1\"><term-id id=\"{id}\"><link href=\"http://spor.ema.europa.eu/v1/lists/{LIST}/terms/{id}\"/></term-id>\
             <term-names><term-name><name translation-status-code=\"CURRENT\" lang=\"en\">{en}</name></term-name>\
             <term-name><name translation-status-code=\"CURRENT\" lang=\"de\">{de}</name></term-name></term-names>\
             <short-names><short-name><name lang=\"en\">{id}S</name></short-name></short-names>\
             <status code=\"{status}\" modified-by=\"ema\" modified-on=\"{modified}\"/>\
             <operational-attributes version-number=\"1\" modified-on=\"{modified}\" created-on=\"2090-01-01T00:00:00Z\"/>\
             <list-reference id=\"{LIST}\" name=\"Synthetic list\" status=\"CURRENT\"/></controlled-term>"
        )
    }

    /// The RMS terms of the list (SPOR API v1 XML).
    #[must_use]
    pub fn rms() -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><terms xmlns=\"http://ema.europa.eu/schema/spor\" xmlns:spor=\"http://ema.europa.eu/schema/spor\">{}{}</terms>",
            term(
                TERM,
                "Synthetic use",
                "Synthetische Anwendung",
                "CURRENT",
                "2099-03-01T10:00:00Z"
            ),
            term(
                OLD_TERM,
                "Former synthetic use",
                "Alte Anwendung",
                "NON_CURRENT",
                "2098-01-01T10:00:00Z"
            )
        )
    }

    /// An OMS organisation (SPOR API v1 JSON).
    #[must_use]
    pub fn oms() -> String {
        serde_json::json!({"organisations": {"organisation": [{
            "operational-attributes": {"created-on": "2090-01-01T00:00:00Z", "modified-on": "2099-02-01T00:00:00Z"},
            "organisation-id": {"id": ORGANISATION, "link": {"href": "http://spor.ema.europa.eu/v1/organisations/ORG-999999001"}},
            "status": "ACTIVE", "name": "Synthetic Medicines Agency"
        }]}})
        .to_string()
    }

    /// An SMS substance (SPOR API v2 FHIR `Bundle`).
    #[must_use]
    pub fn sms() -> String {
        serde_json::json!({"resourceType": "Bundle", "type": "searchset", "entry": [{"resource": {
            "resourceType": "SubstanceDefinition", "id": "s1",
            "meta": {"lastUpdated": "2099-04-01T00:00:00Z"},
            "identifier": [{"system": "http://example.org/sms", "value": SUBSTANCE}],
            "status": {"coding": [{"code": "200000005004", "display": "Current"}]},
            "name": [{"name": "SYNTHETIC ACID", "preferred": true}, {"name": "Acidum syntheticum"}]
        }}]})
        .to_string()
    }

    /// A PMS product (SPOR API v2 FHIR `Bundle`).
    #[must_use]
    pub fn pms() -> String {
        serde_json::json!({"resourceType": "Bundle", "type": "searchset", "entry": [{"resource": {
            "resourceType": "MedicinalProductDefinition", "id": "p1",
            "meta": {"lastUpdated": "2099-05-01T00:00:00Z"},
            "identifier": [{"system": "http://ema.europa.eu/fhir/MPID", "value": PRODUCT}],
            "name": [{"productName": "Synthomab 10 mg tablets"}]
        }}]})
        .to_string()
    }
}

/// SNOMED CT GPS: a freeset over the testkit edition, two active concepts
/// and one inactive row.
pub mod gps {
    use crate::snomed::{CAT, DOG, FISH, item, sctid};

    /// The file name, with the release date SNOMED International writes into it.
    pub const FILE: &str = "SnomedINTL_GPSRelease_PRODUCTION_20990101T120000Z.txt";
    /// The release the file name carries.
    pub const RELEASE: &str = "20990101";

    /// The active concepts of the freeset.
    #[must_use]
    pub fn active() -> [String; 2] {
        [sctid(item(CAT)), sctid(item(DOG))]
    }

    /// The freeset: concept id, active flag, FSN, and US English preferred term.
    #[must_use]
    pub fn freeset() -> String {
        let [cat, dog] = active();
        let fish = sctid(item(FISH));
        [
            String::from("id\tactive\tfsn\tpt"),
            format!("{cat}\t1\tCat (organism)\tCat"),
            format!("{dog}\t1\tDog (organism)\tDog"),
            format!("{fish}\t0\tFish (organism)\tFish"),
        ]
        .join("\n")
    }

    /// Builds the freeset at `freeset` into the value set under `<out>/gps`.
    ///
    /// # Errors
    ///
    /// Returns an I/O error wrapping the build failure.
    pub fn write_value_set(
        freeset: &std::path::Path,
        out: &std::path::Path,
    ) -> std::io::Result<()> {
        ferroterm_build::gps::build(freeset, None, out)
            .map(|_| ())
            .map_err(std::io::Error::other)
    }
}

/// Writes every release into `dir`.
///
/// The files are `npu-codes.csv`, `Topoenglish.txt`, `icdo-morphology.xlsx`,
/// `emdn.xlsx`, `isco08.xlsx`, `en_product1.xml`, `fr_product1.xml`,
/// `dcm.owl`, `edqm.json`, the GPS freeset ([`gps::FILE`]), and the SPOR
/// responses `spor-rms.xml`, `spor-oms.json`, `spor-sms.json`, and
/// `spor-pms.json`.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_releases(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join("npu-codes.csv"), npu::csv())?;
    std::fs::write(dir.join("Topoenglish.txt"), icdo::topography())?;
    let morphology = icdo::morphology();
    let rows: Vec<&[&str]> = morphology.iter().map(<[&str; 3]>::as_slice).collect();
    write_xlsx(&dir.join("icdo-morphology.xlsx"), "Morphology", &rows)?;
    let emdn = emdn::rows();
    let rows: Vec<&[&str]> = emdn.iter().map(<[&str; 6]>::as_slice).collect();
    write_xlsx(&dir.join("emdn.xlsx"), "EMDN_v.2099", &rows)?;
    let isco = isco::rows();
    let rows: Vec<&[&str]> = isco.iter().map(<[&str; 8]>::as_slice).collect();
    write_xlsx(&dir.join("isco08.xlsx"), "ISCO-08", &rows)?;
    std::fs::write(dir.join("en_product1.xml"), orpha::product("en"))?;
    std::fs::write(dir.join("fr_product1.xml"), orpha::product("fr"))?;
    std::fs::write(dir.join("dcm.owl"), dicom::owl())?;
    std::fs::write(dir.join("edqm.json"), edqm::response())?;
    std::fs::write(dir.join(gps::FILE), gps::freeset())?;
    std::fs::write(dir.join("spor-rms.xml"), spor::rms())?;
    std::fs::write(dir.join("spor-oms.json"), spor::oms())?;
    std::fs::write(dir.join("spor-sms.json"), spor::sms())?;
    std::fs::write(dir.join("spor-pms.json"), spor::pms())?;
    Ok(())
}

/// The artifact directory names [`write_artifacts`] writes, one per release.
pub const ARTIFACTS: [&str; 11] = [
    "npu", "icdo", "emdn", "isco", "orpha", "dicom", "edqm", "spor-rms", "spor-oms", "spor-sms",
    "spor-pms",
];

/// Builds every release into its directory under `dir` (see [`ARTIFACTS`])
/// through the `ferroterm-build` release flags.
///
/// # Errors
///
/// Returns an I/O error wrapping a read or build failure.
pub fn write_artifacts(dir: &Path) -> std::io::Result<()> {
    use ferroterm_build::releases::Releases;
    let release = tempfile::tempdir()?;
    let r = release.path();
    write_releases(r)?;
    let builds = [
        Releases {
            npu: Some(r.join("npu-codes.csv")),
            npu_version: Some(VERSION.to_owned()),
            ..Releases::default()
        },
        Releases {
            icdo: vec![r.join("Topoenglish.txt"), r.join("icdo-morphology.xlsx")],
            icdo_version: Some(VERSION.to_owned()),
            ..Releases::default()
        },
        Releases {
            emdn: Some(r.join("emdn.xlsx")),
            ..Releases::default()
        },
        Releases {
            isco: Some(r.join("isco08.xlsx")),
            isco_version: Some(VERSION.to_owned()),
            ..Releases::default()
        },
        Releases {
            orpha: vec![r.join("en_product1.xml"), r.join("fr_product1.xml")],
            ..Releases::default()
        },
        Releases {
            dicom: Some(r.join("dcm.owl")),
            ..Releases::default()
        },
        Releases {
            edqm: vec![r.join("edqm.json")],
            ..Releases::default()
        },
        Releases {
            spor: vec![r.join("spor-rms.xml")],
            ..Releases::default()
        },
        Releases {
            spor: vec![r.join("spor-oms.json")],
            ..Releases::default()
        },
        Releases {
            spor: vec![r.join("spor-sms.json")],
            ..Releases::default()
        },
        Releases {
            spor: vec![r.join("spor-pms.json")],
            ..Releases::default()
        },
    ];
    for (releases, name) in builds.iter().zip(ARTIFACTS) {
        releases
            .build(&dir.join(name))
            .map_err(std::io::Error::other)?;
    }
    Ok(())
}
