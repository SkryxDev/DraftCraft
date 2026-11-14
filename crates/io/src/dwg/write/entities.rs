//! Entities: each record as the DWG entity of its type, its fields in the order the format
//! stores them (the order `super::super::entities` reads them in).

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dwg::file::EntitySpace;

use super::Cx;
use super::common::entity_start;
use super::rec::{Groups, Rec};

/// A writer for the entity `r` as the DWG type `code`, with what every entity starts with.
pub(super) fn begin(cx: &Cx<'_>, code: u16, r: &Rec<'_>) -> ObjectWriter {
    let mut w = cx.writer(code, r);
    let mut start = entity_start(cx, r);
    // A block's BLOCK and ENDBLK are in no chain of entities: they name no neighbours.
    if cx.ver <= V::R2000 && matches!(code, 0x04 | 0x05) {
        start.links = Some((0, 0));
    }
    w.entity(&start);
    w
}

/// As [`begin`], for an entity its owner lists (a block's BLOCK and ENDBLK, a polyline's
/// vertices, a block reference's attributes): it names its owner whatever space that is in.
fn begin_owned(cx: &Cx<'_>, code: u16, r: &Rec<'_>) -> ObjectWriter {
    let mut w = cx.writer(code, r);
    let mut start = entity_start(cx, r);
    start.space = EntitySpace::Owned;
    start.owner = r.owner;
    w.entity(&start);
    w
}

/// The owned entities of `r` and the SEQEND that ends them.
fn followers<'c>(cx: &'c Cx<'_>, r: &Rec<'_>) -> (&'c [u64], u64) {
    let list: &[u64] = cx.owned.get(&r.handle).map_or(&[], Vec::as_slice);
    let seqend = cx.doc_kind(list.last().copied().unwrap_or(0)) == Some("SEQEND");
    match list.split_last() {
        Some((last, rest)) if seqend => (rest, *last),
        _ => (list, 0),
    }
}

/// The references to what an entity owns: the first and last up to AutoCAD 2000, each one from
/// 2004, then the SEQEND.
fn owned_refs(cx: &Cx<'_>, w: &mut ObjectWriter, list: &[u64], seqend: u64) {
    if cx.ver <= V::R2000 {
        w.reference(Ref::SoftPointer, list.first().copied().unwrap_or(0));
        w.reference(Ref::SoftPointer, list.last().copied().unwrap_or(0));
    } else {
        for h in list {
            w.reference(Ref::HardOwner, *h);
        }
    }
    w.reference(Ref::HardOwner, seqend);
}

