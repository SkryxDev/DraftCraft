//! PDF underlays (PDFATTACH, ATTACH of a PDF) and PDFIMPORT.
//!
//! An underlay is an [`Image`] whose [`Image::underlay`] names the PDF page it shows: the page is
//! measured in points (1/72 inch) and placed at a scale in drawing units per inch, so at scale 1
//! an inch drawing shows the page at its true size (a millimetre drawing starts at 25.4). Its
//! pixels are the page rendered when the drawing opens or the PDF is attached, kept with the open
//! drawing ([`Drawing::rasters`]) and never in the file. A page that can't be read stays a frame.
//!
//! PDFIMPORT turns a PDF page (a file, or the page an underlay shows, in the underlay's place)
//! into DraftCraft objects: stroked paths become polylines and splines, filled areas solid
//! hatches and text single-line text, on the layers `PDF_Geometry`, `PDF_Solid Fills` and
//! `PDF_Text`, with the PDF's colours and line widths.
//!
//! The PDF reader is a lower layer (`draftcraft_io::pdf_read`); the app installs it with
//! [`super::file::set_pdf_hooks`].

use std::sync::Arc;

use draftcraft_color::{Color, Rgb};
use draftcraft_doc::{Common, Drawing, EntityKind, Handle, Image, ImportedShape, Lineweight, POINTS_PER_INCH, ShapeClass, Space, Underlay};
use draftcraft_geom::{Mat3, Vec2};
use serde_json::{Value, json};

use super::file::{base64_decode, pdf_hooks};
use super::{CommandSpec, bad, bool_or, f64_or, point_param, str_param};
use crate::{Result, Session};

/// Longest PDF path taken from a caller.
const MAX_PATH: usize = 4096;
/// The resolution underlays are rendered at (dots per inch of the page), at most.
pub const UNDERLAY_DPI: f64 = 150.0;
/// Most underlay pages rendered per drawing.
const MAX_PAGES_LOADED: usize = 200;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pdfattach", "PDF Underlay...", run_pdfattach).alias(&["pdfat"]).menu(&["Insert", "PDF Underlay..."]).params(
            "{path: PDF file (saved as given; a relative path is relative to this drawing's folder), data?: base64 file contents (instead of \
             reading `path`), page?: number from 1 (default 1), at?: [x,y] lower-left corner (default 0,0), scale?: drawing units per inch \
             (default: the drawing's insertion units, 25.4 for millimetres), rotation? (degrees), name?} → {name, handle, path, page, pages, \
             size: [w, h] in drawing units}",
        ),
        CommandSpec::new("pdfimport", "PDF Import", run_pdfimport).menu(&["Insert", "PDF Import"]).params(
            "{handle?: hex of a PDF underlay (imports the page it shows, where it is; `keep?: false` also removes the underlay), \
             path?: PDF file, data?: base64 contents, page?: from 1 (default 1), at?: [x,y] (default 0,0), scale?: drawing units per inch \
             (default as PDFATTACH), rotation?, vectors?: true, fills?: true, text?: true, lineweights?: true (PDF line widths as \
             lineweights)} → {handles: [hex], counts: {vectors, fills, text}, notes: [..]}",
        ),
        CommandSpec::new("ulayers", "Underlay Layers", run_ulayers)
            .params("{} → {underlays: [{handle, name, path, page, status: Loaded|Not Found|Not loaded}]}")
            .noundo(),
    ]
}

/// Whether `ext` (lower case, no dot) is a PDF.
pub(crate) fn is_pdf_ext(ext: &str) -> bool {
    ext == "pdf"
}

/// Drawing units per inch for the drawing's insertion units (INSUNITS); without units, by
/// MEASUREMENT (25.4 for a metric drawing).
pub fn units_per_inch(d: &Drawing) -> f64 {
    match d.header.i64("INSUNITS", 0) {
        1 => 1.0,
        2 => 1.0 / 12.0,
        3 => 1.0 / 63_360.0,
        4 => 25.4,
        5 => 2.54,
        6 => 0.0254,
        7 => 0.0000254,
        8 => 1_000_000.0,
        9 => 1000.0,
        10 => 1.0 / 36.0,
        14 => 254.0,
        _ if d.header.i64("MEASUREMENT", 0) == 1 => 25.4,
        _ => 1.0,
    }
}

