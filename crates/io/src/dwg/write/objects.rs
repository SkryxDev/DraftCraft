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
        "VISUALSTYLE" | "MATERIAL" | "TABLESTYLE" | "MLEADERSTYLE" | "ACDBSECTIONVIEWSTYLE" | "ACDBDETAILVIEWSTYLE" | "DIMASSOC" | "IMAGEDEF"
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