/// One entity. `None` for a type the writer has no encoding for.
pub(super) fn entity(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let g = r.own();
    Some(match r.kind {
        "BLOCK" => {
            let mut w = begin(cx, 0x04, r);
            w.text(r.sub("AcDbBlockBegin").s(2));
            w
        }
        "ENDBLK" => begin(cx, 0x05, r),
        "SEQEND" => begin_owned(cx, 0x06, r),
        "LINE" => {
            let mut w = begin(cx, 0x13, r);
            let (a, b) = (g.p3(10), g.p3(11));
            let flat = a[2] == 0.0 && b[2] == 0.0;
            w.b(flat);
            w.rd(a[0]);
            w.dd(b[0], a[0]);
            w.rd(a[1]);
            w.dd(b[1], a[1]);
            if !flat {
                w.rd(a[2]);
                w.dd(b[2], a[2]);
            }
            w.thickness(g.f(39));
            w.extrusion(g.extrusion());
            w
        }
        "POINT" => {
            let mut w = begin(cx, 0x1B, r);
            w.bd3(g.p3(10));
            w.thickness(g.f(39));
            w.extrusion(g.extrusion());
            w.bd(g.f(50).to_radians());
            w
        }
        "CIRCLE" => {
            let mut w = begin(cx, 0x12, r);
            w.bd3(g.p3(10));
            w.bd(g.f(40));
            w.thickness(g.f(39));
            w.extrusion(g.extrusion());
            w
        }
        "ARC" => {
            let mut w = begin(cx, 0x11, r);
            let c = r.sub("AcDbCircle");
            let c = if c.0.is_empty() { g } else { c };
            w.bd3(c.p3(10));
            w.bd(c.f(40));
            w.thickness(c.f(39));
            w.extrusion(g.extrusion());
            let a = r.sub("AcDbArc");
            let a = if a.0.is_empty() { g } else { a };
            w.bd(a.f(50).to_radians());
            w.bd(a.f(51).to_radians());
            w
        }
        "ELLIPSE" => {
            let mut w = begin(cx, 0x23, r);
            w.bd3(g.p3(10));
            w.bd3(g.p3(11));
            w.bd3(g.extrusion());
            w.bd(g.f_or(40, 1.0));
            w.bd(g.f(41));
            w.bd(g.f_or(42, std::f64::consts::TAU));
            w
        }
        "SOLID" | "TRACE" => {
            let mut w = begin(cx, if r.kind == "SOLID" { 0x1F } else { 0x20 }, r);
            w.thickness(g.f(39));
            w.bd(g.f(30));
            for code in [10, 11, 12, 13] {
                w.rd2(g.p2(code));
            }
            w.extrusion(g.extrusion());
            w
        }
        "3DFACE" => {
            let mut w = begin(cx, 0x1C, r);
            let corners = [g.p3(10), g.p3(11), g.p3(12), if g.has(13) { g.p3(13) } else { g.p3(12) }];
            let edges = u16::try_from(g.i(70)).unwrap_or(0);
            let [first, ..] = corners;
            let flat = first[2] == 0.0;
            w.b(edges == 0);
            w.b(flat);
            w.rd(first[0]);
            w.rd(first[1]);
            if !flat {
                w.rd(first[2]);
            }
            for pair in corners.windows(2) {
                if let [p, c] = pair {
                    w.dd(c[0], p[0]);
                    w.dd(c[1], p[1]);
                    w.dd(c[2], p[2]);
                }
            }
            if edges != 0 {
                w.bs(edges);
            }
            w
        }
        "XLINE" | "RAY" => {
            let mut w = begin(cx, if r.kind == "RAY" { 0x28 } else { 0x29 }, r);
            w.bd3(g.p3(10));
            w.bd3(g.p3(11));
            w
        }
        "TEXT" => {
            let mut w = begin(cx, 0x01, r);
            let valign = r.all().i(73);
            let style = text_fields(cx, &mut w, r.all(), valign);
            w.reference(Ref::HardPointer, style);
            w
        }
        "ATTRIB" | "ATTDEF" => attribute(cx, r)?,
        "LWPOLYLINE" => lwpolyline(cx, r),
        "INSERT" => insert(cx, r),
        "POLYLINE" => polyline(cx, r),
        "VERTEX" => vertex(cx, r),
        _ => return super::annotation::entity(cx, r),
    })
}

/// The fields of a TEXT, which attributes start with too. Returns the text style to refer to.
/// From AutoCAD 2000 a flag byte says which fields have their default and are left out.
fn text_fields(cx: &Cx<'_>, w: &mut ObjectWriter, g: Groups<'_>, valign: i64) -> u64 {
    let insert = g.p3(10);
    let align = g.p2(11);
    let (oblique, rotation) = (g.f(51).to_radians(), g.f(50).to_radians());
    let width = g.f_or(41, 1.0);
    let (generation, halign) = (g.i(71), g.i(72));
    let mut flags = 0u8;
    flags |= u8::from(insert[2] == 0.0);
    flags |= u8::from(align == [0.0, 0.0]) << 1;
    flags |= u8::from(oblique == 0.0) << 2;
    flags |= u8::from(rotation == 0.0) << 3;
    flags |= u8::from(width == 1.0) << 4;
    flags |= u8::from(generation == 0) << 5;
    flags |= u8::from(halign == 0) << 6;
    flags |= u8::from(valign == 0) << 7;
    w.rc(flags);
    if flags & 0x01 == 0 {
        w.rd(insert[2]);
    }
    w.rd2([insert[0], insert[1]]);
    if flags & 0x02 == 0 {
        w.dd(align[0], insert[0]);
        w.dd(align[1], insert[1]);
    }
    // AutoCAD writes a text's extrusion in full, even (0, 0, 1).
    w.b(false);
    w.bd3(g.extrusion());
    w.thickness(g.f(39));
    if flags & 0x04 == 0 {
        w.rd(oblique);
    }
    if flags & 0x08 == 0 {
        w.rd(rotation);
    }
    w.rd(g.f(40));
    if flags & 0x10 == 0 {
        w.rd(width);
    }
    w.text(g.s(1));
    let short = |v: i64| u16::try_from(v).unwrap_or(0);
    if flags & 0x20 == 0 {
        w.bs(short(generation));
    }
    if flags & 0x40 == 0 {
        w.bs(short(halign));
    }
    if flags & 0x80 == 0 {
        w.bs(short(valign));
    }
    cx.style(g.s(7))
}

