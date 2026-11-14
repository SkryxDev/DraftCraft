//! The objects that are not drawn: dictionaries, extension records, layouts, styles.

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dwg::read::nongraphic::{XrType, xr_type};
use draftcraft_dxf::Tag;

use super::Cx;
use super::common::{self, bytes_of, entries};
use super::rec::{Groups, Rec, hex, text};

/// A writer for the object `r` as the DWG type `code`, with what every object starts with.
pub(super) fn begin(cx: &Cx<'_>, code: u16, r: &Rec<'_>) -> ObjectWriter {
    let mut w = cx.writer(code, r);
    w.object(r.owner, &r.reactors, r.xdict);
    w
}

/// One object. `None` for a type the writer has no encoding for.
pub(super) fn object(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    Some(match r.kind {
        "DICTIONARY" | "ACDBDICTIONARYWDFLT" => dictionary(cx, r)?,
        "XRECORD" => {
            let mut w = begin(cx, 0x4F, r);
            let g = r.sub("AcDbXrecord");
            let data = xrecord_data(cx, &w, g);
            w.bl(u32::try_from(data.len()).ok()?);
            w.bytes(&data);
            w.bs(u16::try_from(g.i_or(280, 1)).unwrap_or(1));
            w
        }
        "ACDBPLACEHOLDER" => {
            let mut w = begin(cx, cx.type_code("ACDBPLACEHOLDER", 0x50), r);
            w
        }
        "DICTIONARYVAR" => {
            let mut w = begin(cx, cx.class("DICTIONARYVAR")?, r);
            let g = r.sub("DictionaryVariables");
            w.rc(u8::try_from(g.i(280)).unwrap_or(0));
            w.text(g.s(1));
            w
        }
        "SCALE" => {
            let mut w = begin(cx, cx.class("SCALE")?, r);
            let g = r.sub("AcDbScale");
            w.bs(u16::try_from(g.i(70)).unwrap_or(0));
            w.text(g.s(300));
            w.bd(g.f_or(140, 1.0));
            w.bd(g.f_or(141, 1.0));
            w.b(g.i(290) != 0);
            w
        }
        "GROUP" => {
            let mut w = begin(cx, 0x48, r);
            let g = r.sub("AcDbGroup");
            let members: Vec<u64> = g.all(340).map(|t| hex(text(t))).filter(|h| cx.known(*h) != 0).collect();
            w.text(g.s(300));
            w.bs(u16::try_from(g.i(70)).unwrap_or(0));
            w.bs(u16::try_from(g.i_or(71, 1)).unwrap_or(1));
            w.bl(u32::try_from(members.len()).unwrap_or(0));
            for m in members {
                w.reference(Ref::HardPointer, m);
            }
            w
        }
        "LAYOUT" => layout(cx, r),
        "WIPEOUTVARIABLES" => {
            let mut w = begin(cx, cx.class("WIPEOUTVARIABLES")?, r);
            w.bs(u16::try_from(r.sub("AcDbWipeoutVariables").i(70)).unwrap_or(0));
            w
        }
        "MLEADERSTYLE" => mleader_style(cx, r)?,
        "DIMASSOC" => dim_assoc(cx, r)?,
        "TABLESTYLE" if cx.ver < V::R2010 => table_style(cx, r)?,
        "SPATIAL_FILTER" => spatial_filter(cx, r)?,
        "VISUALSTYLE" | "MATERIAL" | "TABLESTYLE" | "ACDBSECTIONVIEWSTYLE" | "ACDBDETAILVIEWSTYLE" | "IMAGEDEF"
        | "IMAGEDEF_REACTOR" | "RASTERVARIABLES" => grouped(cx, r)?,
        "MLINESTYLE" => mline_style(cx, r),
        _ => return None,
    })
}

/// DICTIONARY, and a dictionary with a default entry.
fn dictionary(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let with_default = r.kind == "ACDBDICTIONARYWDFLT";
    let code = if with_default { cx.class("ACDBDICTIONARYWDFLT")? } else { 0x2A };
    let mut w = begin(cx, code, r);
    let g = r.sub("AcDbDictionary");
    let list: Vec<(&str, u64)> = entries(r).into_iter().filter(|(_, h)| cx.known(*h) != 0).collect();
    w.bl(u32::try_from(list.len()).ok()?);
    w.bs(u16::try_from(g.i_or(281, 1)).unwrap_or(1));
    w.rc(u8::try_from(g.i(280)).unwrap_or(0));
    for (name, _) in &list {
        w.text(name);
    }
    // Entries are soft-owner references even in a dictionary that owns them hard, as AutoCAD
    // writes both.
    for (_, h) in &list {
        w.reference(Ref::SoftOwner, *h);
    }
    if with_default {
        w.reference(Ref::HardPointer, cx.known(r.sub("AcDbDictionaryWithDefault").h(340)));
    }
    Some(w)
}

