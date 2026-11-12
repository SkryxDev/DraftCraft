//! The entities of a DWG file, into the drawing: each is decoded field by field (Open Design
//! Alliance specification, chapter 20) straight into the drawing's own entity, with the
//! conventions the drawing keeps: angles in radians, the points of flat entities out of their
//! object coordinate system, and a mirrored one (extrusion pointing down) as the geometry it draws.

use std::collections::{HashMap, HashSet};

use draftcraft_doc::{
    Arc, Circle, Common, Drawing, Ellipse, Entity, EntityKind, Face3d, HAlign, Handle, Insert, Line, Lineweight, LwPolyline, MText, Point, RawTag,
    RayLine, Solid, Text, VAlign,
};
use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::file::{DwgFile, EntityLinetype, EntityRefs, EntitySpace, ObjectReader, ObjectRef, XDataBlock, XDataItem};
use draftcraft_dwg::records::Result as DwgResult;
use draftcraft_geom::{PolyVertex, Vec2, Vec3};

use super::Unread;
use super::blocks::{Blocks, Space};
use super::tables::Tables;
use crate::dxf_read::{mirror_text, mtext_dxf, ocs_n, transparency_440};

/// Most entities of one drawing that are read.
const MAX_ENTITIES: usize = 1 << 24;

/// The names an entity's references resolve to.
struct Names<'a> {
    tables: &'a Tables,
    blocks: &'a HashMap<u64, String>,
}

/// One entity's place: the block record that owns it, and the entity after it in the chain.
struct Place {
    record: u64,
    next: Option<u64>,
}

/// What reading one entity gave: the entity, when its type is read, and what of it was not.
struct Decoded {
    entity: Option<Entity>,
    partial: Vec<&'static str>,
}

fn v3(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], p[2])
}

/// The properties every entity has.
fn common(r: &ObjectReader, refs: &EntityRefs, n: &Names, partial: &mut Vec<&'static str>) -> Common {
    let mut c = Common::default();
    let Some(head) = r.entity() else { return c };
    c.layer = n.tables.layers.get(&refs.layer).cloned().unwrap_or_else(|| "0".into());
    if let Some(color) = super::tables::color(head.color) {
        c.color = color;
    }
    c.linetype = match refs.linetype {
        EntityLinetype::ByLayer => "ByLayer".into(),
        EntityLinetype::ByBlock => "ByBlock".into(),
        EntityLinetype::Continuous => "Continuous".into(),
        EntityLinetype::Record(h) => n.tables.linetypes.get(&h).cloned().unwrap_or_else(|| "ByLayer".into()),
    };
    c.lineweight = Lineweight::from_dxf(i16::try_from(super::header::lineweight(i64::from(head.lineweight))).unwrap_or(-1));
    c.ltscale = if head.linetype_scale.is_finite() && head.linetype_scale > 0.0 { head.linetype_scale } else { 1.0 };
    if let Some(t) = head.transparency {
        c.transparency = transparency_440(i64::from(t));
    }
    c.visible = !head.invisible;
    // What the entity refers to that is read with the objects.
    if refs.plotstyle != 0 {
        partial.push("entity plot style");
    }
    if refs.material != 0 {
        partial.push("entity material");
    }
    if refs.xdictionary != 0 {
        partial.push("entity extension dictionary");
    }
    if !refs.reactors.is_empty() {
        partial.push("entity reactors");
    }
    c.xdata = xdata(&r.xdata(), n, partial);
    c
}

