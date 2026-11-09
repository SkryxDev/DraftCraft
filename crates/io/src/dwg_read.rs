//! DWG → Drawing, straight from the records `draftcraft-dwg`'s reader decodes: no DXF in between.
//!
//! The reader maps what it covers and counts the rest in [`Coverage`]: the entity types, entity
//! properties and objects it doesn't carry into the drawing yet. [`crate::read`] uses it only when
//! nothing was left out (or when asked to, for tests and measurements); otherwise the file goes
//! through acadrust as before, so opening a file never loses what that path keeps.
//!
//! Where a value means the same in DWG and DXF (an OCS point, a transparency word), the conversion
//! is the DXF reader's own (`dxf_read`), so each rule exists once.

use std::collections::{BTreeMap, HashMap};

use draftcraft_color::{Color, Rgb};
use draftcraft_doc::{
    Arc, Block, Circle, Common, DashElement, Drawing, Ellipse, Entity, EntityKind, Face3d, HAlign, Handle, Insert, Layer, Layout, Line, Linetype,
    Lineweight, LwPolyline, MText, Point, RayLine, Solid, Text, TextStyle, VAlign,
};
use draftcraft_dwg::records::geom;
use draftcraft_dwg::records::model::{self as rec, NonGraphic, Object, ObjectKind};
use draftcraft_geom::{PolyVertex, Vec2, Vec3};

use crate::dxf_read::{ocs_n, style_font_of, transparency_440};
use crate::{IoError, Result};

/// What the DWG reader left out of the drawing: one count per entity type, property or object
/// type (`"HATCH"`, `"INSERT with attributes"`, `"object VISUALSTYLE"`). Empty when the drawing
/// carries everything the file holds that DraftCraft keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub left_out: BTreeMap<String, usize>,
}

impl Coverage {
    /// True when nothing was left out.
    pub fn is_whole(&self) -> bool {
        self.left_out.is_empty()
    }
    fn miss(&mut self, what: impl Into<String>) {
        *self.left_out.entry(what.into()).or_default() += 1;
    }
    /// One line for messages: `HATCH × 3, INSERT with attributes × 1`.
    pub fn summary(&self) -> String {
        self.left_out.iter().map(|(k, n)| format!("{k} × {n}")).collect::<Vec<_>>().join(", ")
    }
}

/// Objects that carry no drawing content of their own, or whose content DraftCraft writes again
/// from its own model on save, so leaving them out loses nothing: table controls, the records
/// read through their tables, block begin/end markers and the dictionaries that index them.
fn structural(o: &Object) -> bool {
    matches!(
        o.kind,
        ObjectKind::Control(_)
            | ObjectKind::BlockBegin { .. }
            | ObjectKind::BlockEnd
            | ObjectKind::SeqEnd
            | ObjectKind::BlockHeader(_)
            | ObjectKind::Layer(_)
            | ObjectKind::Linetype(_)
            | ObjectKind::TextStyle(_)
            | ObjectKind::NonGraphic(NonGraphic::Layout(_))
    )
}

/// Names by handle, for resolving the references entities make.
#[derive(Default)]
struct Names {
    layers: HashMap<u64, String>,
    linetypes: HashMap<u64, String>,
    styles: HashMap<u64, String>,
    blocks: HashMap<u64, String>,
}