/// Every underlay of `d` (model space, layouts, blocks) with its handle.
fn underlays(d: &Drawing) -> impl Iterator<Item = (Handle, &Image)> {
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    stores.flat_map(|s| s.iter()).filter_map(|e| match &e.kind {
        EntityKind::Image(i) if i.underlay.is_some() => Some((e.handle, i)),
        _ => None,
    })
}

/// The file bytes: `data` when the caller hands them over, else the file at `path`.
fn file_bytes(s: &Session, cmd: &str, p: &Value, path: &str) -> Result<Vec<u8>> {
    if let Some(data) = p.get("data") {
        let text = data.as_str().ok_or_else(|| bad(cmd, "`data` must be base64 text"))?;
        return base64_decode(text).ok_or_else(|| bad(cmd, "`data` is not valid base64"));
    }
    let host = s.state()?.path.clone();
    super::image::read_file(path, host.as_deref()).map_err(|e| bad(cmd, format!("{path}: {e}")))
}

fn path_param(cmd: &str, p: &Value) -> Result<String> {
    let path = str_param(p, "path").map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| bad(cmd, "`path` is required"))?;
    if path.len() > MAX_PATH {
        return Err(bad(cmd, "`path` is too long"));
    }
    Ok(path.to_string())
}

fn page_param(cmd: &str, p: &Value) -> Result<usize> {
    match p.get("page") {
        None | Some(Value::Null) => Ok(0),
        Some(v) => {
            let n =
                v.as_u64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok())).ok_or_else(|| bad(cmd, "`page` must be a number from 1"))?;
            usize::try_from(n).ok().and_then(|n| n.checked_sub(1)).ok_or_else(|| bad(cmd, "`page` must be a number from 1"))
        }
    }
}

/// Where and how large a page goes: lower-left corner, rotation (radians), drawing units per inch.
fn placement(s: &Session, cmd: &str, p: &Value) -> Result<(Vec2, f64, f64)> {
    let at = point_param(p, "at").unwrap_or(Vec2::ZERO);
    if !(at.x.is_finite() && at.y.is_finite()) {
        return Err(bad(cmd, "`at` must be a point"));
    }
    let scale = f64_or(p, "scale", units_per_inch(s.doc()?));
    if !(scale.is_finite() && scale > 0.0) {
        return Err(bad(cmd, "`scale` must be greater than zero"));
    }
    Ok((at, f64_or(p, "rotation", 0.0).to_radians(), scale))
}

/// The definition name for a new underlay: the file name without folders and extension.
fn stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => file.to_string(),
    }
}

const ATTACH: &str = "pdfattach";

pub(crate) fn run_pdfattach(s: &mut Session, p: &Value) -> Result<Value> {
    let hooks = pdf_hooks().ok_or_else(|| bad(ATTACH, "PDF files can't be read in this build"))?;
    let path = path_param(ATTACH, p)?;
    let index = page_param(ATTACH, p)?;
    let (at, rotation, scale) = placement(s, ATTACH, p)?;
    let bytes = file_bytes(s, ATTACH, p, &path)?;
    let pages = (hooks.pages)(&bytes).map_err(|e| bad(ATTACH, format!("{path}: {e}")))?;
    let &(w, h) = pages.get(index).ok_or_else(|| bad(ATTACH, format!("{path} has {} page(s)", pages.len())))?;
    let point = scale / POINTS_PER_INCH;
    let u = Vec2::from_angle(rotation) * point;
    if !(point.is_normal() && (point * w.max(h)).is_finite()) {
        return Err(bad(ATTACH, "the page would be too small or too large to draw"));
    }
    let raster = (hooks.rasterize)(&bytes, index, UNDERLAY_DPI).map_err(|e| bad(ATTACH, format!("{path}: {e}")))?;
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| stem(&path));
    let image = Image {
        insert: at.to3(0.0),
        u: u.to3(0.0),
        v: u.perp().to3(0.0),
        size: Vec2::new(w, h),
        path: path.clone(),
        name: name.clone(),
        contrast: Underlay::DEFAULT_CONTRAST,
        fade: Underlay::DEFAULT_FADE,
        underlay: Some(Underlay { page: (index + 1).to_string(), adjust_for_background: true, ..Underlay::default() }),
        ..Image::default()
    };
    let key = image.raster_key();
    s.doc_mut()?.rasters.insert(&key, Arc::new(raster));
    let handle = s.add_entity(EntityKind::Image(image))?;
    Ok(json!({
        "name": name, "handle": handle.hex(), "path": path, "page": index + 1, "pages": pages.len(),
        "size": [w * point, h * point],
    }))
}