/// An extension record's data: each group's code, then its value in the size its code takes.
/// A point's three groups are one value.
fn xrecord_data(cx: &Cx<'_>, w: &ObjectWriter, g: Groups<'_>) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    // The record's own groups start after its cloning flag (280).
    let start = g.0.iter().position(|t| t.code == 280).map_or(0, |i| i + 1);
    let tags = g.0.get(start..).unwrap_or(&[]);
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        i += 1;
        let Some(code) = u16::try_from(t.code).ok() else { continue };
        let Some(kind) = xr_type(code) else { continue };
        b.extend_from_slice(&code.to_le_bytes());
        match kind {
            XrType::Str => {
                if cx.ver.has_string_stream() {
                    let units: Vec<u16> = text(t).encode_utf16().take(usize::from(u16::MAX)).collect();
                    b.extend_from_slice(&u16::try_from(units.len()).unwrap_or(u16::MAX).to_le_bytes());
                    for u in units {
                        b.extend_from_slice(&u.to_le_bytes());
                    }
                } else {
                    let mut bytes = w.ansi(text(t));
                    bytes.truncate(usize::from(u16::MAX));
                    b.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
                    b.push(u8::try_from(cx.codepage).unwrap_or(30));
                    b.extend_from_slice(&bytes);
                }
            }
            XrType::Point => {
                let y = tags.get(i).filter(|n| n.code == t.code + 10).map(Tag::f64);
                let z = tags.get(i + 1).filter(|n| n.code == t.code + 20).map(Tag::f64);
                i += usize::from(y.is_some()) + usize::from(y.is_some() && z.is_some());
                for c in [t.f64(), y.unwrap_or(0.0), z.unwrap_or(0.0)] {
                    b.extend_from_slice(&c.to_le_bytes());
                }
            }
            XrType::Real => b.extend_from_slice(&t.f64().to_le_bytes()),
            XrType::Int8 => b.push(t.i64() as u8),
            XrType::Int16 => b.extend_from_slice(&(t.i64() as i16).to_le_bytes()),
            XrType::Int32 => b.extend_from_slice(&(t.i64() as i32).to_le_bytes()),
            XrType::Int64 => b.extend_from_slice(&t.i64().to_le_bytes()),
            XrType::Bool => b.push(u8::from(t.i64() != 0)),
            XrType::Bytes => {
                let bytes = bytes_of(text(t));
                let bytes = bytes.get(..bytes.len().min(255)).unwrap_or(&[]);
                b.push(u8::try_from(bytes.len()).unwrap_or(u8::MAX));
                b.extend_from_slice(bytes);
            }
            XrType::Handle => b.extend_from_slice(&hex(text(t)).to_le_bytes()),
        }
    }
    b
}

