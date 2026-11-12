//! The drawing on screen: its display list, built for the zoom and the area being looked at, and
//! drawn by the full app's GPU canvas (`draftcraft_ui_egui::gpu`) or, without a GPU render state,
//! with egui shapes.
//!
//! A phone has one thread for everything, so the list is rebuilt only between gestures: while the
//! fingers move, the list built for the last resting view is moved and scaled on the GPU, which
//! costs nothing; when they lift, a list for the new zoom band and area replaces it.

use draftcraft_engine::doc::color::Rgb;
use draftcraft_engine::geom::{Bounds2, Vec2};
use draftcraft_engine::render::{DisplayList, Kind};
use draftcraft_ui_egui::gpu;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::doc::Doc;
use crate::view::{View, band, band_units};

/// How far past the screen (in screens, each side) a list is built: hatch patterns exist only
/// inside the built area, so small pans stay inside it.
const WINDOW_SCREENS: f64 = 1.5;

/// What a display list was built from.
#[derive(Clone, Debug, PartialEq)]
struct BuildKey {
    revision: u64,
    band: i32,
    lineweights: bool,
}

#[derive(Default)]
pub struct Scene {
    pub list: Option<DisplayList>,
    key: Option<BuildKey>,
    /// The area the list was built for.
    window: Option<Bounds2>,
    /// Bumped on every rebuild.
    generation: u64,
    /// Whether the GPU canvas draws (`gpu::install` ran).
    pub gpu: bool,
    slot: gpu::MeshSlot,
    /// What the uploaded mesh was built from: list generation, background, lineweights, origin.
    mesh_for: Option<(u64, Rgb, bool, (u64, u64))>,
    mesh_key: u64,
    mesh_origin: Vec2,
    infinite: Vec<usize>,
}

impl Scene {
    /// Forget the list (another drawing opened).
    pub fn clear(&mut self) {
        let gpu = self.gpu;
        *self = Scene { gpu, mesh_key: self.mesh_key, ..Scene::default() };
    }

    /// Whether the list must be rebuilt now: always for a changed drawing, layer or space; for a
    /// new zoom band or a view leaving the built area only when `resting` (no gesture running).
    pub fn stale(&self, doc: &Doc, view: &View, rect: Rect, lineweights: bool, resting: bool) -> bool {
        let Some(key) = &self.key else { return true };
        if key.revision != doc.revision || key.lineweights != lineweights {
            return true;
        }
        if !resting {
            return false;
        }
        let b = band(view.units_per_point());
        // A finer list than needed is fine one band out; a coarser one shows its corners.
        let band_off = b < key.band || b > key.band + 1;
        let left = self.window.is_some_and(|w| !w.contains_box(&view.world_bounds(rect)));
        band_off || left
    }

    /// Build the list for `view`.
    pub fn build(&mut self, doc: &Doc, view: &View, rect: Rect, lineweights: bool) {
        let b = band(view.units_per_point());
        let screen = view.world_bounds(rect);
        let reach = screen.width().max(screen.height()) * WINDOW_SCREENS;
        let window = if reach.is_finite() && reach > 0.0 { screen.expand(reach) } else { screen };
        let opts = draftcraft_engine::render::Options {
            tolerance: band_units(b) * 0.5,
            min_dash: band_units(b) * 2.0,
            text: true,
            linetypes: true,
            fill: true,
            lineweights,
            view_height: screen.height(),
            plot_style_table: None,
            view: Some(window),
            transparency: true,
            skip_viewport: None,
            cancel: None,
        };
        let list = draftcraft_engine::render::build(&doc.drawing, &doc.space, &opts);
        self.infinite = list.prims.iter().enumerate().filter(|(_, p)| matches!(p.kind, Kind::Infinite { .. })).map(|(i, _)| i).collect();
        self.list = Some(list);
        self.window = Some(window);
        self.key = Some(BuildKey { revision: doc.revision, band: b, lineweights });
        self.generation += 1;
    }

