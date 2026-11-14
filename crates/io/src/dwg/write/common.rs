//! What every record has: its extended data, an entity's common properties, and the structure
//! DWG states that DXF implies (what each block and polyline owns, each entity's neighbours).

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::EntityStart;
use draftcraft_dwg::file::{ColorValue, EntityLinetype, EntitySpace, XDataBlock, XDataItem};
use draftcraft_dxf::Tag;

use super::Cx;
use super::rec::{Groups, Rec, hex, text};

/// Most extended data items taken from one record.
const MAX_XDATA: usize = 1 << 20;

/// Bytes from the hexadecimal text of a binary group.
pub(super) fn bytes_of(hex_text: &str) -> Vec<u8> {
    let digits: Vec<u8> = hex_text.bytes().filter_map(|b| char::from(b).to_digit(16).and_then(|d| u8::try_from(d).ok())).collect();
    digits.as_chunks::<2>().0.iter().map(|[a, b]| a << 4 | b).collect()
}

/// A record's extended data as DWG stores it: one block per application, by the handle of its
/// APPID record. Data of an application the drawing has no record for is left out.
pub(super) fn xdata(cx: &Cx<'_>, tags: &[Tag]) -> Vec<XDataBlock> {
    let mut out: Vec<XDataBlock> = Vec::new();
    let mut skip = true;
    let mut i = 0;
    while let Some(t) = tags.get(i).filter(|_| i < MAX_XDATA) {
        i += 1;
        if t.code == 1001 {
            match cx.apps.get(&text(t).trim().to_uppercase()) {
                Some(&app) => {
                    out.push(XDataBlock { app, items: Vec::new() });
                    skip = false;
                }
                None => skip = true,
            }
            continue;
        }
        let Some(block) = out.last_mut().filter(|_| !skip) else { continue };
        let point = |i: &mut usize| {
            let y = tags.get(*i).filter(|n| n.code == t.code + 10).map(Tag::f64);
            let z = tags.get(*i + 1).filter(|n| n.code == t.code + 20).map(Tag::f64);
            *i += usize::from(y.is_some()) + usize::from(y.is_some() && z.is_some());
            [t.f64(), y.unwrap_or(0.0), z.unwrap_or(0.0)]
        };
        let item = match t.code {
            1000 => XDataItem::Text(text(t).to_string()),
            1002 => {
                if text(t).trim() == "}" {
                    XDataItem::Close
                } else {
                    XDataItem::Open
                }
            }
            1003 => XDataItem::Layer(cx.layer(text(t))),
            1004 => XDataItem::Bytes(bytes_of(text(t))),
            1005 => XDataItem::Handle(hex(text(t))),
            1010 => XDataItem::Point(point(&mut i)),
            1011 => XDataItem::Position(point(&mut i)),
            1012 => XDataItem::Displacement(point(&mut i)),
            1013 => XDataItem::Direction(point(&mut i)),
            1040 => XDataItem::Real(t.f64()),
            1041 => XDataItem::Distance(t.f64()),
            1042 => XDataItem::ScaleFactor(t.f64()),
            1070 => XDataItem::Int16(i16::try_from(t.i64()).unwrap_or_else(|_| t.i64() as i16)),
            1071 => XDataItem::Int32(i32::try_from(t.i64()).unwrap_or_else(|_| t.i64() as i32)),
            _ => continue,
        };
        block.items.push(item);
    }
    out
}

/// Lineweights in hundredths of a millimetre, by the index DWG stores.
const LINEWEIGHTS: [i64; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

/// The index DWG stores for a lineweight (group 370): 29 ByLayer, 30 ByBlock, 31 Default.
pub(super) fn lineweight(w: i64) -> u8 {
    match w {
        -1 => 29,
        -2 => 30,
        -3 => 31,
        w => LINEWEIGHTS.iter().position(|x| *x == w).and_then(|i| u8::try_from(i).ok()).unwrap_or(29),
    }
}

/// A colour from its groups: the index (62), a true colour over it (420).
pub(super) fn color(cx: &Cx<'_>, index: Option<i64>, rgb: Option<i64>) -> ColorValue {
    if let Some(v) = rgb {
        let [_, r, g, b] = u32::try_from(v & 0x00FF_FFFF).unwrap_or(0).to_be_bytes();
        if cx.ver >= V::R2004 {
            return ColorValue::Rgb(r, g, b);
        }
        // AutoCAD 2000 has no true colours: the index the record gives, or the nearest.
        if index.is_none() {
            let i = draftcraft_color::nearest_aci(draftcraft_color::Rgb::from_u32(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)));
            return ColorValue::Index(i);
        }
    }
    match index {
        None | Some(256) => ColorValue::ByLayer,
        Some(0) => ColorValue::ByBlock,
        Some(257) => ColorValue::None,
        Some(i) => u8::try_from(i.unsigned_abs()).map_or(ColorValue::ByLayer, ColorValue::Index),
    }
}

