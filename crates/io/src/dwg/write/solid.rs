//! 3DSOLID, REGION and BODY (§20.4.39 to 41): the solid modeler's description of the shape.
//!
//! Up to AutoCAD 2010 the entity carries it: as SAT text (format 1), the text the DXF record holds
//! in its groups 1 and 3, scrambled the same way in both formats (`super::super::entities`
//! reads it back as stored); or, from AutoCAD 2004, as the binary form (format 2) a data store
//! record holds. A wireframe block follows, empty: AutoCAD draws the shape from the description.
//! From AutoCAD 2013 the description lives in the file's data store section, not here.

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
        // The data store section is not written yet: the shape would be lost.
        return None;
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
