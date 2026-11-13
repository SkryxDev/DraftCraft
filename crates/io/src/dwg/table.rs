//! ACAD_TABLE (§20.4.98, and from AutoCAD 2010 on the table content the entity embeds): the
//! block reference it is drawn as, then its columns, rows and cells. The drawing keeps the
//! sizes, each cell's text, merges and text height; DraftCraft's own extended data (title and
//! header rows, styles, formulas, breaks) is read by the DXF reader's rule.
//!
//! The table's references sit between its fields (a pre-2010 cell's text is there only when its
//! value names no object), so this reads both streams together: the entity's common references
//! are read first.

use draftcraft_doc::TableCell;
use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::file::ObjectReader;
use draftcraft_dwg::records::{Error, Result as DwgResult};

/// Most rows, columns, cells, contents, borders or list items of one table that are read.
const MAX_ITEMS: u32 = 1_000_000;

/// A table's cells as read, before DraftCraft's extended data is applied.
pub(super) struct TableRead {
    pub col_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    pub cells: Vec<Vec<TableCell>>,
    /// The TABLESTYLE the table names.
    pub style: u64,
    /// What of the table the drawing has no place for.
    pub partial: Vec<&'static str>,
}

fn bounded(r: &mut ObjectReader, n: u32, min_bits: usize, what: &str) -> DwgResult<usize> {
    if n > MAX_ITEMS {
        return Err(Error::format(format!("too many {what}s: {n}")));
    }
    r.count(n, min_bits, what)
}

/// A colour with its name and book (a "true colour" record, whatever the version).
fn true_color(r: &mut ObjectReader) -> DwgResult<()> {
    let _index = r.bs()?;
    let _rgb = r.bl()?;
    let flags = r.rc()?;
    if flags & 1 != 0 {
        let _name = r.text()?;
    }
    if flags & 2 != 0 {
        let _book = r.text()?;
    }
    Ok(())
}

/// A text a cell value holds: a byte count, then the bytes (UTF-16 with its terminator from
/// AutoCAD 2007 on, the file's code page before).
fn value_text(r: &mut ObjectReader) -> DwgResult<String> {
    let n = r.bl()?;
    let n = bounded(r, n, 8, "value byte")?;
    let bytes = r.bytes(n)?;
    if r.version() >= V::R2007 {
        let units: Vec<u16> = bytes
            .get(..n.saturating_sub(2))
            .unwrap_or(&[])
            .chunks(2)
            .map(|c| u16::from_le_bytes([c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0)]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    } else {
        let end = if bytes.last() == Some(&0) { n.saturating_sub(1) } else { n };
        Ok(r.ansi(bytes.get(..end).unwrap_or(&[])))
    }
}

/// A cell value: its type and data, then from AutoCAD 2007 on its unit, format and the value as
/// shown. Returns the text it shows: a string's, else the formatted value.
fn value(r: &mut ObjectReader, modern: bool) -> DwgResult<String> {
    let since_2007 = r.version() >= V::R2007;
    let flags = if since_2007 { r.bl()? } else { 0 };
    let code = r.bl()?;
    let kind = if modern { code } else { code & !0x200 };
    let mut text = None;
    if !since_2007 || flags & 1 == 0 {
        match kind {
            0 | 1 => {
                let _long = r.bl()?;
            }
            2 => {
                let _double = r.bd()?;
            }
            4 | 0x200 => text = Some(value_text(r)?),
            8 => {
                let n = r.bl()?;
                let n = bounded(r, n, 8, "date byte")?;
                r.skip_bytes(n)?;
            }
            0x10 => {
                if r.bl()? > 0 {
                    let _point = r.rd2()?;
                }
            }
            0x20 => {
                if r.bl()? > 0 {
                    let _point = (r.rd()?, r.rd()?, r.rd()?);
                }
            }
            0x40 => {
                let _object = r.reference()?;
            }
            _ => {}
        }
    }
    if since_2007 {
        let unit = r.bl()?;
        let _format = r.text()?;
        if unit != 12 {
            let formatted = r.text()?;
            if text.is_none() {
                text = Some(formatted);
            }
        }
    }
    Ok(text.unwrap_or_default())
}

/// A content format: its overrides, value type and format, rotation, scale, alignment, colour,
/// text style and text height. Returns the text height.
fn content_format(r: &mut ObjectReader) -> DwgResult<f64> {
    let _flags = (r.bl()?, r.bl()?, r.bl()?, r.bl()?);
    let _format = r.text()?;
    let _placement = (r.bd()?, r.bd()?, r.bl()?);
    true_color(r)?;
    let _text_style = r.reference()?;
    r.bd()
}

/// A cell border: overrides, type, colour, lineweight, linetype, visibility, double spacing.
fn border(r: &mut ObjectReader) -> DwgResult<()> {
    let _ = (r.bl()?, r.bl()?);
    true_color(r)?;
    let _lineweight = r.bl()?;
    let _linetype = r.reference()?;
    let _ = (r.bl()?, r.bd()?);
    Ok(())
}

/// A cell style (of a cell, row, column or the whole table): when it has data, its overrides,
/// background, layout, content format, margins and borders. Returns its text height.
fn cell_style(r: &mut ObjectReader) -> DwgResult<Option<f64>> {
    let _kind = r.bl()?;
    if r.bs()? == 0 {
        return Ok(None);
    }
    let _ = (r.bl()?, r.bl()?);
    true_color(r)?;
    let _layout = r.bl()?;
    let height = content_format(r)?;
    if r.bs()? & 1 != 0 {
        for _ in 0..6 {
            let _margin = r.bd()?;
        }
    }
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 2, "cell border")? {
        if matches!(r.bl()?, 1 | 2 | 4 | 8 | 0x10 | 0x20) {
            border(r)?;
        }
    }
    Ok(Some(height))
}

