//! ACAD_TABLE: the block reference the table is drawn as, then the table. The record is the
//! table's DXF form, which is the same in every version (the sizes, then each cell's groups from
//! its group 171). Up to AutoCAD 2007 the file holds the same fields; from AutoCAD 2010 it holds
//! the table content AutoCAD 2010 brought in (columns, rows of cells, each cell's contents with
//! their format, merged ranges), built from the same values.

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dxf::Tag;

use super::Cx;
use super::entities::begin;
use super::rec::{Groups, Rec, hex, text};

/// Most rows or columns of one table written (the reader's bound is a million cells).
const MAX_SIDE: usize = 1 << 16;
/// Most cells of one table written.
const MAX_CELLS: usize = 1_000_000;

/// The property flag of a content format or cell style that sets its own text height.
const TEXT_HEIGHT: u32 = 0x80;
/// A colour word for ByBlock.
const BYBLOCK: u32 = 0xC100_0000;

/// One cell of the record.
struct Cell<'a> {
    g: Groups<'a>,
}

impl Cell<'_> {
    /// The cell's text: its groups 2 (250-character pieces) then 1.
    fn text(&self) -> String {
        self.g.0.iter().filter(|t| t.code == 2 || t.code == 1).map(text).collect()
    }
    fn height(&self) -> Option<f64> {
        self.g.get(140).map(Tag::f64).filter(|h| h.is_finite() && *h > 0.0)
    }
    /// The rows and columns a merged cell spans (1 × 1 for a cell that isn't merged, 0 × 0 for
    /// one another cell's merge covers).
    fn span(&self) -> (u32, u32) {
        let n = |code: i32| u32::try_from(self.g.i_or(code, 1).max(0)).unwrap_or(1);
        (n(176), n(175))
    }
    fn merged(&self) -> bool {
        self.g.i(173) != 0
    }
}

/// The table's own groups: its sizes and the groups of each cell.
struct Layout<'a> {
    g: Groups<'a>,
    rows: usize,
    cols: usize,
    cells: Vec<Cell<'a>>,
}

impl Layout<'_> {
    fn cell(&self, row: usize, col: usize) -> Option<&Cell<'_>> {
        self.cells.get(row.checked_mul(self.cols)?.checked_add(col)?)
    }
    fn heights(&self) -> Vec<f64> {
        let mut v: Vec<f64> = self.g.all(141).map(Tag::f64).take(self.rows).collect();
        v.resize(self.rows, 1.0);
        v
    }
    fn widths(&self) -> Vec<f64> {
        let mut v: Vec<f64> = self.g.all(142).map(Tag::f64).take(self.cols).collect();
        v.resize(self.cols, 1.0);
        v
    }
}