/// Read a DWG file into a drawing, with what the reader left out.
pub fn read(bytes: &[u8]) -> Result<(Drawing, Coverage)> {
    let dwg = draftcraft_dwg::read_bytes(bytes).map_err(|e| IoError::Format(format!("DWG: {e}")))?;
    let mut cov = Coverage::default();
    let mut d = Drawing::new_imperial();
    d.layouts.clear();
    d.header.set_i64("INSUNITS", 0);
    d.header.set_str("ACADVER", dwg.version.magic());
    header(&dwg.header, &mut d);
    let names = tables(&dwg, &mut d);
    let layouts = layouts(&dwg, &mut d);
    for o in dwg.objects.values() {
        if o.entity.is_none() && !structural(o) {
            cov.miss(format!("object {}", o.type_name));
        }
        if matches!(o.kind, ObjectKind::Failed(_) | ObjectKind::Unsupported) && o.entity.is_some() {
            cov.miss(format!("undecoded {}", o.type_name));
        }
    }
    let first_layout = d.layouts.iter().min_by_key(|l| l.tab_order).map(|l| l.name.clone());
    for o in block_records(&dwg) {
        let ObjectKind::BlockHeader(bh) = &o.kind else { continue };
        let name = names.blocks.get(&o.handle.0).cloned().unwrap_or_else(|| bh.name.clone());
        let owned = dwg.block_entities(o.handle);
        let ents: Vec<Entity> = owned.into_iter().filter(|e| !marker(e)).filter_map(|e| entity(e, &names, &mut cov)).collect();
        let up = name.to_ascii_uppercase();
        if up == "*MODEL_SPACE" {
            for e in ents {
                d.ensure_layer(&e.common.layer);
                d.model.push(e);
            }
        } else if up.starts_with("*PAPER_SPACE") {
            let lname = layouts.get(&o.handle.0).cloned().or_else(|| first_layout.clone());
            if let Some(l) = lname.and_then(|n| d.layouts.iter_mut().find(|l| l.name == n)) {
                for e in ents {
                    l.entities.push(e);
                }
            }
        } else {
            if bh.is_xref {
                cov.miss("external reference");
                continue;
            }
            let mut b = Block::new(&name);
            b.base = v3(bh.base_point);
            b.description.clone_from(&bh.description);
            b.anonymous = bh.anonymous;
            b.explodable = !bh.not_explodable;
            b.units = u8::try_from(bh.insert_units).unwrap_or(0);
            for e in ents {
                b.entities.push(e);
            }
            d.blocks.insert(name, std::sync::Arc::new(b));
        }
    }
    if d.layouts.is_empty() {
        d.layouts.push(Layout::new("Layout1", 1));
    }
    if d.layer("0").is_none() {
        d.layers.insert(0, Layer::default());
    }
    if let Some(seed) = dwg.objects.keys().map(|h| h.0).max() {
        d.bump_handseed(Handle(seed));
    }
    Ok((d, cov))
}

/// BLOCK and ENDBLK, which open and close a block's entities: the block itself carries them.
fn marker(o: &Object) -> bool {
    matches!(o.kind, ObjectKind::BlockBegin { .. } | ObjectKind::BlockEnd | ObjectKind::SeqEnd)
}

/// The block records in table order: model space, paper space, then the block table's entries,
/// then any the table doesn't list (in handle order).
fn block_records(dwg: &rec::Drawing) -> Vec<&Object> {
    let control = dwg.objects.values().find_map(|o| match &o.kind {
        ObjectKind::Control(c) if o.type_code == 0x30 => Some(c),
        _ => None,
    });
    let mut order: Vec<rec::Handle> = vec![dwg.model_space(), dwg.paper_space()];
    order.extend(control.map(|c| c.entries.clone()).unwrap_or_default());
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<&Object> = order
        .into_iter()
        .filter(|h| seen.insert(*h))
        .filter_map(|h| dwg.objects.get(&h))
        .filter(|o| matches!(o.kind, ObjectKind::BlockHeader(_)))
        .collect();
    out.extend(dwg.objects.values().filter(|o| matches!(o.kind, ObjectKind::BlockHeader(_)) && !seen.contains(&o.handle)));
    out
}