/// Load the pages the underlays of drawing `i` show, just opened, and give each underlay its
/// page size (the file doesn't record it). Pages that can't be read are reported and their
/// underlays stay frames; opening never fails over an underlay.
pub(crate) fn after_open(s: &mut Session, i: usize) {
    let Some(hooks) = pdf_hooks() else { return };
    let Some(st) = s.docs.get(i) else { return };
    let host = st.path.clone();
    // (file, page index, raster key) of each page shown, once.
    let mut wanted: Vec<(String, usize, String)> = Vec::new();
    for (_, img) in underlays(&st.doc) {
        let Some(page) = img.underlay.as_ref().and_then(Underlay::page_index) else { continue };
        let key = img.raster_key();
        if !wanted.iter().any(|(_, _, k)| *k == key) {
            wanted.push((img.path.clone(), page, key));
        }
    }
    if wanted.is_empty() {
        return;
    }
    let mut notes = Vec::new();
    if wanted.len() > MAX_PAGES_LOADED {
        wanted.truncate(MAX_PAGES_LOADED);
        notes.push(format!("PDF underlays: only the first {MAX_PAGES_LOADED} pages are shown"));
    }
    let mut rasters = st.doc.rasters.clone();
    let mut sizes: Vec<(String, (f64, f64))> = Vec::new();
    let mut files: Vec<(String, Option<Vec<(f64, f64)>>)> = Vec::new();
    for (path, page, key) in &wanted {
        let bytes = match super::image::read_file(path, host.as_deref()) {
            Ok(b) => b,
            Err(e) => {
                notes.push(format!("PDF underlay {path}: {e}"));
                continue;
            }
        };
        if !files.iter().any(|(p, _)| p == path) {
            files.push((path.clone(), (hooks.pages)(&bytes).ok()));
        }
        if let Some(size) = files.iter().find(|(p, _)| p == path).and_then(|(_, pages)| pages.as_ref()?.get(*page).copied()) {
            sizes.push((key.clone(), size));
        }
        if rasters.get(key).is_none() {
            match (hooks.rasterize)(&bytes, *page, UNDERLAY_DPI) {
                Ok(r) => rasters.insert(key, Arc::new(r)),
                Err(e) => notes.push(format!("PDF underlay {path}, page {}: {e}", page + 1)),
            }
        }
    }
    let Some(st) = s.docs.get_mut(i) else { return };
    // Loading pages is not a change to the drawing (see `image::after_open`).
    let unchanged = Arc::ptr_eq(&st.doc, &st.saved);
    let d = Arc::make_mut(&mut st.doc);
    d.rasters = rasters;
    apply_sizes(d, &sizes);
    if unchanged {
        st.saved = st.doc.clone();
    }
    for n in notes {
        s.echo(n);
    }
}