/// LAYOUT: its page setup, then the layout's own fields.
fn layout(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let ver = cx.ver;
    let mut w = begin(cx, cx.type_code("LAYOUT", 0x52), r);
    let p = r.sub("AcDbPlotSettings");
    let l = r.sub("AcDbLayout");
    let short = |v: i64| u16::try_from(v).unwrap_or(0);
    w.text(p.s(1));
    w.text(p.s(2));
    w.bs(short(p.i(70)));
    for code in [40, 41, 42, 43, 44, 45] {
        w.bd(p.f(code));
    }
    w.text(p.s(4));
    w.bd2([p.f(46), p.f(47)]);
    w.bs(short(p.i(72)));
    w.bs(short(p.i(73)));
    w.bs(short(p.i(74)));
    w.bd2([p.f(48), p.f(49)]);
    w.bd2([p.f(140), p.f(141)]);
    if ver <= V::R2000 {
        w.text(p.s(6));
    }
    w.bd(p.f_or(142, 1.0));
    w.bd(p.f_or(143, 1.0));
    w.text(p.s(7));
    w.bs(short(p.i(75)));
    w.bd(p.f_or(147, 1.0));
    w.bd2([p.f(148), p.f(149)]);
    if ver >= V::R2004 {
        w.bs(short(p.i(76)));
        w.bs(short(p.i(77)));
        w.bs(short(p.i(78)));
    }
    w.text(l.s(1));
    w.bl(u32::try_from(l.i(71)).unwrap_or(0));
    w.bs(short(l.i(70)));
    w.bd3(l.p3(13));
    w.rd2(l.p2(10));
    w.rd2(l.p2(11));
    w.bd3(l.p3(12));
    w.bd3(if l.has(16) { l.p3(16) } else { [1.0, 0.0, 0.0] });
    w.bd3(if l.has(17) { l.p3(17) } else { [0.0, 1.0, 0.0] });
    w.bd(l.f(146));
    w.bs(short(l.i(76)));
    w.bd3(l.p3(14));
    w.bd3(l.p3(15));
    let block = l.h(330);
    // The layout's viewports: the VIEWPORT entities its block owns.
    let viewports: Vec<u64> =
        cx.owned.get(&block).map(|list| list.iter().copied().filter(|h| cx.doc_kind(*h) == Some("VIEWPORT")).collect()).unwrap_or_default();
    if ver >= V::R2004 {
        w.bl(u32::try_from(viewports.len()).unwrap_or(0));
    }
    if ver >= V::R2004 {
        w.reference(Ref::HardPointer, 0);
    }
    if ver >= V::R2007 {
        w.reference(Ref::SoftPointer, cx.known(p.h(333)));
    }
    w.reference(Ref::SoftPointer, cx.known(block));
    w.reference(Ref::SoftPointer, cx.known(l.h(331)));
    w.reference(Ref::HardPointer, cx.known(l.h(346)));
    w.reference(Ref::HardPointer, cx.known(l.h(345)));
    if ver >= V::R2004 {
        for v in viewports {
            w.reference(Ref::SoftPointer, v);
        }
    }
    w
}

/// DWG keeps the MLINESTYLE flag bits in another order than DXF's group 70: (DWG, DXF).
const MLINE_FLAG_BITS: [(u16, u16); 8] = [(1, 2), (2, 1), (16, 16), (32, 64), (64, 32), (256, 256), (512, 1024), (1024, 512)];

fn mline_style(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let mut w = begin(cx, 0x49, r);
    let g = r.sub("AcDbMlineStyle");
    let dxf = u16::try_from(g.i(70)).unwrap_or(0);
    // One element per group 49, with the colour and linetype that follow it.
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 49).map(|(i, _)| i).collect();
    let head = Groups(g.0.get(..starts.first().copied().unwrap_or(g.0.len())).unwrap_or(&[]));
    let elements: Vec<Groups<'_>> = starts
        .iter()
        .enumerate()
        .take(255)
        .map(|(n, &i)| Groups(g.0.get(i..starts.get(n + 1).copied().unwrap_or(g.0.len())).unwrap_or(&[])))
        .collect();
    w.text(g.s(2));
    w.text(g.s(3));
    w.bs(MLINE_FLAG_BITS.iter().filter(|(_, x)| dxf & x != 0).fold(0, |acc, (d, _)| acc | d));
    w.color(common::color(cx, Some(head.i_or(62, 256)), head.get(420).map(Tag::i64)));
    w.bd(head.f_or(51, 90.0).to_radians());
    w.bd(head.f_or(52, 90.0).to_radians());
    w.rc(u8::try_from(elements.len()).unwrap_or(u8::MAX));
    // Before AutoCAD 2018 an element's linetype is its place in the linetype table.
    let order: Vec<u64> = cx
        .doc
        .table("LTYPE")
        .map(|t| t.records.iter().filter(|r| !matches!(r.all().s(2).to_ascii_uppercase().as_str(), "BYLAYER" | "BYBLOCK")).map(|r| r.handle).collect())
        .unwrap_or_default();
    for e in &elements {
        w.bd(e.f(49));
        w.color(common::color(cx, Some(e.i_or(62, 256)), e.get(420).map(Tag::i64)));
        if cx.ver < V::R2018 {
            let name = e.s(6);
            let index = match name.to_ascii_uppercase().as_str() {
                "" | "BYLAYER" => 0x7FFF,
                "BYBLOCK" => 0x7FFE,
                _ => order.iter().position(|h| *h == cx.linetype(name)).and_then(|i| u16::try_from(i).ok()).unwrap_or(0x7FFF),
            };
            w.bs(index);
        }
    }
    if cx.ver >= V::R2018 {
        for e in &elements {
            let name = e.s(6);
            w.reference(Ref::HardPointer, cx.linetype(if name.is_empty() { "BYLAYER" } else { name }));
        }
    }
    w
}