/// ATTRIB and ATTDEF: a text's fields, then the attribute's own. A multiline attribute (whose
/// text is an MTEXT inside it) has no encoding yet.
fn attribute(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let definition = r.kind == "ATTDEF";
    let ver = cx.ver;
    let t = r.sub("AcDbText");
    let a = r.sub(if definition { "AcDbAttributeDefinition" } else { "AcDbAttribute" });
    if a.i(71) > 1 || r.tags.iter().any(|t| t.code == 101) {
        return None;
    }
    let mut w = if definition { begin(cx, 0x03, r) } else { begin_owned(cx, 0x02, r) };
    let style = text_fields(cx, &mut w, t, a.i(74));
    if ver >= V::R2010 {
        w.rc(0);
    }
    if ver >= V::R2018 {
        w.rc(1);
    }
    w.text(a.s(2));
    w.bs(u16::try_from(a.i(73)).unwrap_or(0));
    w.rc(u8::try_from(a.i(70) & 0xFF).unwrap_or(0));
    if ver >= V::R2007 {
        // The lock is the second group 280 of the subclass (the first is its version).
        w.b(a.all(280).nth(1).is_some_and(|t| t.i64() != 0));
    }
    if definition {
        if ver >= V::R2010 {
            w.rc(0);
        }
        w.text(a.s(3));
    }
    w.reference(Ref::HardPointer, style);
    Some(w)
}

fn lwpolyline(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.own();
    let mut w = begin(cx, 0x4D, r);
    // The vertices: a group 10 starts each.
    struct Vertex {
        p: [f64; 2],
        widths: (f64, f64),
        bulge: f64,
        id: u32,
    }
    let mut vertices: Vec<Vertex> = Vec::new();
    for t in g.0 {
        match (t.code, vertices.last_mut()) {
            (10, _) => vertices.push(Vertex { p: [t.f64(), 0.0], widths: (0.0, 0.0), bulge: 0.0, id: 0 }),
            (20, Some(v)) => v.p[1] = t.f64(),
            (40, Some(v)) => v.widths.0 = t.f64(),
            (41, Some(v)) => v.widths.1 = t.f64(),
            (42, Some(v)) => v.bulge = t.f64(),
            (91, Some(v)) => v.id = u32::try_from(t.i64()).unwrap_or(0),
            _ => {}
        }
    }
    let head = Groups(g.0.get(..g.0.iter().position(|t| t.code == 10).unwrap_or(g.0.len())).unwrap_or(&[]));
    let dxf = g.i(70);
    let (width, elevation, thickness, normal) = (head.f(43), head.f(38), g.f(39), g.extrusion());
    let bulges = vertices.iter().any(|v| v.bulge != 0.0);
    let widths = vertices.iter().any(|v| v.widths != (0.0, 0.0));
    let ids = cx.ver >= V::R2010 && vertices.iter().any(|v| v.id != 0);
    let mut flags = 0u16;
    flags |= u16::from(normal != [0.0, 0.0, 1.0]);
    flags |= u16::from(thickness != 0.0) << 1;
    flags |= u16::from(width != 0.0) << 2;
    flags |= u16::from(elevation != 0.0) << 3;
    flags |= u16::from(bulges) << 4;
    flags |= u16::from(widths) << 5;
    flags |= u16::from(dxf & 128 != 0) << 8;
    flags |= u16::from(dxf & 1 != 0) << 9;
    flags |= u16::from(ids) << 10;
    w.bs(flags);
    if flags & 4 != 0 {
        w.bd(width);
    }
    if flags & 8 != 0 {
        w.bd(elevation);
    }
    if flags & 2 != 0 {
        w.bd(thickness);
    }
    if flags & 1 != 0 {
        w.bd3(normal);
    }
    let n = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
    w.bl(n);
    if bulges {
        w.bl(n);
    }
    if ids {
        w.bl(n);
    }
    if widths {
        w.bl(n);
    }
    let mut prev: Option<[f64; 2]> = None;
    for v in &vertices {
        match prev {
            None => w.rd2(v.p),
            Some(q) => {
                w.dd(v.p[0], q[0]);
                w.dd(v.p[1], q[1]);
            }
        }
        prev = Some(v.p);
    }
    if bulges {
        for v in &vertices {
            w.bd(v.bulge);
        }
    }
    if ids {
        for v in &vertices {
            w.bl(v.id);
        }
    }
    if widths {
        for v in &vertices {
            w.bd(v.widths.0);
            w.bd(v.widths.1);
        }
    }
    w
}

