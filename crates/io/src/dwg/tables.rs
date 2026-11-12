//! The symbol tables of a DWG file, into the drawing: application names, linetypes, text styles
//! and layers. Each record is decoded field by field in the order the format stores it (Open
//! Design Alliance specification, chapter 20) straight into the drawing's own type.

use std::collections::{HashMap, HashSet};

use draftcraft_color::{Color, Rgb};
use draftcraft_doc::{DashElement, Drawing, Layer, Linetype, Lineweight, TextStyle};
use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::file::{ColorValue, DwgFile, ObjectReader, ObjectRef, XDataItem};
use draftcraft_dwg::records::Result as DwgResult;
use draftcraft_geom::Vec2;

use super::Unread;
use crate::dxf_read::style_font_of;

/// Most records of one table that are read: more than any real drawing has.
const MAX_RECORDS: usize = 1 << 20;
/// Most elements AutoCAD allows in one linetype pattern.
const MAX_DASHES: usize = 12;

/// The names of the table records read, by handle, for the references other objects make.
#[derive(Default)]
pub(super) struct Tables {
    pub apps: HashMap<u64, String>,
    pub linetypes: HashMap<u64, String>,
    /// Text styles; an unnamed shape style is here under its shape file.
    pub styles: HashMap<u64, String>,
    pub layers: HashMap<u64, String>,
    /// The handles of the objects this category read.
    pub read: HashSet<u64>,
}

/// A colour of the file as the drawing's colour.
pub(super) fn color(c: ColorValue) -> Option<Color> {
    match c {
        ColorValue::ByLayer => Some(Color::ByLayer),
        ColorValue::ByBlock => Some(Color::ByBlock),
        ColorValue::Index(i) => Some(Color::from_aci(i16::from(i))),
        ColorValue::Rgb(r, g, b) => Some(Color::True(Rgb::from_u32(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)))),
        ColorValue::None => None,
    }
}

/// The fields every table record starts with: its name, then the "referenced" flag, the index of
/// the xref it came from and the "depends on an xref" flag. From AutoCAD 2007 on only the index
/// is stored.
fn record_name(r: &mut ObjectReader) -> DwgResult<String> {
    let name = r.text()?;
    if r.has_string_stream() {
        r.bs()?;
    } else {
        r.b()?;
        r.bs()?;
        r.b()?;
    }
    Ok(name)
}

/// The extended data `app` attached to the object.
fn xdata<'a>(blocks: &'a [draftcraft_dwg::file::XDataBlock], apps: &HashMap<u64, String>, app: &str) -> &'a [XDataItem] {
    blocks.iter().find(|b| apps.get(&b.app).is_some_and(|n| n.eq_ignore_ascii_case(app))).map_or(&[], |b| &b.items)
}

fn put<T>(list: &mut Vec<T>, item: T, name: impl Fn(&T) -> &str) {
    let key = name(&item).to_ascii_lowercase();
    match list.iter_mut().find(|x| name(x).to_ascii_lowercase() == key) {
        Some(slot) => *slot = item,
        None => list.push(item),
    }
}

/// Read the tables into `d`. A record that can't be decoded is counted in `unread`.
pub(super) fn read(file: &DwgFile, objects: &[ObjectRef], d: &mut Drawing, unread: &mut Unread) -> Tables {
    let mut t = Tables::default();
    let of = |name: &'static str| objects.iter().filter(move |o| o.type_name == name).take(MAX_RECORDS);
    // Application names first: extended data names its application by this record.
    for o in of("APPID") {
        if let Ok(name) = file.read_object(o).and_then(|mut r| record_name(&mut r)) {
            // The record is its name, which the drawing keeps with each piece of extended data.
            t.apps.insert(o.handle, name);
            t.read.insert(o.handle);
        }
    }
    // Text styles before linetypes, whose text and shape elements name one.
    for o in of("STYLE") {
        match file.read_object(o).and_then(|mut r| style(&mut r, &t.apps)) {
            Ok((st, shape_file)) => {
                t.styles.insert(o.handle, if st.name.trim().is_empty() { shape_file } else { st.name.clone() });
                // An unnamed style is a shape file that linetypes refer to, not a text style.
                if !st.name.is_empty() {
                    put(&mut d.text_styles, st, |x| &x.name);
                }
                t.read.insert(o.handle);
            }
            Err(_) => unread.count("STYLE (not decoded)"),
        }
    }
    for o in of("LTYPE") {
        match file.read_object(o).and_then(|mut r| linetype(&mut r, &t.styles)) {
            Ok(lt) => {
                t.linetypes.insert(o.handle, lt.name.clone());
                // A name the drawing already has (ByLayer, ByBlock, Continuous) keeps the
                // drawing's own record, as the DXF reader does.
                if !lt.name.is_empty() && !d.linetypes.iter().any(|x| x.name.eq_ignore_ascii_case(&lt.name)) {
                    d.linetypes.push(lt);
                }
                t.read.insert(o.handle);
            }
            Err(_) => unread.count("LTYPE (not decoded)"),
        }
    }
    for o in of("LAYER") {
        match file.read_object(o).and_then(|mut r| layer(&mut r, &t)) {
            Ok((l, plot_style)) => {
                t.layers.insert(o.handle, l.name.clone());
                if !l.name.is_empty() {
                    put(&mut d.layers, l, |x| &x.name);
                }
                // The plot style is named by the plot style dictionary, read with the objects.
                if plot_style != 0 {
                    unread.count("LAYER plot style");
                }
                t.read.insert(o.handle);
            }
            Err(_) => unread.count("LAYER (not decoded)"),
        }
    }
    t
}