/// MLEADERSTYLE, in every version: its groups in their DXF order (the order of AutoCAD's own
/// records, which the codec's layout of 2013 and later holds), with the style's version (from
/// 2010), the three attachment directions (from 2010; the bottom one after the top one) and the
/// extended text flag (from 2013) where the version has them, the "changed" flag before the
/// annotative one. Colours are colour words in the record, handles the objects named.
fn mleader_style(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    use draftcraft_dwg::compose::color_of_word;
    let ver = cx.ver;
    let mut w = begin(cx, cx.class("MLEADERSTYLE")?, r);
    let g = r.sub("AcDbMLeaderStyle");
    let short = |code: i32, default: i64| u16::try_from(g.i_or(code, default)).unwrap_or(0);
    let long = |code: i32, default: i64| u32::try_from(g.i_or(code, default) & 0xFFFF_FFFF).unwrap_or(0);
    let color = |code: i32| color_of_word(u32::try_from(g.i_or(code, 0xC000_0000) & 0xFFFF_FFFF).unwrap_or(0xC000_0000));
    let handle = |code: i32| g.get(code).map_or(0, |t| cx.known(hex(text(t))));
    if ver >= V::R2010 {
        w.bs(short(179, 2));
    }
    w.bs(short(170, 2));
    w.bs(short(171, 1));
    w.bs(short(172, 0));
    w.bl(long(90, 2));
    w.bd(g.f(40));
    w.bd(g.f(41));
    w.bs(short(173, 1));
    w.color(color(91));
    w.reference(Ref::HardPointer, handle(340));
    w.bl(long(92, -2));
    w.b(g.i_or(290, 1) != 0);
    w.bd(g.f_or(42, 2.0));
    w.b(g.i_or(291, 1) != 0);
    w.bd(g.f_or(43, 8.0));
    w.text(g.s(3));
    w.reference(Ref::HardPointer, handle(341));
    w.bd(g.f_or(44, 4.0));
    w.text(g.s(300));
    w.reference(Ref::HardPointer, handle(342));
    w.bs(short(174, 1));
    w.bs(short(178, 1));
    w.bs(short(175, 1));
    w.bs(short(176, 0));
    w.color(color(93));
    w.bd(g.f_or(45, 4.0));
    w.b(g.i(292) != 0);
    w.b(g.i(297) != 0);
    w.bd(g.f_or(46, 4.0));
    w.reference(Ref::HardPointer, handle(343));
    w.color(color(94));
    w.bd(g.f_or(47, 1.0));
    w.bd(g.f_or(49, 1.0));
    w.bd(g.f_or(140, 1.0));
    w.b(g.i(293) != 0);
    w.bd(g.f(141));
    w.b(g.i(294) != 0);
    w.bs(short(177, 0));
    w.bd(g.f_or(142, 1.0));
    w.b(g.i(295) != 0);
    w.b(g.i(296) != 0);
    w.bd(g.f_or(143, 3.75));
    if ver >= V::R2010 {
        w.bs(short(271, 0));
        w.bs(short(273, 9));
        w.bs(short(272, 9));
    }
    if ver >= V::R2013 {
        w.b(g.i(298) != 0);
    }
    Some(w)
}

/// DIMASSOC, in every version: the dimension, which of its points are associated, then for each
/// point its references in turn: the class, the object snap, the objects snapped to, the
/// subentity (its type and marker, no external reference path), the parameter and the point,
/// the objects an intersection snap names too, and whether another reference for the point
/// follows. As AutoCAD's records hold them.
fn dim_assoc(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let mut w = begin(cx, cx.class("DIMASSOC")?, r);
    let g = r.sub("AcDbDimAssoc");
    let flags = u32::try_from(g.i(90) & 0xF).unwrap_or(0);
    w.bl(flags);
    w.b(g.i(70) != 0);
    w.rc(u8::try_from(g.i(71)).unwrap_or(0));
    w.reference(Ref::SoftPointer, g.get(330).map_or(0, |t| cx.known(hex(text(t)))));
    // The point references: each starts at a group 1.
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 1).map(|(i, _)| i).collect();
    let mut refs = starts.iter().enumerate().map(|(n, &i)| Groups(g.0.get(i..starts.get(n + 1).copied().unwrap_or(g.0.len())).unwrap_or(&[])));
    for bit in 0..4 {
        if flags & (1 << bit) == 0 {
            continue;
        }
        loop {
            let p = refs.next().unwrap_or_default();
            let osnap = u8::try_from(p.i(72)).unwrap_or(0);
            w.text(p.s(1));
            w.rc(osnap);
            let objects = |code: i32| -> Vec<u64> { p.all(code).map(|t| cx.known(hex(text(t)))).collect() };
            let mains = objects(331);
            w.bl(u32::try_from(mains.len()).unwrap_or(0));
            for m in &mains {
                w.reference(Ref::SoftPointer, *m);
            }
            if osnap != 0 && !mains.is_empty() {
                w.bs(u16::try_from(p.i(73)).unwrap_or(0));
                w.bl(u32::try_from(p.i(91) & 0xFFFF_FFFF).unwrap_or(0));
                w.bl(0);
            }
            w.bd(p.f(40));
            w.bd3(p.p3(10));
            if matches!(osnap, 6 | 11) {
                let others = objects(332);
                w.bl(u32::try_from(others.len()).unwrap_or(0));
                for o in &others {
                    w.reference(Ref::SoftPointer, *o);
                }
                if !others.is_empty() {
                    w.bs(u16::try_from(p.i(74)).unwrap_or(0));
                    w.bl(u32::try_from(p.i(92) & 0xFFFF_FFFF).unwrap_or(0));
                    w.bl(0);
                }
            }
            let more = p.i(75) != 0;
            w.b(more);
            if !more {
                break;
            }
        }
    }
    Some(w)
}