/// Other applications' extended data as the groups the drawing keeps with the entity, by the DXF
/// reader's rules: the applications that reader turns into properties are not kept as groups (and
/// are counted, as this reader doesn't read them yet), and an application whose list braces don't
/// balance is dropped.
fn xdata(blocks: &[XDataBlock], n: &Names, partial: &mut Vec<&'static str>) -> Vec<RawTag> {
    let real = |code: i32, v: f64| RawTag { code, value: draftcraft_dxf::Value::Real(v).as_str() };
    let mut out = Vec::new();
    for b in blocks {
        let Some(name) = n.tables.apps.get(&b.app) else {
            partial.push("extended data of an unknown application");
            continue;
        };
        if crate::dxf_raw::OWN_APPS.contains(&name.to_ascii_uppercase().as_str()) {
            partial.push("extended data read as properties");
            continue;
        }
        let mut groups = vec![RawTag { code: 1001, value: name.clone() }];
        let (mut depth, mut ok) = (0i32, true);
        for v in &b.items {
            let pt = |base: i32, p: &[f64; 3]| [real(base, p[0]), real(base + 10, p[1]), real(base + 20, p[2])];
            match v {
                XDataItem::Text(t) => groups.push(RawTag { code: 1000, value: t.clone() }),
                XDataItem::Open => {
                    depth += 1;
                    groups.push(RawTag { code: 1002, value: "{".into() });
                }
                XDataItem::Close => {
                    depth -= 1;
                    ok &= depth >= 0;
                    groups.push(RawTag { code: 1002, value: "}".into() });
                }
                XDataItem::Layer(h) => groups.push(RawTag { code: 1003, value: n.tables.layers.get(h).cloned().unwrap_or_else(|| "0".into()) }),
                XDataItem::Bytes(bytes) => groups.push(RawTag { code: 1004, value: bytes.iter().map(|x| format!("{x:02X}")).collect() }),
                XDataItem::Handle(h) => groups.push(RawTag { code: 1005, value: format!("{h:X}") }),
                XDataItem::Point(p) => groups.extend(pt(1010, p)),
                XDataItem::Position(p) => groups.extend(pt(1011, p)),
                XDataItem::Displacement(p) => groups.extend(pt(1012, p)),
                XDataItem::Direction(p) => groups.extend(pt(1013, p)),
                XDataItem::Real(x) => groups.push(real(1040, *x)),
                XDataItem::Distance(x) => groups.push(real(1041, *x)),
                XDataItem::ScaleFactor(x) => groups.push(real(1042, *x)),
                XDataItem::Int16(i) => groups.push(RawTag { code: 1070, value: i.to_string() }),
                XDataItem::Int32(i) => groups.push(RawTag { code: 1071, value: i.to_string() }),
                XDataItem::Undecoded(_) => {
                    partial.push("undecoded extended data");
                    ok = false;
                }
            }
        }
        if ok && depth == 0 && out.len() + groups.len() <= crate::dxf_raw::MAX_XDATA_GROUPS {
            out.extend(groups);
        }
    }
    out
}

/// The entity types read so far.
fn reads(type_name: &str) -> bool {
    matches!(
        type_name,
        "LINE"
            | "POINT"
            | "CIRCLE"
            | "ARC"
            | "ELLIPSE"
            | "LWPOLYLINE"
            | "TEXT"
            | "MTEXT"
            | "INSERT"
            | "MINSERT"
            | "SOLID"
            | "TRACE"
            | "3DFACE"
            | "XLINE"
            | "RAY"
    )
}

/// The fields of one entity's type, held until its references (a text style, a block) are read.
enum Body {
    Ready(Box<EntityKind>),
    /// A text, with its alignment and insertion points in its own coordinate system.
    Text(Box<Text>, Option<Vec3>, Vec3),
    MText(Box<MText>),
    /// A block reference, and whether it has attributes.
    Insert(Box<Insert>, bool),
}

/// What an entity type's fields give: the body, and the thickness and extrusion of the types that
/// have them.
struct Shape {
    body: Body,
    thickness: f64,
    normal: Vec3,
}

const UP: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

fn ready(kind: EntityKind, thickness: f64, normal: Vec3) -> Shape {
    Shape { body: Body::Ready(Box::new(kind)), thickness, normal }
}

/// LINE (§20.4.17). From AutoCAD 2000 on each end's coordinates are stored against the start's.
fn line(r: &mut ObjectReader) -> DwgResult<Shape> {
    let (a, b) = if r.version() <= V::R14 {
        (v3(r.bd3()?), v3(r.bd3()?))
    } else {
        let flat = r.b()?;
        let ax = r.rd()?;
        let bx = r.dd(ax)?;
        let ay = r.rd()?;
        let by = r.dd(ay)?;
        let (az, bz) = if flat {
            (0.0, 0.0)
        } else {
            let az = r.rd()?;
            (az, r.dd(az)?)
        };
        (Vec3::new(ax, ay, az), Vec3::new(bx, by, bz))
    };
    let thickness = r.thickness()?;
    let normal = v3(r.extrusion()?);
    Ok(ready(EntityKind::Line(Line { a, b }), thickness, normal))
}

