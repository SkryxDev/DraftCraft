//! ATTOUT / ATTIN (the Express Tools' Export and Import Attribute Information): the attribute
//! values of block references written to a tab-delimited text file, one row per reference, to be
//! edited in a spreadsheet and read back.
//!
//! The file: a header row `HANDLE`, `BLOCKNAME`, then one column per attribute tag (the tags of
//! every reference written, in the order first met); then one row per block reference: its handle
//! with a leading apostrophe (so a spreadsheet keeps it as text), its block name and its values.
//! A cell is empty when that reference has no such tag. A block with the same tag twice names the
//! second column `TAG(2)`. A value holding a tab, a quote or a line break is quoted, a quote inside
//! it doubled. ATTOUT writes UTF-8 with a byte order mark and CRLF line ends; ATTIN also reads
//! UTF-16 (a spreadsheet's "Unicode text") and Windows-1252 text, LF or CR line ends.
//!
//! ATTIN reads every row, then changes the references it names by handle, all in one undo step.
//! Rows whose handle is missing or isn't a block reference are skipped; tags a file doesn't have,
//! and cells holding `<>`, are left alone; references on locked layers are left alone.

use draftcraft_doc::{Drawing, EntityKind, Handle, Insert};
use serde_json::{Value, json};

use super::attedit::set_value;
use crate::cmd::curves::is_locked;
use crate::cmd::machines::{SelOutcome, SelectPhase};
use crate::cmd::{CommandSpec, bad, str_param, targets};
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

/// Largest file ATTIN reads, and ATTOUT writes (bytes).
const MAX_BYTES: usize = 32 * 1024 * 1024;
/// Most rows in a file (the header included).
const MAX_ROWS: usize = 200_000;
/// Most columns in a row (HANDLE and BLOCKNAME included).
const MAX_COLS: usize = 2_000;
/// Longest cell (bytes).
const MAX_CELL: usize = 65_536;

/// A cell meaning "this reference has no such attribute": left alone by ATTIN.
const NO_ATTRIBUTE: &str = "<>";

pub(super) fn attout_spec() -> CommandSpec {
    CommandSpec::new("attout", "Export Attribute Information...", run_attout)
        .menu(&["Tools", "Express Tools", "Export Attribute Information..."])
        .params(
            "{path?: file to write (tab-delimited .txt; omitted: the text comes back as `text`), handles?: block references (default: the selection, else every block reference with attributes in the current space)} → {references, tags, file?, text?}",
        )
        .noundo()
        .interactive(|_| Ok(Box::new(AttoutM::default())))
}

pub(super) fn attin_spec() -> CommandSpec {
    CommandSpec::new("attin", "Import Attribute Information...", run_attin)
        .menu(&["Tools", "Express Tools", "Import Attribute Information..."])
        .params(
            "{path: file written by ATTOUT (or `text`: its contents), handles?: only these block references may change} → {references, values, skipped, locked, unknownTags, message}",
        )
        .interactive(|_| Ok(Box::new(AttinM)))
}

// ---------------- the file ----------------

/// The column key of each editable attribute of `ins`, by attribute index: its tag in capitals,
/// `TAG(n)` for the n-th attribute with the same tag. Attributes with no tag have none.
fn keys(ins: &Insert) -> Vec<(usize, String)> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut out = Vec::new();
    for (k, a) in ins.attribs.iter().enumerate() {
        let tag = a.tag.trim().to_uppercase();
        if a.constant || tag.is_empty() {
            continue;
        }
        let n = match seen.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, n)) => {
                *n = n.saturating_add(1);
                *n
            }
            None => {
                seen.push((tag.clone(), 1));
                1
            }
        };
        out.push((k, if n == 1 { tag } else { format!("{tag}({n})") }));
    }
    out
}

