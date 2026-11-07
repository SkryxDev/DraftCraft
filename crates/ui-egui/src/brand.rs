//! The DraftCraft logo in the interface: the app icon (a drafting compass and the arc it drew on a
//! blueprint-blue tile, `assets/app-icon/`), drawn on the application button, the Start page and the About
//! window, and the full logo (the icon and the wordmark as one piece, `assets/brand/`) at the left of the
//! top row. Both are WithShalom Studios' marks, used by agreement (`docs/brand/LICENSE-brand.txt`).

use egui::{Align2, Color32, FontId, Rect, pos2};

/// The icon at 64 px, for marks up to about 32 px tall.
const ICON_SMALL: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/64x64/apps/dev.skryx.draftcraft.png");
/// The icon at 256 px, for larger marks.
const ICON_LARGE: &[u8] = include_bytes!("../../../assets/app-icon/draftcraft-256.png");

/// The full logo (icon and wordmark) with light text, for dark chrome; cropped to its ink, 96 px tall.
const FULL_LIGHT: &[u8] = include_bytes!("../../../assets/brand/draftcraft-logo-light-text.png");
/// The full logo with dark text, for light chrome.
const FULL_DARK: &[u8] = include_bytes!("../../../assets/brand/draftcraft-logo-dark-text.png");

/// DraftCraft's blueprint blue, behind the fallback mark.
const BLUEPRINT: Color32 = Color32::from_rgb(0x0e, 0x34, 0x63);

/// The icon as a texture sized for marks of `height` points, decoded once per context. `None` if it
/// doesn't decode.
pub fn texture(ctx: &egui::Context, height: f32) -> Option<egui::TextureHandle> {
    let large = height * ctx.pixels_per_point() > 64.0;
    let (bytes, key, name) =
        if large { (ICON_LARGE, "dc_logo_large", "draftcraft-logo-256") } else { (ICON_SMALL, "dc_logo_small", "draftcraft-logo-64") };
    let id = egui::Id::unique(key);
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(tex);
    }
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [usize::try_from(img.width()).ok()?, usize::try_from(img.height()).ok()?];
    let tex = ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Paint the logo filling `r` (a square); a blueprint-blue DC tile if the icon can't be drawn.
pub fn paint(p: &egui::Painter, r: Rect) {
    if let Some(tex) = texture(p.ctx(), r.height()) {
        p.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        return;
    }
    p.rect_filled(r, r.height() * 0.18, BLUEPRINT);
    p.text(r.center(), Align2::CENTER_CENTER, "DC", FontId::proportional(r.height() * 0.5), Color32::WHITE);
}

/// The full-logo texture for light or dark text, decoded once per context. `None` if it doesn't decode.
fn full_texture(ctx: &egui::Context, light_text: bool) -> Option<egui::TextureHandle> {
    let (bytes, key, name) =
        if light_text { (FULL_LIGHT, "dc_full_light", "draftcraft-logo-light") } else { (FULL_DARK, "dc_full_dark", "draftcraft-logo-dark") };
    let id = egui::Id::unique(key);
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(tex);
    }
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [usize::try_from(img.width()).ok()?, usize::try_from(img.height()).ok()?];
    let tex = ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Paint the full logo (icon and wordmark) `height` points tall with its left edge at `left`, centred on
/// `center_y`, in the version that reads on `chrome` (light text on dark chrome, dark text on light).
/// Returns the width used. If the image can't be drawn, the icon and the name in plain text are drawn instead.
pub fn paint_full(p: &egui::Painter, left: f32, center_y: f32, height: f32, chrome: Color32) -> f32 {
    let light_text = u32::from(chrome.r()) + u32::from(chrome.g()) + u32::from(chrome.b()) < 384;
    if let Some(tex) = full_texture(p.ctx(), light_text) {
        let [w, h] = tex.size();
        let aspect = if h == 0 { 1.0 } else { w as f32 / h as f32 };
        let r = Rect::from_min_size(pos2(left, center_y - height / 2.0), egui::vec2(height * aspect, height));
        p.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        return r.width();
    }
    let icon = Rect::from_min_size(pos2(left, center_y - height / 2.0), egui::vec2(height, height));
    paint(p, icon);
    let ink = if light_text { Color32::WHITE } else { Color32::BLACK };
    let galley = p.layout_no_wrap("DraftCraft".to_string(), FontId::proportional(height * 0.55), ink);
    let text_left = icon.right() + height * 0.3;
    let width = text_left - left + galley.size().x;
    p.galley(pos2(text_left, center_y - galley.size().y / 2.0), galley, ink);
    width
}

/// The logo as a widget of `size` points (square).
pub fn show(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), r);
    response
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_icon_sizes_decode() {
        for (bytes, px) in [(super::ICON_SMALL, 64), (super::ICON_LARGE, 256)] {
            let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
            assert_eq!((img.width(), img.height()), (px, px));
        }
    }

    #[test]
    fn both_full_logos_decode_and_match() {
        let mut sizes = Vec::new();
        for bytes in [super::FULL_LIGHT, super::FULL_DARK] {
            let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
            assert!(img.width() > img.height() * 3, "the full logo is wide");
            sizes.push((img.width(), img.height()));
        }
        assert_eq!(sizes[0], sizes[1]);
    }

    #[test]
    fn the_full_logo_takes_its_width_from_its_height() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let p = ui.painter().clone();
            let a = super::paint_full(&p, 0.0, 10.0, 16.0, egui::Color32::from_rgb(0x31, 0x38, 0x44));
            let b = super::paint_full(&p, 0.0, 10.0, 32.0, egui::Color32::from_rgb(0xd5, 0xd9, 0xdf));
            assert!(a > 16.0 * 3.0);
            assert!((b - a * 2.0).abs() < 0.01);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn the_texture_is_made_once_per_size() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let a = super::texture(ui.ctx(), 22.0).unwrap();
            let b = super::texture(ui.ctx(), 22.0).unwrap();
            assert_eq!(a.id(), b.id());
            let big = super::texture(ui.ctx(), 96.0).unwrap();
            assert_ne!(a.id(), big.id());
        });
        out.textures_delta.clear();
    }
}