fn layout<'a>(g: Groups<'a>) -> Layout<'a> {
    let first = g.0.iter().position(|t| t.code == 171).unwrap_or(g.0.len());
    let head = Groups(g.0.get(..first).unwrap_or(&[]));
    let side = |code: i32| usize::try_from(head.i(code).max(0)).unwrap_or(0).min(MAX_SIDE);
    let (mut rows, mut cols) = (side(91), side(92));
    if rows.saturating_mul(cols) > MAX_CELLS {
        (rows, cols) = (0, 0);
    }
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 171).map(|(i, _)| i).take(rows * cols).collect();
    let cells = starts
        .iter()
        .enumerate()
        .map(|(n, &i)| Cell { g: Groups(g.0.get(i..starts.get(n + 1).copied().unwrap_or(g.0.len())).unwrap_or(&[])) })
        .collect();
    Layout { g: head, rows, cols, cells }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A colour as a table holds it in every version: a "true colour" record (index, colour word,
/// no name or book).
fn true_color(w: &mut ObjectWriter, word: u32) {
    w.bs(0);
    w.bl(word);
    w.rc(0);
}

/// A text a cell value holds: its byte count, then the bytes (UTF-16 with a terminator from
/// AutoCAD 2007, the file's code page with a terminator before).
fn value_text(cx: &Cx<'_>, w: &mut ObjectWriter, s: &str) {
    let mut bytes: Vec<u8> = if cx.ver >= V::R2007 { s.encode_utf16().flat_map(u16::to_le_bytes).collect() } else { super::ansi(s, cx.codepage) };
    bytes.push(0);
    if cx.ver >= V::R2007 {
        bytes.push(0);
    }
    w.bl(count(bytes.len()));
    w.bytes(&bytes);
}

/// A cell value holding text: its flags (from 2007), its type (a string), the text, then (from
/// 2007) its unit, its format and the text as shown.
fn string_value(cx: &Cx<'_>, w: &mut ObjectWriter, s: &str) {
    if cx.ver >= V::R2007 {
        w.bl(0);
    }
    w.bl(4);
    value_text(cx, w, s);
    if cx.ver >= V::R2007 {
        w.bl(0);
        w.text("");
        w.text(s);
    }
}

pub(super) fn table(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let mut w = begin(cx, cx.class("ACAD_TABLE")?, r);
    let insert = r.sub("AcDbBlockReference");
    let t = layout(r.sub("AcDbTable"));
    // The block reference: no scale, no rotation, no attributes.
    w.bd3(insert.p3(10));
    w.bb(3);
    w.bd(0.0);
    w.bd3(insert.extrusion());
    w.b(false);
    let block = t.g.get(343).map(|h| cx.known(hex(text(h)))).filter(|h| *h != 0).unwrap_or_else(|| cx.block(insert.s(2)));
    w.reference(Ref::HardPointer, block);
    let style = t.g.get(342).map_or(0, |h| cx.known(hex(text(h))));
    if cx.ver >= V::R2010 {
        modern(cx, &mut w, r, &t, style);
    } else {
        legacy(cx, &mut w, &t, style);
    }
    Some(w)
}

/// The table up to AutoCAD 2007: its flags, direction, sizes, style, cells, and no overrides of
/// its style.
fn legacy(cx: &Cx<'_>, w: &mut ObjectWriter, t: &Layout<'_>, style: u64) {
    w.bs(u16::try_from(t.g.i(90)).unwrap_or(0));
    w.bd3(if t.g.has(11) { t.g.p3(11) } else { [1.0, 0.0, 0.0] });
    w.bl(count(t.cols));
    w.bl(count(t.rows));
    for x in t.widths() {
        w.bd(x);
    }
    for x in t.heights() {
        w.bd(x);
    }
    w.reference(Ref::HardPointer, style);
    for row in 0..t.rows {
        for col in 0..t.cols {
            legacy_cell(cx, w, t.cell(row, col));
        }
    }
    // No overrides of the table style, its border colours, lineweights or visibility.
    for _ in 0..4 {
        w.b(false);
    }
}

/// One cell up to AutoCAD 2007: a text cell, its edges, merge, rotation, text (before 2007; the
/// value holds it from 2007), and its overrides of the style: the alignment, text style and
/// text height it sets.
fn legacy_cell(cx: &Cx<'_>, w: &mut ObjectWriter, cell: Option<&Cell<'_>>) {
    let g = cell.map_or(Groups(&[]), |c| c.g);
    let contents = cell.map(Cell::text).unwrap_or_default();
    let (rows, cols) = cell.map_or((1, 1), Cell::span);
    w.bs(1);
    w.rc(u8::try_from(g.i(172)).unwrap_or(0));
    w.b(cell.is_some_and(Cell::merged));
    w.b(g.i(174) != 0);
    w.bl(cols);
    w.bl(rows);
    w.bd(g.f(145));
    w.reference(Ref::HardPointer, 0);
    if cx.ver < V::R2007 {
        w.text(&contents);
    }
    let height = cell.and_then(Cell::height);
    let align = g.get(170).map(Tag::i64);
    let text_style = g.get(7).map(|t| cx.style(text(t)));
    let mut flags = 0u32;
    if align.is_some() {
        flags |= 0x01;
    }
    if text_style.is_some() {
        flags |= 0x10;
    }
    if height.is_some() {
        flags |= 0x20;
    }
    if [63, 64, 65, 66, 68, 69].iter().any(|c| g.has(*c)) {
        cx.lose("TABLE cell colours");
    }
    w.b(flags != 0);
    if flags != 0 {
        w.bl(flags);
        w.rc(u8::try_from(g.i(178)).unwrap_or(0));
        if let Some(a) = align {
            w.bs(u16::try_from(a).unwrap_or(0));
        }
        if let Some(s) = text_style {
            w.reference(Ref::HardPointer, s);
        }
        if let Some(h) = height {
            w.bd(h);
        }
    }
    if cx.ver >= V::R2007 {
        w.bl(0);
        string_value(cx, w, &contents);
    }
}

/// Whether the drawing's title and header rows are on, from DraftCraft's extended data of the
/// table (`TABLE`, then the two flags); both on for a table from another program.
fn title_and_header(r: &Rec<'_>) -> (bool, bool) {
    let x = r.xdata;
    let Some(at) = x.iter().position(|t| t.code == 1000 && text(t) == "TABLE") else { return (true, true) };
    let flags: Vec<bool> = x.iter().skip(at + 1).take_while(|t| t.code == 1070).map(|t| t.i64() != 0).collect();
    (flags.first().copied().unwrap_or(true), flags.get(1).copied().unwrap_or(true))
}

/// The table from AutoCAD 2010: a few fields AutoCAD writes as these values, the table content,
/// the direction, and no breaks.
fn modern(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>, t: &Layout<'_>, style: u64) {
    w.rc(0);
    w.reference(Ref::SoftPointer, 0);
    w.bl(0);
    if cx.ver >= V::R2013 {
        w.bl(0);
    } else {
        w.b(true);
    }
    // The content: no name or description, the columns, the rows, no fields, no table cell
    // style, the merged ranges, the style.
    w.text("");
    w.text("");
    w.bl(count(t.cols));
    for x in t.widths() {
        w.text("");
        w.bl(0);
        w.bl(0);
        no_cell_style(w);
        w.bl(0);
        w.bd(x);
    }
    let (title, header) = title_and_header(r);
    w.bl(count(t.rows));
    let mut merges = Vec::new();
    for (row, height) in t.heights().into_iter().enumerate() {
        w.bl(count(t.cols));
        for col in 0..t.cols {
            let cell = t.cell(row, col);
            if let Some(c) = cell
                && c.merged()
            {
                let (rows, cols) = c.span();
                if rows >= 1 && cols >= 1 && (rows, cols) != (1, 1) {
                    let last = |at: usize, n: u32| u32::try_from(at).ok().and_then(|a| a.checked_add(n - 1));
                    if let (Some(bottom), Some(right)) = (last(row, rows), last(col, cols)) {
                        merges.push([count(row), count(col), bottom, right]);
                    }
                }
            }
            modern_cell(cx, w, cell);
        }
        w.bl(0);
        w.bl(0);
        no_cell_style(w);
        // The row's cell style: the title, header or data style of the table style.
        let kind = if title && row == 0 {
            1
        } else if header && row == usize::from(title) {
            2
        } else {
            3
        };
        w.bl(kind);
        w.bd(height);
    }
    w.bl(0);
    no_cell_style(w);
    w.bl(count(merges.len()));
    for m in merges {
        for v in m {
            w.bl(v);
        }
    }
    w.reference(Ref::HardPointer, style);
    w.bs(38);
    w.bd3(if t.g.has(11) { t.g.p3(11) } else { [1.0, 0.0, 0.0] });
    w.bl(0);
    w.bl(0);
}

/// A cell style with no data of its own.
fn no_cell_style(w: &mut ObjectWriter) {
    w.bl(0);
    w.bs(0);
}

/// One cell from AutoCAD 2010: no state, tooltip, custom data or link; its text as one string
/// content, with a format that sets the text height when the cell sets one; no style or
/// geometry of its own.
fn modern_cell(cx: &Cx<'_>, w: &mut ObjectWriter, cell: Option<&Cell<'_>>) {
    let contents = cell.map(Cell::text).unwrap_or_default();
    let height = cell.and_then(Cell::height);
    let text_style = cell.and_then(|c| c.g.get(7)).map(|t| cx.style(text(t)));
    let align = cell.and_then(|c| c.g.get(170)).map(|t| u32::try_from(t.i64()).unwrap_or(0));
    w.bl(0);
    w.text("");
    w.bl(0);
    w.bl(0);
    w.bl(0);
    // The contents: the text, as a string value.
    w.bl(1);
    w.bl(1);
    string_value(cx, w, &contents);
    w.bl(0);
    // Its format: the value is a string (override bits 1 and 2), and its own text height, text
    // style and alignment when the cell sets them.
    w.bs(1);
    let mut overrides = 3;
    if height.is_some() {
        overrides |= TEXT_HEIGHT;
    }
    if text_style.is_some() {
        overrides |= 0x40;
    }
    if align.is_some() {
        overrides |= 0x10;
    }
    w.bl(overrides);
    w.bl(0);
    w.bl(4);
    w.bl(0);
    w.text("");
    w.bd(0.0);
    w.bd(1.0);
    w.bl(align.unwrap_or(0));
    true_color(w, BYBLOCK);
    w.reference(Ref::HardPointer, text_style.unwrap_or(0));
    w.bd(height.unwrap_or(0.0));
    no_cell_style(w);
    w.bl(0);
    w.bl(0);
}