/// Block names by record handle. DWG stores anonymous blocks unnumbered (`*D`, `*U`); each gets
/// a number per prefix in table order, as AutoCAD numbers them when it writes a DXF (`*D0`, `*D1`),
/// skipping names the drawing already uses.
fn block_names(dwg: &rec::Drawing) -> HashMap<u64, String> {
    let records = block_records(dwg);
    let mut taken: std::collections::HashSet<String> =
        records.iter().filter_map(|o| if let ObjectKind::BlockHeader(b) = &o.kind { Some(b.name.to_ascii_uppercase()) } else { None }).collect();
    let mut next: HashMap<String, usize> = HashMap::new();
    let mut names = HashMap::new();
    for o in records {
        let ObjectKind::BlockHeader(b) = &o.kind else { continue };
        let bare = b.name.len() >= 2
            && b.name.starts_with('*')
            && b.name.chars().skip(1).all(|c| c.is_ascii_alphabetic())
            && !b.name.eq_ignore_ascii_case("*Model_Space")
            && !b.name.eq_ignore_ascii_case("*Paper_Space");
        let name = if bare {
            let n = next.entry(b.name.to_ascii_uppercase()).or_insert(0);
            loop {
                let cand = format!("{}{}", b.name, n);
                *n += 1;
                if taken.insert(cand.to_ascii_uppercase()) {
                    break cand;
                }
            }
        } else {
            b.name.clone()
        };
        names.insert(o.handle.0, name);
    }
    names
}

fn v3(p: geom::Vec3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// The header variables the reader decodes, under their DXF names.
fn header(h: &rec::HeaderVars, d: &mut Drawing) {
    let ints = [
        ("INSUNITS", h.insunits),
        ("MEASUREMENT", h.measurement),
        ("LUNITS", h.lunits),
        ("LUPREC", h.luprec),
        ("AUNITS", h.aunits),
        ("AUPREC", h.auprec),
        ("ANGDIR", h.angdir),
    ];
    for (name, v) in ints {
        if let Some(v) = v {
            d.header.set_i64(name, i64::from(v));
        }
    }
    let reals = [("LTSCALE", h.ltscale), ("TEXTSIZE", h.textsize), ("CELTSCALE", h.celtscale), ("ANGBASE", h.angbase)];
    for (name, v) in reals {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            d.header.set_f64(name, v);
        }
    }
    for (name, p) in [("EXTMIN", h.extmin), ("EXTMAX", h.extmax), ("INSBASE", h.insbase)] {
        if let Some(p) = p {
            d.header.set(name, draftcraft_doc::HVal::Point(v3(p)));
        }
    }
    for (name, p) in [("LIMMIN", h.limmin), ("LIMMAX", h.limmax)] {
        if let Some(p) = p {
            d.header.set(name, draftcraft_doc::HVal::Point(Vec3::new(p.x, p.y, 0.0)));
        }
    }
}

fn color(c: rec::Color) -> Color {
    match c {
        rec::Color::ByLayer | rec::Color::None => Color::ByLayer,
        rec::Color::ByBlock => Color::ByBlock,
        rec::Color::Index(i) => Color::from_aci(i16::from(i)),
        rec::Color::Rgb(r, g, b) => Color::True(Rgb::from_u32(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))),
    }
}

fn lineweight(code: u8) -> Lineweight {
    Lineweight::from_dxf(rec::Lineweight::from_code(code).dxf())
}

/// Layers, linetypes and text styles, replacing the new drawing's defaults of the same name.
fn tables(dwg: &rec::Drawing, d: &mut Drawing) -> Names {
    let mut n = Names { blocks: block_names(dwg), ..Names::default() };
    for o in dwg.objects.values() {
        match &o.kind {
            ObjectKind::Linetype(lt) => {
                n.linetypes.insert(o.handle.0, lt.name.clone());
                let pattern = lt.elements.iter().map(|e| DashElement::dash(e.length));
                // As the DXF reader does: a name the drawing already has (ByLayer, ByBlock,
                // Continuous) keeps DraftCraft's own record.
                if !d.linetypes.iter().any(|x| x.name.eq_ignore_ascii_case(&lt.name)) {
                    d.linetypes.push(Linetype { name: lt.name.clone(), description: lt.description.clone(), pattern: pattern.collect() });
                }
            }
            ObjectKind::TextStyle(s) if !s.name.is_empty() => {
                n.styles.insert(o.handle.0, s.name.clone());
                let st = TextStyle {
                    name: s.name.clone(),
                    font: style_font_of(s.font_file.clone(), first_str(&o.xdata, "ACAD"), own_font(&o.xdata)),
                    big_font: s.bigfont_file.clone(),
                    height: s.height,
                    width_factor: s.width_factor,
                    oblique: s.oblique,
                    backwards: s.generation & 2 != 0,
                    upside_down: s.generation & 4 != 0,
                    vertical: s.flags & 4 != 0,
                    annotative: annotative(&o.xdata),
                };
                replace(&mut d.text_styles, st, |x| &x.name);
            }
            _ => {}
        }
    }
    for o in dwg.objects.values() {
        if let ObjectKind::Layer(l) = &o.kind {
            n.layers.insert(o.handle.0, l.name.clone());
            let layer = Layer {
                name: l.name.clone(),
                color: color(l.color),
                linetype: n.linetypes.get(&l.linetype.0).cloned().unwrap_or_else(|| "Continuous".into()),
                lineweight: lineweight(l.lineweight),
                on: !l.off,
                frozen: l.frozen,
                locked: l.locked,
                plot: l.plot,
                ..Layer::new(&l.name)
            };
            replace(&mut d.layers, layer, |x| &x.name);
        }
    }
    n
}

