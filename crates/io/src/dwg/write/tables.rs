//! The symbol tables: each table's control object and its records.

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{ObjectWriter, Ref};
use draftcraft_dwg::file::ColorValue;
use draftcraft_dxf::Tag;

use super::Cx;
use super::common::{self, lineweight};
use super::rec::{Groups, Rec, Table, text};

/// The DWG type of a table's control object and of its records.
fn types(table: &str) -> Option<(u16, u16)> {
    Some(match table.to_ascii_uppercase().as_str() {
        "BLOCK_RECORD" => (0x30, 0x31),
        "LAYER" => (0x32, 0x33),
        "STYLE" => (0x34, 0x35),
        "LTYPE" => (0x38, 0x39),
        "VIEW" => (0x3C, 0x3D),
        "UCS" => (0x3E, 0x3F),
        "VPORT" => (0x40, 0x41),
        "APPID" => (0x42, 0x43),
        "DIMSTYLE" => (0x44, 0x45),
        _ => return None,
    })
}

fn is_layer_linetype(name: &str) -> bool {
    name.eq_ignore_ascii_case("BYLAYER") || name.eq_ignore_ascii_case("BYBLOCK")
}

/// A table's control object: its records, which it owns. The block table lists model space and
/// paper space apart, the linetype table ByBlock and ByLayer.
pub(super) fn control(cx: &Cx<'_>, t: &Table<'_>) -> Option<ObjectWriter> {
    let (code, _) = types(t.name)?;
    let mut w = cx.writer(code, &t.head);
    w.object(0, &t.head.reactors, t.head.xdict);
    let apart = |r: &Rec<'_>| match code {
        0x30 => r.handle == cx.model || r.handle == cx.paper,
        0x38 => is_layer_linetype(r.all().s(2)),
        _ => false,
    };
    let entries: Vec<u64> = t.records.iter().filter(|r| !apart(r)).map(|r| r.handle).collect();
    w.bl(u32::try_from(entries.len()).unwrap_or(u32::MAX));
    if code == 0x44 {
        // The dimension style table counts its records again, and points at each.
        w.rc(u8::try_from(entries.len().min(255)).unwrap_or(u8::MAX));
    }
    for e in &entries {
        w.reference(Ref::SoftOwner, *e);
    }
    match code {
        0x44 => {
            for e in entries.iter().take(255) {
                w.reference(Ref::HardPointer, *e);
            }
        }
        0x30 => {
            w.reference(Ref::HardOwner, cx.model);
            w.reference(Ref::HardOwner, cx.paper);
        }
        0x38 => {
            w.reference(Ref::HardOwner, cx.linetype("BYBLOCK"));
            w.reference(Ref::HardOwner, cx.linetype("BYLAYER"));
        }
        _ => {}
    }
    Some(w)
}

/// What every table record starts with: its name, then (up to AutoCAD 2004) the "referenced"
/// flag, the index of the xref it came from, and the "depends on an xref" flag; from AutoCAD
/// 2007 only the index.
fn record_name(w: &mut ObjectWriter, name: &str, flags: i64) {
    w.text(name);
    if w.has_string_stream() {
        w.bs(0);
    } else {
        // "Referenced when the drawing was last saved": AutoCAD's files have it set on every
        // record, whatever their DXF says.
        let _ = flags & 64;
        w.b(true);
        w.bs(0);
        w.b(flags & 16 != 0);
    }
}

