//! Reading PDF files (PDF underlays and PDFIMPORT), with craft-pdf's reader (hayro; pure Rust).
//!
//! - [`pages`]: each page's size in points (1/72 inch), as the page is shown (after its rotation).
//! - [`rasterize`]: one page as pixels, on white paper, for an underlay to show.
//! - [`import`]: one page's vector content as DraftCraft objects in points, the page's lower-left
//!   corner at the origin and y up: stroked paths as polylines and splines (each cubic Bézier
//!   exactly), filled areas as solid hatches, and text as single-line text. Images, shadings and
//!   clipping are not imported.
//!
//! PDF files are hostile input: the input size, page count, objects and points are capped, and
//! the reader runs inside `catch_unwind` (craft-pdf's guards), so a damaged file is an error.

use craft_pdf::interpret::font::Glyph;
use craft_pdf::interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image as PdfImage, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask,
    TransformExt,
};
use craft_pdf::syntax::page::Page;
use draftcraft_color::Rgb;
use draftcraft_doc::{EntityKind, Hatch, HatchLoop, ImportedShape, LwPolyline, Raster, ShapeClass, Text};
use draftcraft_geom::{PolyVertex, Spline, Vec2};
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};

use crate::{IoError, Result};

/// Largest PDF file read (bytes).
pub const MAX_PDF_BYTES: usize = 256 << 20;
/// Most pages listed.
pub const MAX_PAGES: usize = 10_000;
/// Most objects imported from one page.
pub const MAX_SHAPES: usize = 500_000;
/// Most points kept across one page's import (polyline vertices and spline control points).
const MAX_POINTS: usize = 8_000_000;
/// Longest text run kept (characters).
const MAX_TEXT: usize = 2000;
/// The longest side of a rendered page, in pixels.
pub const MAX_RASTER_SIDE: u32 = 6000;
/// The most pixels of a rendered page.
pub const MAX_RASTER_PIXELS: u64 = 24_000_000;

fn bad(e: impl std::fmt::Display) -> IoError {
    IoError::Format(format!("PDF: {e}"))
}

fn open(bytes: &[u8]) -> Result<craft_pdf::syntax::Pdf> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(bad(format!("the file is larger than {} MB", MAX_PDF_BYTES >> 20)));
    }
    if !bytes.starts_with(b"%PDF") && !bytes.get(..1024).unwrap_or(bytes).windows(4).any(|w| w == b"%PDF") {
        return Err(bad("not a PDF file"));
    }
    std::panic::catch_unwind(|| craft_pdf::open_slice(bytes, None)).map_err(|_| bad("the file can't be read"))?.map_err(|e| bad(format!("{e:?}")))
}

/// Each page's width and height in points, as shown (a rotated page is turned).
pub fn pages(bytes: &[u8]) -> Result<Vec<(f64, f64)>> {
    let pdf = open(bytes)?;
    let pages = pdf.pages();
    let out: Vec<(f64, f64)> = pages.iter().take(MAX_PAGES).map(|p| p.render_dimensions()).map(|(w, h)| (f64::from(w), f64::from(h))).collect();
    if out.is_empty() {
        return Err(bad("the file has no pages"));
    }
    Ok(out)
}

fn page<'a>(pdf: &'a craft_pdf::syntax::Pdf, index: usize) -> Result<&'a Page<'a>> {
    pdf.pages().get(index).ok_or_else(|| bad(format!("there is no page {}", index.saturating_add(1))))
}

/// The scale (pixels per point) that renders a `w` × `h` point page within the raster limits,
/// at most `wanted`.
fn raster_scale(w: f64, h: f64, wanted: f64) -> f64 {
    let side = f64::from(MAX_RASTER_SIDE) / w.max(h).max(1.0);
    let area = (MAX_RASTER_PIXELS as f64 / (w * h).max(1.0)).sqrt();
    wanted.min(side).min(area).max(0.01)
}

/// Page `index` (from 0) as pixels on white paper, at up to `dpi` dots per inch (less for a page
/// too large for [`MAX_RASTER_SIDE`] and [`MAX_RASTER_PIXELS`]).
pub fn rasterize(bytes: &[u8], index: usize, dpi: f64) -> Result<Raster> {
    let pdf = open(bytes)?;
    let page = page(&pdf, index)?;
    let (w, h) = page.render_dimensions();
    let s = raster_scale(f64::from(w), f64::from(h), (dpi / 72.0).clamp(0.05, 8.0)) as f32;
    let cache = craft_pdf::raster::RenderCache::new();
    let options = craft_pdf::raster::Options { scale: [s, s], background: [255, 255, 255, 255], ..Default::default() };
    let r = craft_pdf::raster::render(page, &cache, &InterpreterSettings::default(), &options).map_err(bad)?;
    // Premultiplied on opaque white paper: every pixel is opaque, so the bytes are straight RGBA.
    Raster::new(r.width, r.height, r.bytes().to_vec()).ok_or_else(|| bad("the page could not be drawn"))
}

