//! MULTILEADER (AutoCAD 2007 and later; older files get leaders and text in its place, as the
//! DXF writer lays them out). The record's groups are AutoCAD's: the annotation context in
//! `CONTEXT_DATA{…}`, each leader in `LEADER{…}`, each of its lines in `LEADER_LINE{…}`, then the
//! entity's own values. The file holds the same values in the same order, except that the
//! context's top and bottom attachments are swapped and that a value a record leaves out is
//! the value AutoCAD's Standard style gives it.

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref, color_of_word};
use draftcraft_dwg::file::ColorValue;
use draftcraft_dxf::Tag;

use super::Cx;
use super::entities::begin;
use super::rec::{Groups, Rec, hex, text};

/// Most leaders, lines, points or break items of one multileader written.
const MAX_ITEMS: usize = 1 << 16;

/// The part of `tags` inside each `open … close` pair (groups `open_code` with the text
/// `open`, up to the matching group `close_code` `}`), and the groups outside them.
fn blocks<'a>(tags: &'a [Tag], open_code: i32, open: &str, close_code: i32) -> (Vec<Tag>, Vec<&'a [Tag]>) {
    let mut rest = Vec::new();
    let mut inner = Vec::new();
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        if t.code == open_code && text(t).trim().eq_ignore_ascii_case(open) {
            let start = i + 1;
            let end = tags.iter().skip(start).position(|t| t.code == close_code && text(t).trim() == "}").map_or(tags.len(), |n| start + n);
            if inner.len() < MAX_ITEMS {
                inner.push(tags.get(start..end).unwrap_or(&[]));
            }
            i = end + 1;
        } else {
            rest.push(t.clone());
            i += 1;
        }
    }
    (rest, inner)
}

/// The points of the groups `code`, `code + 10`, `code + 20`, in order.
fn points(tags: &[Tag], code: i32) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = Vec::new();
    for t in tags {
        match (t.code, out.last_mut()) {
            (c, _) if c == code => {
                if out.len() < MAX_ITEMS {
                    out.push([t.f64(), 0.0, 0.0]);
                }
            }
            (c, Some(p)) if c == code + 10 => p[1] = t.f64(),
            (c, Some(p)) if c == code + 20 => p[2] = t.f64(),
            _ => {}
        }
    }
    out
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A short; `default` when the record has none or it doesn't fit.
fn short(g: Groups<'_>, code: i32, default: i64) -> u16 {
    u16::try_from(g.i_or(code, default)).unwrap_or(0)
}

/// A long, kept as its 32 bits (a negative lineweight is a negative long).
fn long(g: Groups<'_>, code: i32, default: i64) -> u32 {
    u32::try_from(g.i_or(code, default) & 0xFFFF_FFFF).unwrap_or(0)
}

/// A colour word (`0xC1000000` ByBlock, `0xC2rrggbb`, `0xC3000000 | index`).
fn color(g: Groups<'_>, code: i32, default: u32) -> ColorValue {
    color_of_word(g.get(code).map_or(default, |t| u32::try_from(t.i64() & 0xFFFF_FFFF).unwrap_or(default)))
}

const BYBLOCK: u32 = 0xC100_0000;
const NO_COLOR: u32 = 0xC800_0000;

/// The object a group names, when the drawing has it.
fn handle(cx: &Cx<'_>, g: Groups<'_>, code: i32) -> u64 {
    g.get(code).map_or(0, |t| cx.known(hex(text(t))))
}

