//! The annotation entities: MTEXT, dimensions, leaders, hatches and the rest.

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dxf::Tag;

use super::Cx;
use super::common;
use super::entities::begin;
use super::rec::{Groups, Rec, text};
use crate::dxf_read::mtext_dxf::{decode_contents, decode_line};

/// One entity of the types this module writes. `None` for a type the writer has no encoding for.
pub(super) fn entity(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    Some(match r.kind {
        "MTEXT" => {
            let mut w = begin(cx, 0x2C, r);
            let style = mtext_fields(cx, &mut w, r.sub("AcDbMText"), r.tags);
            w.reference(Ref::HardPointer, style);
            if cx.ver >= V::R2018 {
                // The application of an annotative text's context: none.
                w.reference(Ref::HardPointer, 0);
            }
            w
        }
        "DIMENSION" => dimension(cx, r)?,
        "SPLINE" => {
            let mut w = begin(cx, 0x24, r);
            spline_fields(cx, &mut w, r.sub("AcDbSpline"));
            w
        }
        "HELIX" => {
            // A spline's data, then the helix: its version, the axis's base point, the start
            // point, the axis's direction, the radius, the turns, a turn's height, whether it
            // turns counter-clockwise and what stays fixed when it is edited.
            let mut w = begin(cx, cx.class("HELIX")?, r);
            spline_fields(cx, &mut w, r.sub("AcDbSpline"));
            let g = r.sub("AcDbHelix");
            w.bl(u32::try_from(g.i_or(90, 29)).unwrap_or(29));
            w.bl(u32::try_from(g.i_or(91, 63)).unwrap_or(63));
            w.bd3(g.p3(10));
            w.bd3(g.p3(11));
            w.bd3(if g.has(12) { g.p3(12) } else { [0.0, 0.0, 1.0] });
            w.bd(g.f(40));
            w.bd(g.f(41));
            w.bd(g.f(42));
            w.b(g.i(290) != 0);
            w.rc(u8::try_from(g.i(280)).unwrap_or(0));
            w
        }
        "OLE2FRAME" => {
            // Its type, its space, its data and (from 2000) whether it keeps its proportions; the
            // frame's corners are in the data's wrapper.
            let mut w = begin(cx, 0x4A, r);
            let g = r.sub("AcDbOle2Frame");
            w.bs(u16::try_from(g.i(71)).unwrap_or(0));
            if cx.ver >= V::R2000 {
                w.bs(u16::try_from(g.i(72)).unwrap_or(0));
            }
            let data: Vec<u8> = g.all(310).flat_map(|t| common::bytes_of(text(t))).collect();
            w.bl(u32::try_from(data.len()).unwrap_or(0));
            w.bytes(&data);
            if cx.ver >= V::R2000 {
                w.rc(u8::try_from(g.i(73)).unwrap_or(0));
            }
            w
        }
        "HATCH" => hatch(cx, r)?,
        "LEADER" => leader(cx, r),
        "TOLERANCE" => {
            let mut w = begin(cx, 0x2E, r);
            let g = r.sub("AcDbFcf");
            w.bd3(g.p3(10));
            w.bd3(if g.has(11) { g.p3(11) } else { [1.0, 0.0, 0.0] });
            w.bd3(g.extrusion());
            // A line break of the frame is `^J` in the record; the file stores the character.
            w.text(&decode_contents(g.s(1)));
            w.reference(Ref::HardPointer, dim_style(cx, g.s(3)));
            w
        }
        "VIEWPORT" => viewport(cx, r),
        "MLINE" => mline(cx, r),
        "IMAGE" | "WIPEOUT" => raster(cx, r)?,
        "MULTILEADER" => super::mleader::mleader(cx, r)?,
        "PDFUNDERLAY" => {
            // The plane, insertion point, rotation, three scales, flags, contrast, fade and clip
            // boundary; the definition is its one reference.
            let g = r.sub("AcDbUnderlayReference");
            let mut w = begin(cx, cx.class("PDFUNDERLAY")?, r);
            w.bd3(g.extrusion());
            w.bd3(g.p3(10));
            w.bd(g.f(50).to_radians());
            w.bd(g.f_or(41, 1.0));
            w.bd(g.f_or(42, 1.0));
            w.bd(g.f_or(43, 1.0));
            let byte = |code: i32, default: i64, top: i64| u8::try_from(g.i_or(code, default).clamp(0, top)).unwrap_or(0);
            w.rc(byte(280, 2, 255));
            w.rc(byte(281, 100, 100));
            w.rc(byte(282, 0, 100));
            let xs: Vec<f64> = g.all(11).map(Tag::f64).collect();
            let ys: Vec<f64> = g.all(21).map(Tag::f64).collect();
            let n = xs.len().min(ys.len());
            w.bl(count(n));
            for (x, y) in xs.iter().zip(&ys).take(n) {
                w.rd2([*x, *y]);
            }
            w.reference(Ref::HardPointer, cx.known(g.h(340)));
            w
        }
        "ACAD_TABLE" => super::table::table(cx, r)?,
        _ => return None,
    })
}