    /// The paper sheet when a layout is on screen.
    pub fn sheet_bounds(&self) -> Option<Bounds2> {
        self.list.as_ref().and_then(|l| l.sheet.as_ref()).map(|s| s.bounds())
    }

    /// What zoom to fit shows: the sheet of a layout, else everything drawn.
    pub fn extents(&self) -> Bounds2 {
        match (&self.list, self.sheet_bounds()) {
            (_, Some(sheet)) => sheet,
            (Some(l), None) => l.bounds,
            (None, None) => Bounds2::EMPTY,
        }
    }

    /// Draw the list: the paper sheet first on a layout, then the drawing.
    pub fn draw(&mut self, p: &Painter, rect: Rect, view: &View, canvas_bg: Color32, model_bg: Rgb, lineweights: bool) {
        p.rect_filled(rect, 0.0, canvas_bg);
        let Some(list) = &self.list else { return };
        let bg = match &list.sheet {
            Some(sheet) => {
                let paper = Rect::from_two_pos(view.to_screen(rect, Vec2::ZERO), view.to_screen(rect, sheet.size));
                p.rect_filled(paper.translate(vec2(4.0, 4.0)), 0.0, Color32::from_black_alpha(90));
                p.rect_filled(paper, 0.0, Color32::WHITE);
                let pa = Rect::from_two_pos(view.to_screen(rect, sheet.printable.min), view.to_screen(rect, sheet.printable.max));
                dashed_rect(p, pa, Color32::from_gray(170));
                Rgb(255, 255, 255)
            }
            None => model_bg,
        };
        if self.gpu {
            self.draw_gpu(p, rect, view, bg, lineweights);
        } else if let Some(list) = &self.list {
            draw_cpu(p, rect, view, list, bg, lineweights);
        }
        if let Some(list) = &self.list {
            let mut shapes = Vec::new();
            for prim in self.infinite.iter().filter_map(|i| list.prims.get(*i)) {
                let pts = list.points(prim);
                if let (Kind::Infinite { ray }, Some(b), Some(d)) = (prim.kind, pts.first(), pts.get(1))
                    && let Some(seg) = clip_infinite(rect, view, *b, *d, ray)
                {
                    shapes.push(Shape::line_segment(seg, Stroke::new(1.0, prim_color(prim, bg))));
                }
            }
            p.extend(shapes);
        }
    }

    fn draw_gpu(&mut self, p: &Painter, rect: Rect, view: &View, bg: Rgb, lineweights: bool) {
        let Some(list) = &self.list else { return };
        let view_height = f64::from(rect.height()) / view.scale.max(1e-300);
        let origin = gpu::choose_origin(&list.bounds, view.center, view_height);
        let wanted = (self.generation, bg, lineweights, (origin.x.to_bits(), origin.y.to_bits()));
        if self.mesh_for != Some(wanted) {
            let mesh = gpu::build_mesh(list, origin, bg, lineweights);
            if let Ok(mut slot) = self.slot.lock() {
                *slot = Some(mesh);
            }
            self.mesh_for = Some(wanted);
            self.mesh_key = self.mesh_key.wrapping_add(1);
            self.mesh_origin = origin;
        }
        let c = rect.center();
        p.add(eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            gpu::CanvasCallback {
                key: self.mesh_key,
                slot: self.slot.clone(),
                offset: [(self.mesh_origin.x - view.center.x) as f32, (self.mesh_origin.y - view.center.y) as f32],
                scale: view.scale as f32,
                center: [c.x, c.y],
                layer: gpu::LAYER_DRAWING,
                xform: gpu::IDENTITY_XFORM,
                dash: 0.0,
                stencil: gpu::Stencil::Off,
            },
        ));
    }
}

fn color32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn prim_color(prim: &draftcraft_engine::render::DPrim, bg: Rgb) -> Color32 {
    let c = prim.display_rgb(bg);
    Color32::from_rgba_unmultiplied(c.0, c.1, c.2, prim.alpha())
}

