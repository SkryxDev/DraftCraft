//! A drawing as its records: what [`crate::dxf_write`] writes for it, split into header
//! variables, classes, tables, blocks, entities and objects, each record with the parts every
//! record has (handle, owner, reactors, extension dictionary, extended data) already found.
//!
//! A record is a DXF record: its groups mean what the DXF Reference says, whichever program
//! wrote them, so AutoCAD's own DXF of a drawing splits the same way.

use std::collections::HashMap;

use draftcraft_dxf::{Tag, Value};

/// Most reactors read from one record.
const MAX_REACTORS: usize = 1 << 16;

/// A group's text, borrowed; empty for a number.
pub(super) fn text(t: &Tag) -> &str {
    match &t.value {
        Value::Str(s) | Value::Hex(s) => s,
        _ => "",
    }
}

/// A handle written in hexadecimal; 0 for anything else.
pub(super) fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim(), 16).unwrap_or(0)
}

/// Some groups of a record: a subclass's, or all of them.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Groups<'a>(pub &'a [Tag]);

impl<'a> Groups<'a> {
    pub fn get(&self, code: i32) -> Option<&'a Tag> {
        self.0.iter().find(|t| t.code == code)
    }
    pub fn has(&self, code: i32) -> bool {
        self.get(code).is_some()
    }
    pub fn all(&self, code: i32) -> impl Iterator<Item = &'a Tag> + 'a {
        self.0.iter().filter(move |t| t.code == code)
    }
    /// A number; 0 when the record has none.
    pub fn f(&self, code: i32) -> f64 {
        self.f_or(code, 0.0)
    }
    pub fn f_or(&self, code: i32, default: f64) -> f64 {
        self.get(code).map_or(default, Tag::f64)
    }
    pub fn i(&self, code: i32) -> i64 {
        self.i_or(code, 0)
    }
    pub fn i_or(&self, code: i32, default: i64) -> i64 {
        self.get(code).map_or(default, Tag::i64)
    }
    pub fn s(&self, code: i32) -> &'a str {
        self.get(code).map_or("", text)
    }
    /// A handle (a hexadecimal string group); 0 for none.
    pub fn h(&self, code: i32) -> u64 {
        hex(self.s(code))
    }
    /// A point of three coordinates at `code`, `code + 10`, `code + 20`.
    pub fn p3(&self, code: i32) -> [f64; 3] {
        [self.f(code), self.f(code + 10), self.f(code + 20)]
    }
    pub fn p2(&self, code: i32) -> [f64; 2] {
        [self.f(code), self.f(code + 10)]
    }
    /// The extrusion direction (210): +Z when the record has none.
    pub fn extrusion(&self) -> [f64; 3] {
        if self.has(210) || self.has(220) || self.has(230) { [self.f(210), self.f(220), self.f_or(230, 0.0)] } else { [0.0, 0.0, 1.0] }
    }
    /// The groups from the first with `code` on.
    pub fn from(&self, code: i32) -> Groups<'a> {
        let at = self.0.iter().position(|t| t.code == code).unwrap_or(self.0.len());
        Groups(self.0.get(at..).unwrap_or(&[]))
    }
    /// The groups after position `n`.
    pub fn skip(&self, n: usize) -> Groups<'a> {
        Groups(self.0.get(n..).unwrap_or(&[]))
    }
}

/// One record.
#[derive(Clone, Debug)]
pub(super) struct Rec<'a> {
    /// The record's type: `LINE`, `LAYER`, `DICTIONARY`.
    pub kind: &'a str,
    pub handle: u64,
    /// The object that owns this one (the first group 330 outside a `{…}` group).
    pub owner: u64,
    pub reactors: Vec<u64>,
    pub xdict: u64,
    /// The record's groups after its type, up to its extended data.
    pub tags: &'a [Tag],
    /// Its extended data: from the first group 1001.
    pub xdata: &'a [Tag],
}