/// The width of one line of text at height 1 when nothing measures it: 0.8 of the height per
/// character. A DWG's MTEXT records its extents, which AutoCAD measures again when it
/// regenerates the text.
fn estimated_width(line: &str) -> f64 {
    line.chars().count() as f64 * 0.8
}

/// The fields of an MTEXT (`g`: the groups of its subclass; `all`: the record's, with an
/// embedded object's after group 101). Returns the text style to refer to.
pub(super) fn mtext_fields(cx: &Cx<'_>, w: &mut ObjectWriter, g: Groups<'_>, all: &[Tag]) -> u64 {
    let ver = cx.ver;
    // The MTEXT's own groups end where an embedded object starts.
    let own = Groups(g.0.get(..g.0.iter().position(|t| t.code == 101).unwrap_or(g.0.len())).unwrap_or(&[]));
    let embedded = all.iter().position(|t| t.code == 101).map(|i| Groups(all.get(i + 1..).unwrap_or(&[])));
    let insert = own.p3(10);
    let x_axis = if own.has(11) {
        own.p3(11)
    } else {
        let (sin, cos) = own.f(50).to_radians().sin_cos();
        [cos, sin, 0.0]
    };
    let height = own.f(40);
    let contents = decode_contents(&own.0.iter().filter(|t| t.code == 3 || t.code == 1).map(text).collect::<String>());
    let attach = u16::try_from(own.i_or(71, 1)).unwrap_or(1);
    let spacing = own.f_or(44, 1.0);
    // The extents: the record's, or an estimate from the text's lines.
    let plain = draftcraft_fonts::plain_mtext(&contents);
    let lines = plain.lines().count().max(1) as f64;
    let extents_width = own.get(42).map_or_else(|| plain.lines().map(estimated_width).fold(0.0, f64::max) * height, Tag::f64);
    let extents_height = own.get(43).map_or(height * (1.0 + (lines - 1.0) * 1.6667 * spacing), Tag::f64);
    let rect_height = own.f(46);
    w.bd3(insert);
    w.bd3(own.extrusion());
    w.bd3(x_axis);
    w.bd(own.f(41));
    if ver >= V::R2007 {
        w.bd(rect_height);
    }
    w.bd(height);
    w.bs(attach);
    w.bs(u16::try_from(own.i_or(72, 1)).unwrap_or(1));
    w.bd(extents_height);
    w.bd(extents_width);
    w.text(&contents);
    w.bs(u16::try_from(own.i_or(73, 1)).unwrap_or(1));
    w.bd(spacing);
    w.b(false);
    if ver >= V::R2004 {
        let background = u32::try_from(own.i(90)).unwrap_or(0);
        w.bl(background);
        if background & 3 != 0 {
            w.bd(own.f_or(45, 1.5));
            w.color(common::color(cx, Some(own.i_or(63, 256)), own.get(421).map(Tag::i64)));
            w.bl(u32::try_from(own.i(441)).unwrap_or(0));
        }
    }
    if ver >= V::R2018 {
        // Not annotative: the text carries its own copy of its frame, and its columns.
        w.b(true);
        w.bs(4);
        w.b(true);
        w.bl(u32::from(attach));
        w.bd3(x_axis);
        w.bd3(insert);
        let e = embedded.unwrap_or_default();
        w.bd(e.f_or(40, own.f(41)));
        w.bd(e.f_or(41, rect_height));
        w.bd(e.f_or(42, extents_width));
        w.bd(e.f_or(43, extents_height));
        let kind = u32::try_from(e.i(71)).unwrap_or(0);
        w.bl(kind);
        if kind != 0 {
            let auto_height = e.i(73) != 0;
            // The heights of dynamic columns of manual heights: the groups 46 after the count.
            let heights: Vec<f64> = if kind == 2 && !auto_height { e.from(72).all(46).map(Tag::f64).collect() } else { Vec::new() };
            // Dynamic columns of automatic height store no count: it follows from the extents.
            let count = if kind == 2 && auto_height { 0 } else { u32::try_from(e.i(72)).unwrap_or(0) };
            w.bl(if heights.is_empty() { count } else { u32::try_from(heights.len()).unwrap_or(0) });
            w.bd(e.f(44));
            w.bd(e.f(45));
            w.b(auto_height);
            w.b(e.i(74) != 0);
            for h in heights {
                w.bd(h);
            }
        }
    }
    cx.style(own.s(7))
}

