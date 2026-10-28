use anyhow::{anyhow, bail, Result};
use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::Workbook;
use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn names(path: &Path) -> Result<Vec<String>> {
    Ok(open_workbook_auto(path)?.sheet_names().to_vec())
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("Conversion canceled");
    }
    Ok(())
}

pub fn delimited_to_excel(
    input: &Path,
    output: &Path,
    delimiter: u8,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .from_path(input)?;
    let mut book = Workbook::new();
    let sheet = book.add_worksheet_with_constant_memory()?;
    sheet.set_name("Data")?;
    for (row, record) in reader.records().enumerate() {
        cancelled(cancel)?;
        let row = u32::try_from(row)?;
        for (column, text) in record?.iter().enumerate() {
            // CSV has no type schema: keep identifiers, leading zeros and formula-looking text verbatim.
            sheet.write_string(row, u16::try_from(column)?, text)?;
        }
    }
    cancelled(cancel)?;
    book.save(output)?;
    Ok(())
}

pub fn records_to_excel(
    value: &serde_json::Value,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    let rows = value
        .as_array()
        .ok_or_else(|| anyhow!("Excel output needs an array of flat records"))?;
    let mut headers = std::collections::BTreeSet::new();
    for row in rows {
        for (key, value) in row
            .as_object()
            .ok_or_else(|| anyhow!("Each row must be an object"))?
        {
            if value.is_object() || value.is_array() {
                bail!("Nested records cannot be flattened into Excel without losing structure.");
            }
            headers.insert(key.clone());
        }
    }
    let mut book = Workbook::new();
    let sheet = book.add_worksheet_with_constant_memory()?;
    sheet.set_name("Data")?;
    for (col, key) in headers.iter().enumerate() {
        sheet.write_string(0, u16::try_from(col)?, key)?;
    }
    for (row, record) in rows.iter().enumerate() {
        cancelled(cancel)?;
        for (col, key) in headers.iter().enumerate() {
            let (row, col) = (u32::try_from(row + 1)?, u16::try_from(col)?);
            match &record[key] {
                serde_json::Value::Null => {}
                serde_json::Value::Bool(value) => {
                    sheet.write_boolean(row, col, *value)?;
                }
                serde_json::Value::Number(value) => {
                    let text = value.to_string();
                    if text.chars().filter(|c| c.is_ascii_digit()).count() > 15 {
                        sheet.write_string(row, col, text)?;
                    } else {
                        sheet.write_number(
                            row,
                            col,
                            value
                                .as_f64()
                                .ok_or_else(|| anyhow!("Number cannot be represented in Excel"))?,
                        )?;
                    }
                }
                serde_json::Value::String(value) => {
                    sheet.write_string(row, col, value)?;
                }
                _ => unreachable!(),
            }
        }
    }
    book.save(output)?;
    Ok(())
}

fn text(value: &Data) -> String {
    match value {
        Data::Empty => String::new(),
        Data::DateTime(value) => value
            .as_datetime()
            .map(|v| v.to_string())
            .unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    }
}

fn write_csv<W: Write>(
    range: &calamine::Range<Data>,
    writer: W,
    delimiter: u8,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(writer);
    let start = range.start().unwrap_or((0, 0));
    for _ in 0..start.0 {
        cancelled(cancel)?;
        writer.write_record(std::iter::repeat_n(
            "",
            range.end().map(|p| p.1 as usize + 1).unwrap_or(0),
        ))?;
    }
    for row in range.rows() {
        cancelled(cancel)?;
        writer.write_record(
            std::iter::repeat_n(String::new(), start.1 as usize).chain(row.iter().map(text)),
        )?;
    }
    writer.flush()?;
    Ok(())
}

