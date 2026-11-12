//! The viewer's controls: the title bar, the tool bar, the sheets that slide up over the drawing
//! (layers, layouts, text search, more), the measuring card and overlay, and the start card.

use draftcraft_engine::doc::Space;
use draftcraft_ui_egui::icons::{self, Icon};
use egui::{Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::app::{RETICLE_LIFT, Sheet, Tool, Viewer};
use crate::measure;

const BAR: Color32 = Color32::from_rgb(24, 29, 35);
const PANEL: Color32 = Color32::from_rgb(30, 36, 43);
const ROW: Color32 = Color32::from_rgb(38, 45, 54);
const TEXT: Color32 = Color32::from_rgb(230, 234, 238);
const DIM: Color32 = Color32::from_rgb(150, 160, 170);
const ACCENT: Color32 = Color32::from_rgb(64, 156, 255);
const MEASURE: Color32 = Color32::from_rgb(255, 176, 32);
/// Most layers listed at once (a filter finds the others).
const MAX_LAYER_ROWS: usize = 2000;

pub fn top_bar(v: &mut Viewer, ui: &mut Ui) {
    egui::Panel::top("viewer_top").exact_size(52.0).frame(Frame::NONE.fill(BAR).inner_margin(Margin::symmetric(10, 6))).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            draftcraft_ui_egui::brand::paint(ui.painter(), rect);
            let title = v.doc.as_ref().map_or(v.w.app, |d| d.name.as_str()).to_string();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Open, 40.0, false).on_hover_text(v.w.open).clicked() {
                    v.pick_file();
                }
                if v.doc.is_some() {
                    if icon_button(ui, Icon::Search, 40.0, v.sheet == Some(Sheet::FindText)).on_hover_text(v.w.find_text).clicked() {
                        toggle(v, Sheet::FindText);
                    }
                    chip(ui, v.w.read_only);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(title).color(TEXT).strong()).truncate());
                });
            });
        });
    });
}

pub fn bottom_bar(v: &mut Viewer, ui: &mut Ui) {
    let has_doc = v.doc.is_some();
    egui::Panel::bottom("viewer_tools").exact_size(64.0).frame(Frame::NONE.fill(BAR).inner_margin(Margin::symmetric(4, 2))).show(ui, |ui| {
        let w = (ui.available_width() / 5.0).floor();
        ui.add_enabled_ui(has_doc, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if tool_button(ui, Icon::ZoomExtents, v.w.fit, false, w).clicked() {
                    v.sheet = None;
                    v.fit();
                }
                if tool_button(ui, Icon::Layers, v.w.layers, v.sheet == Some(Sheet::Layers), w).clicked() {
                    toggle(v, Sheet::Layers);
                }
                if tool_button(ui, Icon::Distance, v.w.measure, v.tool != Tool::View, w).clicked() {
                    let tool = if v.tool == Tool::View { Tool::Distance } else { v.tool };
                    v.set_tool(tool);
                }
                if tool_button(ui, Icon::PageSetup, v.w.layouts, v.sheet == Some(Sheet::Layouts), w).clicked() {
                    toggle(v, Sheet::Layouts);
                }
                if tool_button(ui, Icon::Menu, v.w.more, v.sheet == Some(Sheet::More), w).clicked() {
                    toggle(v, Sheet::More);
                }
            });
        });
    });
}

fn toggle(v: &mut Viewer, s: Sheet) {
    v.sheet = if v.sheet == Some(s) { None } else { Some(s) };
}

fn chip(ui: &mut Ui, text: &str) {
    Frame::NONE.fill(ROW).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(8, 3)).show(ui, |ui| {
        ui.label(RichText::new(text).small().color(DIM));
    });
}

fn icon_button(ui: &mut Ui, icon: Icon, size: f32, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if selected || resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 8.0, ROW);
    }
    icons::paint(ui.painter(), rect.shrink(size * 0.2), icon, !ui.is_enabled());
    resp
}

/// A tool bar button: the icon over its name.
fn tool_button(ui: &mut Ui, icon: Icon, label: &str, selected: bool, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 58.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect.shrink2(vec2(6.0, 3.0)), 10.0, ROW);
    }
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 21.0), vec2(26.0, 26.0));
    icons::paint(p, icon_rect, icon, !ui.is_enabled());
    let color = if !ui.is_enabled() {
        DIM.gamma_multiply(0.5)
    } else if selected {
        ACCENT
    } else {
        DIM
    };
    p.text(pos2(rect.center().x, rect.bottom() - 12.0), Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), color);
    resp
}