/// One table record.
pub(super) fn record(cx: &Cx<'_>, t: &Table<'_>, r: &Rec<'_>) -> Option<ObjectWriter> {
    let (_, code) = types(t.name)?;
    let mut w = cx.writer(code, r);
    let g = r.all();
    // The record's owner is its table (which a record names itself, when it does).
    let control = if r.owner != 0 { r.owner } else { t.head.handle };
    w.object(control, &r.reactors, r.xdict);
    match code {
        0x43 => {
            record_name(&mut w, g.s(2), g.i(70));
            w.rc(0);
            w.reference(Ref::HardPointer, 0);
        }
        0x33 => layer(cx, &mut w, r),
        0x35 => {
            record_name(&mut w, g.s(2), g.i(70));
            let flags = g.i(70);
            w.b(flags & 1 != 0);
            w.b(flags & 4 != 0);
            w.bd(g.f(40));
            w.bd(g.f_or(41, 1.0));
            w.bd(g.f(50).to_radians());
            w.rc(u8::try_from(g.i(71) & 0xFF).unwrap_or(0));
            w.bd(g.f_or(42, 0.2));
            w.text(g.s(3));
            w.text(g.s(4));
            w.reference(Ref::HardPointer, 0);
        }
        0x39 => linetype(cx, &mut w, r),
        0x31 => block_header(cx, &mut w, r),
        0x45 => dim_style(cx, &mut w, r),
        0x3F => {
            record_name(&mut w, g.s(2), g.i(70));
            w.bd3(g.p3(10));
            w.bd3(if g.has(11) { g.p3(11) } else { [1.0, 0.0, 0.0] });
            w.bd3(if g.has(12) { g.p3(12) } else { [0.0, 1.0, 0.0] });
            w.bd(g.f(146));
            w.bs(u16::try_from(g.i(79)).unwrap_or(0));
            // No origins for the six orthographic views of this UCS.
            w.bs(0);
            w.reference(Ref::HardPointer, 0);
            w.reference(Ref::HardPointer, cx.known(g.h(346)));
            w.reference(Ref::HardPointer, 0);
        }
        0x3D => view(cx, &mut w, r),
        0x41 => vport(cx, &mut w, r),
        _ => return None,
    }
    Some(w)
}

fn layer(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.all();
    let flags = g.i(70);
    record_name(w, g.s(2), flags);
    let index = g.i_or(62, 7);
    let mut word: u16 = 0;
    word |= u16::from(flags & 1 != 0);
    word |= u16::from(index < 0) << 1;
    word |= u16::from(flags & 2 != 0) << 2;
    word |= u16::from(flags & 4 != 0) << 3;
    word |= u16::from(g.i_or(290, 1) != 0) << 4;
    word |= u16::from(lineweight(g.i_or(370, -3))) << 5;
    w.bs(word);
    let c = common::color(cx, Some(index.abs().clamp(1, 255)), g.get(420).map(Tag::i64));
    if cx.ver < V::R2004 {
        // The sign marks a layer that is off here too.
        let i = match c {
            ColorValue::Index(i) => i16::from(i),
            _ => 7,
        };
        w.color_index(if index < 0 { -i } else { i });
    } else {
        // A colour book colour's names: `BOOK$NAME` in group 430.
        let named = g.get(430).map(text).filter(|_| matches!(c, ColorValue::Rgb(..)));
        match named.map(|n| n.split_once('$').map_or((n, None), |(b, n)| (n, Some(b)))) {
            Some((name, book)) => w.named_color(c, Some(name), book),
            None => w.color(c),
        }
    }
    w.reference(Ref::HardPointer, 0);
    w.reference(Ref::HardPointer, cx.known(g.h(390)));
    if cx.ver >= V::R2007 {
        w.reference(Ref::HardPointer, cx.known(g.h(347)));
    }
    let lt = g.s(6);
    w.reference(Ref::HardPointer, cx.linetype(if lt.is_empty() { "CONTINUOUS" } else { lt }));
    if cx.ver >= V::R2013 {
        w.reference(Ref::HardPointer, 0);
    }
}

/// One element of a linetype's pattern, from its groups (49 starts each).
struct Dash<'a> {
    g: Groups<'a>,
}

fn dashes<'a>(g: Groups<'a>) -> Vec<Dash<'a>> {
    let starts: Vec<usize> = g.0.iter().enumerate().filter(|(_, t)| t.code == 49).map(|(i, _)| i).collect();
    starts
        .iter()
        .enumerate()
        .take(12)
        .map(|(n, &i)| {
            let end = starts.get(n + 1).copied().unwrap_or(g.0.len());
            Dash { g: Groups(g.0.get(i..end).unwrap_or(&[])) }
        })
        .collect()
}