pub(super) fn mleader(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let ver = cx.ver;
    let all = r.sub("AcDbMLeader").0;
    let mut w = begin(cx, cx.class("MULTILEADER")?, r);
    // The context, and the entity's own groups around it.
    let open = all.iter().position(|t| t.code == 300 && text(t).trim().eq_ignore_ascii_case("CONTEXT_DATA{"));
    let (context, own): (&[Tag], Vec<Tag>) = match open {
        Some(at) => {
            let start = at + 1;
            // The context's own `}` is the group 301; leaders and lines close with 303 and 305.
            let end = all.iter().skip(start).position(|t| t.code == 301).map_or(all.len(), |n| start + n);
            let mut own: Vec<Tag> = all.get(..at).unwrap_or(&[]).to_vec();
            own.extend(all.get(end + 1..).unwrap_or(&[]).iter().cloned());
            (all.get(start..end).unwrap_or(&[]), own)
        }
        None => (&[], all.to_vec()),
    };
    let e = Groups(&own);
    if ver >= V::R2010 {
        w.bs(short(e, 270, 2));
    }
    let mut refs: Vec<(Ref, u64)> = Vec::new();
    context_fields(cx, &mut w, &mut refs, context);
    // The entity's own values.
    refs.push((Ref::HardPointer, handle(cx, e, 340)));
    w.bl(long(e, 90, 0));
    w.bs(short(e, 170, 1));
    w.color(color(e, 91, BYBLOCK));
    refs.push((Ref::HardPointer, if e.has(341) { handle(cx, e, 341) } else { cx.linetype("BYBLOCK") }));
    w.bl(long(e, 171, -2));
    w.b(e.i_or(290, 1) != 0);
    w.b(e.i_or(291, 1) != 0);
    w.bd(e.f_or(41, 8.0));
    refs.push((Ref::HardPointer, handle(cx, e, 342)));
    w.bd(e.f_or(42, 4.0));
    w.bs(short(e, 172, 2));
    refs.push((Ref::HardPointer, handle(cx, e, 343)));
    w.bs(short(e, 173, 1));
    w.bs(short(e, 95, 1));
    w.bs(short(e, 174, 1));
    w.bs(short(e, 175, 0));
    w.color(color(e, 92, BYBLOCK));
    w.b(e.i(292) != 0);
    refs.push((Ref::HardPointer, handle(cx, e, 344)));
    w.color(color(e, 93, BYBLOCK));
    w.bd3(if e.has(10) { e.p3(10) } else { [1.0, 1.0, 1.0] });
    w.bd(e.f(43));
    w.bs(short(e, 176, 0));
    w.b(e.i(293) != 0);
    if ver < V::R2010 {
        // The arrowheads of single leaders: whether each is the default, and the block.
        let arrows: Vec<(bool, u64)> = own
            .iter()
            .enumerate()
            .filter(|(_, t)| t.code == 94)
            .take(MAX_ITEMS)
            .map(|(i, t)| (t.i64() != 0, own.get(i + 1).filter(|n| n.code == 345).map_or(0, |n| cx.known(hex(text(n))))))
            .collect();
        w.bl(count(arrows.len()));
        for (default, h) in arrows {
            w.b(default);
            refs.push((Ref::HardPointer, h));
        }
    }
    // Block attribute values: the definition, the text, its index and width.
    let starts: Vec<usize> = own.iter().enumerate().filter(|(_, t)| t.code == 330).map(|(i, _)| i).take(MAX_ITEMS).collect();
    w.bl(count(starts.len()));
    for (n, &i) in starts.iter().enumerate() {
        let a = Groups(own.get(i..starts.get(n + 1).copied().unwrap_or(own.len())).unwrap_or(&[]));
        refs.push((Ref::HardPointer, handle(cx, a, 330)));
        w.text(a.s(302));
        w.bs(short(a, 177, 0));
        w.bd(a.f(44));
    }
    w.b(e.i(294) != 0);
    w.bs(short(e, 178, 0));
    w.bs(short(e, 179, 1));
    w.bd(e.f_or(45, 1.0));
    if ver >= V::R2010 {
        w.bs(short(e, 271, 0));
        w.bs(short(e, 272, 9));
        w.bs(short(e, 273, 9));
    }
    if ver >= V::R2013 {
        w.b(e.i(295) != 0);
    }
    for (kind, h) in refs {
        w.reference(kind, h);
    }
    Some(w)
}

/// The annotation context: the leaders, the overall values, the text or block content, the
/// plane. Its references are added to `refs` in the order the file holds them.
fn context_fields(cx: &Cx<'_>, w: &mut ObjectWriter, refs: &mut Vec<(Ref, u64)>, context: &[Tag]) {
    let ver = cx.ver;
    let (common, roots) = blocks(context, 302, "LEADER{", 303);
    let g = Groups(&common);
    w.bl(count(roots.len()));
    for root in &roots {
        leader_root(cx, w, refs, root);
    }
    w.bd(g.f_or(40, 1.0));
    w.bd3(g.p3(10));
    w.bd(g.f_or(41, 4.0));
    w.bd(g.f_or(140, 4.0));
    w.bd(g.f_or(145, 2.0));
    w.bs(short(g, 174, 1));
    w.bs(short(g, 175, 1));
    w.bs(short(g, 176, 0));
    w.bs(short(g, 177, 0));
    let has_text = g.i(290) != 0;
    w.b(has_text);
    if has_text {
        w.text(g.s(304));
        w.bd3(if g.has(11) { g.p3(11) } else { [0.0, 0.0, 1.0] });
        refs.push((Ref::HardPointer, handle(cx, g, 340)));
        w.bd3(g.p3(12));
        w.bd3(if g.has(13) { g.p3(13) } else { [1.0, 0.0, 0.0] });
        w.bd(g.f(42));
        w.bd(g.f(43));
        w.bd(g.f(44));
        w.bd(g.f_or(45, 1.0));
        w.bs(short(g, 170, 1));
        w.color(color(g, 90, BYBLOCK));
        w.bs(short(g, 171, 1));
        w.bs(short(g, 172, 1));
        w.color(color(g, 91, NO_COLOR));
        w.bd(g.f_or(141, 1.5));
        w.bl(long(g, 92, 0));
        w.b(g.i(291) != 0);
        w.b(g.i(292) != 0);
        w.bs(short(g, 173, 0));
        w.b(g.i(293) != 0);
        w.bd(g.f(142));
        w.bd(g.f(143));
        w.b(g.i(294) != 0);
        let sizes: Vec<f64> = g.all(144).take(MAX_ITEMS).map(Tag::f64).collect();
        w.bl(count(sizes.len()));
        for s in sizes {
            w.bd(s);
        }
        w.b(g.i(295) != 0);
        w.b(false);
    } else {
        let has_block = g.i(296) != 0;
        w.b(has_block);
        if has_block {
            refs.push((Ref::SoftPointer, handle(cx, g, 341)));
            w.bd3(if g.has(14) { g.p3(14) } else { [0.0, 0.0, 1.0] });
            w.bd3(g.p3(15));
            w.bd3(if g.has(16) { g.p3(16) } else { [1.0, 1.0, 1.0] });
            w.bd(g.f(46));
            w.color(color(g, 93, BYBLOCK));
            let matrix: Vec<f64> = g.all(47).map(Tag::f64).collect();
            for i in 0..16 {
                let identity = if i % 5 == 0 { 1.0 } else { 0.0 };
                w.bd(if matrix.len() == 16 { matrix.get(i).copied().unwrap_or(identity) } else { identity });
            }
        }
    }
    w.bd3(g.p3(110));
    w.bd3(if g.has(111) { g.p3(111) } else { [1.0, 0.0, 0.0] });
    w.bd3(if g.has(112) { g.p3(112) } else { [0.0, 1.0, 0.0] });
    w.b(g.i(297) != 0);
    if ver >= V::R2010 {
        w.bs(short(g, 273, 9));
        w.bs(short(g, 272, 9));
    }
}

