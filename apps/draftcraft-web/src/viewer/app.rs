//! The viewer app: one screen with the drawing, a title bar, a tool bar at the bottom, sheets that
//! slide up over the drawing (layers, layouts, text search, more), and the measuring card.

use std::sync::Arc;

use draftcraft_engine::doc::color::Rgb;
use draftcraft_engine::geom::Vec2;
use egui::{Color32, Pos2, Rect};
use wasm_bindgen::JsCast as _;

use crate::doc::Doc;
use crate::gesture::{Gestures, Op, Phase, Touch};
use crate::measure::{self, Snap};
use crate::picker::Inbox;
use crate::scene::Scene;
use crate::text::Words;
use crate::view::View;

const CANVAS_ID: &str = "draftcraft_canvas";
const LOADING_ID: &str = "draftcraft_loading";
const PREFS_KEY: &str = "draftcraft.viewer";

/// Snap reach for a tap, in points (a fingertip).
pub const SNAP_POINTS: f64 = 26.0;
/// How far above the finger the measuring reticle aims, in points, so the finger never hides it.
pub const RETICLE_LIFT: f32 = 72.0;
/// Most points one measurement takes.
pub const MAX_MEASURE_POINTS: usize = 500;
/// The view rests this long (seconds) after a gesture before the drawing is rebuilt for it.
const REST_AFTER: f64 = 0.12;

/// Dark and light drawing backgrounds (the full app's model space colours).
pub const DARK_BG: Rgb = Rgb(33, 40, 48);
pub const LIGHT_BG: Rgb = Rgb(255, 255, 255);
/// Around a layout's paper.
const PAPER_SURROUND: Color32 = Color32::from_rgb(92, 98, 108);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    View,
    Distance,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Layers,
    Layouts,
    FindText,
    More,
}

/// The reticle a finger is aiming in measuring mode: the snapped world point and how it snapped.
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub world: Vec2,
    pub snap: Snap,
}

pub struct Viewer {
    pub w: &'static Words,
    pub doc: Option<Doc>,
    pub scene: Scene,
    pub view: View,
    /// The drawing area last frame.
    pub rect: Rect,
    gestures: Gestures,
    pub tool: Tool,
    pub points: Vec<Vec2>,
    pub aim: Option<Aim>,
    pub sheet: Option<Sheet>,
    pub layer_filter: String,
    pub text_query: String,
    pub text_hits: Vec<(String, draftcraft_engine::geom::Bounds2)>,
    pub dark: bool,
    pub lineweights: bool,
    /// A file waiting to open, and how many frames to wait first (so "Opening…" shows).
    pending: Option<(String, Vec<u8>, u8)>,
    inbox: Inbox,
    pub toast: Option<(String, f64)>,
    fit_pending: bool,
    /// The view is still the one fitted on opening (no gesture since): it fits again when the
    /// screen changes size (the page settling, the phone turning).
    auto_fit: bool,
    last_time: f64,
    last_gesture: f64,
    /// Touches that began on the drawing (others belong to the bars and sheets).
    canvas_touches: Vec<u64>,
    faults: Arc<draftcraft_ui_egui::gpu::GpuFaults>,
    title_sent: Option<String>,
    /// The drawing is too large to show whole, and the user was told.
    pub told_truncated: bool,
}

impl Viewer {
    fn new(cc: &eframe::CreationContext<'_>, w: &'static Words, cpu: bool) -> Viewer {
        let faults = Arc::new(draftcraft_ui_egui::gpu::GpuFaults::default());
        let mut scene = Scene::default();
        if !cpu && let Some(rs) = &cc.wgpu_render_state {
            draftcraft_ui_egui::gpu::install(rs, &faults, None);
            scene.gpu = true;
        }
        let prefs = cc.storage.and_then(|s| s.get_string(PREFS_KEY)).unwrap_or_default();
        style(&cc.egui_ctx);
        Viewer {
            w,
            doc: None,
            scene,
            view: View::default(),
            rect: Rect::NOTHING,
            gestures: Gestures::default(),
            tool: Tool::View,
            points: Vec::new(),
            aim: None,
            sheet: None,
            layer_filter: String::new(),
            text_query: String::new(),
            text_hits: Vec::new(),
            dark: !prefs.contains("light"),
            lineweights: prefs.contains("lw"),
            pending: None,
            inbox: Inbox::new(&cc.egui_ctx),
            toast: None,
            fit_pending: false,
            auto_fit: false,
            last_time: 0.0,
            last_gesture: 0.0,
            canvas_touches: Vec::new(),
            faults,
            title_sent: None,
            told_truncated: false,
        }
    }

    pub fn pick_file(&self) {
        self.inbox.pick();
    }