/// Where an entity lives and who owns it.
pub(super) fn space(cx: &Cx<'_>, r: &Rec<'_>) -> (EntitySpace, u64) {
    let g = r.sub("AcDbEntity");
    match r.owner {
        0 if g.i(67) == 1 => (EntitySpace::Paper, 0),
        0 => (EntitySpace::Model, 0),
        o if o == cx.model => (EntitySpace::Model, 0),
        o if o == cx.paper => (EntitySpace::Paper, 0),
        o => (EntitySpace::Owned, o),
    }
}

/// What every entity starts with, from the record's common groups. `reactors` is the record's.
pub(super) fn entity_start<'r>(cx: &Cx<'_>, r: &'r Rec<'_>) -> EntityStart<'r> {
    let g = r.sub("AcDbEntity");
    let g = if g.0.is_empty() { r.all() } else { g };
    let (space, owner) = space(cx, r);
    let linetype = match g.get(6).map(text) {
        None => EntityLinetype::ByLayer,
        Some(n) if n.eq_ignore_ascii_case("BYLAYER") => EntityLinetype::ByLayer,
        Some(n) if n.eq_ignore_ascii_case("BYBLOCK") => EntityLinetype::ByBlock,
        Some(n) if n.eq_ignore_ascii_case("CONTINUOUS") => EntityLinetype::Continuous,
        Some(n) => match cx.linetype(n) {
            0 => EntityLinetype::ByLayer,
            h => EntityLinetype::Record(h),
        },
    };
    EntityStart {
        graphics: None,
        space,
        owner,
        reactors: &r.reactors,
        xdictionary: r.xdict,
        color: color(cx, g.get(62).map(Tag::i64), g.get(420).map(Tag::i64)),
        color_book: 0,
        transparency: if cx.ver >= V::R2004 { g.get(440).and_then(|t| u32::try_from(t.i64()).ok()) } else { None },
        linetype_scale: g.f_or(48, 1.0),
        linetype,
        layer: cx.layer(g.s(8)),
        plotstyle: cx.known(g.h(390)),
        material: cx.known(g.h(347)),
        shadow: u8::try_from(g.i(284)).unwrap_or(0),
        invisible: g.i(60) & 1 != 0,
        lineweight: lineweight(g.i_or(370, -1)),
        links: cx.links.get(&r.handle).copied(),
    }
}

/// True for the records that belong to the entity before them: a polyline's vertices, a block
/// reference's attributes, and the SEQEND that ends either.
fn follows(kind: &str) -> bool {
    matches!(kind, "VERTEX" | "ATTRIB" | "SEQEND")
}

/// One list of entities: its top-level members given to `owner`, and each polyline's and block
/// reference's followers given to it.
fn own(cx: &mut Cx<'_>, list: &[Rec<'_>], owner_of: &dyn Fn(&Rec<'_>) -> u64) {
    let mut parent: Option<u64> = None;
    for r in list {
        if follows(r.kind) {
            if let Some(p) = parent {
                cx.owned.entry(p).or_default().push(r.handle);
            }
            if r.kind == "SEQEND" {
                parent = None;
            }
            continue;
        }
        let g = r.all();
        parent = match r.kind {
            "POLYLINE" => Some(r.handle),
            "INSERT" if g.i(66) == 1 => Some(r.handle),
            _ => None,
        };
        cx.owned.entry(owner_of(r)).or_default().push(r.handle);
    }
}