/// What shows before a drawing is open.
pub fn start_card(v: &mut Viewer, ctx: &egui::Context) {
    let rect = v.rect;
    if !rect.is_positive() {
        return;
    }
    let width = (rect.width() - 32.0).clamp(200.0, 420.0);
    egui::Area::new(egui::Id::unique("viewer_start")).fixed_pos(pos2(rect.center().x - width / 2.0, rect.top() + rect.height() * 0.18)).show(
        ctx,
        |ui| {
            Frame::NONE.fill(PANEL).corner_radius(CornerRadius::same(16)).inner_margin(Margin::same(20)).show(ui, |ui| {
                ui.set_width(width - 40.0);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                    draftcraft_ui_egui::brand::paint(ui.painter(), r);
                    ui.add_space(6.0);
                    ui.label(RichText::new(v.w.app).heading().color(TEXT));
                    ui.add_space(4.0);
                    ui.label(RichText::new(v.w.open_hint).color(DIM));
                    ui.add_space(14.0);
                    let big = vec2(ui.available_width(), 48.0);
                    if ui.add_sized(big, egui::Button::new(RichText::new(v.w.open).strong()).fill(ACCENT.gamma_multiply(0.85))).clicked() {
                        v.pick_file();
                    }
                    if ui.add_sized(big, egui::Button::new(v.w.sample)).clicked() {
                        v.open_sample();
                    }
                    let full = egui::Button::new(RichText::new(v.w.full_app).color(Color32::BLACK)).fill(Color32::WHITE);
                    if ui.add_sized(big, full).clicked() {
                        v.open_full_app();
                    }
                });
            });
        },
    );
}

/// The measuring points, lines, area and reticle over the drawing.
pub fn measure_overlay(v: &Viewer, p: &egui::Painter) {
    if v.tool == Tool::View {
        return;
    }
    let rect = v.rect;
    let pts: Vec<Pos2> = v.points.iter().map(|q| v.view.to_screen(rect, *q)).collect();
    let stroke = Stroke::new(2.0, MEASURE);
    if v.tool == Tool::Area && pts.len() >= 3 {
        p.add(egui::Shape::convex_polygon(pts.clone(), MEASURE.gamma_multiply(0.18), Stroke::NONE));
        if let (Some(a), Some(b)) = (pts.last(), pts.first()) {
            p.extend(egui::Shape::dashed_line(&[*a, *b], stroke, 6.0, 4.0));
        }
    }
    if pts.len() >= 2 {
        p.add(egui::Shape::line(pts.clone(), stroke));
    }
    for q in &pts {
        p.circle_filled(*q, 5.0, MEASURE);
        p.circle_stroke(*q, 5.0, Stroke::new(1.5, Color32::BLACK));
    }
    // The running distance at the middle of the last segment.
    if v.tool == Tool::Distance
        && let ([a, b], [wa, wb]) = (pts.as_slice(), v.points.as_slice())
    {
        let text = v.length_text(measure::distance(*wa, *wb).length);
        label(p, pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0 - 18.0), &text);
    }
    if let Some(aim) = v.aim {
        let at = v.view.to_screen(rect, aim.world);
        // A line from the last point to where the reticle aims.
        if let Some(last) = pts.last()
            && !(v.tool == Tool::Distance && pts.len() >= 2)
        {
            p.extend(egui::Shape::dashed_line(&[*last, at], Stroke::new(1.5, MEASURE), 6.0, 4.0));
        }
        let c = Color32::WHITE;
        p.line_segment([pos2(at.x - 22.0, at.y), pos2(at.x - 8.0, at.y)], Stroke::new(1.5, c));
        p.line_segment([pos2(at.x + 8.0, at.y), pos2(at.x + 22.0, at.y)], Stroke::new(1.5, c));
        p.line_segment([pos2(at.x, at.y - 22.0), pos2(at.x, at.y - 8.0)], Stroke::new(1.5, c));
        p.line_segment([pos2(at.x, at.y + 8.0), pos2(at.x, at.y + RETICLE_LIFT - 30.0)], Stroke::new(1.0, c.gamma_multiply(0.5)));
        crate::scene::snap_marker(p, at, aim.snap, MEASURE);
        if let (Some(last), Tool::Distance, 1) = (v.points.last(), v.tool, v.points.len()) {
            label(p, pos2(at.x, at.y - 34.0), &v.length_text(measure::distance(*last, aim.world).length));
        }
    }
}