/// POINT (§20.4.27).
fn point(r: &mut ObjectReader) -> DwgResult<Shape> {
    let p = v3(r.bd3()?);
    let thickness = r.thickness()?;
    let normal = v3(r.extrusion()?);
    let angle = r.bd()?;
    Ok(ready(EntityKind::Point(Point { p, angle }), thickness, normal))
}

/// CIRCLE and ARC (§20.4.18, §20.4.19): the centre in the object coordinate system.
fn circle_or_arc(r: &mut ObjectReader, arc: bool) -> DwgResult<Shape> {
    let center = v3(r.bd3()?);
    let radius = r.bd()?.abs();
    let thickness = r.thickness()?;
    let normal = v3(r.extrusion()?);
    let center = ocs_n(normal, center);
    if !arc {
        return Ok(ready(EntityKind::Circle(Circle { center, radius }), thickness, normal));
    }
    let mut a = Arc { center, radius, start: r.bd()?, end: r.bd()? };
    // In a mirrored coordinate system the arc runs the other way.
    if normal.z < 0.0 {
        let (s, e) = (std::f64::consts::PI - a.end, std::f64::consts::PI - a.start);
        a.start = draftcraft_geom::norm_angle(s);
        a.end = draftcraft_geom::norm_angle(e);
    }
    Ok(ready(EntityKind::Arc(a), thickness, normal))
}

/// ELLIPSE (§20.4.35).
fn ellipse(r: &mut ObjectReader) -> DwgResult<Shape> {
    let center = v3(r.bd3()?);
    let major = v3(r.bd3()?);
    let normal = v3(r.bd3()?);
    let mut e = Ellipse { center, major, ratio: r.bd()?.clamp(1e-9, 1.0), start: r.bd()?, end: r.bd()? };
    // Centre and major axis are in world coordinates, but the minor axis is extrusion × major:
    // mirrored, a partial ellipse runs the other way.
    if normal.z < 0.0 && (draftcraft_geom::ccw_sweep(e.start, e.end) - std::f64::consts::TAU).abs() > 1e-9 {
        let (s, t) = (-e.end, -e.start);
        e.start = draftcraft_geom::norm_angle(s);
        e.end = draftcraft_geom::norm_angle(t);
    }
    Ok(ready(EntityKind::Ellipse(e), 0.0, normal))
}

/// LWPOLYLINE (§20.4.85): flags saying which fields are stored, then the counts and the lists.
fn lwpolyline(r: &mut ObjectReader) -> DwgResult<Shape> {
    let ver = r.version();
    let flags = r.bs()?;
    let const_width = if flags & 4 != 0 { r.bd()? } else { 0.0 };
    let elevation = if flags & 8 != 0 { r.bd()? } else { 0.0 };
    let thickness = if flags & 2 != 0 { r.bd()? } else { 0.0 };
    let normal = if flags & 1 != 0 { v3(r.bd3()?) } else { UP };
    let points = r.bl()?;
    let bulges = if flags & 16 != 0 { r.bl()? } else { 0 };
    let ids = if ver >= V::R2010 && flags & 1024 != 0 { r.bl()? } else { 0 };
    let widths = if flags & 32 != 0 { r.bl()? } else { 0 };
    let points = r.count(points, 4, "LWPOLYLINE point")?;
    let bulges = r.count(bulges, 2, "LWPOLYLINE bulge")?;
    let ids = r.count(ids, 2, "LWPOLYLINE vertex id")?;
    let widths = r.count(widths, 4, "LWPOLYLINE width")?;
    let mut vertices: Vec<PolyVertex> = Vec::with_capacity(points);
    let mut prev = Vec2::ZERO;
    for i in 0..points {
        // From AutoCAD 2000 on each point is stored against the one before.
        let p = if ver <= V::R14 || i == 0 {
            let [x, y] = r.rd2()?;
            Vec2::new(x, y)
        } else {
            let x = r.dd(prev.x)?;
            Vec2::new(x, r.dd(prev.y)?)
        };
        prev = p;
        vertices.push(PolyVertex { p, start_width: 0.0, end_width: 0.0, bulge: 0.0 });
    }
    for i in 0..bulges {
        let b = r.bd()?;
        if let Some(v) = vertices.get_mut(i) {
            v.bulge = b;
        }
    }
    for _ in 0..ids {
        r.bl()?;
    }
    for i in 0..widths {
        let (s, e) = (r.bd()?, r.bd()?);
        if let Some(v) = vertices.get_mut(i) {
            (v.start_width, v.end_width) = (s, e);
        }
    }
    // The vertices are in the polyline's own coordinate system: mirrored, x runs the other way
    // and the arcs bulge the other way.
    if normal.z < 0.0 {
        for v in &mut vertices {
            v.p.x = -v.p.x;
            v.bulge = -v.bulge;
        }
    }
    let p = LwPolyline { vertices, closed: flags & 512 != 0, const_width, elevation, plinegen: flags & 256 != 0 };
    Ok(ready(EntityKind::LwPolyline(p), thickness, normal))
}