/// `text` as a cell: quoted when it holds a tab, a quote or a line break.
fn cell(text: &str) -> String {
    if text.contains(['\t', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}

/// A block reference's name as written: its block's.
fn block_name(d: &Drawing, ins: &Insert) -> String {
    d.block(&ins.block).map(|b| b.name.clone()).unwrap_or_else(|| ins.block.clone())
}

/// The block references with editable attributes among `hs` (each once, in order).
fn references(d: &Drawing, hs: &[Handle]) -> Vec<Handle> {
    let mut out: Vec<Handle> = Vec::new();
    for h in hs {
        if out.contains(h) {
            continue;
        }
        if let Some(EntityKind::Insert(ins)) = d.entity(*h).map(|e| &e.kind)
            && !keys(ins).is_empty()
        {
            out.push(*h);
        }
    }
    out
}

/// The file for block references `hs`: its text and the tag columns.
fn attout_text(d: &Drawing, hs: &[Handle]) -> Result<(String, Vec<String>)> {
    let mut tags: Vec<String> = Vec::new();
    let mut rows: Vec<(Handle, String, Vec<(String, String)>)> = Vec::new();
    for h in hs {
        let Some(EntityKind::Insert(ins)) = d.entity(*h).map(|e| &e.kind) else { continue };
        let values: Vec<(String, String)> =
            keys(ins).into_iter().filter_map(|(k, key)| ins.attribs.get(k).map(|a| (key, a.text.value.clone()))).collect();
        for (key, _) in &values {
            if !tags.contains(key) {
                tags.push(key.clone());
            }
        }
        rows.push((*h, block_name(d, ins), values));
    }
    if rows.len() >= MAX_ROWS || tags.len().saturating_add(2) > MAX_COLS {
        return Err(EngineError::Other(format!(
            "ATTOUT: too much for one file ({} block references, {} tags; at most {} and {}).",
            rows.len(),
            tags.len(),
            MAX_ROWS - 1,
            MAX_COLS - 2
        )));
    }
    // A byte order mark, so a spreadsheet reads the file as UTF-8.
    let mut out = String::from("\u{feff}");
    let header: Vec<String> = ["HANDLE", "BLOCKNAME"].iter().map(|t| t.to_string()).chain(tags.iter().map(|t| cell(t))).collect();
    out.push_str(&header.join("\t"));
    out.push_str("\r\n");
    for (h, name, values) in &rows {
        let mut line = vec![format!("'{}", h.hex()), cell(name)];
        line.extend(tags.iter().map(|t| values.iter().find(|(k, _)| k == t).map(|(_, v)| cell(v)).unwrap_or_default()));
        out.push_str(&line.join("\t"));
        out.push_str("\r\n");
        if out.len() > MAX_BYTES {
            return Err(EngineError::Other(format!("ATTOUT: the file would be larger than {} MB.", MAX_BYTES / (1024 * 1024))));
        }
    }
    Ok((out, tags))
}

/// The text of a file: UTF-8 (with or without a byte order mark), UTF-16 with a byte order mark,
/// or else Windows-1252, as a spreadsheet saves "text" on Windows.
fn decode(bytes: &[u8]) -> String {
    let text = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        encoding_rs::UTF_16LE.decode_without_bom_handling(rest).0.into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        encoding_rs::UTF_16BE.decode_without_bom_handling(rest).0.into_owned()
    } else {
        match std::str::from_utf8(bytes) {
            Ok(t) => t.to_string(),
            Err(_) => encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned(),
        }
    };
    match text.strip_prefix('\u{feff}') {
        Some(t) => t.to_string(),
        None => text,
    }
}

fn too_big(what: String) -> EngineError {
    EngineError::Other(format!("ATTIN: {what}"))
}

/// The rows of tab-delimited `text`. A cell starting with a quote runs to the closing quote (tabs
/// and line breaks inside it are part of the value, `""` is one quote); CRLF, LF and CR end rows.
/// Blank lines are dropped.
fn parse(text: &str) -> Result<Vec<Vec<String>>> {
    if text.len() > MAX_BYTES {
        return Err(too_big(format!("the file is larger than {} MB", MAX_BYTES / (1024 * 1024))));
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    // At the start of a cell (a quote there opens a quoted value).
    let mut fresh = true;
    let push_cell = |row: &mut Vec<String>, cur: &mut String, line: usize| -> Result<()> {
        if row.len() >= MAX_COLS {
            return Err(too_big(format!("line {line} has more than {MAX_COLS} columns")));
        }
        row.push(std::mem::take(cur));
        Ok(())
    };
    let push_row = |rows: &mut Vec<Vec<String>>, row: &mut Vec<String>| -> Result<()> {
        let r = std::mem::take(row);
        if r.len() == 1 && r.first().is_some_and(|c| c.trim().is_empty()) {
            return Ok(()); // a blank line
        }
        if rows.len() >= MAX_ROWS {
            return Err(too_big(format!("the file has more than {MAX_ROWS} rows")));
        }
        rows.push(r);
        Ok(())
    };
    while let Some(c) = chars.next() {
        match c {
            '"' if fresh => {
                let start = line;
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            cur.push('"');
                        }
                        Some('"') => break,
                        Some(ch) => {
                            if ch == '\n' {
                                line = line.saturating_add(1);
                            }
                            cur.push(ch);
                        }
                        None => return Err(EngineError::Other(format!("ATTIN: line {start}: a quoted value is never closed"))),
                    }
                    if cur.len() > MAX_CELL {
                        return Err(too_big(format!("line {start}: a value is longer than {MAX_CELL} bytes")));
                    }
                }
                fresh = false;
            }
            '\t' => {
                push_cell(&mut row, &mut cur, line)?;
                fresh = true;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                push_cell(&mut row, &mut cur, line)?;
                push_row(&mut rows, &mut row)?;
                line = line.saturating_add(1);
                fresh = true;
            }
            _ => {
                cur.push(c);
                fresh = false;
                if cur.len() > MAX_CELL {
                    return Err(too_big(format!("line {line}: a value is longer than {MAX_CELL} bytes")));
                }
            }
        }
    }
    if !fresh || !cur.is_empty() || !row.is_empty() {
        push_cell(&mut row, &mut cur, line)?;
        push_row(&mut rows, &mut row)?;
    }
    Ok(rows)
}

