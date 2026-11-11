//! The DesignCenter palette (ADCENTER): pick a drawing file, look at its blocks, layers,
//! linetypes and styles, and add the chosen ones to the current drawing, insert a block, or make
//! a tool palette of the file's blocks. Everything goes through the engine's ADCENTER command.

use std::collections::BTreeSet;

use egui::vec2;
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The content kinds with their labels, in ADCENTER's `kind` names.
pub(crate) const KINDS: &[(&str, &str)] = &[
    ("blocks", "Blocks"),
    ("layers", "Layers"),
    ("linetypes", "Linetypes"),
    ("textStyles", "Text Styles"),
    ("dimStyles", "Dimension Styles"),
    ("tableStyles", "Table Styles"),
    ("mleaderStyles", "Multileader Styles"),
    ("mlineStyles", "Multiline Styles"),
];

/// Most drawings the folder list shows.
const MAX_FOLDER: usize = 500;

/// What the palette is looking at.
#[derive(Default)]
pub struct State {
    /// The drawing file typed or picked.
    pub(crate) path: String,
    /// ADCENTER's listing of the loaded file.
    pub(crate) listing: Option<Value>,
    pub(crate) kind: usize,
    pub(crate) selected: BTreeSet<String>,
    /// The drawings in the loaded file's folder.
    folder: Vec<String>,
    error: Option<String>,
}

impl State {
    /// List `path` with ADCENTER; on success it becomes the file shown.
    pub(crate) fn load(&mut self, app: &mut CadApp, path: &str) {
        match app.run("adcenter", json!({ "path": path })) {
            Ok(v) => {
                self.path = path.to_string();
                self.listing = Some(v);
                self.selected.clear();
                self.error = None;
                self.folder = folder_drawings(path);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn kind_id(&self) -> &'static str {
        KINDS.get(self.kind).map(|k| k.0).unwrap_or("blocks")
    }

    /// The names of the current kind in the listing.
    fn names(&self) -> Vec<(String, Option<u64>)> {
        let Some(l) = &self.listing else { return Vec::new() };
        let Some(items) = l.get(self.kind_id()).and_then(Value::as_array) else { return Vec::new() };
        items
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some((s.clone(), None)),
                Value::Object(o) => o.get("name").and_then(Value::as_str).map(|n| (n.to_string(), o.get("objects").and_then(Value::as_u64))),
                _ => None,
            })
            .collect()
    }

