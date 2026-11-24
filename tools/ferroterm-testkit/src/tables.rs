// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic publisher tables, with invented codes and terms.
//!
//! Each is in its publisher's distribution format: NPU (the IFCC code file),
//! ICD-O-3 (the WHO topography and morphology tables), EMDN (the Commission's
//! workbook), and ISCO-08 (the ILO structure workbook). `write_xlsx` is a
//! minimal Office Open XML workbook writer.

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

/// Writes every table into `dir`: `npu-codes.csv`, `Topoenglish.txt`,
/// `icdo-morphology.xlsx`, `emdn.xlsx`, and `isco08.xlsx`.
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
    Ok(())
}

/// Builds the four tables into `<dir>/npu`, `<dir>/icdo`, `<dir>/emdn`, and
/// `<dir>/isco`, each an artifact directory.
///
/// # Errors
///
/// Returns an I/O error wrapping a read or build failure.
pub fn write_artifacts(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    let r = release.path();
    write_releases(r)?;
    let other = |e: ::classification::sheet::SheetError| std::io::Error::other(e);
    let builds = [
        (
            ::classification::npu::read(&r.join("npu-codes.csv"), VERSION).map_err(other)?,
            ::classification::npu::SYSTEM,
            "npu",
        ),
        (
            ::classification::icdo::read(
                &[&r.join("Topoenglish.txt"), &r.join("icdo-morphology.xlsx")],
                VERSION,
            )
            .map_err(other)?,
            ::classification::icdo::SYSTEM,
            "icdo",
        ),
        (
            ::classification::emdn::read(&r.join("emdn.xlsx"), None).map_err(other)?,
            ::classification::emdn::SYSTEM,
            "emdn",
        ),
        (
            ::classification::isco::read(&r.join("isco08.xlsx"), Some(VERSION)).map_err(other)?,
            ::classification::isco::SYSTEM,
            "isco",
        ),
    ];
    for (classification, system, name) in builds {
        ferroterm_build::classification::build(&classification, system, None, &dir.join(name))
            .map_err(std::io::Error::other)?;
    }
    Ok(())
}