/// TEXT (§20.4.3). From AutoCAD 2000 on a flag byte says which fields have their default and are
/// left out.
fn text(r: &mut ObjectReader) -> DwgResult<Shape> {
    let (mut elevation, mut oblique, mut rotation, mut width_factor) = (0.0, 0.0, 0.0, 1.0);
    let (mut h_align, mut v_align) = (0u16, 0u16);
    let (insertion, alignment, height, value, normal, thickness);
    if r.version() <= V::R14 {
        elevation = r.bd()?;
        insertion = r.rd2()?;
        alignment = r.rd2()?;
        normal = v3(r.bd3()?);
        thickness = r.bd()?;
        oblique = r.bd()?;
        rotation = r.bd()?;
        height = r.bd()?;
        width_factor = r.bd()?;
        value = r.text()?;
        let _generation = r.bs()?;
        h_align = r.bs()?;
        v_align = r.bs()?;
    } else {
        let flags = r.rc()?;
        if flags & 0x01 == 0 {
            elevation = r.rd()?;
        }
        insertion = r.rd2()?;
        alignment = if flags & 0x02 == 0 { [r.dd(insertion[0])?, r.dd(insertion[1])?] } else { insertion };
        normal = v3(r.extrusion()?);
        thickness = r.thickness()?;
        if flags & 0x04 == 0 {
            oblique = r.rd()?;
        }
        if flags & 0x08 == 0 {
            rotation = r.rd()?;
        }
        height = r.rd()?;
        if flags & 0x10 == 0 {
            width_factor = r.rd()?;
        }
        value = r.text()?;
        if flags & 0x20 == 0 {
            let _generation = r.bs()?;
        }
        if flags & 0x40 == 0 {
            h_align = r.bs()?;
        }
        if flags & 0x80 == 0 {
            v_align = r.bs()?;
        }
    }
    let halign = match h_align {
        1 => HAlign::Center,
        2 => HAlign::Right,
        3 => HAlign::Aligned,
        4 => HAlign::Middle,
        5 => HAlign::Fit,
        _ => HAlign::Left,
    };
    let valign = match v_align {
        1 => VAlign::Bottom,
        2 => VAlign::Middle,
        3 => VAlign::Top,
        _ => VAlign::Baseline,
    };
    let at = |p: [f64; 2]| Vec3::new(p[0], p[1], elevation);
    let plain = halign == HAlign::Left && valign == VAlign::Baseline;
    let t = Text {
        insert: at(insertion),
        align_pt: (!plain).then(|| at(alignment)),
        height: height.max(1e-9),
        value: mtext_dxf::decode_line(&value),
        rotation,
        width_factor,
        oblique,
        style: "Standard".into(),
        halign,
        valign,
    };
    Ok(Shape { body: Body::Text(Box::new(t), Some(at(alignment)), at(insertion)), thickness, normal })
}