/// STYLE (specification §20.4.56). Returns the style and its font file, which names an unnamed
/// shape style.
fn style(r: &mut ObjectReader, apps: &HashMap<u64, String>) -> DwgResult<(TextStyle, String)> {
    let name = record_name(r)?;
    let vertical = r.b()?;
    let _shape_file = r.b()?;
    let height = r.bd()?;
    let width_factor = r.bd()?;
    let oblique = r.bd()?;
    let generation = r.rc()?;
    let _last_height = r.bd()?;
    let font_file = r.text()?;
    let big_font = r.text()?;
    let x = r.xdata();
    // The TrueType typeface AutoCAD keeps in the style's ACAD data, and DraftCraft's own font name.
    let face = xdata(&x, apps, "ACAD").iter().find_map(|v| if let XDataItem::Text(s) = v { Some(s.trim().to_string()) } else { None });
    let own = xdata(&x, apps, crate::dxf_ext::APP).windows(2).find_map(|w| match w {
        [XDataItem::Text(m), XDataItem::Text(n)] if m == "FONT" => Some(n.clone()),
        _ => None,
    });
    // Annotative: the second integer of the AnnotativeData list.
    let a = xdata(&x, apps, "AcadAnnotative");
    let start = a.iter().position(|v| matches!(v, XDataItem::Text(s) if s == "AnnotativeData"));
    let annotative = a
        .iter()
        .skip(start.map_or(a.len(), |i| i + 1))
        .filter_map(|v| if let XDataItem::Int16(i) = v { Some(*i) } else { None })
        .nth(1)
        .is_some_and(|i| i != 0);
    let ok = |v: f64, d: f64| if v.is_finite() { v } else { d };
    let st = TextStyle {
        name,
        font: style_font_of(font_file.clone(), face, own),
        big_font,
        height: ok(height, 0.0).max(0.0),
        width_factor: if width_factor.is_finite() && width_factor > 0.0 { width_factor } else { 1.0 },
        oblique: ok(oblique, 0.0),
        backwards: generation & 2 != 0,
        upside_down: generation & 4 != 0,
        vertical,
        annotative,
    };
    Ok((st, font_file))
}