/// An infinite line or ray as a segment across the screen.
fn clip_infinite(rect: Rect, view: &View, base: Vec2, dir: Vec2, ray: bool) -> Option<[Pos2; 2]> {
    let wb = view.world_bounds(rect);
    let far = (wb.width() + wb.height()) * 2.0 + base.dist(wb.center());
    if !far.is_finite() || !dir.is_finite() {
        return None;
    }
    let a = if ray { base } else { base - dir * far };
    Some([view.to_screen(rect, a), view.to_screen(rect, base + dir * far)])
}

/// The list drawn with egui shapes (no GPU canvas: `?cpu`, or no render state).
fn draw_cpu(p: &Painter, rect: Rect, view: &View, list: &DisplayList, bg: Rgb, lineweights: bool) {
    let vis = view.world_bounds(rect);
    let px = 1.0 / view.scale;
    let mut shapes: Vec<Shape> = Vec::with_capacity(list.prims.len().min(1 << 20));
    for prim in &list.prims {
        let col = prim_color(prim, bg);
        let pts = list.points(prim);
        match prim.kind {
            Kind::Polyline => {
                let bb = Bounds2::from_points(pts.iter().copied());
                if !bb.intersects(&vis) {
                    continue;
                }
                if bb.width() < px && bb.height() < px {
                    shapes.push(Shape::rect_filled(Rect::from_center_size(view.to_screen(rect, bb.center()), vec2(1.0, 1.0)), 0.0, col));
                    continue;
                }
                let w = if lineweights && prim.lw > 0.0 { (prim.lw * 3.78).clamp(1.0, 12.0) } else { 1.0 };
                shapes.push(Shape::line(pts.iter().map(|q| view.to_screen(rect, *q)).collect(), Stroke::new(w, col)));
            }
            Kind::Tris | Kind::Mask => {
                let col = if prim.kind == Kind::Mask { color32(bg) } else { col };
                let mut mesh = egui::Mesh::default();
                for tri in pts.as_chunks::<3>().0 {
                    let i = mesh.vertices.len() as u32;
                    for q in tri {
                        mesh.colored_vertex(view.to_screen(rect, *q), col);
                    }
                    mesh.add_triangle(i, i + 1, i + 2);
                }
                if !mesh.vertices.is_empty() {
                    shapes.push(Shape::mesh(mesh));
                }
            }
            Kind::Point => {
                if let Some(q) = pts.first()
                    && vis.contains(*q)
                {
                    shapes.push(Shape::rect_filled(Rect::from_center_size(view.to_screen(rect, *q), vec2(1.5, 1.5)), 0.0, col));
                }
            }
            Kind::Infinite { .. } | Kind::Image { .. } => {}
        }
    }
    p.extend(shapes);
}

/// A thin dashed rectangle (the printable area).
fn dashed_rect(p: &Painter, r: Rect, c: Color32) {
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in corners.windows(2) {
        if let [a, b] = w {
            p.extend(Shape::dashed_line(&[*a, *b], Stroke::new(1.0, c), 4.0, 3.0));
        }
    }
}

/// A point marker for measuring: where `at` snapped and how.
pub fn snap_marker(p: &Painter, at: Pos2, snap: crate::measure::Snap, c: Color32) {
    use crate::measure::Snap;
    let s = 7.0;
    let stroke = Stroke::new(2.0, c);
    match snap {
        Snap::End => {
            p.rect_stroke(Rect::from_center_size(at, vec2(2.0 * s, 2.0 * s)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        Snap::Mid => {
            p.add(Shape::closed_line(vec![pos2(at.x, at.y - s), pos2(at.x + s, at.y + s * 0.8), pos2(at.x - s, at.y + s * 0.8)], stroke));
        }
        Snap::Near => {
            p.add(Shape::closed_line(
                vec![pos2(at.x - s, at.y - s), pos2(at.x + s, at.y - s), pos2(at.x - s, at.y + s), pos2(at.x + s, at.y + s)],
                stroke,
            ));
        }
        Snap::Free => {
            p.line_segment([pos2(at.x - s, at.y - s), pos2(at.x + s, at.y + s)], stroke);
            p.line_segment([pos2(at.x - s, at.y + s), pos2(at.x + s, at.y - s)], stroke);
        }
    }
}