/// MTEXT (§20.4.46).
fn mtext(r: &mut ObjectReader, partial: &mut Vec<&'static str>) -> DwgResult<Shape> {
    let ver = r.version();
    let insert = v3(r.bd3()?);
    let normal = v3(r.bd3()?);
    let x_axis = v3(r.bd3()?);
    let width = r.bd()?;
    if ver >= V::R2007 {
        let _rect_height = r.bd()?;
    }
    let height = r.bd()?;
    let attach = r.bs()?;
    let _direction = r.bs()?;
    let _extents_height = r.bd()?;
    let _extents_width = r.bd()?;
    let contents = r.text()?;
    let (mut line_spacing, mut line_spacing_exact) = (1.0, false);
    if ver >= V::R2000 {
        line_spacing_exact = r.bs()? == 2;
        line_spacing = r.bd()?;
        let _unknown = r.b()?;
    }
    // A background fill follows from AutoCAD 2004 on, and column data in AutoCAD 2018.
    if ver >= V::R2004 && r.bl().is_ok_and(|background| background != 0) {
        partial.push("MTEXT background fill");
    }
    let mut rotation = Vec2::new(x_axis.x, x_axis.y).angle();
    let mut attach = u8::try_from(attach.clamp(1, 9)).unwrap_or(1);
    // Mirrored, the glyphs run the other way: drawn readable with the same footprint, the text
    // turns round and left and right attachment swap.
    if normal.z < 0.0 {
        rotation = draftcraft_geom::norm_angle(rotation + std::f64::consts::PI);
        attach = match attach {
            1 | 4 | 7 => attach + 2,
            3 | 6 | 9 => attach - 2,
            a => a,
        };
    }
    let t = MText {
        insert,
        height: height.max(1e-9),
        width: width.max(0.0),
        attach,
        rotation,
        style: "Standard".into(),
        contents: mtext_dxf::decode_contents(&contents),
        line_spacing,
        line_spacing_exact,
        columns: None,
    };
    Ok(Shape { body: Body::MText(Box::new(t)), thickness: 0.0, normal })
}

/// INSERT and MINSERT (§20.4.7, §20.4.8). From AutoCAD 2000 on the scale is stored against what it
/// usually is: all three equal, or 1.
fn insert(r: &mut ObjectReader, minsert: bool) -> DwgResult<Shape> {
    let ver = r.version();
    let mut p = v3(r.bd3()?);
    let mut scale = if ver <= V::R14 {
        v3(r.bd3()?)
    } else {
        match r.bb()? {
            0 => {
                let x = r.rd()?;
                let y = r.dd(x)?;
                Vec3::new(x, y, r.dd(x)?)
            }
            1 => {
                let y = r.dd(1.0)?;
                Vec3::new(1.0, y, r.dd(1.0)?)
            }
            2 => {
                let x = r.rd()?;
                Vec3::new(x, x, x)
            }
            _ => Vec3::new(1.0, 1.0, 1.0),
        }
    };
    let mut rotation = r.bd()?;
    let normal = v3(r.bd3()?);
    let has_attributes = r.b()?;
    if ver >= V::R2004 && has_attributes {
        let _owned = r.bl()?;
    }
    let (mut cols, mut rows, mut col_spacing, mut row_spacing) = (1u32, 1u32, 0.0, 0.0);
    if minsert {
        cols = u32::from(r.bs()?);
        rows = u32::from(r.bs()?);
        col_spacing = r.bd()?;
        row_spacing = r.bd()?;
    }
    // A mirrored block reference: in the world that is the mirrored point, a negative x scale,
    // the opposite rotation and a negative column spacing. A truly tilted one is left as read.
    if normal.z < 0.0 && normal.x.hypot(normal.y) <= 1e-3 * normal.z.abs() {
        p = ocs_n(normal, p);
        scale.x = -scale.x;
        rotation = -rotation;
        col_spacing = -col_spacing;
    }
    let i = Insert {
        block: String::new(),
        insert: p,
        scale,
        rotation,
        attribs: Vec::new(),
        cols: cols.clamp(1, 10_000),
        rows: rows.clamp(1, 10_000),
        col_spacing,
        row_spacing,
        clip: None,
    };
    Ok(Shape { body: Body::Insert(Box::new(i), has_attributes), thickness: 0.0, normal })
}

