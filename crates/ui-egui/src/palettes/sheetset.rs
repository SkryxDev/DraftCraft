//! The Sheet Set Manager palette (SHEETSET): make or open a sheet set file, add drawings' layouts
//! as sheets, renumber, retitle, reorder and remove them, open a sheet's drawing, and publish the
//! set to PDF. Everything goes through the engine's SHEETSET command.

use egui::vec2;
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The sheet set the palette shows.
#[derive(Default)]
pub struct State {
    /// The sheet set file.
    pub(crate) path: String,
    /// SHEETSET's description of it.
    pub(crate) set: Option<Value>,
    selected: Option<usize>,
    /// The selected sheet's number and title being edited.
    number: String,
    title: String,
    error: Option<String>,
}

impl State {
    fn run(&mut self, app: &mut CadApp, params: Value) -> bool {
        match app.run("sheetset", params) {
            Ok(v) => {
                if let Some(m) = v.get("message").and_then(Value::as_str) {
                    app.set_status(m.to_string());
                }
                if v.get("name").is_some() {
                    if let Some(p) = v.get("path").and_then(Value::as_str) {
                        self.path = p.to_string();
                    }
                    self.set = Some(v);
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

    fn sheets(&self) -> Vec<Value> {
        self.set.as_ref().and_then(|s| s.get("sheets")).and_then(Value::as_array).cloned().unwrap_or_default()
    }

    fn select(&mut self, i: Option<usize>) {
        self.selected = i;
        let sheet = i.and_then(|i| self.sheets().get(i).cloned());
        let field = |k: &str| sheet.as_ref().and_then(|s| s.get(k)).and_then(Value::as_str).unwrap_or_default().to_string();
        self.number = field("number");
        self.title = field("title");
    }
}

/// Open the palette (SHEETSET typed or chosen without parameters).
pub(crate) fn open(app: &mut CadApp) -> Result<Value, String> {
    crate::docking::open_on_demand(app, crate::docking::Panel::SheetSet, 320.0)
}

/// A sheet's drawing, against the sheet set file's folder.
fn sheet_file(set_path: &str, file: &str) -> String {
    let f = std::path::Path::new(file);
    if f.is_absolute() {
        return file.to_string();
    }
    match std::path::Path::new(set_path).parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(f).to_string_lossy().into_owned(),
        _ => file.to_string(),
    }
}

/// The palette's body.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let mut st = std::mem::take(&mut app.side_palettes.sheet_set);
    let mut todo: Option<Value> = None;
    let mut open_sheet: Option<(String, String)> = None;
    ui.add_space(6.0);
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if ui.button(crate::tl!("New Sheet Set...")).clicked()
            && let Some(p) = app.services.pick_save.as_ref().and_then(|pick| pick("Sheet Set.dcss"))
        {
            todo = Some(json!({ "operation": "new", "path": p, "overwrite": true }));
        }
        if ui.button(crate::tl!("Open Sheet Set...")).clicked()
            && let Some(p) = app.services.pick_file.as_ref().and_then(|pick| pick("Sheet sets (DCSS)", &["dcss"]))
        {
            todo = Some(json!({ "operation": "list", "path": p }));
        }
    });
    if app.services.pick_file.is_none() {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let r =
                ui.add(egui::TextEdit::singleline(&mut st.path).hint_text(crate::tl!("Sheet set file")).desired_width(ui.available_width() - 8.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !st.path.trim().is_empty() {
                todo = Some(json!({ "operation": "list", "path": st.path.trim() }));
            }
        });
    }
    if let Some(e) = &st.error {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.colored_label(t.warn, e);
        });
    }
    let Some(set) = st.set.clone() else {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(crate::tl!("Make or open a sheet set to list its sheets.")).color(t.text_dim));
        });
        app.side_palettes.sheet_set = st;
        if let Some(p) = todo {
            let mut st = std::mem::take(&mut app.side_palettes.sheet_set);
            st.run(app, p);
            app.side_palettes.sheet_set = st;
        }
        return;
    };
    let path = st.path.clone();
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.strong(set.get("name").and_then(Value::as_str).unwrap_or_default());
    });
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if ui.button(crate::tl!("Add Drawing...")).clicked()
            && let Some(f) = app.services.pick_file.as_ref().and_then(|pick| pick("Drawings (DWG, DXF)", &["dwg", "dxf"]))
        {
            todo = Some(json!({ "operation": "add", "path": path, "file": f }));
        }
        if ui.button(crate::tl!("Publish to PDF...")).clicked()
            && let Some(p) = app.services.pick_save.as_ref().and_then(|pick| pick("Sheets.pdf"))
        {
            let dir = std::path::Path::new(&p).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            todo = Some(json!({ "operation": "publish", "path": path, "output": dir }));
        }
    });
    ui.separator();
    let sheets = st.sheets();
    let mut select: Option<Option<usize>> = None;
    egui::ScrollArea::vertical().id_salt("sheetset-sheets").max_height((ui.available_height() - 90.0).max(80.0)).show(ui, |ui| {
        if sheets.is_empty() {
            ui.label(egui::RichText::new(crate::tl!("No sheets yet: add a drawing's layouts.")).color(t.text_dim));
        }
        for (i, sh) in sheets.iter().enumerate() {
            let f = |k: &str| sh.get(k).and_then(Value::as_str).unwrap_or_default();
            let r = ui.selectable_label(st.selected == Some(i), format!("{}  {}", f("number"), f("title"))).on_hover_text(format!(
                "{} · {}",
                f("file"),
                f("layout")
            ));
            if r.clicked() {
                select = Some(Some(i));
            }
            if r.double_clicked() {
                open_sheet = Some((sheet_file(&path, f("file")), f("layout").to_string()));
            }
        }
    });
    if let Some(s) = select {
        st.select(s);
    }
    if let Some(i) = st.selected.filter(|i| *i < sheets.len()) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.add(egui::TextEdit::singleline(&mut st.number).hint_text(crate::tl!("Number")).desired_width(60.0));
            ui.add(egui::TextEdit::singleline(&mut st.title).hint_text(crate::tl!("Title")).desired_width(ui.available_width() - 8.0));
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_space(6.0);
            if ui.button(crate::tl!("Apply")).clicked() {
                todo = Some(json!({ "operation": "update", "path": path, "index": i, "number": st.number, "title": st.title }));
            }
            if ui.add_enabled(i > 0, egui::Button::new("▲")).on_hover_text(crate::tl!("Move Up")).clicked() {
                todo = Some(json!({ "operation": "move", "path": path, "index": i, "to": i - 1 }));
                st.selected = Some(i - 1);
            }
            if ui.add_enabled(i + 1 < sheets.len(), egui::Button::new("▼")).on_hover_text(crate::tl!("Move Down")).clicked() {
                todo = Some(json!({ "operation": "move", "path": path, "index": i, "to": i + 1 }));
                st.selected = Some(i + 1);
            }
            if ui.button(crate::tl!("Open")).clicked()
                && let Some(sh) = sheets.get(i)
            {
                let f = |k: &str| sh.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                open_sheet = Some((sheet_file(&path, &f("file")), f("layout")));
            }
            if ui.button(crate::tl!("Remove")).clicked() {
                todo = Some(json!({ "operation": "remove", "path": path, "index": i }));
                st.selected = None;
            }
        });
    }
    if let Some(p) = todo {
        st.run(app, p);
    }
    app.side_palettes.sheet_set = st;
    if let Some((file, _layout)) = open_sheet {
        app.open_path(std::path::Path::new(&file));
    }
}