/// A value as an attribute holds it: no control characters but tabs, and line breaks only in a
/// multiline attribute (a space in a single-line one).
fn clean(value: &str, multiline: bool) -> String {
    let value = value.replace("\r\n", "\n");
    value
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' if multiline => Some('\n'),
            '\n' | '\r' => Some(' '),
            '\t' => Some('\t'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// A handle cell: hex digits, perhaps after an apostrophe.
fn handle_cell(text: &str) -> Option<Handle> {
    let t = text.trim();
    Handle::parse_hex(t.strip_prefix('\'').unwrap_or(t).trim())
}

/// What ATTIN did.
#[derive(Debug, Default)]
struct Imported {
    references: usize,
    values: usize,
    skipped: usize,
    locked: usize,
    unknown_tags: Vec<String>,
}

impl Imported {
    fn message(&self) -> String {
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut m = format!(
            "{} updated ({}).",
            plural(self.references, "block reference", "block references"),
            plural(self.values, "attribute value", "attribute values")
        );
        if self.skipped > 0 {
            m.push_str(&format!(" {} skipped: no block reference with that handle.", plural(self.skipped, "row", "rows")));
        }
        if self.locked > 0 {
            m.push_str(&format!(" {} on locked layers left alone.", plural(self.locked, "block reference", "block references")));
        }
        m
    }
}

/// Apply rows read from a file (the header first). `only`: just these references may change.
fn import(s: &mut Session, rows: &[Vec<String>], only: Option<&[Handle]>) -> Result<Imported> {
    let Some((header, body)) = rows.split_first() else { return Err(EngineError::Other("ATTIN: the file is empty.".into())) };
    let names: Vec<String> = header.iter().map(|h| h.trim().to_uppercase()).collect();
    let handle_col = names
        .iter()
        .position(|n| n == "HANDLE")
        .ok_or_else(|| EngineError::Other("ATTIN: the file has no HANDLE column; write it with ATTOUT.".into()))?;
    // The tag columns: everything but HANDLE and BLOCKNAME, each name once.
    let mut tag_cols: Vec<(usize, &str)> = Vec::new();
    for (c, n) in names.iter().enumerate() {
        if c != handle_col && n != "BLOCKNAME" && !n.is_empty() && !tag_cols.iter().any(|(_, t)| *t == n.as_str()) {
            tag_cols.push((c, n.as_str()));
        }
    }
    let mut used = vec![false; tag_cols.len()];
    let mut out = Imported::default();
    // Work out every change first, then make them.
    let mut changes: Vec<(Handle, Vec<(usize, String)>)> = Vec::new();
    {
        let d = s.doc()?;
        for row in body {
            let Some(h) = row.get(handle_col).and_then(|c| handle_cell(c)) else {
                out.skipped = out.skipped.saturating_add(1);
                continue;
            };
            let Some(EntityKind::Insert(ins)) = d.entity(h).map(|e| &e.kind) else {
                out.skipped = out.skipped.saturating_add(1);
                continue;
            };
            if only.is_some_and(|o| !o.contains(&h)) {
                out.skipped = out.skipped.saturating_add(1);
                continue;
            }
            let keys = keys(ins);
            let mut edits: Vec<(usize, String)> = Vec::new();
            for (n, (c, tag)) in tag_cols.iter().enumerate() {
                let Some(raw) = row.get(*c) else { continue };
                let Some((k, _)) = keys.iter().find(|(_, key)| key == tag) else { continue };
                if let Some(u) = used.get_mut(n) {
                    *u = true;
                }
                if raw.trim() == NO_ATTRIBUTE {
                    continue;
                }
                let Some(a) = ins.attribs.get(*k) else { continue };
                let value = clean(raw, a.multiline.is_some());
                if value != a.text.value {
                    edits.push((*k, value));
                }
            }
            if !edits.is_empty() {
                changes.push((h, edits));
            }
        }
    }
    out.unknown_tags = tag_cols.iter().zip(&used).filter(|(_, u)| !**u).map(|((_, t), _)| t.to_string()).collect();
    // A reference named twice: its last row wins.
    let mut merged: Vec<(Handle, Vec<(usize, String)>)> = Vec::new();
    for (h, edits) in changes {
        match merged.iter_mut().find(|(m, _)| *m == h) {
            Some((_, m)) => {
                for (k, v) in edits {
                    match m.iter_mut().find(|(mk, _)| *mk == k) {
                        Some(e) => e.1 = v,
                        None => m.push((k, v)),
                    }
                }
            }
            None => merged.push((h, edits)),
        }
    }
    for (h, edits) in merged {
        if is_locked(s, h) || attribute_on_locked_layer(s, h, &edits) {
            out.locked = out.locked.saturating_add(1);
            continue;
        }
        let mut n = 0usize;
        s.doc_mut()?.modify_entity(h, |e| {
            if let EntityKind::Insert(ins) = &mut e.kind {
                for (k, v) in &edits {
                    if let Some(a) = ins.attribs.get_mut(*k)
                        && a.text.value != *v
                    {
                        set_value(&mut a.text, v.clone());
                        n = n.saturating_add(1);
                    }
                }
            }
        })?;
        if n > 0 {
            out.references = out.references.saturating_add(1);
            out.values = out.values.saturating_add(n);
        }
    }
    Ok(out)
}

/// Whether one of the attributes `edits` would change is on a locked layer of its own.
fn attribute_on_locked_layer(s: &Session, h: Handle, edits: &[(usize, String)]) -> bool {
    let Ok(d) = s.doc() else { return false };
    let Some(EntityKind::Insert(ins)) = d.entity(h).map(|e| &e.kind) else { return false };
    edits.iter().any(|(k, _)| ins.attribs.get(*k).and_then(|a| a.props.layer.as_deref()).and_then(|l| d.layer(l)).is_some_and(|l| l.locked))
}

// ---------------- files ----------------

fn write_file(path: &str, text: &str) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::atomic_write::write_atomic_making_folders(std::path::Path::new(path), text.as_bytes())
            .map_err(|e| bad("attout", format!("{path}: {e}")))
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::cmd::exchange::deliver_file("attout", path, text.as_bytes(), crate::cmd::file::deliver())
    }
}