/// The first string of application `app`'s extended data.
fn first_str(x: &[rec::XData], app: &str) -> Option<String> {
    let a = x.iter().find(|a| a.app_name().is_some_and(|n| n.eq_ignore_ascii_case(app)))?;
    a.items.iter().find_map(|v| if let rec::XValue::Str(s) = v { Some(s.trim().to_string()) } else { None })
}

/// The font name in DraftCraft's own extended data (`FONT`, name).
fn own_font(x: &[rec::XData]) -> Option<String> {
    let a = x.iter().find(|a| a.app_name().is_some_and(|n| n.eq_ignore_ascii_case(crate::dxf_ext::APP)))?;
    a.items.windows(2).find_map(|w| match w {
        [rec::XValue::Str(m), rec::XValue::Str(n)] if m == "FONT" => Some(n.clone()),
        _ => None,
    })
}

/// A style's annotative flag, from `AcadAnnotative` extended data as the DXF reader reads it: the
/// second integer of its `AnnotativeData` list.
fn annotative(x: &[rec::XData]) -> bool {
    let Some(a) = x.iter().find(|a| a.app_name().is_some_and(|n| n.eq_ignore_ascii_case("AcadAnnotative"))) else { return false };
    let start = a.items.iter().position(|v| matches!(v, rec::XValue::Str(s) if s == "AnnotativeData"));
    let ints = a.items.iter().skip(start.map_or(a.items.len(), |i| i + 1)).filter_map(|v| match v {
        rec::XValue::Int16(i) => Some(i64::from(*i)),
        _ => None,
    });
    ints.into_iter().nth(1).is_some_and(|i| i != 0)
}

/// Put `item` in place of the entry with the same name (case-insensitive), or at the end.
fn replace<T>(list: &mut Vec<T>, item: T, name: impl Fn(&T) -> &String) {
    let key = name(&item).to_ascii_uppercase();
    match list.iter_mut().find(|x| name(x).to_ascii_uppercase() == key) {
        Some(slot) => *slot = item,
        None => list.push(item),
    }
}

/// The drawing's paper-space layouts, in tab order: paper-space block record → layout name.
fn layouts(dwg: &rec::Drawing, d: &mut Drawing) -> HashMap<u64, String> {
    let mut found: Vec<&rec::Layout> = dwg
        .objects
        .values()
        .filter_map(|o| if let ObjectKind::NonGraphic(NonGraphic::Layout(l)) = &o.kind { Some(l.as_ref()) } else { None })
        .filter(|l| !l.name.eq_ignore_ascii_case("Model"))
        .collect();
    found.sort_by_key(|l| l.tab_order);
    let mut by_block = HashMap::new();
    for l in found {
        d.layouts.push(Layout::new(&l.name, l.tab_order));
        by_block.insert(l.block.0, l.name.clone());
    }
    by_block
}