/// One leader: where it meets the content, its direction, its breaks, its index, the landing
/// distance, its lines and (from 2010) the side it attaches on.
fn leader_root(cx: &Cx<'_>, w: &mut ObjectWriter, refs: &mut Vec<(Ref, u64)>, root: &[Tag]) {
    let (own, lines) = blocks(root, 304, "LEADER_LINE{", 305);
    let g = Groups(&own);
    w.b(g.i_or(290, 1) != 0);
    w.b(g.i_or(291, 1) != 0);
    w.bd3(g.p3(10));
    w.bd3(if g.has(11) { g.p3(11) } else { [1.0, 0.0, 0.0] });
    let starts = points(&own, 12);
    let ends = points(&own, 13);
    let n = starts.len().min(ends.len());
    w.bl(count(n));
    for (s, e) in starts.iter().zip(&ends).take(n) {
        w.bd3(*s);
        w.bd3(*e);
    }
    w.bl(long(g, 90, 0));
    w.bd(g.f(40));
    w.bl(count(lines.len()));
    for (n, line) in lines.iter().enumerate() {
        leader_line(cx, w, refs, line, n);
    }
    if cx.ver >= V::R2010 {
        w.bs(short(g, 271, 0));
    }
}

/// One leader line: its points, its breaks (per segment, the start and end of each), its index
/// and (from 2010) its own type, colour, linetype, lineweight and arrowhead.
fn leader_line(cx: &Cx<'_>, w: &mut ObjectWriter, refs: &mut Vec<(Ref, u64)>, line: &[Tag], n: usize) {
    let g = Groups(line);
    let pts = points(line, 10);
    w.bl(count(pts.len()));
    for p in &pts {
        w.bd3(*p);
    }
    // A break item starts at each group 90 and holds the 11/12 pairs after it.
    let items: Vec<usize> = line.iter().enumerate().filter(|(_, t)| t.code == 90).map(|(i, _)| i).take(MAX_ITEMS).collect();
    w.bl(count(items.len()));
    for (k, &i) in items.iter().enumerate() {
        let item = line.get(i..items.get(k + 1).copied().unwrap_or(line.len())).unwrap_or(&[]);
        let item = item.get(..item.iter().position(|t| t.code == 91).unwrap_or(item.len())).unwrap_or(&[]);
        w.bl(long(Groups(item), 90, 0));
        let starts = points(item, 11);
        let ends = points(item, 12);
        let m = starts.len().min(ends.len());
        w.bl(count(m));
        for (s, e) in starts.iter().zip(&ends).take(m) {
            w.bd3(*s);
            w.bd3(*e);
        }
    }
    w.bl(long(g, 91, i64::try_from(n).unwrap_or(0)));
    if cx.ver >= V::R2010 {
        w.bs(short(g, 170, 1));
        w.color(color(g, 92, BYBLOCK));
        refs.push((Ref::HardPointer, handle(cx, g, 340)));
        w.bl(long(g, 171, -2));
        w.bd(g.f_or(40, 4.0));
        refs.push((Ref::HardPointer, handle(cx, g, 341)));
        w.bl(long(g, 93, 0));
    }
}
