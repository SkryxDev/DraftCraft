//! The status bar's drawing units: it shows the length format (LUNITS) and switches it through
//! UNITS; its last entry opens the Drawing Units dialog. The same as AutoCAD's Units status
//! control, so the units can be seen and changed without typing UNITS.

use egui::{Rect, Sense, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// LUNITS values and their names, in the order the menu lists them.
pub const LENGTH_FORMATS: [(i64, &str); 5] = [(4, "Architectural"), (2, "Decimal"), (3, "Engineering"), (5, "Fractional"), (1, "Scientific")];

/// The name of a LUNITS value ("Decimal" for one out of range, as AutoCAD reads it).
pub fn format_name(lunits: i64) -> &'static str {
    LENGTH_FORMATS.iter().find(|(v, _)| *v == lunits).map_or("Decimal", |(_, n)| n)
}

/// Draw the units control ending at `right`; returns its left edge, or `right` when there is no
/// drawing.
pub fn status(app: &mut CadApp, ui: &mut egui::Ui, right: f32, cy: f32) -> f32 {
    let Ok(d) = app.session.doc() else { return right };
    let lunits = d.header.i64("LUNITS", 2);
    let current = crate::i18n::t(format_name(lunits)).to_string();
    let t = Tokens::get();
    let p = ui.painter().clone();
    let g = p.layout_no_wrap(current, crate::theme::small(), t.text);
    let w = g.size().x.min(140.0) + 22.0;
    let r = Rect::from_min_size(pos2(right - w, cy - 9.0), vec2(w, 18.0));
    let resp = ui.interact(r, ui.scope_id().with("drawing_units"), Sense::click());
    p.rect_filled(r, 3.0, if resp.hovered() { t.control_hover } else { t.control.gamma_multiply(0.5) });
    p.with_clip_rect(r.shrink2(vec2(4.0, 0.0))).galley(pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, t.text);
    let (ax, ay) = (r.right() - 9.0, r.center().y);
    p.line_segment([pos2(ax - 3.0, ay - 1.5), pos2(ax, ay + 1.5)], egui::Stroke::new(1.2, t.text_dim));
    p.line_segment([pos2(ax, ay + 1.5), pos2(ax + 3.0, ay - 1.5)], egui::Stroke::new(1.2, t.text_dim));
    crate::icons::describe_control(ui, &resp, "Drawing units", None, false);
    let resp = resp.on_hover_text(crate::tl!("Drawing units"));
    let mut pick: Option<i64> = None;
    let mut dialog = false;
    egui::Popup::from_toggle_button_response(&resp).width(170.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        for (v, name) in LENGTH_FORMATS {
            if ui.selectable_label(v == lunits, crate::i18n::t(name)).clicked() {
                pick = Some(v);
                ui.close();
            }
        }
        ui.separator();
        if ui.selectable_label(false, crate::i18n::t("Units...")).clicked() {
            dialog = true;
            ui.close();
        }
    });
    if let Some(v) = pick {
        let res = app.run("units", json!({ "lunits": v }));
        app.canvas.list = None;
        if let Err(e) = res {
            app.status = Some((e, crate::now_ms()));
        }
    }
    if dialog {
        app.start("units");
    }
    r.left() - 6.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every format has a name, and the menu's pick reaches the drawing.
    #[test]
    fn the_units_control_names_and_sets_the_format() {
        assert_eq!(format_name(4), "Architectural");
        assert_eq!(format_name(1), "Scientific");
        assert_eq!(format_name(99), "Decimal");
        let mut app = CadApp::new(draftcraft_engine::Session::new(), crate::Services::default());
        for (v, _) in LENGTH_FORMATS {
            app.run("units", json!({ "lunits": v })).unwrap();
            assert_eq!(app.session.doc().unwrap().header.i64("LUNITS", 0), v);
        }
    }
}