fn label(p: &egui::Painter, at: Pos2, text: &str) {
    let galley = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(14.0), Color32::BLACK);
    let r = Rect::from_center_size(at, galley.size() + vec2(12.0, 6.0));
    p.rect_filled(r, 6.0, MEASURE);
    p.galley(r.center() - galley.size() / 2.0, galley, Color32::BLACK);
}

/// The measuring card over the bottom of the drawing: the mode, the values and the actions.
pub fn measure_card(v: &mut Viewer, ctx: &egui::Context) {
    if v.tool == Tool::View || v.doc.is_none() {
        return;
    }
    let rect = v.rect;
    let width = (rect.width() - 16.0).max(200.0);
    egui::Area::new(egui::Id::unique("viewer_measure"))
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(ctx.content_rect().bottom() - rect.bottom()) - 8.0))
        .show(ctx, |ui| {
            Frame::NONE.fill(PANEL.gamma_multiply(0.97)).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(width - 24.0);
                ui.horizontal(|ui| {
                    for (tool, name) in [(Tool::Distance, v.w.distance), (Tool::Area, v.w.area)] {
                        if ui.selectable_label(v.tool == tool, RichText::new(name).strong()).clicked() && v.tool != tool {
                            v.tool = tool;
                            v.points.clear();
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(v.w.done).clicked() {
                            v.set_tool(Tool::View);
                        }
                        if !v.points.is_empty() {
                            if icon_button(ui, Icon::Erase, 40.0, false).on_hover_text(v.w.clear).clicked() {
                                v.points.clear();
                            }
                            if icon_button(ui, Icon::Undo, 40.0, false).on_hover_text(v.w.undo_point).clicked() {
                                v.points.pop();
                            }
                        }
                    });
                });
                ui.add_space(2.0);
                // The main value large, the others in one small line: the card stays low, so the
                // drawing being measured stays in view.
                let (main, rest): ((&str, String), Vec<(&str, String)>) = match (v.tool, v.points.as_slice()) {
                    (Tool::Distance, [a, b]) => {
                        let d = measure::distance(*a, *b);
                        (
                            (v.w.length, v.length_text(d.length)),
                            vec![("ΔX", v.length_text(d.dx)), ("ΔY", v.length_text(d.dy)), (v.w.angle, format!("{:.2}°", d.angle_deg))],
                        )
                    }
                    (Tool::Area, pts) if pts.len() >= 2 => (
                        (v.w.area, if pts.len() >= 3 { v.area_text(measure::area(pts)) } else { "—".into() }),
                        vec![(v.w.perimeter, v.length_text(measure::perimeter(pts, true))), (v.w.points, pts.len().to_string())],
                    ),
                    (_, []) => (("", v.w.tap_first.to_string()), Vec::new()),
                    _ => (("", v.w.tap_next.to_string()), Vec::new()),
                };
                ui.horizontal_wrapped(|ui| {
                    if !main.0.is_empty() {
                        ui.label(RichText::new(main.0).color(DIM));
                    }
                    ui.label(RichText::new(main.1).color(TEXT).strong().monospace().size(18.0));
                });
                if !rest.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (k, val) in rest {
                            ui.label(RichText::new(k).small().color(DIM));
                            ui.label(RichText::new(val).small().color(TEXT).monospace());
                            ui.add_space(6.0);
                        }
                    });
                }
                if v.points.is_empty() {
                    ui.label(RichText::new(v.w.drag_hint).small().color(DIM));
                }
            });
        });
}

/// The open sheet, slid up over the bottom of the drawing.
pub fn sheet(v: &mut Viewer, ctx: &egui::Context) {
    let Some(kind) = v.sheet else { return };
    if v.doc.is_none() {
        v.sheet = None;
        return;
    }
    let rect = v.rect;
    let height = (rect.height() * 0.66).max(160.0);
    let title = match kind {
        Sheet::Layers => v.w.layers,
        Sheet::Layouts => v.w.layouts,
        Sheet::FindText => v.w.find_text,
        Sheet::More => v.w.more,
    };
    egui::Area::new(egui::Id::unique("viewer_sheet")).fixed_pos(pos2(rect.left(), rect.bottom() - height)).show(ctx, |ui| {
        Frame::NONE
            .fill(PANEL)
            .corner_radius(CornerRadius { nw: 16, ne: 16, sw: 0, se: 0 })
            .inner_margin(Margin { left: 14, right: 14, top: 8, bottom: 10 })
            .shadow(egui::Shadow { offset: [0, -2], blur: 16, spread: 0, color: Color32::from_black_alpha(120) })
            .show(ui, |ui| {
                ui.set_width(rect.width() - 28.0);
                ui.set_height(height - 18.0);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 5.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, DIM.gamma_multiply(0.5));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).heading().color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::Close, 36.0, false).on_hover_text(v.w.close).clicked() {
                            v.sheet = None;
                        }
                    });
                });
                ui.add_space(4.0);
                match kind {
                    Sheet::Layers => layers(v, ui),
                    Sheet::Layouts => layouts(v, ui),
                    Sheet::FindText => find_text(v, ui),
                    Sheet::More => more(v, ui),
                }
            });
    });
}

