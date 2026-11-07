//! The DraftCraft logo in the interface: the app icon (a drafting compass and the arc it drew on a
//! blueprint-blue tile, `assets/app-icon/`), drawn on the application button, the Start page, the About
//! window and DraftCraft's own title bar. The icon is WithShalom Studios' mark, used by agreement
//! (`docs/brand/LICENSE-brand.txt`).

use egui::{Align2, Color32, FontId, Rect, pos2};

/// The icon at 64 px, for marks up to about 32 px tall.
const ICON_SMALL: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/64x64/apps/dev.skryx.draftcraft.png");
/// The icon at 256 px, for larger marks.
const ICON_LARGE: &[u8] = include_bytes!("../../../assets/app-icon/draftcraft-256.png");

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