/// TABLESTYLE before AutoCAD 2010: the description, the flow direction, the flags, the
/// horizontal and vertical margins, whether the title and the header rows are suppressed, then
/// the three rows in the record's order, each its text style, height, alignment, text and fill
/// colours, whether it is filled and its six borders (lineweight, visibility, colour), and from
/// 2007 its value's type, unit and format. Every colour is a full colour record, in every
/// version. From 2010 the codec writes the cell styles that replaced the rows.
fn table_style(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    use draftcraft_dwg::compose::method_word;
    use draftcraft_dwg::file::ColorValue;
    let mut w = begin(cx, cx.class("TABLESTYLE")?, r);
    let all = r.sub("AcDbTableStyle");
    // AutoCAD's later DXF starts with the style's version (a 280 before the description).
    let g = match all.0 {
        [first, second, rest @ ..] if first.code == 280 && second.code == 3 => Groups(all.0.get(1..).unwrap_or(rest)),
        _ => all,
    };
    let rows_at = g.0.iter().position(|t| t.code == 7).unwrap_or(g.0.len());
    let head = Groups(g.0.get(..rows_at).unwrap_or(&[]));
    let color = |w: &mut ObjectWriter, aci: i64| {
        let c = match aci {
            0 => ColorValue::ByBlock,
            256 => ColorValue::ByLayer,
            257 => ColorValue::None,
            i => u8::try_from(i).map_or(ColorValue::ByBlock, ColorValue::Index),
        };
        w.bs(0);
        w.bl(method_word(c));
        w.rc(0);
    };
    w.text(head.s(3));
    w.bs(u16::try_from(head.i(70)).unwrap_or(0));
    w.bs(u16::try_from(head.i(71)).unwrap_or(0));
    w.bd(head.f_or(40, 0.06));
    w.bd(head.f_or(41, 0.06));
    w.b(head.i(280) != 0);
    w.b(head.i(281) != 0);
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 7).map(|(i, _)| i).collect();
    for n in 0..3 {
        let row = starts.get(n).map_or(Groups::default(), |&i| Groups(g.0.get(i..starts.get(n + 1).copied().unwrap_or(g.0.len())).unwrap_or(&[])));
        w.reference(Ref::HardPointer, cx.style(if row.has(7) { row.s(7) } else { "Standard" }));
        w.bd(row.f_or(140, 0.18));
        w.bs(u16::try_from(row.i_or(170, 5)).unwrap_or(5));
        color(&mut w, row.i_or(62, 0));
        color(&mut w, row.i_or(63, 7));
        w.b(row.i(283) != 0);
        for k in 0..6 {
            w.bs(i16::try_from(row.i_or(274 + k, -2)).unwrap_or(-2) as u16);
            w.b(row.i_or(284 + k, 1) != 0);
            color(&mut w, row.i_or(64 + k, 0));
        }
        if cx.ver >= V::R2007 {
            w.bl(u32::try_from(row.i_or(90, 512)).unwrap_or(512));
            w.bl(u32::try_from(row.i(91)).unwrap_or(0));
            w.text(row.s(1));
        }
    }
    Some(w)
}