/// What [`import`] reads.
#[derive(Clone, Copy, Debug)]
pub struct ImportOptions {
    pub vectors: bool,
    pub fills: bool,
    pub text: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions { vectors: true, fills: true, text: true }
    }
}

/// What was left out of an import, for the result's notes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportNotes {
    pub images: usize,
    pub truncated: bool,
}

/// Page `index` (from 0) as DraftCraft objects in points, the page's lower-left corner at the
/// origin, y up.
pub fn import(bytes: &[u8], index: usize, opts: ImportOptions) -> Result<(Vec<ImportedShape>, ImportNotes)> {
    let pdf = open(bytes)?;
    let page = page(&pdf, index)?;
    let (w, h) = page.render_dimensions();
    let cache = InterpreterCache::new();
    let mut ctx = Context::new(
        page.initial_transform(false).to_kurbo(),
        Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut dev = Collector { opts, page: (f64::from(w), f64::from(h)), ..Collector::default() };
    craft_pdf::interpret_page(page, &mut ctx, &mut dev).map_err(bad)?;
    dev.flush_text();
    let notes = ImportNotes { images: dev.images, truncated: dev.truncated };
    Ok((dev.shapes, notes))
}

/// [`import`] with plain arguments and a text error, the form the engine's hook takes: the
/// shapes, how many images were left out and whether the page had more than was read.
pub fn import_flat(
    bytes: &[u8],
    index: usize,
    vectors: bool,
    fills: bool,
    text: bool,
) -> std::result::Result<(Vec<ImportedShape>, usize, bool), String> {
    import(bytes, index, ImportOptions { vectors, fills, text }).map(|(s, n)| (s, n.images, n.truncated)).map_err(|e| e.to_string())
}

/// A text run being collected: glyphs on one baseline.
struct Run {
    text: String,
    origin: Vec2,
    /// Unit vector along the baseline.
    dir: Vec2,
    /// The font size (em) in points.
    em: f64,
    /// Where the last glyph's ink ends, along the baseline from `origin`.
    end: f64,
    color: Option<Rgb>,
}

#[derive(Default)]
struct Collector {
    opts: ImportOptions,
    page: (f64, f64),
    shapes: Vec<ImportedShape>,
    points: usize,
    images: usize,
    truncated: bool,
    run: Option<Run>,
}

fn v(p: Point) -> Vec2 {
    Vec2::new(p.x, p.y)
}

fn rgb(paint: &Paint<'_>) -> Option<Rgb> {
    match paint {
        Paint::Color(c) => {
            let [r, g, b, _] = c.to_rgba().to_rgba8();
            Some(Rgb(r, g, b))
        }
        Paint::Pattern(_) => None,
    }
}

/// A cubic Bézier chain (start point, then three points per curve) as one exact spline:
/// knots of multiplicity three at each joint.
fn bezier_chain(control: Vec<Vec2>) -> Spline {
    let curves = control.len().saturating_sub(1) / 3;
    let mut knots = vec![0.0; 4];
    for i in 1..curves {
        knots.extend([i as f64; 3]);
    }
    knots.extend([curves as f64; 4]);
    let mut s = Spline::from_control(control, 3);
    s.knots = knots;
    s
}

/// The subpaths of a path in page points: each a list of pieces, a run of straight segments (a
/// polyline) or a run of cubic curves (a spline), and whether it is closed.
fn subpaths(path: &BezPath, m: Affine) -> Vec<(Vec<Piece>, bool)> {
    let mut out = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut start = Point::ZERO;
    let mut cur = Point::ZERO;
    let mut closed = false;
    let finish = |pieces: &mut Vec<Piece>, closed: bool, out: &mut Vec<(Vec<Piece>, bool)>| {
        if !pieces.is_empty() {
            out.push((std::mem::take(pieces), closed));
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut pieces, closed, &mut out);
                closed = false;
                start = m * p;
                cur = start;
            }
            PathEl::LineTo(p) => {
                let p = m * p;
                match pieces.last_mut() {
                    Some(Piece::Lines(pts)) => pts.push(v(p)),
                    _ => pieces.push(Piece::Lines(vec![v(cur), v(p)])),
                }
                cur = p;
            }
            PathEl::QuadTo(q, p) => {
                let (q, p) = (m * q, m * p);
                let c1 = cur + (q - cur) * (2.0 / 3.0);
                let c2 = p + (q - p) * (2.0 / 3.0);
                push_cubic(&mut pieces, cur, c1, c2, p);
                cur = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                let (c1, c2, p) = (m * c1, m * c2, m * p);
                push_cubic(&mut pieces, cur, c1, c2, p);
                cur = p;
            }
            PathEl::ClosePath => {
                if cur.distance(start) > 1e-9 {
                    match pieces.last_mut() {
                        Some(Piece::Lines(pts)) => pts.push(v(start)),
                        _ => pieces.push(Piece::Lines(vec![v(cur), v(start)])),
                    }
                }
                cur = start;
                closed = true;
                finish(&mut pieces, closed, &mut out);
                closed = false;
            }
        }
    }
    finish(&mut pieces, closed, &mut out);
    out
}