/// INSERT, and MINSERT when the reference is an array (more than one row or column).
fn insert(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.own();
    let (cols, rows) = (g.i_or(70, 1).max(1), g.i_or(71, 1).max(1));
    let array = cols > 1 || rows > 1;
    let mut w = begin(cx, if array { 0x08 } else { 0x07 }, r);
    w.bd3(g.p3(10));
    let s = [g.f_or(41, 1.0), g.f_or(42, 1.0), g.f_or(43, 1.0)];
    if s == [1.0, 1.0, 1.0] {
        w.bb(3);
    } else if s[0] == 1.0 {
        w.bb(1);
        w.dd(s[1], 1.0);
        w.dd(s[2], 1.0);
    } else if s[1] == s[0] && s[2] == s[0] {
        w.bb(2);
        w.rd(s[0]);
    } else {
        w.bb(0);
        w.rd(s[0]);
        w.dd(s[1], s[0]);
        w.dd(s[2], s[0]);
    }
    w.bd(g.f(50).to_radians());
    w.bd3(g.extrusion());
    let (attributes, seqend) = followers(cx, r);
    let has = !attributes.is_empty() || seqend != 0;
    w.b(has);
    if has && cx.ver >= V::R2004 {
        w.bl(u32::try_from(attributes.len()).unwrap_or(u32::MAX));
    }
    if array {
        w.bs(u16::try_from(cols).unwrap_or(1));
        w.bs(u16::try_from(rows).unwrap_or(1));
        w.bd(g.f(44));
        w.bd(g.f(45));
    }
    w.reference(Ref::HardPointer, cx.block(g.s(2)));
    if has {
        owned_refs(cx, &mut w, attributes, seqend);
    }
    w
}

/// POLYLINE: a 2D or 3D polyline, a polygon mesh or a polyface mesh, by its flags.
fn polyline(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.own();
    let flags = g.i(70);
    let short = |v: i64| u16::try_from(v).unwrap_or(0);
    let (vertices, seqend) = followers(cx, r);
    let mut w;
    if flags & 64 != 0 {
        w = begin(cx, 0x1D, r);
        w.bs(short(g.i(71)));
        w.bs(short(g.i(72)));
    } else if flags & 16 != 0 {
        w = begin(cx, 0x1E, r);
        w.bs(short(flags & !16));
        w.bs(short(g.i(75)));
        for code in [71, 72, 73, 74] {
            w.bs(short(g.i(code)));
        }
    } else if flags & 8 != 0 {
        w = begin(cx, 0x10, r);
        // Curve type: 0 none, 1 quadratic B-spline, 2 cubic, 3 Bézier.
        w.rc(match g.i(75) {
            5 => 1,
            6 => 2,
            8 => 3,
            _ => 0,
        });
        w.rc(u8::from(flags & 1 != 0));
    } else {
        w = begin(cx, 0x0F, r);
        w.bs(short(flags));
        w.bs(short(g.i(75)));
        w.bd(g.f(40));
        w.bd(g.f(41));
        w.thickness(g.f(39));
        w.bd(g.f(30));
        w.extrusion(g.extrusion());
    }
    if cx.ver >= V::R2004 {
        w.bl(u32::try_from(vertices.len()).unwrap_or(u32::MAX));
    }
    owned_refs(cx, &mut w, vertices, seqend);
    w
}

/// VERTEX: one of five DWG types, by its flags.
fn vertex(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.own();
    let flags = g.i(70);
    let byte = u8::try_from(flags & 0xFF).unwrap_or(0);
    if flags & 128 != 0 && flags & 64 == 0 {
        let mut w = begin_owned(cx, 0x0E, r);
        for code in [71, 72, 73, 74] {
            w.bs(i16::try_from(g.i(code)).unwrap_or(0).cast_unsigned());
        }
        return w;
    }
    let code = if flags & 128 != 0 {
        0x0D
    } else if flags & 64 != 0 {
        0x0C
    } else if flags & 32 != 0 {
        0x0B
    } else {
        0x0A
    };
    let mut w = begin_owned(cx, code, r);
    w.rc(byte);
    w.bd3(g.p3(10));
    if code == 0x0A {
        let (start, end) = (g.f(40), g.f(41));
        if start == end && start != 0.0 {
            w.bd(-start);
        } else {
            w.bd(start);
            w.bd(end);
        }
        w.bd(g.f(42));
        if cx.ver >= V::R2010 {
            w.bl(u32::try_from(g.i(91)).unwrap_or(0));
        }
        w.bd(g.f(50).to_radians());
    }
    w
}