impl<'a> Rec<'a> {
    /// The record whose type group is `kind` and whose other groups are `tags`.
    pub fn new(kind: &'a str, tags: &'a [Tag]) -> Rec<'a> {
        let x = tags.iter().position(|t| t.code == 1001).unwrap_or(tags.len());
        let (body, xdata) = tags.split_at(x);
        let mut r = Rec { kind, handle: 0, owner: 0, reactors: Vec::new(), xdict: 0, tags: body, xdata };
        // The handle is group 5, or 105 for a dimension style. The owner and the two groups in
        // braces come before the record's first subclass, or right after it.
        let handle_code = if kind == "DIMSTYLE" { 105 } else { 5 };
        let mut group: Option<&str> = None;
        let mut owner_seen = false;
        for t in body {
            match (t.code, group) {
                (102, _) => {
                    let name = text(t);
                    group = if name.starts_with('{') { Some(name) } else { None };
                }
                (330, Some(g)) if g.eq_ignore_ascii_case("{ACAD_REACTORS") => {
                    if r.reactors.len() < MAX_REACTORS {
                        r.reactors.push(hex(text(t)));
                    }
                }
                (360, Some(g)) if g.eq_ignore_ascii_case("{ACAD_XDICTIONARY") => r.xdict = hex(text(t)),
                (_, Some(_)) => {}
                (c, None) if c == handle_code && r.handle == 0 => r.handle = hex(text(t)),
                (330, None) if !owner_seen => {
                    r.owner = hex(text(t));
                    owner_seen = true;
                }
                _ => {}
            }
        }
        r
    }

    /// Every group of the record.
    pub fn all(&self) -> Groups<'a> {
        Groups(self.tags)
    }

    /// The groups of the subclass `marker` (after its group 100, up to the next subclass);
    /// empty when the record has no such subclass.
    pub fn sub(&self, marker: &str) -> Groups<'a> {
        let Some(at) = self.tags.iter().position(|t| t.code == 100 && text(t).eq_ignore_ascii_case(marker)) else { return Groups(&[]) };
        let rest = self.tags.get(at + 1..).unwrap_or(&[]);
        let end = rest.iter().position(|t| t.code == 100).unwrap_or(rest.len());
        Groups(rest.get(..end).unwrap_or(&[]))
    }

    /// The groups after the last of the common subclasses (`AcDbEntity`, or the record's first
    /// subclass): the record's own.
    pub fn own(&self) -> Groups<'a> {
        let at = self.tags.iter().position(|t| t.code == 100).map_or(0, |i| i + 1);
        Groups(self.tags.get(at..).unwrap_or(&[]))
    }
}

/// A table: its head record and its records.
#[derive(Clone, Debug)]
pub(super) struct Table<'a> {
    pub name: &'a str,
    pub head: Rec<'a>,
    pub records: Vec<Rec<'a>>,
}

/// A block definition: its BLOCK, its entities and its ENDBLK.
#[derive(Clone, Debug)]
pub(super) struct BlockDef<'a> {
    pub begin: Rec<'a>,
    pub entities: Vec<Rec<'a>>,
    pub end: Option<Rec<'a>>,
}

/// A class of the CLASSES section.
#[derive(Clone, Debug)]
pub(super) struct ClassRec<'a> {
    pub dxf_name: &'a str,
    pub cpp_name: &'a str,
    pub app_name: &'a str,
    pub proxy_flags: i64,
    pub instance_count: i64,
    pub was_zombie: bool,
    pub is_entity: bool,
}

/// The drawing's records.
#[derive(Clone, Debug, Default)]
pub(super) struct Doc<'a> {
    /// Header variables by name without the `$`: the groups that follow each.
    pub header: HashMap<&'a str, &'a [Tag]>,
    pub classes: Vec<ClassRec<'a>>,
    pub tables: Vec<Table<'a>>,
    pub blocks: Vec<BlockDef<'a>>,
    pub entities: Vec<Rec<'a>>,
    pub objects: Vec<Rec<'a>>,
}

/// The records of one section: (type, groups after the type).
fn records(tags: &[Tag]) -> Vec<(&str, &[Tag])> {
    let starts: Vec<usize> = tags.iter().enumerate().filter(|(_, t)| t.code == 0).map(|(i, _)| i).collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(n, &i)| {
            let end = starts.get(n + 1).copied().unwrap_or(tags.len());
            Some((text(tags.get(i)?), tags.get(i + 1..end)?))
        })
        .collect()
}