/// Custom data: a name and a value.
fn custom_data(r: &mut ObjectReader) -> DwgResult<()> {
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 4, "custom data item")? {
        let _name = r.text()?;
        value(r, true)?;
    }
    Ok(())
}

/// One cell of the table content (2010 and later).
fn cell(r: &mut ObjectReader, partial: &mut Vec<&'static str>) -> DwgResult<TableCell> {
    let _state = r.bl()?;
    let _tooltip = r.text()?;
    let _custom = r.bl()?;
    custom_data(r)?;
    if r.bl()? & 1 != 0 {
        let _link = r.reference()?;
        let _ = (r.bl()?, r.bl()?, r.bl()?);
    }
    let mut text = String::new();
    let mut text_height = None;
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 4, "cell content")? {
        match r.bl()? {
            1 => text.push_str(&value(r, true)?),
            2 => {
                let _field = r.reference()?;
                partial.push("TABLE cell field");
            }
            4 => {
                let _block = r.reference()?;
                partial.push("TABLE cell block");
            }
            _ => {}
        }
        let n = r.bl()?;
        for _ in 0..bounded(r, n, 4, "cell attribute")? {
            let _definition = r.reference()?;
            let _value = r.text()?;
            let _index = r.bl()?;
        }
        if r.bs()? != 0 {
            let h = content_format(r)?;
            text_height = text_height.or(Some(h).filter(|h| h.is_finite() && *h > 0.0 && *h < 1e9));
        }
    }
    cell_style(r)?;
    let _style_id = r.bl()?;
    if r.bl()? != 0 {
        let _ = (r.bl()?, r.bd()?, r.bd()?);
        let _geometry = r.reference()?;
        let n = r.bl()?;
        for _ in 0..bounded(r, n, 16, "cell content geometry")? {
            let _ = (r.bd3()?, r.bd3()?, r.bd()?, r.bd()?, r.bd()?, r.bd()?, r.bl()?);
        }
    }
    Ok(TableCell { text, text_height, ..TableCell::default() })
}