fn layers(v: &mut Viewer, ui: &mut Ui) {
    let w = v.w;
    ui.add(egui::TextEdit::singleline(&mut v.layer_filter).hint_text(w.search).desired_width(f32::INFINITY));
    let Some(doc) = &mut v.doc else { return };
    ui.horizontal_wrapped(|ui| {
        if ui.button(w.all_on).clicked() {
            doc.set_all(true);
        }
        if ui.button(w.all_off).clicked() {
            doc.set_all(false);
        }
        if ui.button(w.reset).clicked() {
            doc.reset_layers();
        }
        ui.label(RichText::new(format!("{}/{}", doc.visible_count(), doc.drawing.layers.len())).color(DIM));
    });
    ui.add_space(4.0);
    let filter = v.layer_filter.trim().to_lowercase();
    let rows: Vec<usize> = (0..doc.drawing.layers.len())
        .filter(|i| doc.drawing.layers.get(*i).is_some_and(|l| filter.is_empty() || l.name.to_lowercase().contains(&filter)))
        .take(MAX_LAYER_ROWS)
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new(w.no_layers).color(DIM));
        return;
    }
    let row_h = 48.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
        for i in rows.get(range).unwrap_or_default().iter().copied() {
            let Some(layer) = doc.drawing.layers.get(i) else { continue };
            let visible = doc.layer_visible(i);
            let (name, color) = (layer.name.clone(), layer.color);
            let count = doc.counts.get(i).copied().unwrap_or(0);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h - 4.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 10.0, ROW);
            let eye = Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(24.0, 24.0));
            icons::paint(p, eye, if visible { Icon::Bulb } else { Icon::BulbOff }, !visible);
            let rgb = color.resolve(color, color);
            let swatch = Rect::from_center_size(pos2(rect.left() + 56.0, rect.center().y), vec2(16.0, 16.0));
            p.rect_filled(swatch, 4.0, Color32::from_rgb(rgb.0, rgb.1, rgb.2));
            p.rect_stroke(swatch, 4.0, Stroke::new(1.0, Color32::from_black_alpha(140)), egui::StrokeKind::Inside);
            let only = Rect::from_min_max(pos2(rect.right() - 64.0, rect.top() + 7.0), pos2(rect.right() - 8.0, rect.bottom() - 7.0));
            let text_right = only.left() - 8.0;
            let name_color = if visible { TEXT } else { DIM };
            let galley = p.layout(name, egui::FontId::proportional(15.0), name_color, (text_right - rect.left() - 76.0).max(20.0));
            let ty = rect.center().y - (galley.size().y + 14.0) / 2.0;
            p.galley(pos2(rect.left() + 76.0, ty), galley.clone(), name_color);
            p.text(
                pos2(rect.left() + 76.0, ty + galley.size().y + 1.0),
                Align2::LEFT_TOP,
                format!("{count} {}", w.objects),
                egui::FontId::proportional(11.0),
                DIM,
            );
            let only_resp = ui.interact(only, resp.id.with("only"), Sense::click());
            ui.painter().rect_stroke(only, 8.0, Stroke::new(1.0, DIM.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            ui.painter().text(only.center(), Align2::CENTER_CENTER, w.only, egui::FontId::proportional(13.0), TEXT);
            if only_resp.clicked() {
                doc.isolate(i);
            } else if resp.clicked() {
                doc.set_layer_visible(i, !visible);
            }
            ui.add_space(4.0);
        }
    });
}