fn read_file(path: &str) -> Result<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::capped_read::read_capped(std::path::Path::new(path), MAX_BYTES).map_err(|e| bad("attin", format!("{path}: {e}")))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(bad("attin", format!("{path}: files can't be read in the browser; pass the file's contents as `text`")))
    }
}

/// A typed file name without the quotes around it.
fn unquoted(t: &str) -> &str {
    let t = t.trim();
    t.strip_prefix('"').and_then(|u| u.strip_suffix('"')).unwrap_or(t)
}

/// The default file: the drawing's name with a .txt extension.
fn default_file(s: &Session) -> String {
    let Ok(st) = s.state() else { return "attributes.txt".into() };
    let base = st.path.clone().unwrap_or_else(|| st.title.clone());
    std::path::Path::new(&base).with_extension("txt").to_string_lossy().into_owned()
}

// ---------------- the commands ----------------

/// The block references ATTOUT writes: `handles`, else the selection, else every one with
/// attributes in the current space.
fn attout_targets(s: &Session, p: &Value) -> Result<Vec<Handle>> {
    let mut hs = targets(s, p)?;
    if hs.is_empty() && p.get("handles").is_none() && p.get("handle").is_none() {
        let d = s.doc()?;
        hs = d.space(&s.space()).map(|st| st.iter().map(|e| e.handle).collect()).unwrap_or_default();
    }
    Ok(references(s.doc()?, &hs))
}