fn linetype(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.all();
    record_name(w, g.s(2), g.i(70));
    w.text(g.s(3));
    w.bd(g.f(40));
    w.rc(u8::try_from(g.i_or(72, 65)).unwrap_or(b'A'));
    let list = dashes(g);
    w.rc(u8::try_from(list.len()).unwrap_or(0));
    // The text of the text elements, one after another, each ended by a 0: 256 bytes up to
    // AutoCAD 2004, 512 bytes of UTF-16 from 2007 (there only when some element is text).
    let wide = cx.ver >= V::R2007;
    let mut area = vec![0u8; if wide { 512 } else { 256 }];
    let mut at = 0usize;
    let mut any_text = false;
    for d in &list {
        let flags = d.g.i(74);
        w.bd(d.g.f(49));
        let code = if flags & 2 != 0 {
            any_text = true;
            let start = at;
            let bytes: Vec<u8> = if wide { d.g.s(9).encode_utf16().flat_map(u16::to_le_bytes).collect() } else { w.ansi(d.g.s(9)) };
            let room = area.len().saturating_sub(at + if wide { 2 } else { 1 });
            let n = bytes.len().min(room);
            if let (Some(to), Some(from)) = (area.get_mut(at..at + n), bytes.get(..n)) {
                to.copy_from_slice(from);
            }
            at += n + if wide { 2 } else { 1 };
            u16::try_from(start).unwrap_or(0)
        } else {
            u16::try_from(d.g.i(75)).unwrap_or(0)
        };
        w.bs(code);
        w.rd(d.g.f(44));
        w.rd(d.g.f(45));
        w.bd(d.g.f_or(46, if flags & 6 != 0 { 1.0 } else { 0.0 }));
        w.bd(d.g.f(50).to_radians());
        w.bs(u16::try_from(flags).unwrap_or(0));
    }
    if !wide || any_text {
        w.bytes(&area);
    }
    w.reference(Ref::HardPointer, 0);
    for d in &list {
        w.reference(Ref::HardPointer, cx.known(d.g.h(340)));
    }
}

fn block_header(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.all();
    let name = g.s(2);
    // The BLOCK entity holds the definition's flags, base point, path and description.
    let begin = cx.doc.blocks.iter().find(|b| b.begin.owner == r.handle).map(|b| b.begin.sub("AcDbBlockBegin"));
    let b = begin.unwrap_or_default();
    let flags = b.i(70);
    let (xref, overlay) = (flags & 4 != 0, flags & 8 != 0);
    let owned: &[u64] = cx.owned.get(&r.handle).map_or(&[], Vec::as_slice);
    let has_attributes = cx.doc.blocks.iter().find(|d| d.begin.owner == r.handle).is_some_and(|d| d.entities.iter().any(|e| e.kind == "ATTDEF"));
    let stored = cx.block_names.get(&r.handle);
    record_name(w, stored.map_or(name, String::as_str), 0);
    // A name stored unnumbered is an anonymous block's, whatever flags its definition carried.
    w.b(flags & 1 != 0 || stored.is_some());
    w.b(has_attributes || flags & 2 != 0);
    w.b(xref);
    w.b(overlay);
    w.b(false);
    let lists = !xref && !overlay;
    if cx.ver >= V::R2004 && lists {
        w.bl(u32::try_from(owned.len()).unwrap_or(u32::MAX));
    }
    w.bd3(b.p3(10));
    w.text(b.s(1));
    let inserts: &[u64] = cx.inserts.get(&r.handle).map_or(&[], Vec::as_slice);
    for _ in inserts {
        w.rc(1);
    }
    w.rc(0);
    w.text(b.s(4));
    w.bl(0);
    if cx.ver >= V::R2007 {
        w.bs(u16::try_from(g.i(70)).unwrap_or(0));
        w.b(g.i_or(280, 1) != 0);
        w.rc(u8::try_from(g.i(281)).unwrap_or(0));
    }
    w.reference(Ref::HardPointer, 0);
    let (first, last) = cx.block_ends.get(&r.handle).copied().unwrap_or((0, 0));
    w.reference(Ref::HardOwner, first);
    if lists {
        if cx.ver >= V::R2004 {
            for e in owned {
                w.reference(Ref::HardOwner, *e);
            }
        } else {
            w.reference(Ref::SoftPointer, owned.first().copied().unwrap_or(0));
            w.reference(Ref::SoftPointer, owned.last().copied().unwrap_or(0));
        }
    }
    w.reference(Ref::HardOwner, last);
    for i in inserts {
        w.reference(Ref::SoftPointer, *i);
    }
    w.reference(Ref::HardPointer, cx.known(g.h(340)));
}