/// DIMENSION: one of seven DWG types, by the kind in its flags.
fn dimension(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let ver = cx.ver;
    let g = r.sub("AcDbDimension");
    let flags = g.i(70);
    let code = match flags & 7 {
        0 => 0x15,
        1 => 0x16,
        2 => 0x18,
        3 => 0x1A,
        4 => 0x19,
        5 => 0x17,
        6 => 0x14,
        _ => return None,
    };
    let mut w = begin(cx, code, r);
    if ver >= V::R2010 {
        w.rc(0);
    }
    let mid = g.p3(11);
    w.bd3(g.extrusion());
    w.rd2([mid[0], mid[1]]);
    w.bd(mid[2]);
    // Bit 0: the text is where the style puts it (not placed by the user); bit 1: the block is
    // this dimension's alone; bit 2: an ordinate dimension of X; bit 3 is set in AutoCAD's files.
    w.rc(u8::from(flags & 128 == 0) | u8::from(flags & 32 != 0) << 1 | u8::from(flags & 64 != 0) << 2 | 8);
    w.text(&decode_line(g.s(1)));
    w.bd(g.f(53).to_radians());
    w.bd(g.f(51).to_radians());
    w.bd3([1.0, 1.0, 1.0]);
    w.bd(g.f(54).to_radians());
    w.bs(u16::try_from(g.i_or(71, 5)).unwrap_or(5));
    w.bs(u16::try_from(g.i_or(72, 1)).unwrap_or(1));
    w.bd(g.f_or(41, 1.0));
    w.bd(g.f(42));
    if ver >= V::R2007 {
        w.b(false);
        w.b(g.i(74) != 0);
        w.b(g.i(75) != 0);
    }
    w.rd2(g.p2(12));
    let defpt = g.p3(10);
    let (aligned, rotated) = (r.sub("AcDbAlignedDimension"), r.sub("AcDbRotatedDimension"));
    match code {
        0x14 => {
            let o = r.sub("AcDbOrdinateDimension");
            w.bd3(defpt);
            w.bd3(o.p3(13));
            w.bd3(o.p3(14));
            w.rc(u8::from(flags & 64 != 0));
        }
        0x15 | 0x16 => {
            w.bd3(aligned.p3(13));
            w.bd3(aligned.p3(14));
            w.bd3(defpt);
            // The oblique angle and the rotation are the aligned subclass's, or (AutoCAD's own
            // DXF) the rotated one's.
            let pick = |code: i32| aligned.get(code).or_else(|| rotated.get(code)).map_or(0.0, Tag::f64).to_radians();
            w.bd(pick(52));
            if code == 0x15 {
                w.bd(pick(50));
            }
        }
        0x17 => {
            let a = r.sub("AcDb3PointAngularDimension");
            w.bd3(defpt);
            w.bd3(a.p3(13));
            w.bd3(a.p3(14));
            w.bd3(a.p3(15));
        }
        0x18 => {
            let a = r.sub("AcDb2LineAngularDimension");
            w.rd2(a.p2(16));
            w.bd3(a.p3(13));
            w.bd3(a.p3(14));
            w.bd3(a.p3(15));
            w.bd3(defpt);
        }
        0x19 => {
            let a = r.sub("AcDbRadialDimension");
            w.bd3(defpt);
            w.bd3(a.p3(15));
            w.bd(a.f(40));
        }
        _ => {
            // The far end of the chord (15) first, then its near end (10).
            let a = r.sub("AcDbDiametricDimension");
            w.bd3(a.p3(15));
            w.bd3(defpt);
            w.bd(a.f(40));
        }
    }
    w.reference(Ref::HardPointer, dim_style(cx, g.s(3)));
    w.reference(Ref::HardPointer, cx.block(g.s(2)));
    Some(w)
}