fn attout(s: &mut Session, hs: &[Handle], path: Option<&str>) -> Result<Value> {
    if hs.is_empty() {
        return Err(EngineError::Other("ATTOUT: no block references with attributes.".into()));
    }
    let (text, tags) = attout_text(s.doc()?, hs)?;
    let mut r = json!({ "references": hs.len(), "tags": tags });
    match path {
        Some(path) => {
            write_file(path, &text)?;
            r["file"] = json!(path);
            r["message"] = json!(format!("{} block reference{} written to {path}.", hs.len(), if hs.len() == 1 { "" } else { "s" }));
        }
        None => r["text"] = json!(text.strip_prefix('\u{feff}').unwrap_or(&text)),
    }
    Ok(r)
}

fn run_attout(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = attout_targets(s, p)?;
    let path = str_param(p, "path").map(unquoted).filter(|f| !f.is_empty());
    attout(s, &hs, path)
}

fn run_attin(s: &mut Session, p: &Value) -> Result<Value> {
    let text = match (str_param(p, "text"), str_param(p, "path").map(unquoted).filter(|f| !f.is_empty())) {
        (Some(t), _) => t.strip_prefix('\u{feff}').unwrap_or(t).to_string(),
        (None, Some(path)) => decode(&read_file(path)?),
        (None, None) => return Err(bad("attin", "`path` (a file written by ATTOUT) or `text` is required")),
    };
    let only = if p.get("handles").is_some() || p.get("handle").is_some() { Some(targets(s, p)?) } else { None };
    let rows = parse(&text)?;
    let r = import(s, &rows, only.as_deref())?;
    Ok(json!({
        "references": r.references,
        "values": r.values,
        "skipped": r.skipped,
        "locked": r.locked,
        "unknownTags": r.unknown_tags,
        "message": r.message(),
    }))
}

/// ATTOUT at the command line: the file name, then the block references.
#[derive(Default)]
struct AttoutM {
    file: Option<String>,
    sel: SelectPhase,
}

impl AttoutM {
    fn finish(&self, s: &mut Session, picked: &[Handle]) -> Result<Step> {
        let hs = references(s.doc()?, picked);
        let file = self.file.clone().unwrap_or_else(|| default_file(s));
        let r = attout(s, &hs, Some(&file))?;
        if let Some(m) = r.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        s.set_selection(Vec::new());
        Ok(Step::Done)
    }
}