/// Give underlays the page sizes of `sizes` (by raster key), and their display clip.
fn apply_sizes(d: &mut Drawing, sizes: &[(String, (f64, f64))]) {
    let fix = |store: &mut draftcraft_doc::EntityStore| {
        let hs: Vec<Handle> = store
            .iter()
            .filter(|e| matches!(&e.kind, EntityKind::Image(i) if i.underlay.is_some() && sizes.iter().any(|(k, _)| *k == i.raster_key())))
            .map(|e| e.handle)
            .collect();
        for h in hs {
            store.modify(h, |e| {
                if let EntityKind::Image(i) = &mut e.kind
                    && let Some((_, (w, h))) = sizes.iter().find(|(k, _)| *k == i.raster_key())
                {
                    i.size = Vec2::new(*w, *h);
                    i.clip = i.underlay_display_clip();
                }
            });
        }
    };
    fix(&mut d.model);
    for l in &mut d.layouts {
        fix(&mut l.entities);
    }
    for b in d.blocks.values_mut() {
        if underlays_in(&b.entities) {
            fix(&mut Arc::make_mut(b).entities);
        }
    }
}

fn underlays_in(store: &draftcraft_doc::EntityStore) -> bool {
    store.iter().any(|e| matches!(&e.kind, EntityKind::Image(i) if i.underlay.is_some()))
}

fn run_ulayers(s: &mut Session, _p: &Value) -> Result<Value> {
    let host = s.state()?.path.clone();
    let d = s.doc()?;
    let rows: Vec<Value> = underlays(d)
        .map(|(h, i)| {
            let loaded = d.rasters.get(&i.raster_key()).is_some();
            let found = super::blocks::xref::find_file(&i.path, host.as_deref()).is_some();
            let status = if loaded {
                "Loaded"
            } else if found {
                "Not loaded"
            } else {
                "Not Found"
            };
            json!({ "handle": h.hex(), "name": i.name, "path": i.path, "page": i.underlay.as_ref().map(|u| u.page.clone()), "status": status })
        })
        .collect();
    Ok(json!({ "underlays": rows }))
}

const IMPORT: &str = "pdfimport";

/// The transform taking a page point (x, y) to `origin + x·u + y·v`.
fn frame(u: Vec2, v: Vec2, origin: Vec2) -> Mat3 {
    Mat3 { a: u.x, b: u.y, c: v.x, d: v.y, e: origin.x, f: origin.y }
}

/// The standard lineweights in hundredths of a millimetre.
const LINEWEIGHTS: [u16; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

/// The standard lineweight nearest a width in points; `None` for a hairline (width 0).
fn lineweight(points: f64) -> Option<Lineweight> {
    if !(points.is_finite() && points > 0.0) {
        return None;
    }
    let mm100 = points / POINTS_PER_INCH * 2540.0;
    LINEWEIGHTS.iter().copied().min_by(|a, b| (f64::from(*a) - mm100).abs().total_cmp(&(f64::from(*b) - mm100).abs())).map(Lineweight::Mm100)
}

/// A PDF colour as a drawing colour: black and white follow the background (colour 7), others
/// stay true colours.
fn color(c: Option<Rgb>) -> Color {
    match c {
        None => Color::ByLayer,
        Some(Rgb(0, 0, 0)) => Color::Index(7),
        Some(rgb) => Color::True(rgb),
    }
}

fn layer_of(class: ShapeClass) -> &'static str {
    match class {
        ShapeClass::Vector => "PDF_Geometry",
        ShapeClass::Fill => "PDF_Solid Fills",
        ShapeClass::Text => "PDF_Text",
    }
}