fn layouts(v: &mut Viewer, ui: &mut Ui) {
    let Some(doc) = &v.doc else { return };
    let (spaces, current) = (doc.spaces(), doc.space.clone());
    let mut chosen = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for space in spaces {
            let name = match &space {
                Space::Model => v.w.model.to_string(),
                Space::Paper(n) => n.clone(),
            };
            let selected = space == current;
            let text = RichText::new(name).color(if selected { ACCENT } else { TEXT });
            if ui.add_sized(vec2(ui.available_width(), 44.0), egui::Button::new(text).selected(selected)).clicked() {
                chosen = Some(space);
            }
        }
    });
    if let Some(space) = chosen {
        v.set_space(space);
        v.sheet = None;
    }
}

fn find_text(v: &mut Viewer, ui: &mut Ui) {
    let resp = ui.add(egui::TextEdit::singleline(&mut v.text_query).hint_text(v.w.search).desired_width(f32::INFINITY));
    if resp.changed()
        && let Some(doc) = &v.doc
    {
        v.text_hits = doc.find_text(&v.text_query);
    }
    if v.text_query.trim().is_empty() {
        return;
    }
    if v.text_hits.is_empty() {
        ui.label(RichText::new(v.w.no_matches).color(DIM));
        return;
    }
    let mut go = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (i, (words, _)) in v.text_hits.iter().enumerate() {
            let b = egui::Button::new(RichText::new(words).color(TEXT)).wrap_mode(egui::TextWrapMode::Truncate);
            if ui.add_sized(vec2(ui.available_width(), 42.0), b).clicked() {
                go = Some(i);
            }
        }
    });
    if let Some(b) = go.and_then(|i| v.text_hits.get(i)).map(|(_, b)| *b) {
        v.zoom_to(&b);
        v.sheet = None;
    }
}

fn more(v: &mut Viewer, ui: &mut Ui) {
    let w = v.w;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(w.background).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.selectable_label(!v.dark, w.light).clicked() {
                    v.dark = false;
                }
                if ui.selectable_label(v.dark, w.dark).clicked() {
                    v.dark = true;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(w.lineweights).color(DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.selectable_label(v.lineweights, w.on).clicked() {
                    v.lineweights = true;
                }
                if ui.selectable_label(!v.lineweights, w.off).clicked() {
                    v.lineweights = false;
                }
            });
        });
        ui.separator();
        if let Some(doc) = &v.doc {
            ui.label(RichText::new(w.info).strong().color(TEXT));
            let h = &doc.drawing.header;
            let unit = measure::unit_suffix(h.i64("INSUNITS", 0));
            let b = doc.fit_bounds();
            let size = if b.is_empty() { "—".to_string() } else { format!("{} × {}", v.length_text(b.width()), v.length_text(b.height())) };
            egui::Grid::new("doc_info").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                for (k, val) in [
                    (w.file, doc.name.clone()),
                    (w.entities, doc.drawing.entity_count().to_string()),
                    (w.layers, doc.drawing.layers.len().to_string()),
                    (w.units, if unit.is_empty() { "—".to_string() } else { unit.to_string() }),
                    (w.size, size),
                ] {
                    ui.label(RichText::new(k).color(DIM));
                    ui.add(egui::Label::new(RichText::new(val).color(TEXT)).truncate());
                    ui.end_row();
                }
            });
            ui.separator();
        }
        if ui.add_sized(vec2(ui.available_width(), 44.0), egui::Button::new(w.full_app)).clicked() {
            v.open_full_app();
        }
        ui.label(RichText::new(w.full_app_hint).small().color(DIM));
    });
}

/// "Opening…" while a file is read, and the latest message.
pub fn overlays(v: &mut Viewer, ctx: &egui::Context, now: f64) {
    let rect = v.rect;
    if v.loading() {
        egui::Area::new(egui::Id::unique("viewer_loading")).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            Frame::NONE.fill(PANEL).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(18)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(v.w.loading).color(TEXT));
                });
            });
        });
    }
    if v.scene.list.as_ref().is_some_and(|l| l.truncated) && !v.told_truncated {
        v.told_truncated = true;
        let msg = v.w.truncated;
        v.say(msg, now);
    }
    let Some((msg, until)) = v.toast.clone() else { return };
    if now > until {
        v.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    let width = (rect.width() - 24.0).max(160.0);
    egui::Area::new(egui::Id::unique("viewer_toast")).fixed_pos(pos2(rect.left() + 12.0, rect.top() + 10.0)).interactable(true).show(ctx, |ui| {
        let r = Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(12, 14, 18, 235))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_max_width(width - 20.0);
                ui.label(RichText::new(msg).color(TEXT));
            });
        if r.response.interact(Sense::click()).clicked() {
            v.toast = None;
        }
    });
}
