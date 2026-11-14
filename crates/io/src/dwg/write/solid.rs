//! 3DSOLID, REGION and BODY (§20.4.39 to 41): the solid modeler's description of the shape.
//!
//! Up to AutoCAD 2010 the entity carries it: as SAT text (format 1), the text the DXF record holds
//! in its groups 1 and 3, scrambled the same way in both formats (`super::super::entities`
//! reads it back as stored); or, from AutoCAD 2004, as the binary form (format 2) a data store
//! record holds. A wireframe block follows, empty: AutoCAD draws the shape from the description.
//! From AutoCAD 2013 the description lives in the file's data store section (written by
//! `draftcraft_dwg::datastore`), and the entity holds the wireframe block and its GUID (group 2).

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};

use super::Cx;
use super::entities::begin;
use super::rec::{Groups, Rec, text};

/// Most bytes in one block of SAT text: AutoCAD refuses a drawing with a larger one.
const SAT_BLOCK: usize = 4096;

/// The modeler entity `r` of DWG type `code`. `None` when the file can't hold its description.
pub(super) fn modeler(cx: &Cx<'_>, r: &Rec<'_>, code: u16) -> Option<ObjectWriter> {
    let g = match r.sub("AcDbModelerGeometry") {
        g if !g.0.is_empty() => g,
        _ => r.own(),
    };
    let sat = sat_text(&g);
    let sab = cx.doc.data_store.get(&r.handle).filter(|_| sat.is_none());
    if cx.ver >= V::R2013 {
        // The description goes in the data store: without a record there, the shape would be
        // lost (SAT text has no place in these files).
        return sab.map(|_| modern(cx, r, code, &g));
    }
    let mut w = begin(cx, code, r);
    match (&sat, sab) {
        (Some(sat), _) => {
            w.b(false);
            w.b(true);
            w.bs(1);
            for block in sat.chunks(SAT_BLOCK) {
                w.bl(u32::try_from(block.len()).unwrap_or(0));
                w.bytes(block);
            }
            w.bl(0);
        }
        (None, Some(sab)) if cx.ver >= V::R2004 => {
            w.b(false);
            w.b(false);
            w.bs(2);
            w.bytes(sab);
        }
        // An AutoCAD 2000 file holds SAT text only.
        (None, Some(_)) => return None,
        (None, None) => w.b(true),
    }
    // The wireframe block, announced whenever there is a description: no reference point, no
    // isolines, no silhouettes.
    let described = sat.is_some() || sab.is_some();
    w.b(described);
    if described {
        w.b(false);
        w.bl(0);
        w.b(false);
        w.bl(0);
    }
    // No further modeler data.
    w.b(true);
    if cx.ver >= V::R2007 {
        // No materials bound to faces.
        w.bl(0);
        let history = if r.kind == "3DSOLID" { cx.known(r.sub("AcDb3dSolid").h(350)) } else { 0 };
        w.reference(Ref::SoftPointer, history);
    }
    Some(w)
}

/// A modeler entity of AutoCAD 2013 and later, as AutoCAD writes it: its description in the data
/// store, a wireframe block with no wires (AutoCAD draws from the description), no further
/// modeler data, no materials, group 290 and the GUID of group 2, then (a 3D solid) its history.
fn modern(cx: &Cx<'_>, r: &Rec<'_>, code: u16, g: &Groups<'_>) -> ObjectWriter {
    // The entity tells AutoCAD its description is in the data store.
    let mut w = cx.writer(code, r);
    let mut start = super::common::entity_start(cx, r);
    start.data_store = true;
    w.entity(&start);
    // The description is elsewhere; a wireframe block follows.
    w.b(true);
    w.b(true);
    w.bd3([0.0; 3]);
    w.bl(4);
    w.b(true);
    // No wires, no silhouettes.
    w.bl(0);
    w.bl(0);
    w.b(true);
    w.bl(0);
    w.b(g.0.iter().find(|t| t.code == 290).is_none_or(|_| g.i(290) != 0));
    let (a, b, c, d) = guid(g.s(2));
    w.bl(a);
    w.bs(b);
    w.bs(c);
    for x in d {
        w.rc(x);
    }
    w.bl(0);
    if r.kind == "3DSOLID" {
        w.reference(Ref::SoftPointer, cx.known(r.sub("AcDb3dSolid").h(350)));
    }
    w
}

/// A GUID as `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` in its four stored parts; zero when the
/// text is not one.
pub(crate) fn guid(text: &str) -> (u32, u16, u16, [u8; 8]) {
    let hex: String = text.chars().filter(char::is_ascii_hexdigit).collect();
    let part = |from: usize, len: usize| hex.get(from..from + len).and_then(|h| u64::from_str_radix(h, 16).ok());
    if hex.len() != 32 {
        return (0, 0, 0, [0; 8]);
    }
    let mut tail = [0u8; 8];
    for (i, t) in tail.iter_mut().enumerate() {
        *t = part(16 + 2 * i, 2).and_then(|v| u8::try_from(v).ok()).unwrap_or(0);
    }
    (
        part(0, 8).and_then(|v| u32::try_from(v).ok()).unwrap_or(0),
        part(8, 4).and_then(|v| u16::try_from(v).ok()).unwrap_or(0),
        part(12, 4).and_then(|v| u16::try_from(v).ok()).unwrap_or(0),
        tail,
    )
}

/// The SAT text of the modeler groups (format 1): each group 1 a line, continued by groups 3,
/// with DXF's caret escape undone, each line ended by CR LF as AutoCAD stores it, and without
/// the end-of-data line DWG's sized blocks have no use for. `None` when the groups hold none.
fn sat_text(g: &Groups<'_>) -> Option<Vec<u8>> {
    if g.i(70) != 1 && !g.0.iter().any(|t| t.code == 1) {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    for t in g.0.iter().filter(|t| matches!(t.code, 1 | 3)) {
        let part = text(t).replace("^ ", "^");
        match (t.code, lines.last_mut()) {
            (3, Some(line)) => line.push_str(&part),
            _ => lines.push(part),
        }
    }
    if lines.last().is_some_and(|l| is_end(l)) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(lines.iter().map(|l| l.len() + 2).sum());
    for l in &lines {
        // The text is one byte per character, as the reader takes it.
        out.extend(l.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')));
        out.extend_from_slice(b"\r\n");
    }
    Some(out)
}

/// The line that ends ACIS data, as written or scrambled (each printable character `c` stored
/// as `159 - c`).
fn is_end(line: &str) -> bool {
    let plain: String = line.chars().map(|c| if ('!'..='~').contains(&c) { char::from(159 - c as u8) } else { c }).collect();
    [line, plain.as_str()].iter().any(|l| l.starts_with("End-of-ACIS-data") || l.starts_with("End-of-ASM-data"))
}
