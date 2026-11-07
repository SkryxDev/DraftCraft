//! DraftCraft's own window frame, as Windows CAD programs draw theirs. The desktop's title bar is
//! turned off and DraftCraft draws one row in its place: the logo, the name, the menu bar (File,
//! Edit, View … Help) and the minimize, maximize and close buttons. Its empty part moves the
//! window (a double-click maximizes it), and the window's edges and corners resize it. The menu
//! bar no longer needs a row of its own under the ribbon.
//!
//! On by default on Windows. `RUSTDRAFT_TITLEBAR=system` keeps the desktop's title bar there;
//! `RUSTDRAFT_TITLEBAR=app` draws this frame on any desktop (to try it). macOS keeps its own
//! integrated title bar and Linux the desktop's.

use egui::{Align2, Color32, CursorIcon, FontId, Rect, ResizeDirection, Sense, Stroke, StrokeKind, ViewportCommand, pos2, vec2};

use crate::CadApp;
use crate::theme::Tokens;

/// Height of the title bar row.
pub const TITLE_H: f32 = 30.0;
/// Width of each window button, as Windows draws them.
const BUTTON_W: f32 = 46.0;
/// How far in from the window's edge a press resizes it.
const EDGE: f32 = 5.0;
/// Corners reach this far along each edge.
const CORNER: f32 = 12.0;

/// Whether RustDraft draws its own window frame, given `RUSTDRAFT_TITLEBAR`: `app` draws it, and
/// `system` (or `desktop`) leaves the desktop's; otherwise it is drawn on Windows only.
pub fn draws_own_frame(var: Option<&str>) -> bool {
    match var.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("app") => true,
        Some("system" | "desktop") => false,
        _ => cfg!(target_os = "windows"),
    }
}

/// [`draws_own_frame`] for this process's environment.
pub fn draws_own_frame_here() -> bool {
    draws_own_frame(std::env::var("RUSTDRAFT_TITLEBAR").ok().as_deref())
}

/// The window buttons, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// The title bar row: logo, name, menu bar, the empty part that moves the window, window buttons.
pub fn title_bar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let mut clicked = None;
    egui::Panel::top("cc_window_title").exact_size(TITLE_H).frame(egui::Frame::NONE.fill(t.chrome_dark)).show(ui, |ui| {
        let r = ui.max_rect();
        let buttons = Rect::from_min_max(pos2(r.right() - BUTTON_W * 3.0, r.top()), r.max);
        let free = Rect::from_min_max(r.min, pos2(buttons.left(), r.bottom()));
        // First, so the controls drawn over it keep their own clicks.
        craft_window::title_bar::empty_area(ui, free).on_hover_text("DraftCraft");
        let p = ui.painter().clone();
        let logo = Rect::from_center_size(pos2(r.left() + 17.0, r.center().y), vec2(18.0, 18.0));
        paint_logo(&p, logo);
        let name = p.layout_no_wrap("DraftCraft".to_string(), FontId::proportional(13.0), t.text);
        let name_x = logo.right() + 7.0;
        let name_w = name.size().x;
        p.galley(pos2(name_x, r.center().y - name.size().y / 2.0), name, t.text);
        if app.ui.in_window_menu {
            let menus = Rect::from_min_max(pos2(name_x + name_w + 12.0, r.top()), pos2(free.right(), r.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(menus).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                ui.set_clip_rect(menus);
                clicked = crate::menus::menu_titles(app, ui);
            });
        }
        caption_buttons(ui, buttons);
    });
    if let Some(id) = clicked {
        crate::menus::activate(app, &id);
    }
}

/// The app icon, 64 px, drawn in the title bar.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/64x64/apps/dev.skryx.rustdraft.png");

