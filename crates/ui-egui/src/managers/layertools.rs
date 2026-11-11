//! Layer Translator (LAYTRANS), Layer Walk (LAYWALK) and Geographic Location
//! (GEOGRAPHICLOCATION) dialogs. They read the drawing and change it only through those
//! commands, so every change is scriptable.

use std::collections::BTreeSet;

use egui::{RichText, vec2};
use serde_json::{Value, json};

use super::Action;
use crate::CadApp;
use crate::theme::Tokens;

fn temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str) -> Option<T> {
    ctx.data_mut(|d| d.get_temp::<T>(egui::Id::unique(id)))
}

fn set_temp<T: Clone + Send + Sync + 'static>(ctx: &egui::Context, id: &str, v: T) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(id), v));
}

fn layer_names(app: &CadApp) -> Vec<String> {
    app.session.doc().map(|d| d.layers.iter().map(|l| l.name.clone()).collect()).unwrap_or_default()
}

// ---------- Layer Translator ----------

/// The translator's state while the dialog is open.
#[derive(Clone, Default)]
struct Trans {
    from: BTreeSet<String>,
    to: String,
    standards: String,
    /// The standards drawing's layers.
    targets: Vec<String>,
    map: Vec<(String, String)>,
    force: bool,
    error: Option<String>,
}