fn push_cubic(pieces: &mut Vec<Piece>, from: Point, c1: Point, c2: Point, to: Point) {
    match pieces.last_mut() {
        Some(Piece::Curves(pts)) => pts.extend([v(c1), v(c2), v(to)]),
        _ => pieces.push(Piece::Curves(vec![v(from), v(c1), v(c2), v(to)])),
    }
}

enum Piece {
    Lines(Vec<Vec2>),
    Curves(Vec<Vec2>),
}

impl Piece {
    fn len(&self) -> usize {
        match self {
            Piece::Lines(p) | Piece::Curves(p) => p.len(),
        }
    }
}

/// The length of the vector (1, 0) under `m`, for widths and sizes.
fn mean_scale(m: Affine) -> f64 {
    let c = m.as_coeffs();
    ((c[0] * c[3] - c[1] * c[2]).abs()).sqrt()
}

impl Collector {
    fn room(&mut self, points: usize) -> bool {
        if self.shapes.len() >= MAX_SHAPES || self.points.saturating_add(points) > MAX_POINTS {
            self.truncated = true;
            return false;
        }
        self.points += points;
        true
    }

    fn push(&mut self, kind: EntityKind, color: Option<Rgb>, width: f64, class: ShapeClass) {
        self.shapes.push(ImportedShape { kind, color, width, class });
    }