/// The table content (2010 and later): columns, rows of cells, fields, the table's own cell
/// style, the merged ranges and the table style.
fn content(r: &mut ObjectReader, t: &mut TableRead) -> DwgResult<()> {
    let _name = r.text()?;
    let _description = r.text()?;
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 8, "table column")? {
        let _name = r.text()?;
        let _custom = r.bl()?;
        custom_data(r)?;
        cell_style(r)?;
        let _style_id = r.bl()?;
        t.col_widths.push(r.bd()?);
    }
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 8, "table row")? {
        let n = r.bl()?;
        let mut row = Vec::new();
        for _ in 0..bounded(r, n, 16, "table cell")? {
            row.push(cell(r, &mut t.partial)?);
        }
        let _custom = r.bl()?;
        custom_data(r)?;
        cell_style(r)?;
        let _style_id = r.bl()?;
        t.row_heights.push(r.bd()?);
        t.cells.push(row);
    }
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 0, "table field")? {
        let _field = r.reference()?;
    }
    cell_style(r)?;
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 8, "merged range")? {
        let [top, left, bottom, right] = [r.bl()?, r.bl()?, r.bl()?, r.bl()?];
        let rows = bottom.checked_sub(top).and_then(|d| d.checked_add(1));
        let cols = right.checked_sub(left).and_then(|d| d.checked_add(1));
        let at = t.cells.get_mut(usize::try_from(top).unwrap_or(usize::MAX)).and_then(|row| row.get_mut(usize::try_from(left).unwrap_or(usize::MAX)));
        if let (Some(c), Some(rows), Some(cols)) = (at, rows, cols) {
            c.merged = Some((rows, cols));
        }
    }
    t.style = r.reference()?;
    Ok(())
}

/// One cell before 2010: its type, edges, merge, rotation, value (text, or a block with
/// attribute values) and the overrides it has of its style.
fn legacy_cell(r: &mut ObjectReader, partial: &mut Vec<&'static str>) -> DwgResult<TableCell> {
    let ver = r.version();
    let kind = r.bs()?;
    let _edges = r.rc()?;
    let _merged = r.b()?;
    let _autofit = r.b()?;
    let (merge_width, merge_height) = (r.bl()?, r.bl()?);
    let _rotation = r.bd()?;
    let value_object = r.reference()?;
    let mut text = String::new();
    if kind == 2 {
        let _scale = r.bd()?;
        if r.b()? {
            let n = u32::from(r.bs()?);
            for _ in 0..bounded(r, n, 4, "cell attribute")? {
                let _definition = r.reference()?;
                let _index = r.bs()?;
                let _value = r.text()?;
            }
        }
        partial.push("TABLE cell block");
    } else if value_object == 0 && ver < V::R2007 {
        text = r.text()?;
    } else if value_object != 0 {
        partial.push("TABLE cell field");
    }
    let mut text_height = None;
    if r.b()? {
        let flags = r.bl()?;
        let _virtual_edge = r.rc()?;
        if flags & 0x01 != 0 {
            let _alignment = r.bs()?;
        }
        if flags & 0x02 != 0 {
            let _no_fill = r.b()?;
        }
        if flags & 0x04 != 0 {
            true_color(r)?;
        }
        if flags & 0x08 != 0 {
            true_color(r)?;
        }
        if flags & 0x10 != 0 {
            let _text_style = r.reference()?;
        }
        if flags & 0x20 != 0 {
            text_height = Some(r.bd()?).filter(|h| h.is_finite() && *h > 0.0 && *h < 1e9);
        }
        for (color, lineweight) in [(0x40, 0x400), (0x80, 0x800), (0x100, 0x1000), (0x200, 0x2000)] {
            if flags & color != 0 {
                true_color(r)?;
            }
            if flags & lineweight != 0 {
                let _ = (r.bs()?, r.bs()?);
            }
        }
    }
    if ver >= V::R2007 {
        let _flag = r.bl()?;
        let shown = value(r, ver >= V::R2007)?;
        if text.is_empty() {
            text = shown;
        }
    }
    let merged = (merge_height >= 1 && merge_width >= 1 && (merge_height, merge_width) != (1, 1)).then_some((merge_height, merge_width));
    Ok(TableCell { text, merged, text_height, ..TableCell::default() })
}

/// The table's own fields after its block reference's, up to the end of the record.
pub(super) fn read(r: &mut ObjectReader) -> DwgResult<TableRead> {
    let mut t = TableRead { col_widths: Vec::new(), row_heights: Vec::new(), cells: Vec::new(), style: 0, partial: Vec::new() };
    if r.version() >= V::R2010 {
        modern(r, &mut t)?
    } else {
        legacy(r, &mut t)?
    }
    Ok(t)
}