pub fn export(input: &Path, output: &Path, format: &str, cancel: &AtomicBool) -> Result<()> {
    let mut book = open_workbook_auto(input)?;
    let names = book.sheet_names().to_vec();
    if !format.ends_with(".zip") && format != "json" && names.len() != 1 {
        bail!("This workbook has multiple sheets. Choose CSV sheets or TSV sheets to preserve all of them.");
    }
    let delimiter = if format.starts_with("tsv") {
        b'\t'
    } else {
        b','
    };
    let mut archive = if format.ends_with(".zip") {
        Some(zip::ZipWriter::new(File::create(output)?))
    } else {
        None
    };
    let mut json = if format == "json" {
        Some(std::io::BufWriter::new(File::create(output)?))
    } else {
        None
    };
    if let Some(json) = &mut json {
        json.write_all(b"{\n")?;
    }
    for (index, name) in names.iter().enumerate() {
        cancelled(cancel)?;
        let range = book.worksheet_range(name)?;
        let formulas = book.worksheet_formula(name)?;
        for cell in formulas.used_cells() {
            if !cell.2.is_empty()
                && range
                    .get_value((
                        cell.0 as u32 + formulas.start().unwrap().0,
                        cell.1 as u32 + formulas.start().unwrap().1,
                    ))
                    .is_none_or(|v| *v == Data::Empty)
            {
                bail!("A formula in {name} has no cached result. Recalculate and save the workbook before exporting values.");
            }
        }
        if let Some(json) = &mut json {
            if index > 0 {
                json.write_all(b",\n")?;
            }
            serde_json::to_writer(&mut *json, name)?;
            json.write_all(b": [\n")?;
            let start = range.start().unwrap_or((0, 0));
            let mut first = true;
            for _ in 0..start.0 {
                cancelled(cancel)?;
                if !first {
                    json.write_all(b",\n")?;
                }
                json.write_all(b"[]")?;
                first = false;
            }
            for row in range.rows() {
                cancelled(cancel)?;
                if !first {
                    json.write_all(b",\n")?;
                }
                first = false;
                let mut cells = vec![serde_json::Value::Null; start.1 as usize];
                cells.extend(row.iter().map(|cell| match cell {
                    Data::Empty => serde_json::Value::Null,
                    Data::Bool(v) => serde_json::Value::Bool(*v),
                    Data::Int(v) => serde_json::json!(v),
                    Data::Float(v) if v.is_finite() => serde_json::json!(v),
                    _ => serde_json::Value::String(text(cell)),
                }));
                serde_json::to_writer(&mut *json, &cells)?;
            }
            json.write_all(b"\n]")?;
        } else if let Some(archive) = &mut archive {
            let safe = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            archive.start_file(
                format!(
                    "{:02}-{safe}.{}",
                    index + 1,
                    if delimiter == b'\t' { "tsv" } else { "csv" }
                ),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )?;
            write_csv(&range, archive, delimiter, cancel)?;
        } else {
            write_csv(&range, File::create(output)?, delimiter, cancel)?;
        }
    }
    if let Some(archive) = archive {
        archive.finish()?;
    }
    if let Some(json) = &mut json {
        json.write_all(b"\n}")?;
        json.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_preserves_both_sheets_and_cached_formula_results() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("book.xlsx");
        let archive = dir.path().join("sheets.csv.zip");
        let mut book = Workbook::new();
        book.add_worksheet()
            .set_name("First")
            .unwrap()
            .write_string(0, 0, "0042")
            .unwrap();
        book.add_worksheet()
            .set_name("Second")
            .unwrap()
            .write_formula(
                0,
                0,
                rust_xlsxwriter::Formula::new("=40+2").set_result("42"),
            )
            .unwrap();
        book.save(&source).unwrap();
        export(&source, &archive, "csv.zip", &AtomicBool::new(false)).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        use std::io::Read;
        let mut text = String::new();
        zip.by_name("02-Second.csv")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "42\n");
        assert!(export(
            &source,
            &dir.path().join("bad.csv"),
            "csv",
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn csv_excel_preserves_leading_zeros_and_formula_text() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("input.csv");
        let out = dir.path().join("out.xlsx");
        std::fs::write(&source, "id,formula\n0042,=1+1\n").unwrap();
        delimited_to_excel(&source, &out, b',', &AtomicBool::new(false)).unwrap();
        let range = open_workbook_auto(out)
            .unwrap()
            .worksheet_range("Data")
            .unwrap();
        assert_eq!(range.get_value((1, 0)), Some(&Data::String("0042".into())));
        assert_eq!(range.get_value((1, 1)), Some(&Data::String("=1+1".into())));
    }
}