/// The dimension style `name`, or Standard.
fn dim_style(cx: &Cx<'_>, name: &str) -> u64 {
    cx.dim_styles.get(&name.trim().to_uppercase()).or_else(|| cx.dim_styles.get("STANDARD")).copied().unwrap_or(0)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The fields of a SPLINE: by its fit points, or by its control points and knots.
pub(super) fn spline_fields(cx: &Cx<'_>, w: &mut ObjectWriter, g: Groups<'_>) {
    let ver = cx.ver;
    let flags = g.i(70);
    let knots: Vec<f64> = g.all(40).map(Tag::f64).collect();
    let weights: Vec<f64> = g.all(41).map(Tag::f64).collect();
    let points = |code: i32| -> Vec<[f64; 3]> {
        let mut out: Vec<[f64; 3]> = Vec::new();
        for t in g.0 {
            match (t.code - code, out.last_mut()) {
                (0, _) => out.push([t.f64(), 0.0, 0.0]),
                (10, Some(p)) => p[1] = t.f64(),
                (20, Some(p)) => p[2] = t.f64(),
                _ => {}
            }
        }
        out
    };
    let (control, fit) = (points(10), points(11));
    // By its fit points when it has no others; from AutoCAD 2013 whenever it has them (the
    // file then says so, and a reader fits the curve again).
    let by_fit = !fit.is_empty() && (control.is_empty() || ver >= V::R2013);
    w.bl(if by_fit { 2 } else { 1 });
    if ver >= V::R2013 {
        w.bl(u32::from(by_fit) | u32::from(flags & 1 != 0) << 2);
        // How the knots are spaced: by chord length for fit points; the spline's own for
        // control points.
        w.bl(if by_fit { 0 } else { 15 });
    }
    w.bl(u32::try_from(g.i_or(71, 3)).unwrap_or(3));
    if by_fit {
        w.bd(g.f_or(44, 1e-10));
        w.bd3(g.p3(12));
        w.bd3(g.p3(13));
        w.bl(count(fit.len()));
        for p in &fit {
            w.bd3(*p);
        }
        return;
    }
    let weighted = !weights.is_empty();
    w.b(flags & 4 != 0);
    w.b(flags & 1 != 0);
    w.b(flags & 2 != 0);
    w.bd(g.f_or(42, 1e-10));
    w.bd(g.f_or(43, 1e-10));
    w.bl(count(knots.len()));
    w.bl(count(control.len()));
    w.b(weighted);
    for k in &knots {
        w.bd(*k);
    }
    for (i, p) in control.iter().enumerate() {
        w.bd3(*p);
        if weighted {
            w.bd(weights.get(i).copied().unwrap_or(1.0));
        }
    }
}

/// A cursor over a record's groups, for the types whose groups mean what their place says.
struct Cursor<'a> {
    tags: &'a [Tag],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<i32> {
        self.tags.get(self.at).map(|t| t.code)
    }
    /// The next group when it is `code`.
    fn take(&mut self, code: i32) -> Option<&'a Tag> {
        let t = self.tags.get(self.at).filter(|t| t.code == code)?;
        self.at += 1;
        Some(t)
    }
    fn f(&mut self, code: i32) -> f64 {
        self.take(code).map_or(0.0, Tag::f64)
    }
    fn i(&mut self, code: i32) -> i64 {
        self.take(code).map_or(0, Tag::i64)
    }
    fn p2(&mut self, code: i32) -> [f64; 2] {
        [self.f(code), self.f(code + 10)]
    }
    /// Skip to the next group `code`; false when the record has no more.
    fn seek(&mut self, code: i32) -> bool {
        while let Some(c) = self.peek() {
            if c == code {
                return true;
            }
            self.at += 1;
        }
        false
    }
}