/// The properties every entity has.
fn common(c: &rec::EntityCommon, extrusion: Option<geom::Vec3>, thickness: f64, names: &Names, cov: &mut Coverage) -> Common {
    let linetype = match c.linetype {
        rec::LinetypeRef::ByLayer => "ByLayer".to_string(),
        rec::LinetypeRef::ByBlock => "ByBlock".to_string(),
        rec::LinetypeRef::Continuous => "Continuous".to_string(),
        rec::LinetypeRef::Handle(h) => names.linetypes.get(&h.0).cloned().unwrap_or_else(|| "ByLayer".into()),
    };
    if !c.plotstyle.is_null() {
        cov.miss("entity plot style");
    }
    if !c.material.is_null() {
        cov.miss("entity material");
    }
    Common {
        layer: names.layers.get(&c.layer.0).cloned().unwrap_or_else(|| "0".into()),
        color: color(c.color),
        linetype,
        lineweight: lineweight(c.lineweight),
        ltscale: if c.linetype_scale > 0.0 { c.linetype_scale } else { 1.0 },
        transparency: c.transparency.map(|v| transparency_440(i64::from(v))).unwrap_or_default(),
        visible: !c.invisible,
        thickness,
        extrusion: extrusion.map(v3).unwrap_or(Vec3::new(0.0, 0.0, 1.0)),
        ..Common::default()
    }
}

/// One entity, or `None` (counted in `cov`) when the reader doesn't map it yet.
fn entity(o: &Object, names: &Names, cov: &mut Coverage) -> Option<Entity> {
    let ec = o.entity.as_ref()?;
    if !o.xdictionary.is_null() {
        cov.miss("entity extension dictionary");
    }
    let (ext, thick) = extrusion_of(&o.kind);
    let c = common(ec, ext, thick, names, cov);
    let kind = match kind(o, &c, names) {
        Some(k) => k,
        None => {
            cov.miss(o.type_name.clone());
            return None;
        }
    };
    let mut e = Entity::new(Handle(o.handle.0), kind);
    e.common = c;
    e.common.xdata = xdata(&o.xdata, names, cov);
    Some(e)
}

fn extrusion_of(k: &ObjectKind) -> (Option<geom::Vec3>, f64) {
    match k {
        ObjectKind::Line(l) => (Some(l.extrusion), l.thickness),
        ObjectKind::Circle(c) => (Some(c.extrusion), c.thickness),
        ObjectKind::Arc(a) => (Some(a.extrusion), a.thickness),
        ObjectKind::Point(p) => (Some(p.extrusion), p.thickness),
        ObjectKind::Ellipse(e) => (Some(e.extrusion), 0.0),
        ObjectKind::Text(t) => (Some(t.extrusion), t.thickness),
        ObjectKind::MText(t) => (Some(t.extrusion), 0.0),
        ObjectKind::Insert(i) => (Some(i.extrusion), 0.0),
        ObjectKind::LwPolyline(p) => (Some(p.extrusion), p.thickness),
        ObjectKind::Solid(s) => (Some(s.extrusion), s.thickness),
        _ => (None, 0.0),
    }
}