/// The table from AutoCAD 2010 on: a few fields, the table content, the direction and the
/// break settings.
fn modern(r: &mut ObjectReader, t: &mut TableRead) -> DwgResult<()> {
    let _unknown = r.rc()?;
    let _object = r.reference()?;
    let _ = r.bl()?;
    if r.version() >= V::R2013 {
        let _ = r.bl()?;
    } else {
        let _ = r.b()?;
    }
    content(r, t)?;
    let _unknown = r.bs()?;
    let _direction = r.bd3()?;
    if r.bl()? == 1 {
        let _ = (r.bl()?, r.bl()?, r.bd()?, r.bl()?, r.bl()?);
        let n = r.bl()?;
        for _ in 0..bounded(r, n, 10, "table break")? {
            let _ = (r.bd3()?, r.bd()?, r.bl()?);
        }
    }
    let n = r.bl()?;
    for _ in 0..bounded(r, n, 10, "table break range")? {
        let _ = (r.bd3()?, r.bl()?, r.bl()?);
    }
    if t.cells.iter().any(|row| row.len() != t.col_widths.len()) {
        return Err(Error::format("table rows of other lengths than its columns"));
    }
    Ok(())
}

/// The table before 2010: its sizes, style and cells, then its overrides.
fn legacy(r: &mut ObjectReader, t: &mut TableRead) -> DwgResult<()> {
    let _flags = r.bs()?;
    let _direction = r.bd3()?;
    let (cols, rows) = (r.bl()?, r.bl()?);
    let cells = u64::from(cols).saturating_mul(u64::from(rows));
    if cells > u64::from(MAX_ITEMS) {
        return Err(Error::format(format!("table of {rows} × {cols} cells")));
    }
    let cols = bounded(r, cols, 2, "table column")?;
    let rows = bounded(r, rows, 2, "table row")?;
    for _ in 0..cols {
        t.col_widths.push(r.bd()?);
    }
    for _ in 0..rows {
        t.row_heights.push(r.bd()?);
    }
    t.style = r.reference()?;
    for _ in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for _ in 0..cols {
            row.push(legacy_cell(r, &mut t.partial)?);
        }
        t.cells.push(row);
    }
    legacy_overrides(r)
}

/// The overrides of the table style, then of its borders' colours, lineweights and visibility,
/// each there when its flag is: none of these is kept by the drawing.
fn legacy_overrides(r: &mut ObjectReader) -> DwgResult<()> {
    if r.b()? {
        let flags = r.bl()?;
        let has = |bits: &[u32]| bits.iter().filter(|b| flags & **b != 0).count();
        for _ in 0..has(&[0x01, 0x02]) {
            let _ = r.b()?;
        }
        if flags & 0x04 != 0 {
            let _ = r.bs()?;
        }
        for _ in 0..has(&[0x08, 0x10]) {
            let _ = r.bd()?;
        }
        for _ in 0..has(&[0x20, 0x40, 0x80]) {
            true_color(r)?;
        }
        for _ in 0..has(&[0x100, 0x200, 0x400]) {
            let _ = r.b()?;
        }
        for _ in 0..has(&[0x800, 0x1000, 0x2000]) {
            true_color(r)?;
        }
        for _ in 0..has(&[0x4000, 0x8000, 0x10000]) {
            let _ = r.bs()?;
        }
        for _ in 0..has(&[0x20000, 0x40000, 0x80000]) {
            let _ = r.reference()?;
        }
        for _ in 0..has(&[0x100000, 0x200000, 0x400000]) {
            let _ = r.bd()?;
        }
    }
    if r.b()? {
        let flags = r.bl()?;
        for _ in 0..(0..18).filter(|bit| flags & (1 << bit) != 0).count() {
            true_color(r)?;
        }
    }
    for _ in 0..2 {
        if r.b()? {
            let flags = r.bl()?;
            for _ in 0..(0..18).filter(|bit| flags & (1 << bit) != 0).count() {
                let _ = r.bs()?;
            }
        }
    }
    Ok(())
}