    fn stroke(&mut self, path: &BezPath, m: Affine, color: Option<Rgb>, width: f64) {
        for (pieces, closed) in subpaths(path, m) {
            let whole = pieces.len() == 1;
            for piece in pieces {
                if !self.room(piece.len()) {
                    return;
                }
                let kind = match piece {
                    Piece::Lines(pts) => {
                        let mut pts = pts;
                        let closed = closed && whole && pts.len() > 2;
                        if closed && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.dist(*b) < 1e-9) {
                            pts.pop();
                        }
                        if pts.len() < 2 {
                            continue;
                        }
                        let vertices = pts.into_iter().map(PolyVertex::new).collect();
                        EntityKind::LwPolyline(LwPolyline { vertices, closed, const_width: 0.0, elevation: 0.0, plinegen: false })
                    }
                    Piece::Curves(pts) => EntityKind::Spline(bezier_chain(pts)),
                };
                self.push(kind, color, width, ShapeClass::Vector);
            }
        }
    }

    fn fill(&mut self, path: &BezPath, m: Affine, color: Option<Rgb>) {
        // Curves flattened to within a hundredth of a point.
        let mut loops = Vec::new();
        let mut pts: Vec<Vec2> = Vec::new();
        let flat = |pts: &mut Vec<Vec2>, loops: &mut Vec<HatchLoop>| {
            if pts.len() >= 3 {
                loops.push(HatchLoop { vertices: pts.drain(..).map(PolyVertex::new).collect(), outer: loops.is_empty(), sources: Vec::new() });
            }
            pts.clear();
        };
        let path = m * path.clone();
        let mut count = 0usize;
        kurbo::flatten(path.elements().iter().copied(), 0.01, |el| {
            count += 1;
            if count > MAX_POINTS {
                return;
            }
            match el {
                PathEl::MoveTo(p) => {
                    flat(&mut pts, &mut loops);
                    pts.push(v(p));
                }
                PathEl::LineTo(p) => pts.push(v(p)),
                PathEl::ClosePath => flat(&mut pts, &mut loops),
                _ => {}
            }
        });
        flat(&mut pts, &mut loops);
        let points: usize = loops.iter().map(|l| l.vertices.len()).sum();
        if loops.is_empty() || !self.room(points) {
            return;
        }
        // The page's own white background is paper, not drawing.
        let bbox = path.bounding_box();
        let (pw, ph) = self.page;
        if color == Some(Rgb(255, 255, 255)) && bbox.width() >= pw * 0.95 && bbox.height() >= ph * 0.95 {
            return;
        }
        let hatch = Hatch {
            pattern: "SOLID".into(),
            solid: true,
            loops,
            scale: 1.0,
            angle: 0.0,
            associative: false,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
            pattern_lines: Vec::new(),
        };
        self.push(EntityKind::Hatch(hatch), color, 0.0, ShapeClass::Fill);
    }

    fn flush_text(&mut self) {
        let Some(run) = self.run.take() else { return };
        let value = run.text.trim_end().to_string();
        if value.is_empty() || !self.room(1) {
            return;
        }
        let text = Text {
            insert: run.origin.to3(0.0),
            align_pt: None,
            // Capital letters are about 0.7 em high.
            height: run.em * 0.7,
            value,
            rotation: run.dir.y.atan2(run.dir.x),
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: Default::default(),
            valign: Default::default(),
        };
        self.push(EntityKind::Text(text), run.color, 0.0, ShapeClass::Text);
    }

    fn glyph(&mut self, glyph: &Glyph<'_>, m: Affine, color: Option<Rgb>) {
        let Some(ch) = glyph.as_unicode() else { return };
        let ch = match ch {
            craft_pdf::interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
            craft_pdf::interpret::hayro_cmap::BfString::String(s) => s,
        };
        if ch.chars().any(char::is_control) {
            return;
        }
        let origin = v(m * Point::ZERO);
        let o = m * Point::ZERO;
        let up = m * Point::new(0.0, 1000.0) - o;
        let along = m * Point::new(1000.0, 0.0) - o;
        let em = up.hypot();
        if !(em.is_finite() && em > 1e-6 && origin.x.is_finite() && origin.y.is_finite()) {
            return;
        }
        let dir = Vec2::new(along.x, along.y);
        let dir = if dir.len() > 1e-9 { dir * (1.0 / dir.len()) } else { Vec2::new(1.0, 0.0) };
        // Where the glyph's ink ends (its outline's right side), along the baseline.
        let ink = match glyph {
            Glyph::Outline(g) => {
                let b = (m * g.outline()).bounding_box();
                let corners = [Vec2::new(b.x0, b.y0), Vec2::new(b.x1, b.y0), Vec2::new(b.x0, b.y1), Vec2::new(b.x1, b.y1)];
                corners.iter().map(|c| (*c - origin).dot(dir)).fold(f64::NEG_INFINITY, f64::max)
            }
            Glyph::Type3(_) => em * 0.5,
        };
        let ink = if ink.is_finite() && ink > 0.0 { ink } else { em * 0.5 };
        if let Some(run) = &mut self.run {
            let rel = origin - run.origin;
            let along = rel.dot(run.dir);
            let across = rel.dot(run.dir.perp()).abs();
            let same = (em - run.em).abs() <= run.em * 0.15 && run.dir.dot(dir) > 0.999 && run.color == color;
            let gap = along - run.end;
            if same && across <= run.em * 0.2 && gap > -run.em * 0.5 && gap < run.em * 1.5 && run.text.chars().count() < MAX_TEXT {
                if gap > run.em * 0.2 && !run.text.ends_with(' ') && ch != " " {
                    run.text.push(' ');
                }
                run.text.push_str(&ch);
                run.end = run.end.max(along + ink);
                return;
            }
        }
        self.flush_text();
        if ch.trim().is_empty() {
            return;
        }
        self.run = Some(Run { text: ch, origin, dir, em, end: ink, color });
    }
}

impl<'a> Device<'a> for Collector {
    fn set_soft_mask(&mut self, _mask: Option<SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _blend_mode: BlendMode) {}