/// HATCH: the gradient (from AutoCAD 2004), the boundary loops, the pattern, the seed points.
fn hatch(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let ver = cx.ver;
    let g = r.sub("AcDbHatch");
    let mut w = begin(cx, 0x4E, r);
    if ver >= V::R2004 {
        // The gradient's groups come after the seed points.
        let grad = g.from(450);
        w.bl(u32::try_from(grad.i(450)).unwrap_or(0));
        w.bl(u32::try_from(grad.i(451)).unwrap_or(0));
        w.bd(grad.f(460));
        w.bd(grad.f(461));
        w.bl(u32::try_from(grad.i(452)).unwrap_or(0));
        w.bd(grad.f(462));
        // One colour per group 463, with the index (63) and true colour (421) after it.
        let starts: Vec<usize> = grad.0.iter().enumerate().filter(|(_, t)| t.code == 463).map(|(i, _)| i).collect();
        w.bl(count(starts.len()));
        for (n, &i) in starts.iter().enumerate() {
            let c = Groups(grad.0.get(i..starts.get(n + 1).copied().unwrap_or(grad.0.len())).unwrap_or(&[]));
            let c = Groups(c.0.get(..c.0.iter().position(|t| t.code == 470).unwrap_or(c.0.len())).unwrap_or(&[]));
            w.bd(c.f(463));
            w.color(common::color(cx, c.get(63).map(Tag::i64), c.get(421).map(Tag::i64)));
        }
        w.text(grad.s(470));
    }
    let head = Groups(g.0.get(..g.0.iter().position(|t| t.code == 91).unwrap_or(g.0.len())).unwrap_or(&[]));
    w.bd(head.f(30));
    w.bd3(head.extrusion());
    w.text(head.s(2));
    let solid = head.i(70) != 0;
    w.b(solid);
    w.b(head.i(71) != 0);
    let mut c = Cursor { tags: g.0, at: 0 };
    if !c.seek(91) {
        return None;
    }
    let loops = c.i(91);
    w.bl(u32::try_from(loops).ok()?);
    let mut sources: Vec<u64> = Vec::new();
    let mut derived = false;
    for _ in 0..loops {
        if !c.seek(92) {
            return None;
        }
        let flags = c.i(92);
        derived |= flags & 4 != 0;
        w.bl(u32::try_from(flags).unwrap_or(0));
        if flags & 2 != 0 {
            let bulges = c.i(72) != 0;
            let closed = c.i(73) != 0;
            let n = c.i(93);
            w.b(bulges);
            w.b(closed);
            w.bl(u32::try_from(n).ok()?);
            for _ in 0..n {
                c.peek()?;
                w.rd2(c.p2(10));
                let bulge = c.f(42);
                if bulges {
                    w.bd(bulge);
                }
            }
        } else {
            let n = c.i(93);
            w.bl(u32::try_from(n).ok()?);
            for _ in 0..n {
                c.peek()?;
                hatch_edge(cx, &mut w, &mut c)?;
            }
        }
        let n = c.i(97);
        w.bl(u32::try_from(n).ok()?);
        for _ in 0..n {
            sources.push(cx.known(super::rec::hex(text(c.take(330)?))));
        }
    }
    let rest = Groups(c.tags.get(c.at..).unwrap_or(&[]));
    let short = |v: i64| u16::try_from(v).unwrap_or(0);
    w.bs(short(rest.i(75)));
    w.bs(short(rest.i_or(76, 1)));
    if !solid {
        w.bd(rest.f(52).to_radians());
        w.bd(rest.f_or(41, 1.0));
        w.b(rest.i(77) != 0);
        // One line family per group 53.
        let starts: Vec<usize> = rest.0.iter().enumerate().filter(|(_, t)| t.code == 53).map(|(i, _)| i).collect();
        let end = rest.0.iter().position(|t| matches!(t.code, 47 | 98)).unwrap_or(rest.0.len());
        w.bs(u16::try_from(starts.len()).ok()?);
        for (n, &i) in starts.iter().enumerate() {
            let l = Groups(rest.0.get(i..starts.get(n + 1).copied().unwrap_or(end).min(end)).unwrap_or(&[]));
            w.bd(l.f(53).to_radians());
            w.bd2([l.f(43), l.f(44)]);
            w.bd2([l.f(45), l.f(46)]);
            let dashes: Vec<f64> = l.all(49).map(Tag::f64).collect();
            w.bs(u16::try_from(dashes.len()).ok()?);
            for d in dashes {
                w.bd(d);
            }
        }
    }
    if derived {
        w.bd(rest.f(47));
    }
    // The seed points: the pairs 10/20 after their count (98).
    let seeds = rest.from(98);
    let seeds = Groups(seeds.0.get(..seeds.0.iter().position(|t| t.code == 450).unwrap_or(seeds.0.len())).unwrap_or(&[]));
    let xs: Vec<f64> = seeds.all(10).map(Tag::f64).collect();
    let ys: Vec<f64> = seeds.all(20).map(Tag::f64).collect();
    w.bl(count(xs.len()));
    for (i, x) in xs.iter().enumerate() {
        w.rd2([*x, ys.get(i).copied().unwrap_or(0.0)]);
    }
    for s in sources {
        w.reference(Ref::SoftPointer, s);
    }
    Some(w)
}