/// Fill in the structure of the drawing: what owns what, the neighbours of each entity (for
/// AutoCAD 2000 files), each block's references, the named-object dictionary.
pub(super) fn structure(cx: &mut Cx<'_>) {
    let doc = cx.doc;
    let (model, paper) = (cx.model, cx.paper);
    for b in &doc.blocks {
        let record = b.begin.owner;
        cx.block_ends.insert(record, (b.begin.handle, b.end.as_ref().map_or(0, |e| e.handle)));
        own(cx, &b.entities, &|_| record);
    }
    own(cx, &doc.entities, &|r| match r.owner {
        0 if r.sub("AcDbEntity").i(67) == 1 => paper,
        0 => model,
        o => o,
    });
    let every = || doc.blocks.iter().flat_map(|b| b.entities.iter()).chain(doc.entities.iter());
    for r in every().filter(|r| r.kind == "INSERT") {
        let name = r.sub("AcDbBlockReference").s(2);
        let name = if name.is_empty() { r.sub("AcDbMInsertBlock").s(2) } else { name };
        if let Some(&record) = cx.blocks.get(&name.trim().to_uppercase()) {
            cx.inserts.entry(record).or_default().push(r.handle);
        }
    }
    if cx.ver <= V::R2000 {
        let lists: Vec<Vec<u64>> = cx.owned.values().cloned().collect();
        for list in lists {
            for (i, &h) in list.iter().enumerate() {
                let prev = if i == 0 { 0 } else { list.get(i - 1).copied().unwrap_or(0) };
                let next = list.get(i + 1).copied().unwrap_or(0);
                if !(prev == h.wrapping_sub(1) && next == h.wrapping_add(1)) {
                    cx.links.insert(h, (prev, next));
                }
            }
        }
    }
    for t in &doc.tables {
        cx.handles.insert(t.head.handle);
        cx.handles.extend(t.records.iter().map(|r| r.handle));
    }
    for b in &doc.blocks {
        cx.handles.insert(b.begin.handle);
        cx.handles.extend(b.end.iter().map(|r| r.handle));
    }
    cx.handles.extend(every().map(|r| r.handle));
    cx.kinds = every().map(|r| (r.handle, r.kind)).collect();
    cx.handles.extend(doc.objects.iter().map(|r| r.handle));
    cx.handles.remove(&0);
    // The named-object dictionary is the first dictionary without an owner.
    if let Some(root) =
        doc.objects.iter().find(|r| r.kind == "DICTIONARY" && r.owner == 0).or_else(|| doc.objects.iter().find(|r| r.kind == "DICTIONARY"))
    {
        cx.root = root.handle;
        cx.named = entries(root).into_iter().map(|(n, h)| (n.to_ascii_uppercase(), h)).collect();
    }
    anonymous_names(cx);
    if let Some(t) = doc.table("BLOCK_RECORD") {
        for r in &t.records {
            let layout = r.all().h(340);
            if layout != 0 {
                cx.layouts.insert(r.handle, layout);
            }
        }
    }
}

/// The names anonymous blocks are stored under. DWG stores them unnumbered (`*D` for `*D0`) and
/// a reader numbers them again, per prefix, in table order. So the number is left out only for a
/// prefix whose blocks are numbered 0, 1, 2… in that order: reading gives each its name back.
fn anonymous_names(cx: &mut Cx<'_>) {
    let Some(table) = cx.doc.table("BLOCK_RECORD") else { return };
    let anonymous: std::collections::HashSet<u64> =
        cx.doc.blocks.iter().filter(|b| b.begin.sub("AcDbBlockBegin").i(70) & 1 != 0).map(|b| b.begin.owner).collect();
    // Per prefix: the records and whether their numbers count up from 0.
    let mut by_prefix: std::collections::HashMap<String, (Vec<u64>, bool)> = std::collections::HashMap::new();
    for r in &table.records {
        let name = r.all().s(2);
        let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let prefix = name.get(..name.len() - digits).unwrap_or("");
        let lettered = prefix.len() >= 2 && prefix.starts_with('*') && prefix.chars().skip(1).all(|c| c.is_ascii_alphabetic());
        if !lettered || !anonymous.contains(&r.handle) {
            continue;
        }
        let e = by_prefix.entry(prefix.to_ascii_uppercase()).or_insert((Vec::new(), true));
        let number = name.get(name.len() - digits..).and_then(|d| d.parse::<usize>().ok());
        e.1 &= digits > 0 && number == Some(e.0.len()) && !name.get(name.len() - digits..).is_some_and(|d| d.len() > 1 && d.starts_with('0'));
        e.0.push(r.handle);
    }
    for r in &table.records {
        let name = r.all().s(2);
        let prefix = name.trim_end_matches(|c: char| c.is_ascii_digit());
        if by_prefix.get(&prefix.to_ascii_uppercase()).is_some_and(|(list, counted)| *counted && list.contains(&r.handle)) {
            cx.block_names.insert(r.handle, prefix.to_string());
        }
    }
}

/// A dictionary's entries: (name, handle), from its groups 3 and 350 or 360.
pub(super) fn entries<'a>(r: &Rec<'a>) -> Vec<(&'a str, u64)> {
    let g: Groups<'a> = r.sub("AcDbDictionary");
    let mut out = Vec::new();
    let mut name: Option<&'a str> = None;
    for t in g.0 {
        match t.code {
            3 => name = Some(text(t)),
            350 | 360 => {
                if let Some(n) = name.take() {
                    out.push((n, hex(text(t))));
                }
            }
            _ => {}
        }
    }
    out
}