impl Interactive for AttoutM {
    fn name(&self) -> &'static str {
        "ATTOUT"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.sel = SelectPhase::begin(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match self.file {
            None => Prompt::new("Enter output file name", Accept::TEXT).default(default_file(s)),
            Some(_) => self.sel.prompt(),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.file.is_none() {
            let f = match &i {
                Input::Text(t) if !unquoted(t).is_empty() => unquoted(t).to_string(),
                Input::Text(_) | Input::Enter => default_file(s),
                _ => return Ok(Step::Continue),
            };
            self.file = Some(f);
            if self.sel.done {
                let hs = self.sel.picked.clone();
                return self.finish(s, &hs);
            }
            return Ok(Step::Continue);
        }
        match self.sel.feed(s, &i)? {
            SelOutcome::More => Ok(Step::Continue),
            SelOutcome::Empty => Ok(Step::Done),
            SelOutcome::Done(hs) => self.finish(s, &hs),
        }
    }
}

/// ATTIN at the command line: the file name.
#[derive(Default)]
struct AttinM;

impl Interactive for AttinM {
    fn name(&self) -> &'static str {
        "ATTIN"
    }
    fn prompt(&self, s: &Session) -> Prompt {
        Prompt::new("Enter input file name", Accept::TEXT).default(default_file(s))
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let f = match &i {
            Input::Text(t) if !unquoted(t).is_empty() => unquoted(t).to_string(),
            Input::Text(_) | Input::Enter => default_file(s),
            _ => return Ok(Step::Continue),
        };
        // The whole file is applied in this one step: one undo step.
        let r = run_attin(s, &json!({ "path": f }))?;
        if let Some(m) = r.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(Step::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_round_trip_through_the_parser() {
        let values = ["plain", "a\tb", "say \"hi\"", "two\nlines", "", "\"", "crlf\r\nend", "<>"];
        let line: Vec<String> = values.iter().map(|v| cell(v)).collect();
        let text = format!("HANDLE\tBLOCKNAME\r\n{}\r\n", line.join("\t"));
        let rows = parse(&text).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], values.iter().map(|v| v.to_string()).collect::<Vec<_>>());
    }

    #[test]
    fn line_ends_blank_lines_and_a_missing_last_line_end() {
        let rows = parse("A\tB\rC\tD\n\n\r\nE\tF").unwrap();
        assert_eq!(rows, vec![vec!["A", "B"], vec!["C", "D"], vec!["E", "F"]]);
        // An empty cell at the end of a line is kept.
        assert_eq!(parse("A\t\n").unwrap(), vec![vec!["A".to_string(), String::new()]]);
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn hostile_text_is_refused_not_panicked_on() {
        assert!(parse("A\t\"never closed\n").is_err());
        assert!(parse(&"\t".repeat(MAX_COLS + 1)).is_err());
        assert!(parse(&"x\n".repeat(MAX_ROWS + 1)).is_err());
        assert!(parse(&"x".repeat(MAX_CELL + 1)).is_err());
        assert!(parse(&format!("\"{}\"", "x".repeat(MAX_CELL + 1))).is_err());
        // A quote inside an unquoted value is just a character.
        assert_eq!(parse("a\"b\tc").unwrap(), vec![vec!["a\"b", "c"]]);
        // Text after a closing quote stays with the value.
        assert_eq!(parse("\"a\"b").unwrap(), vec![vec!["ab"]]);
    }

    #[test]
    fn files_decode_from_utf8_utf16_and_windows_1252() {
        assert_eq!(decode(b"\xEF\xBB\xBFHANDLE"), "HANDLE");
        assert_eq!(decode("caffè".as_bytes()), "caffè");
        // "è" saved as Windows-1252 text.
        assert_eq!(decode(b"caff\xE8"), "caffè");
        let mut le = vec![0xFF, 0xFE];
        le.extend("Hè\t1".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&le), "Hè\t1");
        let mut be = vec![0xFE, 0xFF];
        be.extend("Hè".encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode(&be), "Hè");
        // An odd byte out and invalid UTF-8 after a mark don't panic.
        let _ = decode(b"\xFF\xFE\x41");
        let _ = decode(b"\xEF\xBB\xBF\xC3");
    }

    #[test]
    fn handles_values_and_messages() {
        assert_eq!(handle_cell("'1A2"), Some(Handle(0x1A2)));
        assert_eq!(handle_cell(" 1a2 "), Some(Handle(0x1A2)));
        assert_eq!(handle_cell("'"), None);
        assert_eq!(handle_cell("nope"), None);
        assert_eq!(clean("a\r\nb\u{0}c\td", false), "a bc\td");
        assert_eq!(clean("a\r\nb", true), "a\nb");
        let m = Imported { references: 1, values: 2, skipped: 3, locked: 1, unknown_tags: vec![] }.message();
        assert!(m.contains("1 block reference updated (2 attribute values)") && m.contains("3 rows skipped") && m.contains("locked"), "{m}");
    }
}