pub(super) fn laytrans(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    const KEY: &str = "laytrans-state";
    let t = Tokens::get();
    let mut st: Trans = temp(ctx, KEY).unwrap_or_default();
    let names = layer_names(app);
    let mut action = None;
    let mut load: Option<String> = None;
    let mut close = false;
    egui::Window::new(crate::tl!("Layer Translator")).open(open).collapsible(false).resizable(true).default_width(520.0).show(ctx, |ui| {
        ui.columns(2, |cols| {
            cols[0].strong(crate::tl!("Translate From"));
            egui::ScrollArea::vertical().id_salt("laytrans-from").max_height(220.0).show(&mut cols[0], |ui| {
                for n in &names {
                    let on = st.from.contains(n);
                    if ui.selectable_label(on, n).clicked() {
                        if on {
                            st.from.remove(n);
                        } else {
                            st.from.insert(n.clone());
                        }
                    }
                }
            });
            cols[1].strong(crate::tl!("Translate To"));
            cols[1].horizontal(|ui| {
                if ui.button(crate::tl!("Load...")).clicked() {
                    load = app.services.pick_file.as_ref().and_then(|pick| pick("Drawings (DWG, DXF)", &["dwg", "dxf"]));
                }
                ui.label(RichText::new(st.standards.rsplit(['/', '\\']).next().unwrap_or_default()).color(t.text_dim));
            });
            egui::ScrollArea::vertical().id_salt("laytrans-to").max_height(170.0).show(&mut cols[1], |ui| {
                for n in &st.targets {
                    if ui.selectable_label(*n == st.to, n).clicked() {
                        st.to = n.clone();
                    }
                }
            });
            cols[1].add(egui::TextEdit::singleline(&mut st.to).hint_text(crate::tl!("New layer name")));
        });
        ui.horizontal(|ui| {
            let can = !st.from.is_empty() && !st.to.trim().is_empty();
            if ui.add_enabled(can, egui::Button::new(crate::tl!("Map"))).clicked() {
                let to = st.to.trim().to_string();
                for f in std::mem::take(&mut st.from) {
                    st.map.retain(|(a, _)| *a != f);
                    st.map.push((f, to.clone()));
                }
            }
            ui.checkbox(&mut st.force, crate::tl!("Force object color and linetype to ByLayer"));
        });
        ui.separator();
        ui.strong(crate::tl!("Layer Translation Mappings"));
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("laytrans-map").max_height(140.0).show(ui, |ui| {
            if st.map.is_empty() {
                ui.label(RichText::new(crate::tl!("No mappings yet: pick layers on the left, a name on the right, then Map.")).color(t.text_dim));
            }
            for (i, (f, to)) in st.map.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{f}  →  {to}"));
                    if ui.small_button("×").on_hover_text(crate::tl!("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            st.map.remove(i);
        }
        if let Some(e) = &st.error {
            ui.colored_label(t.warn, e);
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(!st.map.is_empty(), egui::Button::new(crate::tl!("Translate"))).clicked() {
                let map: serde_json::Map<String, Value> = st.map.iter().map(|(f, to)| (f.clone(), json!(to))).collect();
                let mut p = json!({ "map": map, "forceByLayer": st.force });
                if !st.standards.is_empty() {
                    p["standards"] = json!(st.standards);
                }
                action = Some(("laytrans", p));
                close = true;
            }
            if ui.button(crate::tl!("Cancel")).clicked() {
                close = true;
            }
        });
    });
    if let Some(path) = load {
        match app.run("adcenter", json!({ "path": path })) {
            Ok(v) => {
                st.targets = v["layers"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                st.standards = path;
                st.error = None;
            }
            Err(e) => st.error = Some(e),
        }
    }
    if close {
        *open = false;
    }
    if *open {
        set_temp(ctx, KEY, st);
    } else {
        ctx.data_mut(|d| d.remove::<Trans>(egui::Id::unique(KEY)));
    }
    action
}

// ---------- Layer Walk ----------

#[derive(Clone)]
struct Walk {
    shown: BTreeSet<String>,
    restore: bool,
}

pub(super) fn laywalk(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    const KEY: &str = "laywalk-state";
    let names = layer_names(app);
    let mut st: Walk = temp(ctx, KEY).unwrap_or_else(|| Walk { shown: BTreeSet::new(), restore: true });
    let mut action = None;
    let mut close = false;
    let t = Tokens::get();
    egui::Window::new(crate::tl!("Layer Walk")).open(open).collapsible(false).resizable(true).default_width(260.0).show(ctx, |ui| {
        ui.label(RichText::new(crate::tl!("Click layers to show only them; Ctrl or Shift adds to the choice.")).color(t.text_dim));
        egui::ScrollArea::vertical().id_salt("laywalk-list").max_height(320.0).show(ui, |ui| {
            for n in &names {
                let on = st.shown.contains(n);
                if ui.selectable_label(on, n).clicked() {
                    let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    if !multi {
                        st.shown.clear();
                    }
                    if on && multi {
                        st.shown.remove(n);
                    } else {
                        st.shown.insert(n.clone());
                    }
                    action = Some(("laywalk", json!({ "layers": st.shown.iter().collect::<Vec<_>>() })));
                }
            }
        });
        ui.add_space(vec2(0.0, 4.0).y);
        ui.checkbox(&mut st.restore, crate::tl!("Restore on exit"));
        if ui.button(crate::tl!("Close")).clicked() {
            close = true;
        }
    });
    if close {
        *open = false;
    }
    if *open {
        set_temp(ctx, KEY, st);
    } else {
        ctx.data_mut(|d| d.remove::<Walk>(egui::Id::unique(KEY)));
        // Closing ends the walk; it ran only if a layer was picked.
        if app.session.laywalk.is_some() {
            return Some(("laywalk", json!({ "operation": "end", "restore": st.restore })));
        }
    }
    action
}

// ---------- Geographic Location ----------

#[derive(Clone)]
struct Geo {
    latitude: String,
    longitude: String,
    north: String,
}

pub(super) fn geo(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) -> Action {
    const KEY: &str = "geo-state";
    let has = app.session.doc().is_ok_and(|d| d.header.get("LATITUDE").is_some() && d.header.get("LONGITUDE").is_some());
    let mut st: Geo = temp(ctx, KEY).unwrap_or_else(|| {
        let h = app.session.doc().map(|d| d.header.clone()).unwrap_or_default();
        let f = |k: &str| h.get(k).and_then(draftcraft_engine::doc::HVal::as_f64).map(|v| format!("{v:.6}")).unwrap_or_default();
        let north = h
            .get("NORTHDIRECTION")
            .and_then(draftcraft_engine::doc::HVal::as_f64)
            .map(|v| format!("{:.2}", v.to_degrees()))
            .unwrap_or_else(|| "0".into());
        Geo { latitude: f("LATITUDE"), longitude: f("LONGITUDE"), north }
    });
    let t = Tokens::get();
    let mut action = None;
    let mut close = false;
    egui::Window::new(crate::tl!("Geographic Location")).open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.label(RichText::new(crate::tl!("The drawing's origin is at this place (WGS 84 degrees).")).color(t.text_dim));
        egui::Grid::new("geo-grid").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
            for (label, value) in [("Latitude", &mut st.latitude), ("Longitude", &mut st.longitude), ("North direction", &mut st.north)] {
                let l = ui.label(crate::i18n::t(label));
                ui.add(egui::TextEdit::singleline(value).desired_width(140.0)).labelled_by(l.id);
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            let num = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
            let parsed = (num(&st.latitude), num(&st.longitude), num(&st.north));
            if ui.add_enabled(parsed.0.is_some() && parsed.1.is_some(), egui::Button::new(crate::tl!("OK"))).clicked()
                && let (Some(la), Some(lo)) = (parsed.0, parsed.1)
            {
                action = Some(("geographiclocation", json!({ "latitude": la, "longitude": lo, "north": parsed.2.unwrap_or(0.0) })));
                close = true;
            }
            if ui.add_enabled(has, egui::Button::new(crate::tl!("Remove Location"))).clicked() {
                action = Some(("georemove", json!({})));
                close = true;
            }
            if ui.button(crate::tl!("Cancel")).clicked() {
                close = true;
            }
        });
    });
    if close {
        *open = false;
    }
    if *open {
        set_temp(ctx, KEY, st);
    } else {
        ctx.data_mut(|d| d.remove::<Geo>(egui::Id::unique(KEY)));
    }
    action
}