/// The app icon as a texture, decoded once per context. `None` if it doesn't decode.
fn icon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::unique("dc_window_icon");
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(tex);
    }
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [usize::try_from(img.width()).ok()?, usize::try_from(img.height()).ok()?];
    let tex = ctx.load_texture("window-icon", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// The app icon, small; a DC tile if the icon can't be drawn.
fn paint_logo(p: &egui::Painter, r: Rect) {
    if let Some(tex) = icon_texture(p.ctx()) {
        p.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        return;
    }
    p.rect_filled(r, 4.0, Color32::from_rgb(0x0e, 0x34, 0x63));
    p.text(r.center(), Align2::CENTER_CENTER, "DC", FontId::proportional(r.height() * 0.5), Color32::WHITE);
}

fn caption_buttons(ui: &mut egui::Ui, area: Rect) {
    let t = Tokens::get();
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    let p = ui.painter().clone();
    for (i, kind) in [Caption::Minimize, Caption::Maximize, Caption::Close].into_iter().enumerate() {
        let br = Rect::from_min_size(pos2(area.left() + BUTTON_W * i as f32, area.top()), vec2(BUTTON_W, area.height()));
        let resp = ui.interact(br, ui.scope_id().with(("window_button", i)), Sense::click());
        let close = kind == Caption::Close;
        if resp.hovered() {
            p.rect_filled(br, 0.0, if close { Color32::from_rgb(0xc4, 0x2b, 0x1c) } else { t.control });
        }
        let color = if close && resp.hovered() { Color32::WHITE } else { t.text };
        let s = Stroke::new(1.0, color);
        let m = br.center();
        match kind {
            Caption::Minimize => {
                p.line_segment([m + vec2(-5.0, 0.0), m + vec2(5.0, 0.0)], s);
            }
            Caption::Maximize if maximized => {
                p.rect_stroke(Rect::from_center_size(m + vec2(-1.0, 1.0), vec2(8.0, 8.0)), 0.0, s, StrokeKind::Inside);
                p.line_segment([m + vec2(1.0, -5.0), m + vec2(5.0, -5.0)], s);
                p.line_segment([m + vec2(5.0, -5.0), m + vec2(5.0, 3.0)], s);
            }
            Caption::Maximize => {
                p.rect_stroke(Rect::from_center_size(m, vec2(10.0, 10.0)), 0.0, s, StrokeKind::Inside);
            }
            Caption::Close => {
                p.line_segment([m + vec2(-5.0, -5.0), m + vec2(5.0, 5.0)], s);
                p.line_segment([m + vec2(-5.0, 5.0), m + vec2(5.0, -5.0)], s);
            }
        }
        let label = match kind {
            Caption::Minimize => crate::tl!("Minimize"),
            Caption::Maximize if maximized => crate::tl!("Restore Down"),
            Caption::Maximize => crate::tl!("Maximize"),
            Caption::Close => crate::tl!("Close"),
        };
        crate::icons::describe_control(ui, &resp, label, None, false);
        let clicked = resp.clicked();
        resp.on_hover_text(label);
        if clicked {
            ui.ctx().send_viewport_cmd(match kind {
                Caption::Minimize => ViewportCommand::Minimized(true),
                Caption::Maximize => ViewportCommand::Maximized(!maximized),
                // The app's close path asks about unsaved drawings.
                Caption::Close => ViewportCommand::Close,
            });
        }
    }
}

/// The edge or corner of `window` that `pos` is on, `edge` deep.
fn edge_at(window: Rect, pos: egui::Pos2, edge: f32) -> Option<ResizeDirection> {
    if !window.contains(pos) {
        return None;
    }
    let (l, r) = (pos.x - window.left(), window.right() - pos.x);
    let (top, bottom) = (pos.y - window.top(), window.bottom() - pos.y);
    let (near_l, near_r, near_t, near_b) = (l < edge, r < edge, top < edge, bottom < edge);
    // A corner reaches `CORNER` along both of its edges.
    let top_side = near_t || ((near_l || near_r) && top < CORNER);
    let bottom_side = near_b || ((near_l || near_r) && bottom < CORNER);
    let left_side = near_l || ((near_t || near_b) && l < CORNER);
    let right_side = near_r || ((near_t || near_b) && r < CORNER);
    Some(match (top_side, bottom_side, left_side, right_side) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (true, _, _, true) => ResizeDirection::NorthEast,
        (_, true, true, _) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, _, _, _) => ResizeDirection::North,
        (_, true, _, _) => ResizeDirection::South,
        (_, _, true, _) => ResizeDirection::West,
        (_, _, _, true) => ResizeDirection::East,
        _ => return None,
    })
}

fn cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}

/// The window's edges and corners resize it, and a thin border marks them. Call once per pass,
/// after the rest of the UI. Nothing happens while the window is maximized or full screen.
pub fn edges(ctx: &egui::Context) {
    let (maximized, fullscreen) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    if maximized || fullscreen {
        return;
    }
    let window = ctx.viewport_rect();
    let t = Tokens::get();
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("rd_window_edges"));
    ctx.layer_painter(layer).rect_stroke(window, 0.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    // A foreground area over the edges takes the press, so the drawing or a panel under the
    // edge never starts a pick or a drag there.
    egui::Area::new(egui::Id::unique("rd_window_edge_area")).order(egui::Order::Foreground).fixed_pos(window.min).interactable(true).show(
        ctx,
        |ui| {
            let strips = [
                Rect::from_min_max(window.min, pos2(window.right(), window.top() + EDGE)),
                Rect::from_min_max(pos2(window.left(), window.bottom() - EDGE), window.max),
                Rect::from_min_max(window.min, pos2(window.left() + EDGE, window.bottom())),
                Rect::from_min_max(pos2(window.right() - EDGE, window.top()), window.max),
            ];
            for (i, strip) in strips.into_iter().enumerate() {
                let resp = ui.interact(strip, ui.scope_id().with(("edge", i)), Sense::drag());
                let Some(dir) = ctx.pointer_hover_pos().and_then(|p| edge_at(window, p, EDGE)) else { continue };
                if resp.hovered() {
                    ctx.set_cursor_icon(cursor(dir));
                }
                if resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_pressed()) {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_is_drawn_on_windows_unless_asked_otherwise() {
        assert!(draws_own_frame(Some("app")) && draws_own_frame(Some(" APP ")));
        assert!(!draws_own_frame(Some("system")) && !draws_own_frame(Some("desktop")));
        assert_eq!(draws_own_frame(None), cfg!(target_os = "windows"));
        assert_eq!(draws_own_frame(Some("whatever")), cfg!(target_os = "windows"));
    }

    #[test]
    fn edges_and_corners() {
        let w = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0));
        let at = |x: f32, y: f32| edge_at(w, pos2(x, y), EDGE);
        assert_eq!(at(500.0, 2.0), Some(ResizeDirection::North));
        assert_eq!(at(500.0, 798.0), Some(ResizeDirection::South));
        assert_eq!(at(2.0, 400.0), Some(ResizeDirection::West));
        assert_eq!(at(998.0, 400.0), Some(ResizeDirection::East));
        assert_eq!(at(2.0, 2.0), Some(ResizeDirection::NorthWest));
        assert_eq!(at(8.0, 2.0), Some(ResizeDirection::NorthWest), "the corner reaches along the top");
        assert_eq!(at(998.0, 8.0), Some(ResizeDirection::NorthEast), "and down the side");
        assert_eq!(at(2.0, 798.0), Some(ResizeDirection::SouthWest));
        assert_eq!(at(998.0, 798.0), Some(ResizeDirection::SouthEast));
        assert_eq!(at(500.0, 400.0), None);
        assert_eq!(at(-1.0, 400.0), None);
    }
}