    fn draw_path(&mut self, path: &BezPath, transform: Affine, paint: &Paint<'a>, draw_mode: &PathDrawMode) {
        match draw_mode {
            PathDrawMode::Stroke(props) if self.opts.vectors => {
                self.flush_text();
                let width = f64::from(props.line_width) * mean_scale(transform);
                self.stroke(path, transform, rgb(paint), if width.is_finite() { width } else { 0.0 });
            }
            PathDrawMode::Fill(_) if self.opts.fills => {
                self.flush_text();
                self.fill(path, transform, rgb(paint));
            }
            _ => {}
        }
    }

    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}
    fn push_transparency_group(&mut self, _opacity: f32, _mask: Option<SoftMask<'a>>, _blend_mode: BlendMode) {}

    fn draw_glyph(&mut self, glyph: &Glyph<'a>, transform: Affine, glyph_transform: Affine, paint: &Paint<'a>, draw_mode: &GlyphDrawMode) {
        if !self.opts.text || matches!(draw_mode, GlyphDrawMode::Clip | GlyphDrawMode::Invisible) {
            return;
        }
        self.glyph(glyph, transform * glyph_transform, rgb(paint));
    }

    fn draw_image(&mut self, _image: PdfImage<'a, '_>, _transform: Affine) {
        self.images = self.images.saturating_add(1);
    }

    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF with a stroked line, a stroked curve, a red filled square and a text.
    fn sample_pdf() -> Vec<u8> {
        let content = b"q 2 w 0 0 1 RG 72 72 m 144 72 l S 72 144 m 100 200 150 200 180 144 c S 1 0 0 rg 200 200 50 50 re f Q \
BT /F1 12 Tf 72 300 Td (Hi there) Tj ET";
        let mut objs: Vec<Vec<u8>> = Vec::new();
        objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        objs.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
        objs.push(b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec());
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content);
        stream.extend_from_slice(b"\nendstream");
        objs.push(stream);
        objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
        out
    }

    #[test]
    fn pages_lists_the_page_size() {
        assert_eq!(pages(&sample_pdf()).unwrap(), vec![(612.0, 792.0)]);
    }

    #[test]
    fn rasterize_draws_on_white_paper() {
        let r = rasterize(&sample_pdf(), 0, 36.0).unwrap();
        assert_eq!((r.width(), r.height()), (306, 396));
        assert!(r.opaque());
        // The red square (200..250 points from the left, 200..250 from the bottom).
        assert_eq!(r.pixel(112, 396 - 112), Some([255, 0, 0, 255]));
        assert_eq!(r.pixel(5, 5), Some([255, 255, 255, 255]));
        assert!(rasterize(&sample_pdf(), 1, 72.0).is_err());
    }

    #[test]
    fn import_reads_lines_curves_fills_and_text() {
        let (shapes, notes) = import(&sample_pdf(), 0, ImportOptions::default()).unwrap();
        assert_eq!(notes, ImportNotes::default());
        let line = shapes.iter().find(|s| matches!(s.kind, EntityKind::LwPolyline(_))).unwrap();
        let EntityKind::LwPolyline(p) = &line.kind else { panic!("a polyline") };
        assert_eq!(p.vertices.iter().map(|v| v.p).collect::<Vec<_>>(), vec![Vec2::new(72.0, 72.0), Vec2::new(144.0, 72.0)]);
        assert_eq!(line.color, Some(Rgb(0, 0, 255)));
        assert!((line.width - 2.0).abs() < 1e-9);
        let curve = shapes.iter().find_map(|s| if let EntityKind::Spline(s) = &s.kind { Some(s) } else { None }).unwrap();
        assert_eq!(curve.control.len(), 4);
        assert_eq!(curve.knots, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        let fill = shapes.iter().find(|s| s.class == ShapeClass::Fill).unwrap();
        assert_eq!(fill.color, Some(Rgb(255, 0, 0)));
        let text = shapes.iter().find_map(|s| if let EntityKind::Text(t) = &s.kind { Some(t) } else { None }).unwrap();
        assert_eq!(text.value, "Hi there");
        assert!((text.insert.x - 72.0).abs() < 1e-6 && (text.insert.y - 300.0).abs() < 1e-6, "{:?}", text.insert);
        let only_text = import(&sample_pdf(), 0, ImportOptions { vectors: false, fills: false, text: true }).unwrap().0;
        assert_eq!(only_text.len(), 1);
    }

    #[test]
    fn hostile_input_is_an_error() {
        assert!(pages(b"not a pdf").is_err());
        assert!(pages(b"%PDF-1.4\n garbage").is_err());
        let mut cut = sample_pdf();
        cut.truncate(cut.len() / 2);
        let _ = pages(&cut);
        let _ = import(&cut, 0, ImportOptions::default());
    }

    #[test]
    fn bezier_chains_have_a_joint_knot_per_curve() {
        let s = bezier_chain((0..7).map(|i| Vec2::new(f64::from(i), 0.0)).collect());
        assert_eq!(s.knots, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0]);
    }
}