impl<'a> Doc<'a> {
    /// Split a drawing's groups into its records.
    pub fn parse(tags: &'a [Tag]) -> Doc<'a> {
        let mut doc = Doc::default();
        let mut i = 0;
        while let Some(t) = tags.get(i) {
            if !(t.code == 0 && text(t) == "SECTION") {
                i += 1;
                continue;
            }
            let name = tags.get(i + 1).filter(|t| t.code == 2).map_or("", text);
            let start = i + 2;
            let len =
                tags.get(start..).unwrap_or(&[]).iter().position(|t| t.code == 0 && text(t) == "ENDSEC").unwrap_or(tags.len().saturating_sub(start));
            let body = tags.get(start..start + len).unwrap_or(&[]);
            match name {
                "HEADER" => doc.read_header(body),
                "CLASSES" => doc.read_classes(body),
                "TABLES" => doc.read_tables(body),
                "BLOCKS" => doc.read_blocks(body),
                "ENTITIES" => doc.entities = records(body).into_iter().map(|(k, t)| Rec::new(k, t)).collect(),
                "OBJECTS" => doc.objects = records(body).into_iter().map(|(k, t)| Rec::new(k, t)).collect(),
                _ => {}
            }
            i = start + len + 1;
        }
        doc
    }

    fn read_header(&mut self, body: &'a [Tag]) {
        let starts: Vec<usize> = body.iter().enumerate().filter(|(_, t)| t.code == 9).map(|(i, _)| i).collect();
        for (n, &i) in starts.iter().enumerate() {
            let end = starts.get(n + 1).copied().unwrap_or(body.len());
            if let (Some(name), Some(values)) = (body.get(i).map(text), body.get(i + 1..end)) {
                self.header.insert(name.trim_start_matches('$'), values);
            }
        }
    }

    fn read_classes(&mut self, body: &'a [Tag]) {
        for (kind, tags) in records(body) {
            if kind != "CLASS" {
                continue;
            }
            let g = Groups(tags);
            self.classes.push(ClassRec {
                dxf_name: g.s(1),
                cpp_name: g.s(2),
                app_name: g.s(3),
                proxy_flags: g.i(90),
                instance_count: g.i(91),
                was_zombie: g.i(280) != 0,
                is_entity: g.i(281) != 0,
            });
        }
    }

    fn read_tables(&mut self, body: &'a [Tag]) {
        let mut current: Option<Table<'a>> = None;
        for (kind, tags) in records(body) {
            match kind {
                "TABLE" => {
                    if let Some(t) = current.take() {
                        self.tables.push(t);
                    }
                    let name = Groups(tags).s(2);
                    current = Some(Table { name, head: Rec::new("TABLE", tags), records: Vec::new() });
                }
                "ENDTAB" => {
                    if let Some(t) = current.take() {
                        self.tables.push(t);
                    }
                }
                _ => {
                    if let Some(t) = current.as_mut() {
                        t.records.push(Rec::new(kind, tags));
                    }
                }
            }
        }
        if let Some(t) = current {
            self.tables.push(t);
        }
    }

    fn read_blocks(&mut self, body: &'a [Tag]) {
        let mut current: Option<BlockDef<'a>> = None;
        for (kind, tags) in records(body) {
            match kind {
                "BLOCK" => {
                    if let Some(b) = current.take() {
                        self.blocks.push(b);
                    }
                    current = Some(BlockDef { begin: Rec::new(kind, tags), entities: Vec::new(), end: None });
                }
                "ENDBLK" => {
                    if let Some(mut b) = current.take() {
                        b.end = Some(Rec::new(kind, tags));
                        self.blocks.push(b);
                    }
                }
                _ => {
                    if let Some(b) = current.as_mut() {
                        b.entities.push(Rec::new(kind, tags));
                    }
                }
            }
        }
        if let Some(b) = current {
            self.blocks.push(b);
        }
    }

    pub fn table(&self, name: &str) -> Option<&Table<'a>> {
        self.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}