/// A dimension variable's value when the record has no group for it: AutoCAD's default.
fn dim_default(code: i32) -> f64 {
    match code {
        40 | 144 | 146 | 49 => 1.0,
        41 | 44 | 140 => 0.18,
        42 => 0.0625,
        43 => 0.38,
        141 | 147 => 0.09,
        143 => 25.4,
        50 => std::f64::consts::FRAC_PI_4,
        73 | 74 | 283 => 1.0,
        171 | 273 | 274 | 277 => 2.0,
        271 | 272 => 4.0,
        278 => 46.0,
        289 => 3.0,
        295 | 297 => 100.0,
        371 | 372 => -2.0,
        _ => 0.0,
    }
}

fn dim_style(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.sub("AcDbDimStyleTableRecord");
    let g = if g.0.is_empty() { r.all() } else { g };
    let ver = cx.ver;
    let f = |code: i32| g.f_or(code, dim_default(code));
    let i = |code: i32| g.i_or(code, dim_default(code) as i64);
    let int = |code: i32| u16::try_from(i(code)).unwrap_or_else(|_| i(code) as i16 as u16);
    let color = |code: i32| common::color(cx, Some(i(code)), None);
    record_name(w, g.s(2), g.i(70));
    w.text(g.s(3));
    w.text(g.s(4));
    for code in [40, 41, 42, 43, 44, 45, 46, 47, 48] {
        w.bd(f(code));
    }
    if ver >= V::R2007 {
        w.bd(f(49));
        w.bd(f(50));
        w.bs(int(69));
        w.color(color(70));
    }
    for code in [71, 72, 73, 74, 75, 76] {
        w.b(i(code) != 0);
    }
    for code in [77, 78, 79] {
        w.bs(int(code));
    }
    if ver >= V::R2007 {
        w.bs(int(90));
    }
    for code in [140, 141, 142, 143, 144, 145, 146, 147, 148] {
        w.bd(f(code));
    }
    w.b(i(170) != 0);
    w.bs(int(171));
    for code in [172, 173, 174, 175] {
        w.b(i(code) != 0);
    }
    for code in [176, 177, 178] {
        w.color(color(code));
    }
    for code in [179, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280] {
        w.bs(int(code));
    }
    for code in [281, 282] {
        w.b(i(code) != 0);
    }
    for code in [283, 284, 285, 286] {
        w.bs(int(code));
    }
    w.b(i(288) != 0);
    w.bs(int(289));
    if ver >= V::R2007 {
        w.b(i(290) != 0);
    }
    if ver >= V::R2010 {
        w.b(i(294) != 0);
        w.bd(f(295));
        w.text(g.s(296));
        w.bd(f(297));
        w.text(g.s(298));
    }
    w.bs(int(371));
    w.bs(int(372));
    w.b(false);
    w.reference(Ref::HardPointer, 0);
    let style = match g.h(340) {
        0 => cx.style("STANDARD"),
        h => cx.known(h),
    };
    w.reference(Ref::HardPointer, style);
    for code in [341, 342, 343, 344] {
        w.reference(Ref::HardPointer, cx.known(g.h(code)));
    }
    if ver >= V::R2007 {
        for code in [345, 346, 347] {
            w.reference(Ref::HardPointer, cx.known(g.h(code)));
        }
    }
}

fn view(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.all();
    let ver = cx.ver;
    record_name(w, g.s(2), g.i(70));
    w.bd(g.f(40));
    w.bd(g.f(41));
    w.rd2(g.p2(10));
    w.bd3(g.p3(12));
    w.bd3(if g.has(11) || g.has(31) { g.p3(11) } else { [0.0, 0.0, 1.0] });
    w.bd(g.f(50).to_radians());
    w.bd(g.f_or(42, 50.0));
    w.bd(g.f(43));
    w.bd(g.f(44));
    let mode = g.i(71);
    w.b(mode & 1 != 0);
    w.b(mode & 2 != 0);
    w.b(mode & 4 != 0);
    w.b(mode & 16 == 0);
    w.rc(u8::try_from(g.i(281)).unwrap_or(0));
    if ver >= V::R2007 {
        w.b(g.i_or(292, 1) != 0);
        w.rc(u8::try_from(g.i_or(282, 1)).unwrap_or(1));
        w.bd(g.f(141));
        w.bd(g.f(142));
        w.color(common::color(cx, Some(g.i_or(63, 250)), g.get(421).map(Tag::i64)));
    }
    w.b(g.i(70) & 1 != 0);
    let ucs = g.i(72) != 0;
    w.b(ucs);
    if ucs {
        w.bd3(g.p3(110));
        w.bd3(if g.has(111) { g.p3(111) } else { [1.0, 0.0, 0.0] });
        w.bd3(if g.has(112) { g.p3(112) } else { [0.0, 1.0, 0.0] });
        w.bd(g.f(146));
        w.bs(u16::try_from(g.i(79)).unwrap_or(0));
    }
    if ver >= V::R2007 {
        w.b(g.i(73) != 0);
    }
    w.reference(Ref::HardPointer, 0);
    if ver >= V::R2007 {
        w.reference(Ref::SoftPointer, cx.known(g.h(332)));
        w.reference(Ref::HardPointer, cx.known(g.h(348)));
        w.reference(Ref::HardOwner, cx.known(g.h(361)));
    }
    if ucs {
        w.reference(Ref::HardPointer, cx.known(g.h(346)));
        w.reference(Ref::HardPointer, cx.known(g.h(345)));
    }
    if ver >= V::R2007 {
        w.reference(Ref::SoftPointer, cx.known(g.h(334)));
    }
}