/// SPATIAL_FILTER: a block reference's clip boundary. Its vertices, the boundary's normal and
/// origin, whether it is shown, the front and back clip planes (each a flag and, when set, its
/// distance), then the inverse of the block's transform and the boundary's own, twelve numbers
/// each. The groups are read in their order: the front distance and the matrices are all
/// groups 40.
fn spatial_filter(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let mut w = begin(cx, cx.class("SPATIAL_FILTER")?, r);
    let g = r.sub("AcDbSpatialFilter");
    // The next group `code` after the last one taken: (as a number, as a whole number).
    let mut it = g.0.iter();
    let mut take = |code: i32| -> Option<(f64, i64)> { it.by_ref().find(|t| t.code == code).map(|t| (t.f64(), t.i64())) };
    let real = |v: Option<(f64, i64)>, default: f64| v.map_or(default, |v| v.0);
    let n = take(70).map_or(0, |v| v.1);
    let n = usize::try_from(n).unwrap_or(0).min(1 << 16);
    let mut points = Vec::with_capacity(n);
    for _ in 0..n {
        let x = real(take(10), 0.0);
        let y = real(take(20), 0.0);
        points.push([x, y]);
    }
    let normal = match take(210) {
        Some((x, _)) => [x, real(take(220), 0.0), real(take(230), 1.0)],
        None => [0.0, 0.0, 1.0],
    };
    let origin = match take(11) {
        Some((x, _)) => [x, real(take(21), 0.0), real(take(31), 0.0)],
        None => [0.0; 3],
    };
    let display = take(71).map_or(1, |v| v.1);
    let front = take(72).map_or(0, |v| v.1);
    let front_distance = if front != 0 { real(take(40), 0.0) } else { 0.0 };
    let back = take(73).map_or(0, |v| v.1);
    let back_distance = if back != 0 { real(take(41), 0.0) } else { 0.0 };
    let matrices: Vec<f64> = (0..24).map(|i| real(take(40), if matches!(i % 12, 0 | 5 | 10) { 1.0 } else { 0.0 })).collect();
    w.bs(u16::try_from(points.len()).unwrap_or(0));
    for p in points {
        w.rd2(p);
    }
    w.bd3(normal);
    w.bd3(origin);
    w.bs(u16::try_from(display).unwrap_or(1));
    w.bs(u16::from(front != 0));
    if front != 0 {
        w.bd(front_distance);
    }
    w.bs(u16::from(back != 0));
    if back != 0 {
        w.bd(back_distance);
    }
    for v in matrices {
        w.bd(v);
    }
    Some(w)
}

/// An object the codec encodes from its DXF groups: the groups from the first subclass marker
/// on, each as the value its code holds, a point's three groups as one.
fn grouped(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    use draftcraft_dwg::file::{Handle, XrValue};
    use draftcraft_dwg::records::geom::Vec3;
    let mut w = begin(cx, cx.class(r.kind)?, r);
    let start = r.tags.iter().position(|t| t.code == 100)?;
    let tags = r.tags.get(start..)?;
    let mut items: Vec<(u16, XrValue)> = Vec::with_capacity(tags.len());
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        i += 1;
        let Some(code) = u16::try_from(t.code).ok() else { continue };
        let value = match xr_type(code)? {
            XrType::Str => XrValue::Str(text(t).to_string()),
            XrType::Point => {
                let y = tags.get(i).filter(|n| n.code == t.code + 10).map(Tag::f64);
                let z = tags.get(i + 1).filter(|n| n.code == t.code + 20).map(Tag::f64);
                i += usize::from(y.is_some()) + usize::from(y.is_some() && z.is_some());
                XrValue::Point(Vec3::new(t.f64(), y.unwrap_or(0.0), z.unwrap_or(0.0)))
            }
            XrType::Real => XrValue::Real(t.f64()),
            XrType::Int8 => XrValue::Int8(t.i64() as i8),
            XrType::Int16 => XrValue::Int16(t.i64() as i16),
            XrType::Int32 => XrValue::Int32(t.i64() as i32),
            XrType::Int64 => XrValue::Int64(t.i64()),
            XrType::Bool => XrValue::Bool(t.i64() != 0),
            XrType::Bytes => XrValue::Bytes(bytes_of(text(t))),
            XrType::Handle => XrValue::Handle(Handle(hex(text(t)))),
        };
        items.push((code, value));
    }
    let styles: Vec<(String, u64)> = cx.styles.iter().map(|(n, h)| (n.clone(), *h)).collect();
    w.grouped(r.kind, items, &styles).ok()?;
    Some(w)
}