/// One edge of a hatch's boundary loop: a line, a circular or elliptic arc, or a spline.
fn hatch_edge(cx: &Cx<'_>, w: &mut ObjectWriter, c: &mut Cursor<'_>) -> Option<()> {
    let kind = c.i(72);
    w.rc(u8::try_from(kind).ok()?);
    match kind {
        1 => {
            w.rd2(c.p2(10));
            w.rd2(c.p2(11));
        }
        2 => {
            w.rd2(c.p2(10));
            w.bd(c.f(40));
            w.bd(c.f(50).to_radians());
            w.bd(c.f(51).to_radians());
            w.b(c.i(73) != 0);
        }
        3 => {
            w.rd2(c.p2(10));
            w.rd2(c.p2(11));
            w.bd(c.f(40));
            w.bd(c.f(50).to_radians());
            w.bd(c.f(51).to_radians());
            w.b(c.i(73) != 0);
        }
        4 => {
            let degree = c.i(94);
            let rational = c.i(73) != 0;
            let periodic = c.i(74) != 0;
            let (knots, control) = (c.i(95), c.i(96));
            w.bl(u32::try_from(degree).ok()?);
            w.b(rational);
            w.b(periodic);
            w.bl(u32::try_from(knots).ok()?);
            w.bl(u32::try_from(control).ok()?);
            for _ in 0..knots {
                w.bd(c.take(40)?.f64());
            }
            for _ in 0..control {
                c.peek()?;
                w.rd2(c.p2(10));
                let weight = c.take(42).map_or(1.0, Tag::f64);
                if rational {
                    w.bd(weight);
                }
            }
            let fit = c.i(97);
            let mut points = Vec::new();
            for _ in 0..fit {
                c.peek()?;
                points.push(c.p2(11));
            }
            let tangents = (c.p2(12), c.p2(13));
            if cx.ver >= V::R2010 {
                w.bl(count(points.len()));
                for p in &points {
                    w.rd2(*p);
                }
                if !points.is_empty() {
                    w.rd2(tangents.0);
                    w.rd2(tangents.1);
                }
            }
        }
        _ => return None,
    }
    Some(())
}