/// SOLID and TRACE (§20.4.31, §20.4.32): four corners in the object coordinate system.
fn solid(r: &mut ObjectReader, trace: bool) -> DwgResult<Shape> {
    let thickness = r.thickness()?;
    let elevation = r.bd()?;
    let mut flat = [[0.0; 2]; 4];
    for c in &mut flat {
        *c = r.rd2()?;
    }
    let normal = v3(r.extrusion()?);
    let corners = flat.map(|q| ocs_n(normal, Vec3::new(q[0], q[1], elevation)));
    let kind = if trace { EntityKind::Trace(Solid { corners }) } else { EntityKind::Solid(Solid { corners }) };
    Ok(ready(kind, thickness, normal))
}

/// 3DFACE (§20.4.33). From AutoCAD 2000 on each corner is stored against the one before.
fn face3d(r: &mut ObjectReader) -> DwgResult<Shape> {
    let mut corners = [Vec3::ZERO; 4];
    let mut edges = 0u16;
    if r.version() <= V::R14 {
        for c in &mut corners {
            *c = v3(r.bd3()?);
        }
        edges = r.bs()?;
    } else {
        let no_flags = r.b()?;
        let flat = r.b()?;
        let (x, y) = (r.rd()?, r.rd()?);
        let mut prev = Vec3::new(x, y, if flat { 0.0 } else { r.rd()? });
        if let Some(first) = corners.first_mut() {
            *first = prev;
        }
        for c in corners.iter_mut().skip(1) {
            let x = r.dd(prev.x)?;
            let y = r.dd(prev.y)?;
            prev = Vec3::new(x, y, r.dd(prev.z)?);
            *c = prev;
        }
        if !no_flags {
            edges = r.bs()?;
        }
    }
    Ok(ready(EntityKind::Face3d(Face3d { corners, hidden_edges: u8::try_from(edges & 0x0f).unwrap_or(0) }), 0.0, UP))
}

/// XLINE and RAY (§20.4.43, §20.4.42).
fn ray_line(r: &mut ObjectReader, ray: bool) -> DwgResult<Shape> {
    let line = RayLine { base: v3(r.bd3()?), dir: v3(r.bd3()?) };
    Ok(ready(if ray { EntityKind::Ray(line) } else { EntityKind::XLine(line) }, 0.0, UP))
}

/// One entity's type fields, or `None` for a type read in a later step.
fn shape(r: &mut ObjectReader, type_name: &str, partial: &mut Vec<&'static str>) -> DwgResult<Option<Shape>> {
    Ok(Some(match type_name {
        "LINE" => line(r)?,
        "POINT" => point(r)?,
        "CIRCLE" => circle_or_arc(r, false)?,
        "ARC" => circle_or_arc(r, true)?,
        "ELLIPSE" => ellipse(r)?,
        "LWPOLYLINE" => lwpolyline(r)?,
        "TEXT" => text(r)?,
        "MTEXT" => mtext(r, partial)?,
        "INSERT" => insert(r, false)?,
        "MINSERT" => insert(r, true)?,
        "SOLID" => solid(r, false)?,
        "TRACE" => solid(r, true)?,
        "3DFACE" => face3d(r)?,
        "XLINE" => ray_line(r, false)?,
        "RAY" => ray_line(r, true)?,
        _ => return Ok(None),
    }))
}

/// One entity. The type's own fields come first, then its references.
fn entity(r: &mut ObjectReader, o: &ObjectRef, n: &Names) -> DwgResult<(Decoded, EntityRefs)> {
    let mut partial = Vec::new();
    let Some(Shape { body, thickness, normal }) = shape(r, &o.type_name, &mut partial)? else {
        // A type read in a later step: only its place is known.
        let refs = r.entity_refs()?;
        return Ok((Decoded { entity: None, partial }, refs));
    };
    let refs = r.entity_refs()?;
    let style = |r: &mut ObjectReader| -> DwgResult<String> {
        let h = r.reference()?;
        Ok(n.tables.styles.get(&h).cloned().unwrap_or_else(|| "Standard".into()))
    };
    let kind = match body {
        Body::Ready(k) => Some(*k),
        Body::Text(mut t, align, insert) => {
            t.style = style(r)?;
            if normal.z < 0.0 {
                mirror_text(&mut t, align.map(|a| ocs_n(normal, a)), ocs_n(normal, insert));
            }
            Some(EntityKind::Text(*t))
        }
        Body::MText(mut t) => {
            t.style = style(r)?;
            Some(EntityKind::MText(*t))
        }
        Body::Insert(mut i, has_attributes) => {
            let block = r.reference()?;
            match n.blocks.get(&block) {
                // The attributes of a block reference are read with the attributes.
                Some(name) if !has_attributes => {
                    i.block.clone_from(name);
                    Some(EntityKind::Insert(*i))
                }
                _ => None,
            }
        }
    };
    let entity = kind.map(|kind| {
        let mut c = common(r, &refs, n, &mut partial);
        c.thickness = thickness;
        c.extrusion = normal;
        let mut e = Entity::new(Handle(o.handle), kind);
        e.common = c;
        e
    });
    Ok((Decoded { entity, partial }, refs))
}