/// Other applications' extended data as the DXF groups the drawing keeps (`Common::xdata`), with
/// the DXF reader's rules (`dxf_raw`): DraftCraft's own applications and the ones its reader turns
/// into properties are left to that (and counted here, as this reader doesn't yet), and an
/// application whose list braces don't balance is dropped.
fn xdata(x: &[rec::XData], names: &Names, cov: &mut Coverage) -> Vec<draftcraft_doc::RawTag> {
    use draftcraft_doc::RawTag;
    use rec::XValue;
    let real = |code: i32, v: f64| RawTag { code, value: draftcraft_dxf::Value::Real(v).as_str() };
    let mut out = Vec::new();
    for app in x {
        let Some(name) = app.app_name() else {
            cov.miss("extended data of an unknown application");
            continue;
        };
        let up = name.to_ascii_uppercase();
        if crate::dxf_raw::OWN_APPS.contains(&up.as_str()) {
            cov.miss(format!("{up} extended data"));
            continue;
        }
        let mut groups = vec![RawTag { code: 1001, value: name.to_string() }];
        let mut depth = 0i32;
        let mut ok = true;
        for v in &app.items {
            let pt = |base: i32, p: &geom::Vec3| [real(base, p.x), real(base + 10, p.y), real(base + 20, p.z)];
            match v {
                XValue::Str(t) => groups.push(RawTag { code: 1000, value: t.clone() }),
                XValue::Open => {
                    depth += 1;
                    groups.push(RawTag { code: 1002, value: "{".into() });
                }
                XValue::Close => {
                    depth -= 1;
                    ok &= depth >= 0;
                    groups.push(RawTag { code: 1002, value: "}".into() });
                }
                XValue::Layer(h) => groups.push(RawTag { code: 1003, value: names.layers.get(&h.0).cloned().unwrap_or_else(|| "0".into()) }),
                XValue::Bytes(b) => groups.push(RawTag { code: 1004, value: b.iter().map(|x| format!("{x:02X}")).collect() }),
                XValue::Handle(h) => groups.push(RawTag { code: 1005, value: format!("{:X}", h.0) }),
                XValue::Point(p) => groups.extend(pt(1010, p)),
                XValue::Position(p) => groups.extend(pt(1011, p)),
                XValue::Displacement(p) => groups.extend(pt(1012, p)),
                XValue::Direction(p) => groups.extend(pt(1013, p)),
                XValue::Real(r) => groups.push(real(1040, *r)),
                XValue::Distance(r) => groups.push(real(1041, *r)),
                XValue::ScaleFactor(r) => groups.push(real(1042, *r)),
                XValue::Int16(i) => groups.push(RawTag { code: 1070, value: i.to_string() }),
                XValue::Int32(i) => groups.push(RawTag { code: 1071, value: i.to_string() }),
                XValue::Undecoded { .. } => {
                    cov.miss("undecoded extended data");
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

/// The DraftCraft entity for a DWG entity, with the DXF reader's conventions: angles in radians,
/// 2D entities' points from their OCS, and a mirrored OCS (extrusion z < 0) turned into the WCS
/// geometry it draws.
fn kind(o: &Object, c: &Common, names: &Names) -> Option<EntityKind> {
    let n = c.extrusion;
    let mirrored = n.z < 0.0;
    Some(match &o.kind {
        ObjectKind::Line(l) => EntityKind::Line(Line { a: v3(l.start), b: v3(l.end) }),
        ObjectKind::Point(p) => EntityKind::Point(Point { p: v3(p.position), angle: p.x_axis_angle }),
        ObjectKind::Circle(ci) => EntityKind::Circle(Circle { center: ocs_n(n, v3(ci.center)), radius: ci.radius.abs() }),
        ObjectKind::Arc(a) => {
            let mut arc = Arc { center: ocs_n(n, v3(a.center)), radius: a.radius.abs(), start: a.start_angle, end: a.end_angle };
            if mirrored {
                let (s, e) = (std::f64::consts::PI - arc.end, std::f64::consts::PI - arc.start);
                arc.start = draftcraft_geom::norm_angle(s);
                arc.end = draftcraft_geom::norm_angle(e);
            }
            EntityKind::Arc(arc)
        }
        ObjectKind::Ellipse(el) => {
            let mut e = Ellipse {
                center: v3(el.center),
                major: v3(el.major_axis),
                ratio: el.axis_ratio.clamp(1e-9, 1.0),
                start: el.start_param,
                end: el.end_param,
            };
            if mirrored && (draftcraft_geom::ccw_sweep(e.start, e.end) - std::f64::consts::TAU).abs() > 1e-9 {
                let (s, e2) = (-e.end, -e.start);
                e.start = draftcraft_geom::norm_angle(s);
                e.end = draftcraft_geom::norm_angle(e2);
            }
            EntityKind::Ellipse(e)
        }
        ObjectKind::LwPolyline(p) => {
            let mut vertices: Vec<PolyVertex> = p
                .points
                .iter()
                .enumerate()
                .map(|(i, q)| {
                    let (sw, ew) = p.widths.get(i).copied().unwrap_or((0.0, 0.0));
                    PolyVertex { p: Vec2::new(q.x, q.y), start_width: sw, end_width: ew, bulge: p.bulges.get(i).copied().unwrap_or(0.0) }
                })
                .collect();
            if mirrored {
                for v in &mut vertices {
                    v.p.x = -v.p.x;
                    v.bulge = -v.bulge;
                }
            }
            EntityKind::LwPolyline(LwPolyline {
                vertices,
                closed: p.flags & 512 != 0,
                const_width: p.const_width,
                elevation: p.elevation,
                plinegen: p.flags & 256 != 0,
            })
        }
        ObjectKind::Text(t) if !mirrored => {
            let halign = match t.h_align {
                1 => HAlign::Center,
                2 => HAlign::Right,
                3 => HAlign::Aligned,
                4 => HAlign::Middle,
                5 => HAlign::Fit,
                _ => HAlign::Left,
            };
            let valign = match t.v_align {
                1 => VAlign::Bottom,
                2 => VAlign::Middle,
                3 => VAlign::Top,
                _ => VAlign::Baseline,
            };
            let at = |p: geom::Vec2| Vec3::new(p.x, p.y, t.elevation);
            let plain = halign == HAlign::Left && valign == VAlign::Baseline;
            EntityKind::Text(Text {
                insert: at(t.insertion),
                align_pt: (!plain).then(|| at(t.alignment)),
                height: t.height.max(1e-9),
                value: draftcraft_dxf::unescape_unicode(&t.value),
                rotation: t.rotation,
                width_factor: t.width_factor,
                oblique: t.oblique,
                style: names.styles.get(&t.style.0).cloned().unwrap_or_else(|| "Standard".into()),
                halign,
                valign,
            })
        }
        ObjectKind::MText(t) if !mirrored => EntityKind::MText(MText {
            insert: v3(t.insertion),
            height: t.text_height,
            width: t.rect_width,
            attach: u8::try_from(t.attachment.clamp(1, 9)).unwrap_or(1),
            rotation: Vec2::new(t.x_axis.x, t.x_axis.y).angle(),
            style: names.styles.get(&t.style.0).cloned().unwrap_or_else(|| "Standard".into()),
            contents: draftcraft_dxf::unescape_unicode(&t.contents),
            line_spacing: t.linespacing_factor,
            line_spacing_exact: t.linespacing_exact,
            columns: None,
        }),
        ObjectKind::Insert(i) if !i.has_attribs => {
            let block = names.blocks.get(&i.block_header.0).cloned()?;
            let flat = n.x.hypot(n.y) <= 1e-3 * n.z.abs();
            let mut p = v3(i.insertion);
            let mut scale = v3(i.scale);
            let mut rotation = i.rotation;
            let (cols, rows, mut col_spacing, row_spacing) = i.array.map_or((1, 1, 0.0, 0.0), |(c, r, cs, rs)| (u32::from(c), u32::from(r), cs, rs));
            if mirrored && flat {
                p = ocs_n(n, p);
                scale.x = -scale.x;
                rotation = -rotation;
                col_spacing = -col_spacing;
            }
            EntityKind::Insert(Insert {
                block,
                insert: p,
                scale,
                rotation,
                attribs: Vec::new(),
                cols: cols.clamp(1, 10_000),
                rows: rows.clamp(1, 10_000),
                col_spacing,
                row_spacing,
                clip: None,
            })
        }
        ObjectKind::Solid(s) => {
            let at = |q: geom::Vec2| ocs_n(n, Vec3::new(q.x, q.y, s.elevation));
            let corners = s.corners.map(at);
            if s.is_trace { EntityKind::Trace(Solid { corners }) } else { EntityKind::Solid(Solid { corners }) }
        }
        ObjectKind::Face3d(f) => {
            EntityKind::Face3d(Face3d { corners: f.corners.map(v3), hidden_edges: u8::try_from(f.invisible_edges & 0x0f).unwrap_or(0) })
        }
        ObjectKind::Xline(x) => EntityKind::XLine(RayLine { base: v3(x.base), dir: v3(x.direction) }),
        ObjectKind::Ray(x) => EntityKind::Ray(RayLine { base: v3(x.base), dir: v3(x.direction) }),
        _ => return None,
    })
}