    pub fn open_sample(&mut self) {
        let d = draftcraft_engine::sample::default_sample();
        self.set_doc(Doc::new("Bracket".into(), d));
    }

    fn set_doc(&mut self, doc: Doc) {
        self.doc = Some(doc);
        self.scene.clear();
        self.points.clear();
        self.aim = None;
        self.sheet = None;
        self.text_hits.clear();
        self.text_query.clear();
        self.layer_filter.clear();
        self.fit_pending = true;
        self.told_truncated = false;
        self.gestures.stop();
    }

    pub fn say(&mut self, message: impl Into<String>, now: f64) {
        self.toast = Some((message.into(), now + 4.0));
    }

    /// Fit the drawing (or the paper) to the screen.
    pub fn fit(&mut self) {
        let b = match &self.doc {
            Some(doc) => doc.fit_bounds(),
            None => draftcraft_engine::geom::Bounds2::EMPTY,
        };
        let b = if b.is_empty() { self.scene.extents() } else { b };
        self.view = self.view.fit(&b, self.rect);
        self.gestures.stop();
    }

    /// Show `b` filling most of the screen, no closer than a few text heights.
    pub fn zoom_to(&mut self, b: &draftcraft_engine::geom::Bounds2) {
        let size = b.width().max(b.height()).max(1e-9);
        let around = b.expand(size * 2.0);
        self.view = self.view.fit(&around, self.rect);
        self.gestures.stop();
    }

    pub fn set_space(&mut self, space: draftcraft_engine::doc::Space) {
        if let Some(doc) = &mut self.doc {
            doc.set_space(space);
            self.points.clear();
            self.fit_pending = true;
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = if self.tool == tool { Tool::View } else { tool };
        self.points.clear();
        self.aim = None;
        self.sheet = None;
    }

    pub fn open_full_app(&self) {
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href("./?desktop");
        }
    }

    /// Take in what the picker read: fonts first (so the drawing draws with them), then the
    /// first drawing waits a frame to open.
    fn receive(&mut self, ctx: &egui::Context, now: f64) {
        self.inbox.take_dropped(ctx);
        let mut fonts = false;
        for upload in self.inbox.take() {
            match upload {
                Ok((name, bytes)) if name.to_ascii_lowercase().ends_with(".shx") => match draftcraft_engine::fonts::shx::register(&name, &bytes) {
                    Ok(()) => fonts = true,
                    Err(e) => self.say(format!("{name}: {e}"), now),
                },
                Ok((name, bytes)) => self.pending = Some((name, bytes, 2)),
                Err(e) => self.say(e, now),
            }
        }
        if fonts && let Some(doc) = &mut self.doc {
            doc.revision += 1;
        }
    }