fn run_pdfimport(s: &mut Session, p: &Value) -> Result<Value> {
    let hooks = pdf_hooks().ok_or_else(|| bad(IMPORT, "PDF files can't be read in this build"))?;
    let opts = (bool_or(p, "vectors", true), bool_or(p, "fills", true), bool_or(p, "text", true));
    // The page and where it goes: from an underlay, or from the parameters.
    let (bytes, page, place, from) = match str_param(p, "handle") {
        Some(hex) => {
            let h = Handle::parse_hex(hex.trim()).ok_or_else(|| bad(IMPORT, "`handle` must be a hex handle"))?;
            let d = s.doc()?;
            let space = s.space();
            let store = d.space(&space).ok_or_else(|| bad(IMPORT, "no current space"))?;
            let img = match store.get(h).map(|e| &e.kind) {
                Some(EntityKind::Image(i)) if i.underlay.is_some() => i.clone(),
                _ => return Err(bad(IMPORT, format!("{hex} is not a PDF underlay in the current space"))),
            };
            let page = img.underlay.as_ref().and_then(Underlay::page_index).unwrap_or(0);
            // A point of the page to the drawing: the underlay's own placement.
            let m = frame(img.u.xy(), img.v.xy(), img.insert.xy());
            let bytes = file_bytes(s, IMPORT, &json!({}), &img.path)?;
            (bytes, page, m, Some(h))
        }
        None => {
            let path = if p.get("data").is_some() { str_param(p, "path").unwrap_or("data.pdf").to_string() } else { path_param(IMPORT, p)? };
            let page = page_param(IMPORT, p)?;
            let (at, rotation, scale) = placement(s, IMPORT, p)?;
            let point = scale / POINTS_PER_INCH;
            let u = Vec2::from_angle(rotation) * point;
            let m = frame(u, u.perp(), at);
            (file_bytes(s, IMPORT, p, &path)?, page, m, None)
        }
    };
    let (shapes, images, truncated) = (hooks.import)(&bytes, page, opts.0, opts.1, opts.2).map_err(|e| bad(IMPORT, e))?;
    let lineweights = bool_or(p, "lineweights", true);
    let base = s.current_common();
    let space: Space = s.space();
    let mut handles = Vec::new();
    let (mut vectors, mut fills, mut texts) = (0usize, 0usize, 0usize);
    for ImportedShape { mut kind, color: c, width, class } in shapes {
        kind.transform(&place);
        let common = Common {
            layer: layer_of(class).to_string(),
            color: color(c),
            lineweight: if lineweights && class == ShapeClass::Vector {
                lineweight(width).unwrap_or(Lineweight::ByLayer)
            } else {
                Lineweight::ByLayer
            },
            ..base.clone()
        };
        match class {
            ShapeClass::Vector => vectors += 1,
            ShapeClass::Fill => fills += 1,
            ShapeClass::Text => texts += 1,
        }
        handles.push(s.doc_mut()?.add(&space, common, kind)?.hex());
    }
    let mut notes = Vec::new();
    if images > 0 {
        notes.push(format!("{images} raster image(s) of the page were not imported"));
    }
    if truncated {
        notes.push("the page has more content than PDFIMPORT reads; the rest was left out".to_string());
    }
    if let Some(h) = from
        && !bool_or(p, "keep", true)
    {
        s.doc_mut()?.space_mut(&space).map(|st| st.remove(h));
    }
    for n in &notes {
        s.echo(format!("PDFIMPORT: {n}"));
    }
    Ok(json!({ "handles": handles, "counts": { "vectors": vectors, "fills": fills, "text": texts }, "notes": notes }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lineweights_round_to_the_standard_ones() {
        assert_eq!(lineweight(0.0), None);
        // 1 point is 0.3528 mm.
        assert_eq!(lineweight(1.0), Some(Lineweight::Mm100(35)));
        assert_eq!(lineweight(100.0), Some(Lineweight::Mm100(211)));
    }

    #[test]
    fn units_per_inch_follow_insunits() {
        let mut d = Drawing::new_metric();
        assert_eq!(units_per_inch(&d), 25.4);
        d.header.set_i64("INSUNITS", 1);
        assert_eq!(units_per_inch(&d), 1.0);
        d.header.set_i64("INSUNITS", 6);
        assert!((units_per_inch(&d) - 0.0254).abs() < 1e-12);
    }

    #[test]
    fn black_follows_the_background() {
        assert_eq!(color(Some(Rgb(0, 0, 0))), Color::Index(7));
        assert_eq!(color(Some(Rgb(1, 2, 3))), Color::True(Rgb(1, 2, 3)));
        assert_eq!(color(None), Color::ByLayer);
    }
}