/// LTYPE (specification §20.4.58): the name, the description and the pattern, whose text and
/// shape elements each name a style.
fn linetype(r: &mut ObjectReader, styles: &HashMap<u64, String>) -> DwgResult<Linetype> {
    let name = record_name(r)?;
    let description = r.text()?;
    let _pattern_length = r.bd()?;
    let _alignment = r.rc()?;
    let n = usize::from(r.rc()?).min(MAX_DASHES);
    // Per element: the pattern element and its flags (1 absolute rotation, 2 text, 4 shape) with
    // the shape number, or the place of its text in the text area.
    let mut elements: Vec<(DashElement, u16, u16)> = Vec::with_capacity(n);
    for _ in 0..n {
        let length = r.bd()?;
        let code = r.bs()?;
        let [x, y] = r.rd2()?;
        let scale = r.bd()?;
        let rotation = r.bd()?;
        let flags = r.bs()?;
        let mut el = DashElement::dash(if length.is_finite() { length } else { 0.0 });
        if flags & 6 != 0 {
            el.absolute = flags & 1 != 0;
            if scale.is_finite() {
                el.scale = scale;
            }
            if rotation.is_finite() {
                el.rotation = rotation;
            }
            if x.is_finite() && y.is_finite() {
                el.offset = Vec2::new(x, y);
            }
            if flags & 2 != 0 {
                el.text = Some(String::new());
            } else {
                el.shape = Some(code);
            }
        }
        elements.push((el, flags, code));
    }
    // The text of the text elements: an area of 0-ended strings, 256 bytes up to AutoCAD 2004,
    // and 512 bytes of UTF-16 from 2007 when some element is text.
    let has_text = elements.iter().any(|(_, f, _)| f & 2 != 0);
    if r.version() <= V::R2004 {
        if let Ok(area) = r.bytes(256) {
            for (el, _, at) in elements.iter_mut().filter(|(_, f, _)| f & 2 != 0) {
                let from = area.get(usize::from(*at)..).unwrap_or(&[]);
                let end = from.iter().position(|&b| b == 0).unwrap_or(from.len());
                el.text = Some(r.ansi(from.get(..end).unwrap_or(&[])));
            }
        }
    } else if has_text && let Ok(area) = r.bytes(512) {
        for (el, _, at) in elements.iter_mut().filter(|(_, f, _)| f & 2 != 0) {
            let units: Vec<u16> = area
                .get(usize::from(*at)..)
                .unwrap_or(&[])
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            el.text = Some(String::from_utf16_lossy(&units));
        }
    }
    r.common_refs()?;
    let _xref_block = r.reference()?;
    // One style per element; only a text or shape element uses its own.
    let mut pattern = Vec::with_capacity(elements.len());
    for (mut el, flags, _) in elements {
        let style = r.reference().unwrap_or(0);
        if flags & 6 != 0 {
            el.style = styles.get(&style).filter(|n| !n.trim().is_empty()).cloned();
        }
        pattern.push(el);
    }
    Ok(Linetype { name, description, pattern })
}

/// LAYER (specification §20.4.54). Returns the layer and the handle of its plot style.
fn layer(r: &mut ObjectReader, t: &Tables) -> DwgResult<(Layer, u64)> {
    let name = record_name(r)?;
    let mut l = Layer { name, ..Layer::default() };
    let before_2000 = r.version() <= V::R14;
    let mut flags = 0u16;
    if before_2000 {
        l.frozen = r.b()?;
        let _on = r.b()?;
        l.vp_freeze_new = r.b()?;
        l.locked = r.b()?;
    } else {
        flags = r.bs()?;
        l.frozen = flags & 0x01 != 0;
        l.vp_freeze_new = flags & 0x04 != 0;
        l.locked = flags & 0x08 != 0;
        l.plot = flags & 0x10 != 0;
        l.lineweight = Lineweight::from_dxf(i16::try_from(super::header::lineweight(i64::from((flags & 0x03E0) >> 5))).unwrap_or(-3));
    }
    let (c, index) = r.color()?;
    // Up to R14 a negative colour number marks a layer that is off. From AutoCAD 2000 on it is
    // the 0x02 bit of the flags: set on a layer that is off, in AutoCAD's own files.
    l.on = if before_2000 { index >= 0 } else { flags & 0x02 == 0 };
    l.color = match (c, index) {
        (ColorValue::Rgb(..), _) => color(c).unwrap_or(Color::Index(7)),
        (ColorValue::Index(i), _) => Color::Index(i.max(1)),
        // Before AutoCAD 2004 the colour is its index field.
        (_, i) if i != 0 => Color::Index(u8::try_from(i.unsigned_abs().clamp(1, 255)).unwrap_or(7)),
        _ => Color::Index(7),
    };
    let x = r.xdata();
    if let Some(pct) = xdata(&x, &t.apps, crate::dxf_ext::LAYER_TRANSPARENCY_APP)
        .iter()
        .find_map(|v| if let XDataItem::Int32(a) = v { crate::dxf_ext::transparency_from_dxf(i64::from(*a)) } else { None })
    {
        l.transparency = pct;
    }
    if let Some(desc) = xdata(&x, &t.apps, crate::dxf_ext::LAYER_DESCRIPTION_APP)
        .iter()
        .filter_map(|v| if let XDataItem::Text(s) = v { Some(s) } else { None })
        .nth(1)
    {
        l.description.clone_from(desc);
    }
    r.common_refs()?;
    let _xref_block = r.reference()?;
    let plot_style = if before_2000 { 0 } else { r.reference()? };
    if r.version() >= V::R2007 {
        let _material = r.reference()?;
    }
    let lt = r.reference()?;
    if let Some(n) = t.linetypes.get(&lt) {
        l.linetype.clone_from(n);
    }
    Ok((l, plot_style))
}