fn leader(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.sub("AcDbLeader");
    let mut w = begin(cx, 0x2D, r);
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    for t in g.0 {
        match (t.code, vertices.last_mut()) {
            (10, _) => vertices.push([t.f64(), 0.0, 0.0]),
            (20, Some(p)) => p[1] = t.f64(),
            (30, Some(p)) => p[2] = t.f64(),
            _ => {}
        }
    }
    w.b(false);
    w.bs(u16::try_from(g.i_or(73, 3)).unwrap_or(3));
    w.bs(u16::try_from(g.i(72)).unwrap_or(0));
    w.bl(count(vertices.len()));
    for p in &vertices {
        w.bd3(*p);
    }
    w.bd3([0.0; 3]);
    w.bd3(g.extrusion());
    w.bd3(if g.has(211) { g.p3(211) } else { [1.0, 0.0, 0.0] });
    w.bd3(g.p3(212));
    w.bd3(g.p3(213));
    // The annotation's height and width, up to AutoCAD 2007.
    if cx.ver <= V::R2007 {
        w.bd(g.f(40));
        w.bd(g.f(41));
    }
    w.b(g.i(74) != 0);
    w.b(g.i_or(71, 1) != 0);
    w.bs(0);
    w.b(false);
    w.b(false);
    w.reference(Ref::HardPointer, cx.known(g.h(340)));
    w.reference(Ref::HardPointer, dim_style(cx, g.s(3)));
    w
}

/// VIEWPORT: the sheet rectangle, the view it shows, its settings, the layers frozen in it.
fn viewport(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let ver = cx.ver;
    let g = r.sub("AcDbViewport");
    let mut w = begin(cx, 0x22, r);
    let short = |v: i64| u16::try_from(v).unwrap_or(0);
    let frozen: Vec<u64> = g.all(331).chain(g.all(341)).map(|t| cx.known(super::rec::hex(text(t)))).filter(|h| *h != 0).collect();
    w.bd3(g.p3(10));
    w.bd(g.f(40));
    w.bd(g.f(41));
    w.bd3(g.p3(17));
    w.bd3(if g.has(16) || g.has(36) { g.p3(16) } else { [0.0, 0.0, 1.0] });
    w.bd(g.f(51).to_radians());
    w.bd(g.f_or(45, 1.0));
    w.bd(g.f_or(42, 50.0));
    w.bd(g.f(43));
    w.bd(g.f(44));
    w.bd(g.f(50).to_radians());
    w.rd2(g.p2(12));
    w.rd2(g.p2(13));
    w.rd2(if g.has(14) { g.p2(14) } else { [0.5, 0.5] });
    w.rd2(if g.has(15) { g.p2(15) } else { [0.5, 0.5] });
    w.bs(short(g.i_or(72, 1000)));
    if ver >= V::R2007 {
        w.bs(short(g.i_or(61, 5)));
    }
    w.bl(count(frozen.len()));
    w.bl(u32::try_from(g.i_or(90, 32768)).unwrap_or(32768));
    w.text(g.s(1));
    w.rc(u8::try_from(g.i(281)).unwrap_or(0));
    w.b(g.i_or(74, 1) != 0);
    w.b(g.i(71) != 0);
    w.bd3(g.p3(110));
    w.bd3(if g.has(111) { g.p3(111) } else { [1.0, 0.0, 0.0] });
    w.bd3(if g.has(112) { g.p3(112) } else { [0.0, 1.0, 0.0] });
    w.bd(g.f(146));
    w.bs(short(g.i(79)));
    if ver >= V::R2004 {
        w.bs(short(g.i(170)));
    }
    if ver >= V::R2007 {
        w.b(g.i_or(292, 1) != 0);
        w.rc(u8::try_from(g.i_or(282, 1)).unwrap_or(1));
        w.bd(g.f(141));
        w.bd(g.f(142));
        w.color(common::color(cx, Some(g.i_or(63, 250)), g.get(421).map(Tag::i64)));
    }
    for h in frozen {
        w.reference(if ver >= V::R2004 { Ref::SoftPointer } else { Ref::HardPointer }, h);
    }
    w.reference(Ref::SoftPointer, cx.known(g.h(340)));
    if ver == V::R2000 {
        w.reference(Ref::HardPointer, 0);
    }
    w.reference(Ref::HardPointer, cx.known(g.h(345)));
    w.reference(Ref::HardPointer, cx.known(g.h(346)));
    if ver >= V::R2007 {
        w.reference(Ref::SoftPointer, cx.known(g.h(332)));
        w.reference(Ref::HardPointer, cx.known(g.h(348)));
        w.reference(Ref::SoftPointer, cx.known(g.h(333)));
        w.reference(Ref::HardOwner, cx.known(g.h(361)));
    }
    w
}