/// Read the entities into `d`, each into the space or block that owns it, in the block's order.
/// Returns the handles of the objects read.
pub(super) fn read(file: &DwgFile, objects: &[ObjectRef], tables: &Tables, blocks: &Blocks, d: &mut Drawing, unread: &mut Unread) -> HashSet<u64> {
    let names = Names { tables, blocks: &blocks.names };
    let mut read = HashSet::new();
    let mut decoded: HashMap<u64, Entity> = HashMap::new();
    let mut places: HashMap<u64, Place> = HashMap::new();
    // The object after each one in the file, for a chain that leaves the next entity unsaid.
    let mut following: HashMap<u64, u64> = HashMap::new();
    let entities: Vec<&ObjectRef> = objects.iter().filter(|o| o.is_entity && !blocks.read.contains(&o.handle)).take(MAX_ENTITIES).collect();
    for pair in entities.windows(2) {
        if let [a, b] = pair {
            following.insert(a.handle, b.handle);
        }
    }
    for o in &entities {
        let Ok((dec, refs)) = file.read_object(o).and_then(|mut r| entity(&mut r, o, &names)) else {
            if reads(&o.type_name) {
                unread.count(&format!("{} (not decoded)", o.type_name));
            }
            continue;
        };
        let record = match file.read_object(o).ok().and_then(|r| r.entity()).map(|h| h.space) {
            Some(EntitySpace::Model) => blocks.model,
            Some(EntitySpace::Paper) => blocks.paper,
            _ => refs.owner,
        };
        places.insert(o.handle, Place { record, next: refs.next });
        for what in dec.partial {
            unread.count(what);
        }
        match dec.entity {
            Some(e) => {
                decoded.insert(o.handle, e);
            }
            None if reads(&o.type_name) => unread.count(&format!("{} with attributes or an unknown block", o.type_name)),
            None => {}
        }
    }
    // Each block's entities in its own order: the list it stores (AutoCAD 2004 and later) or the
    // chain from its first entity; then any entity that names the block as its owner and was in
    // neither, in file order.
    let mut placed: HashSet<u64> = HashSet::new();
    for rec in &blocks.records {
        let mut order: Vec<u64> = rec.owned.clone();
        let mut at = rec.first;
        let mut steps = 0usize;
        while at != 0 && steps < MAX_ENTITIES {
            steps += 1;
            let Some(place) = places.get(&at).filter(|p| p.record == rec.handle) else { break };
            order.push(at);
            at = place.next.or_else(|| following.get(&at).copied()).unwrap_or(0);
        }
        let listed: HashSet<u64> = order.iter().copied().collect();
        order.extend(entities.iter().map(|o| o.handle).filter(|h| !listed.contains(h) && places.get(h).is_some_and(|p| p.record == rec.handle)));
        for h in order {
            if !placed.insert(h) {
                continue;
            }
            let Some(e) = decoded.remove(&h) else { continue };
            read.insert(h);
            match &rec.space {
                Space::Model => {
                    d.ensure_layer(&e.common.layer);
                    d.model.push(e);
                }
                Space::Layout(name) => {
                    if let Some(l) = d.layouts.iter_mut().find(|l| &l.name == name) {
                        l.entities.push(e);
                    }
                }
                Space::Block(name) => {
                    if let Some(b) = d.blocks.get_mut(name) {
                        std::sync::Arc::make_mut(b).entities.push(e);
                    }
                }
                Space::Xref => {}
            }
        }
    }
    // An entity no block owns (a sub-entity of a type not read yet) stays unread.
    read
}