    /// Open the waiting file once "Opening…" has been on screen.
    fn open_pending(&mut self, ctx: &egui::Context, now: f64) {
        let Some((name, bytes, wait)) = self.pending.take() else { return };
        if wait > 0 {
            self.pending = Some((name, bytes, wait - 1));
            ctx.request_repaint();
            return;
        }
        match draftcraft_io::read(&bytes, &name) {
            Ok(d) => {
                let note = d.read_notes.first().cloned();
                self.set_doc(Doc::new(name, d));
                if let Some(note) = note {
                    self.say(note, now);
                }
            }
            Err(e) => self.say(format!("{name}: {e}"), now),
        }
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Follow the fingers (or the mouse) on the drawing.
    fn navigate(&mut self, ctx: &egui::Context, response: &egui::Response, now: f64, dt: f64) {
        let rect = self.rect;
        let reticle = self.tool != Tool::View;
        let touches = self.canvas_touch_events(ctx, rect);
        let touch_screen = ctx.input(|i| i.has_touch_screen());
        let mut ops = self.gestures.feed(&touches, now, dt, reticle);
        if !touch_screen {
            ops.extend(mouse_ops(ctx, response, rect, reticle));
        }
        if !ops.is_empty() {
            self.last_gesture = now;
            self.auto_fit = false;
        }
        for op in ops {
            match op {
                Op::Pan(d) => self.view.pan(d),
                Op::Zoom { anchor, factor } => self.view.zoom_at(rect, anchor, factor),
                Op::DoubleTap(at) => self.view.zoom_at(rect, at, 2.0),
                Op::TwoFingerTap(at) => self.view.zoom_at(rect, at, 0.5),
                Op::Tap(at) => {
                    if self.sheet.take().is_none() && reticle {
                        let aim = self.aim_at(at, SNAP_POINTS);
                        self.add_point(aim.world);
                    }
                }
                Op::Drag(at) => self.aim = Some(self.aim_at(at - egui::vec2(0.0, RETICLE_LIFT), SNAP_POINTS * 0.7)),
                Op::DragEnd(_) => {
                    if let Some(aim) = self.aim.take() {
                        self.add_point(aim.world);
                    }
                }
            }
        }
        if self.gestures.gliding() {
            ctx.request_repaint();
        }
    }

    /// This frame's touches that belong to the drawing: those that started on it, not on a bar
    /// or a sheet above it.
    fn canvas_touch_events(&mut self, ctx: &egui::Context, rect: Rect) -> Vec<Touch> {
        let events = ctx.input(|i| i.events.clone());
        let mut out = Vec::new();
        for e in events {
            let egui::Event::Touch { id, phase, pos, .. } = e else { continue };
            let id = id.0;
            let phase = match phase {
                egui::TouchPhase::Start => Phase::Start,
                egui::TouchPhase::Move => Phase::Move,
                egui::TouchPhase::End | egui::TouchPhase::Cancel => Phase::End,
            };
            if phase == Phase::Start {
                let on_canvas = rect.contains(pos) && ctx.layer_id_at(pos).is_none_or(|l| l == egui::LayerId::background());
                if !on_canvas {
                    continue;
                }
                if !self.canvas_touches.contains(&id) && self.canvas_touches.len() < 16 {
                    self.canvas_touches.push(id);
                }
            } else if !self.canvas_touches.contains(&id) {
                continue;
            }
            if phase == Phase::End {
                self.canvas_touches.retain(|t| *t != id);
            }
            out.push(Touch { id, phase, pos });
        }
        out
    }

    /// Where a finger at `at` aims: the drawing point it snaps to within `reach` points.
    pub fn aim_at(&self, at: Pos2, reach: f64) -> Aim {
        let world = self.view.to_world(self.rect, at);
        match &self.scene.list {
            Some(list) => {
                let (world, snap) = measure::snap(list, world, reach / self.view.scale);
                Aim { world, snap }
            }
            None => Aim { world, snap: Snap::Free },
        }
    }

    fn add_point(&mut self, p: Vec2) {
        if !p.is_finite() {
            return;
        }
        match self.tool {
            Tool::Distance if self.points.len() >= 2 => {
                self.points.clear();
                self.points.push(p);
            }
            Tool::Distance | Tool::Area if self.points.len() < MAX_MEASURE_POINTS => self.points.push(p),
            _ => {}
        }
    }

    /// A length in the drawing's units and format, with the unit name.
    pub fn length_text(&self, v: f64) -> String {
        let Some(doc) = &self.doc else { return format!("{v:.4}") };
        let h = &doc.drawing.header;
        let s = draftcraft_engine::units::format_distance(v, h.i64("LUNITS", 2), h.i64("LUPREC", 4));
        let unit = measure::unit_suffix(h.i64("INSUNITS", 0));
        if unit.is_empty() || matches!(h.i64("LUNITS", 2), 3 | 4) { s } else { format!("{s} {unit}") }
    }

    pub fn area_text(&self, v: f64) -> String {
        let Some(doc) = &self.doc else { return format!("{v:.4}") };
        let h = &doc.drawing.header;
        let prec = u8::try_from(h.i64("LUPREC", 4).clamp(0, 8)).unwrap_or(4);
        let s = draftcraft_engine::render::units::format_decimal(v, prec, 0, ".");
        let unit = measure::unit_suffix(h.i64("INSUNITS", 0));
        if unit.is_empty() { s } else { format!("{s} {unit}²") }
    }

    /// Rebuild the drawing for the resting view, fit it when asked to.
    fn update_scene(&mut self, now: f64) {
        if self.doc.is_none() || self.rect.width() <= 0.0 || self.rect.height() <= 0.0 {
            return;
        }
        let resting = !self.gestures.active() && !self.gestures.gliding() && now - self.last_gesture >= REST_AFTER;
        if self.fit_pending {
            self.fit();
            self.fit_pending = false;
            self.auto_fit = true;
        }
        if let Some(doc) = &self.doc
            && self.scene.stale(doc, &self.view, self.rect, self.lineweights, resting)
        {
            self.scene.build(doc, &self.view, self.rect, self.lineweights);
        }
    }

    fn set_title(&mut self) {
        let title = match &self.doc {
            Some(d) => format!("{} — DraftCraft", d.name),
            None => "DraftCraft".to_string(),
        };
        if self.title_sent.as_deref() != Some(title.as_str())
            && let Some(document) = web_sys::window().and_then(|w| w.document())
        {
            document.set_title(&title);
            self.title_sent = Some(title);
        }
    }
}

/// Mouse and trackpad on a computer (`?mobile` on a desktop browser): drag pans (or aims), the
/// wheel and pinch zoom, a click taps and a double click zooms in.
fn mouse_ops(ctx: &egui::Context, response: &egui::Response, rect: Rect, reticle: bool) -> Vec<Op> {
    let mut ops = Vec::new();
    let Some(at) = response.hover_pos().or_else(|| response.interact_pointer_pos()) else { return ops };
    let (scroll, zoom) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
    if response.hovered() {
        if zoom != 1.0 {
            ops.push(Op::Zoom { anchor: at, factor: f64::from(zoom) });
        } else if scroll.y != 0.0 {
            ops.push(Op::Zoom { anchor: at, factor: (f64::from(scroll.y) / 300.0).exp() });
        }
    }
    if response.dragged() {
        if reticle {
            ops.push(Op::Drag(at));
        } else {
            ops.push(Op::Pan(response.drag_delta()));
        }
    }
    if response.drag_stopped() && reticle {
        ops.push(Op::DragEnd(at));
    }
    if response.double_clicked() && !reticle {
        ops.push(Op::DoubleTap(at));
    } else if response.clicked() {
        ops.push(Op::Tap(at));
    }
    let _ = rect;
    ops
}

/// Large touch targets and text for a phone, on the dark theme.
fn style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(16.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(16.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(20.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(13.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(15.0));
        s.spacing.interact_size.y = 40.0;
        s.spacing.button_padding = egui::vec2(14.0, 9.0);
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
        s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
        s.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
        s.scroll_animation = egui::style::ScrollAnimation::none();
    });
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_time).clamp(0.0, 0.25);
        self.last_time = now;
        if ctx.current_pass_index() == 0 {
            self.receive(&ctx, now);
        }
        if let Some(fault) = self.faults.take() {
            log::error!("gpu: {}", fault.message);
            if fault.lost {
                let msg = self.w.gpu_failed;
                self.say(msg, now);
            }
        }
        crate::sheets::top_bar(self, ui);
        crate::sheets::bottom_bar(self, ui);
        let canvas_bg = |v: &Viewer| {
            let on_paper = v.doc.as_ref().is_some_and(|d| d.space != draftcraft_engine::doc::Space::Model);
            let bg = if v.dark { DARK_BG } else { LIGHT_BG };
            if on_paper { PAPER_SURROUND } else { Color32::from_rgb(bg.0, bg.1, bg.2) }
        };
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let rect = ui.max_rect();
            if self.auto_fit && (rect.size() - self.rect.size()).length() > 0.5 {
                self.fit_pending = true;
            }
            self.rect = rect;
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            if self.doc.is_some() {
                self.navigate(&ctx, &response, now, dt);
            }
            self.open_pending(&ctx, now);
            self.update_scene(now);
            let painter = ui.painter_at(rect);
            let model_bg = if self.dark { DARK_BG } else { LIGHT_BG };
            let bg = canvas_bg(self);
            if self.doc.is_some() {
                let (view, lw) = (self.view, self.lineweights);
                self.scene.draw(&painter, rect, &view, bg, model_bg, lw);
                crate::sheets::measure_overlay(self, &painter);
            } else {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(DARK_BG.0, DARK_BG.1, DARK_BG.2));
            }
            // After a gesture, come back once it has rested to rebuild the drawing for the view.
            if now - self.last_gesture < REST_AFTER * 2.0 {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        });
        if self.doc.is_none() && !self.loading() {
            crate::sheets::start_card(self, &ctx);
        }
        crate::sheets::measure_card(self, &ctx);
        crate::sheets::sheet(self, &ctx);
        crate::sheets::overlays(self, &ctx, now);
        self.set_title();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let prefs = format!("{}{}", if self.dark { "dark" } else { "light" }, if self.lineweights { ",lw" } else { "" });
        storage.set_string(PREFS_KEY, prefs);
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

fn has_flag(query: &str, name: &str) -> bool {
    query.trim_start_matches('?').split('&').any(|p| p == name || p.strip_prefix(name).is_some_and(|r| r.starts_with('=')))
}

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(CANVAS_ID))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        log::error!("missing <canvas id=\"{CANVAS_ID}\">");
        return;
    };
    let q = query();
    let mut languages: Vec<String> =
        web_sys::window().map(|w| w.navigator().languages().iter().filter_map(|v| v.as_string()).collect()).unwrap_or_default();
    if let Some(lang) = crate::text::lang_from_query(&q) {
        languages.insert(0, lang);
    }
    let words = crate::text::for_display(crate::text::pick(&languages));
    wasm_bindgen_futures::spawn_local(async move {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let mut options = eframe::WebOptions::default();
        if has_flag(&q, "webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let cpu = has_flag(&q, "cpu");
        let sample = has_flag(&q, "sample");
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    let mut viewer = Viewer::new(cc, words, cpu);
                    if sample {
                        viewer.open_sample();
                    }
                    Ok(Box::new(viewer))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>DraftCraft could not start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>")),
            }
        }
    });
}