fn vport(cx: &Cx<'_>, w: &mut ObjectWriter, r: &Rec<'_>) {
    let g = r.all();
    let ver = cx.ver;
    record_name(w, g.s(2), g.i(70));
    let height = g.f_or(40, 1.0);
    w.bd(height);
    // The file holds the view's width: the aspect ratio times the height.
    w.bd(g.f_or(41, 1.0) * height);
    w.rd2(g.p2(12));
    w.bd3(g.p3(17));
    w.bd3(if g.has(16) || g.has(36) { g.p3(16) } else { [0.0, 0.0, 1.0] });
    w.bd(g.f(51).to_radians());
    w.bd(g.f_or(42, 50.0));
    w.bd(g.f(43));
    w.bd(g.f(44));
    let mode = g.i(71);
    w.b(mode & 1 != 0);
    w.b(mode & 2 != 0);
    w.b(mode & 4 != 0);
    w.b(mode & 16 == 0);
    w.rc(u8::try_from(g.i(281)).unwrap_or(0));
    if ver >= V::R2007 {
        w.b(g.i_or(292, 1) != 0);
        w.rc(u8::try_from(g.i_or(282, 1)).unwrap_or(1));
        w.bd(g.f(141));
        w.bd(g.f(142));
        w.color(common::color(cx, Some(g.i_or(63, 250)), g.get(421).map(Tag::i64)));
    }
    w.rd2(g.p2(10));
    w.rd2(if g.has(11) { g.p2(11) } else { [1.0, 1.0] });
    w.b(mode & 8 != 0);
    w.bs(u16::try_from(g.i_or(72, 1000)).unwrap_or(1000));
    w.b(g.i_or(73, 1) != 0);
    w.bb(u8::try_from(g.i_or(74, 3) & 3).unwrap_or(3));
    w.b(g.i(76) != 0);
    w.rd2(g.p2(15));
    w.b(g.i(75) != 0);
    w.b(g.i(77) != 0);
    w.bs(u16::try_from(g.i(78)).unwrap_or(0));
    w.bd(g.f(50).to_radians());
    w.rd2(g.p2(13));
    w.rd2(g.p2(14));
    w.b(false);
    w.b(g.i_or(65, 1) != 0);
    w.bd3(g.p3(110));
    w.bd3(if g.has(111) { g.p3(111) } else { [1.0, 0.0, 0.0] });
    w.bd3(if g.has(112) { g.p3(112) } else { [0.0, 1.0, 0.0] });
    w.bd(g.f(146));
    w.bs(u16::try_from(g.i(79)).unwrap_or(0));
    if ver >= V::R2007 {
        w.bs(u16::try_from(g.i_or(60, 3)).unwrap_or(3));
        w.bs(u16::try_from(g.i_or(61, 5)).unwrap_or(5));
    }
    w.reference(Ref::HardPointer, 0);
    if ver >= V::R2007 {
        w.reference(Ref::SoftPointer, cx.known(g.h(332)));
        w.reference(Ref::HardPointer, cx.known(g.h(348)));
        w.reference(Ref::HardOwner, cx.known(g.h(361)));
    }
    w.reference(Ref::HardPointer, cx.known(g.h(345)));
    w.reference(Ref::HardPointer, cx.known(g.h(346)));
}