/// MLINE: per vertex its point, direction and miter, and per element of the style the
/// parameters of its segment and of its fill.
fn mline(cx: &Cx<'_>, r: &Rec<'_>) -> ObjectWriter {
    let g = r.sub("AcDbMline");
    let mut w = begin(cx, 0x2F, r);
    let elements = usize::try_from(g.i(73)).unwrap_or(0).min(255);
    w.bd(g.f_or(40, 1.0));
    w.rc(u8::try_from(g.i(70)).unwrap_or(0));
    w.bd3(g.p3(10));
    w.bd3(g.extrusion());
    w.bs(u16::try_from(g.i(71)).unwrap_or(0));
    w.rc(u8::try_from(elements).unwrap_or(0));
    // One vertex per group 11.
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 11).map(|(i, _)| i).take(usize::from(u16::MAX)).collect();
    w.bs(u16::try_from(starts.len()).unwrap_or(u16::MAX));
    for (n, &i) in starts.iter().enumerate() {
        let v = g.0.get(i..starts.get(n + 1).copied().unwrap_or(g.0.len())).unwrap_or(&[]);
        let head = Groups(v);
        w.bd3(head.p3(11));
        w.bd3(head.p3(12));
        w.bd3(head.p3(13));
        let mut c = Cursor { tags: v, at: 0 };
        for _ in 0..elements {
            let mut list = |count_code: i32, value: i32, w: &mut ObjectWriter| {
                let mut values = Vec::new();
                if c.seek(count_code) {
                    let n = c.i(count_code);
                    for _ in 0..n.clamp(0, 1 << 16) {
                        match c.take(value) {
                            Some(t) => values.push(t.f64()),
                            None => break,
                        }
                    }
                }
                w.bs(u16::try_from(values.len()).unwrap_or(0));
                for x in values {
                    w.bd(x);
                }
            };
            list(74, 41, &mut w);
            list(75, 42, &mut w);
        }
    }
    w.reference(Ref::HardPointer, cx.known(g.h(340)));
    w
}

/// IMAGE and WIPEOUT: placement, size in pixels, display settings and clip boundary.
fn raster(cx: &Cx<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let g = r.own();
    let mut w = begin(cx, cx.class(r.kind)?, r);
    let size = g.p2(13);
    w.bl(u32::try_from(g.i(90)).unwrap_or(0));
    w.bd3(g.p3(10));
    w.bd3(g.p3(11));
    w.bd3(g.p3(12));
    w.rd2(size);
    w.bs(u16::try_from(g.i(70)).unwrap_or(0));
    w.b(g.i(280) != 0);
    let pct = |code: i32, default: i64| u8::try_from(g.i_or(code, default).clamp(0, 100)).unwrap_or(0);
    w.rc(pct(281, 50));
    w.rc(pct(282, 50));
    w.rc(pct(283, 0));
    if cx.ver >= V::R2010 {
        w.b(g.i(290) != 0);
    }
    let xs: Vec<f64> = g.all(14).map(Tag::f64).collect();
    let ys: Vec<f64> = g.all(24).map(Tag::f64).collect();
    let mut clip: Vec<[f64; 2]> = xs.iter().enumerate().map(|(i, x)| [*x, ys.get(i).copied().unwrap_or(0.0)]).collect();
    // No boundary is the whole image, pixel centres at whole numbers.
    if clip.len() < 2 {
        clip = vec![[-0.5, -0.5], [size[0] - 0.5, size[1] - 0.5]];
    }
    let rectangle = clip.len() == 2;
    w.bs(if rectangle { 1 } else { 2 });
    if !rectangle {
        w.bl(count(clip.len()));
    }
    for p in &clip {
        w.rd2(*p);
    }
    w.reference(Ref::HardPointer, cx.known(g.h(340)));
    w.reference(Ref::HardOwner, cx.known(g.h(360)));
    Some(w)
}