    /// Copy the selected names into the drawing. Returns whether the command ran.
    fn add(&mut self, app: &mut CadApp, names: Vec<String>) -> bool {
        let r = app.run("adcenter", json!({ "path": self.path, "kind": self.kind_id(), "names": names }));
        match r {
            Ok(v) => {
                if let Some(m) = v.get("message").and_then(Value::as_str) {
                    app.set_status(m.to_string());
                }
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }
}

/// The DWG and DXF files beside `path`, by name (desktop only).
fn folder_drawings(path: &str) -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(dir) = std::path::Path::new(path).parent() else { return Vec::new() };
        let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<String> = rd
            .flatten()
            .take(20 * MAX_FOLDER)
            .filter_map(|e| {
                let p = e.path();
                let ext = p.extension()?.to_string_lossy().to_ascii_lowercase();
                matches!(ext.as_str(), "dwg" | "dxf").then(|| p.to_string_lossy().into_owned())
            })
            .take(MAX_FOLDER)
            .collect();
        out.sort();
        out
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Vec::new()
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Open the palette (ADCENTER typed or chosen without parameters).
pub(crate) fn open(app: &mut CadApp) -> Result<Value, String> {
    crate::docking::open_on_demand(app, crate::docking::Panel::DesignCenter, 340.0)
}

/// The palette's body.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let mut st = std::mem::take(&mut app.side_palettes.design_center);
    let mut load: Option<String> = None;
    let mut insert: Option<String> = None;
    let mut add = false;
    let mut make_palette = false;
    ui.add_space(6.0);
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if ui.button(crate::tl!("Open...")).clicked() {
            match app.services.pick_file.as_ref() {
                Some(pick) => load = pick("Drawings (DWG, DXF)", &["dwg", "dxf"]),
                None => st.error = Some(crate::i18n::t("Type the drawing's path, then press Enter.").to_string()),
            }
        }
        let field = egui::TextEdit::singleline(&mut st.path).hint_text(crate::tl!("Drawing file")).desired_width(ui.available_width() - 8.0);
        let r = ui.add(field);
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !st.path.trim().is_empty() {
            load = Some(st.path.trim().to_string());
        }
    });
    if let Some(e) = &st.error {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.colored_label(t.warn, e);
        });
    }
    if st.folder.len() > 1 {
        egui::CollapsingHeader::new(crate::tl!("Folder")).id_salt("adc-folder").show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("adc-folder-list").max_height(120.0).show(ui, |ui| {
                for f in &st.folder {
                    if ui.selectable_label(*f == st.path, file_name(f)).clicked() {
                        load = Some(f.clone());
                    }
                }
            });
        });
    }
    if st.listing.is_some() {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let current = KINDS.get(st.kind).map(|k| k.1).unwrap_or("Blocks");
            craft_ui::ComboBox::from_id_salt("adc-kind").selected_text(crate::i18n::t(current)).show_ui(ui, |ui| {
                for (i, (_, label)) in KINDS.iter().enumerate() {
                    if ui.selectable_value(&mut st.kind, i, crate::i18n::t(label)).changed() {
                        st.selected.clear();
                    }
                }
            });
            ui.label(egui::RichText::new(file_name(&st.path)).color(t.text_dim));
        });
        let names = st.names();
        let blocks = st.kind_id() == "blocks";
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let some = !st.selected.is_empty();
            if ui.add_enabled(some, egui::Button::new(crate::tl!("Add to Drawing"))).clicked() {
                add = true;
            }
            if blocks {
                if ui.add_enabled(st.selected.len() == 1, egui::Button::new(crate::tl!("Insert"))).clicked() {
                    insert = st.selected.iter().next().cloned();
                }
                if ui.add_enabled(!names.is_empty(), egui::Button::new(crate::tl!("Create Tool Palette"))).clicked() {
                    make_palette = true;
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("adc-items").auto_shrink([false, false]).show(ui, |ui| {
            if names.is_empty() {
                ui.label(egui::RichText::new(crate::tl!("Nothing of this kind in the file.")).color(t.text_dim));
            }
            for (name, objects) in &names {
                let on = st.selected.contains(name);
                let text = match objects {
                    Some(n) => format!("{name}  ({n})"),
                    None => name.clone(),
                };
                let r = ui.selectable_label(on, text);
                if r.clicked() {
                    let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    if !multi {
                        st.selected.clear();
                    }
                    if on && multi {
                        st.selected.remove(name);
                    } else {
                        st.selected.insert(name.clone());
                    }
                }
                if blocks && r.double_clicked() {
                    insert = Some(name.clone());
                }
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(crate::tl!("Open a drawing to see its blocks, layers and styles.")).color(t.text_dim));
        });
    }
    if let Some(p) = load {
        st.load(app, &p);
    }
    if add {
        let names: Vec<String> = st.selected.iter().cloned().collect();
        st.add(app, names);
    }
    if let Some(name) = insert
        && st.add(app, vec![name.clone()])
    {
        // -INSERT asks for the insertion point at the command line.
        app.cmdline("-insert");
        if let Err(e) = app.session.input(draftcraft_engine::Input::Text(name)) {
            st.error = Some(e.to_string());
        }
    }
    if make_palette {
        let blocks: Vec<String> = st.names().into_iter().map(|(n, _)| n).collect();
        let name = file_name(&st.path).rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| file_name(&st.path).to_string());
        let r = app.run("ui.toolpalettes", json!({ "operation": "newPalette", "palette": name, "blocks": blocks, "file": st.path }));
        match r {
            Ok(_) => {
                let _ = super::toolpalettes::open(app);
            }
            Err(e) => st.error = Some(e),
        }
    }
    app.side_palettes.design_center = st;
}
