//! Workspaces (WORKSPACE, WSCURRENT, WSSAVE) and interface customization (CUI).
//!
//! A workspace is a named arrangement of the interface: the palettes and where they dock, which
//! bars show (ribbon, file tabs, status bar, command line), the Quick Access Toolbar's commands
//! and which ribbon tabs show. "2D Drafting" is the built-in one, DraftCraft's default layout;
//! the others are saved with the preferences. The Customize User Interface dialog (CUI) edits the
//! Quick Access Toolbar and the ribbon tabs and manages the workspaces.
//!
//! The JSON forms never open a dialog: `workspace {operation, name, to}`, `wscurrent {name}`,
//! `wssave {name}`, `ui.cui {qat, hiddenTabs}`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The built-in workspace: DraftCraft's default layout, which can't be renamed or deleted.
pub const BUILT_IN: &str = "2D Drafting";
/// Most saved workspaces, and most Quick Access Toolbar commands.
const MAX_WORKSPACES: usize = 50;
pub(crate) const MAX_QAT: usize = 40;
/// Longest workspace name kept.
const MAX_NAME: usize = 100;

/// A saved workspace.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedWorkspace {
    pub name: String,
    pub docking: crate::docking::Workspace,
    pub show_toolbar: bool,
    pub ribbon_minimized: bool,
    pub show_file_tabs: bool,
    pub show_status_bar: bool,
    pub show_command_line: bool,
    pub show_toolsets: bool,
    pub show_palettes: bool,
    /// The Quick Access Toolbar's commands; `None`: the standard ones.
    pub qat: Option<Vec<String>>,
    pub hidden_tabs: Vec<String>,
}

impl Default for SavedWorkspace {
    fn default() -> Self {
        let ui = crate::UiState::default();
        SavedWorkspace::capture(BUILT_IN, &ui)
    }
}

impl SavedWorkspace {
    fn capture(name: &str, ui: &crate::UiState) -> Self {
        SavedWorkspace {
            name: name.to_string(),
            docking: ui.docking.clone(),
            show_toolbar: ui.show_toolbar,
            ribbon_minimized: ui.ribbon_minimized,
            show_file_tabs: ui.show_file_tabs,
            show_status_bar: ui.show_status_bar,
            show_command_line: ui.show_command_line,
            show_toolsets: ui.show_toolsets,
            show_palettes: ui.show_palettes,
            qat: ui.qat.clone(),
            hidden_tabs: ui.hidden_tabs.clone(),
        }
    }

    fn apply(&self, ui: &mut crate::UiState) {
        // A saved layout that no longer validates gives way to the default one.
        ui.docking = if crate::docking::validate(&self.docking).is_ok() { self.docking.clone() } else { crate::docking::default_workspace() };
        ui.show_toolbar = self.show_toolbar;
        ui.ribbon_minimized = self.ribbon_minimized;
        ui.show_file_tabs = self.show_file_tabs;
        ui.show_status_bar = self.show_status_bar;
        ui.show_command_line = self.show_command_line;
        ui.show_toolsets = self.show_toolsets;
        ui.show_palettes = self.show_palettes;
        ui.qat = self.qat.clone();
        ui.hidden_tabs = self.hidden_tabs.clone();
    }
}

fn clip(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>().trim().to_string()
}

/// The workspaces' names, the built-in one first.
pub fn names(app: &CadApp) -> Vec<String> {
    std::iter::once(BUILT_IN.to_string()).chain(app.ui.workspaces.iter().map(|w| w.name.clone())).collect()
}

/// The current workspace's name.
pub fn current(app: &CadApp) -> String {
    if app.ui.current_workspace.is_empty() { BUILT_IN.to_string() } else { app.ui.current_workspace.clone() }
}

fn find(app: &CadApp, name: &str) -> Option<usize> {
    app.ui.workspaces.iter().position(|w| w.name.eq_ignore_ascii_case(name))
}

fn listing(app: &CadApp) -> Value {
    json!({ "workspaces": names(app), "current": current(app) })
}

/// Make `name` current: lay the interface out as it was saved.
pub fn set_current(app: &mut CadApp, name: &str) -> Result<Value, String> {
    if name.eq_ignore_ascii_case(BUILT_IN) {
        SavedWorkspace::default().apply(&mut app.ui);
        app.ui.current_workspace = BUILT_IN.into();
    } else {
        let i = find(app, name).ok_or_else(|| format!("no workspace \"{name}\""))?;
        let w = app.ui.workspaces.get(i).cloned().ok_or("no such workspace")?;
        w.apply(&mut app.ui);
        app.ui.current_workspace = w.name;
    }
    app.set_status(format!("{} {}", crate::i18n::t("Workspace:"), current(app)));
    Ok(listing(app))
}

/// Save the interface as it is now as workspace `name` (replacing one of that name) and make it
/// current.
pub fn save_as(app: &mut CadApp, name: &str) -> Result<Value, String> {
    let name = clip(name);
    if name.is_empty() {
        return Err("a workspace needs a name".into());
    }
    if name.eq_ignore_ascii_case(BUILT_IN) {
        return Err(format!("\"{BUILT_IN}\" is the built-in workspace: save under another name"));
    }
    let w = SavedWorkspace::capture(&name, &app.ui);
    match find(app, &name) {
        Some(i) => {
            if let Some(slot) = app.ui.workspaces.get_mut(i) {
                *slot = w;
            }
        }
        None if app.ui.workspaces.len() >= MAX_WORKSPACES => return Err(format!("at most {MAX_WORKSPACES} workspaces")),
        None => app.ui.workspaces.push(w),
    }
    app.ui.current_workspace = name;
    Ok(listing(app))
}

/// `workspace {operation: list|setCurrent|saveAs|rename|delete, name, to}` and its relatives.
pub fn command(app: &mut CadApp, id: &str, p: &Value) -> Result<Value, String> {
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(clip);
    let op = match id {
        "wscurrent" => {
            if s("name").is_none() {
                return Ok(listing(app));
            }
            "setcurrent".to_string()
        }
        "wssave" => "saveas".to_string(),
        _ => s("operation").unwrap_or_else(|| "list".into()).to_ascii_lowercase(),
    };
    let name = || s("name").filter(|n| !n.is_empty()).ok_or_else(|| "`name` is the workspace".to_string());
    match op.as_str() {
        "list" => Ok(listing(app)),
        "setcurrent" | "set" => set_current(app, &name()?),
        "saveas" | "save" => save_as(app, &name()?),
        "rename" => {
            let from = name()?;
            let to = s("to").filter(|n| !n.is_empty()).ok_or("`to` is the new name")?;
            if from.eq_ignore_ascii_case(BUILT_IN) || to.eq_ignore_ascii_case(BUILT_IN) {
                return Err(format!("\"{BUILT_IN}\" is the built-in workspace"));
            }
            if find(app, &to).is_some_and(|j| Some(j) != find(app, &from)) {
                return Err(format!("a workspace is already named \"{to}\""));
            }
            let i = find(app, &from).ok_or_else(|| format!("no workspace \"{from}\""))?;
            if let Some(w) = app.ui.workspaces.get_mut(i) {
                w.name = to.clone();
            }
            if app.ui.current_workspace.eq_ignore_ascii_case(&from) {
                app.ui.current_workspace = to;
            }
            Ok(listing(app))
        }
        "delete" => {
            let n = name()?;
            if n.eq_ignore_ascii_case(BUILT_IN) {
                return Err(format!("\"{BUILT_IN}\" is the built-in workspace"));
            }
            if n.eq_ignore_ascii_case(&current(app)) {
                return Err("the current workspace can't be deleted: make another current first".into());
            }
            let i = find(app, &n).ok_or_else(|| format!("no workspace \"{n}\""))?;
            app.ui.workspaces.remove(i);
            Ok(listing(app))
        }
        other => Err(format!("unknown operation \"{other}\" (list, setCurrent, saveAs, rename, delete)")),
    }
}

/// `ui.cui {qat?: [command ids] | null (the standard ones), hiddenTabs?: [tab names]}` → the
/// customization.
pub fn cui_command(app: &mut CadApp, p: &Value) -> Result<Value, String> {
    if let Some(q) = p.get("qat") {
        app.ui.qat = match q {
            Value::Null => None,
            Value::Array(a) => {
                let mut ids = Vec::new();
                for v in a.iter().take(MAX_QAT) {
                    let id = v.as_str().map(|x| x.trim().to_ascii_lowercase()).ok_or("`qat` lists command ids")?;
                    if draftcraft_engine::find_command(&id).is_none() && !crate::menus::is_ui_command(&id) {
                        return Err(format!("unknown command \"{id}\""));
                    }
                    ids.push(id);
                }
                Some(ids)
            }
            _ => return Err("`qat` lists command ids".into()),
        };
    }
    if let Some(h) = p.get("hiddenTabs") {
        let a = h.as_array().ok_or("`hiddenTabs` lists ribbon tab names")?;
        let tabs = crate::ribbon::tab_names();
        let mut hidden = Vec::new();
        for v in a {
            let n = v.as_str().ok_or("`hiddenTabs` lists ribbon tab names")?;
            let tab = tabs.iter().find(|t| t.eq_ignore_ascii_case(n)).ok_or_else(|| format!("no ribbon tab \"{n}\""))?;
            hidden.push(tab.to_string());
        }
        if hidden.len() >= tabs.len() {
            return Err("at least one ribbon tab must show".into());
        }
        app.ui.hidden_tabs = hidden;
    }
    Ok(json!({ "qat": crate::ribbon::qat_ids(app), "hiddenTabs": app.ui.hidden_tabs, "custom": app.ui.qat.is_some() }))
}

/// Route WORKSPACE, WSCURRENT, WSSAVE, WSSETTINGS, CUI and `ui.cui`: typed or chosen without
/// parameters, they open the Customize User Interface dialog (WSSAVE asks for a name there).
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    match id {
        "ui.cui" => Some(cui_command(app, params)),
        "cui" | "workspace" | "-workspace" | "wssave" | "wssettings" if params.is_null() => {
            app.ui.dialog = Some("cui".into());
            Some(Ok(Value::Null))
        }
        "wscurrent" if params.is_null() => {
            let r = listing(app);
            app.session.echo(format!("WSCURRENT = \"{}\"", current(app)));
            Some(Ok(r))
        }
        "workspace" | "-workspace" | "wscurrent" | "wssave" => Some(command(app, id.trim_start_matches('-'), params)),
        _ => None,
    }
}

/// The dialog's state while it is open.
#[derive(Clone, Default)]
struct Cui {
    selected: Option<String>,
    new_name: String,
    add_command: String,
    error: Option<String>,
}

/// The Customize User Interface dialog: workspaces, the Quick Access Toolbar and the ribbon tabs.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let key = egui::Id::unique("cui-state");
    let mut st: Cui = ctx.data_mut(|d| d.get_temp(key)).unwrap_or_default();
    let t = Tokens::get();
    let mut todo: Vec<(&str, Value)> = Vec::new();
    egui::Window::new(crate::tl!("Customize User Interface")).open(open).collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.columns(3, |cols| {
            // Workspaces.
            cols[0].strong(crate::tl!("Workspaces"));
            let cur = current(app);
            for n in names(app) {
                let label = if n == cur { format!("{n}  ✓") } else { n.clone() };
                if cols[0].selectable_label(st.selected.as_deref() == Some(&n), label).clicked() {
                    st.selected = Some(n.clone());
                }
            }
            cols[0].add_space(4.0);
            let sel = st.selected.clone();
            cols[0].horizontal_wrapped(|ui| {
                if ui.add_enabled(sel.is_some(), egui::Button::new(crate::tl!("Set Current"))).clicked()
                    && let Some(n) = &sel
                {
                    todo.push(("workspace", json!({ "operation": "setCurrent", "name": n })));
                }
                let deletable = sel.as_deref().is_some_and(|n| n != BUILT_IN && n != cur);
                if ui.add_enabled(deletable, egui::Button::new(crate::tl!("Delete"))).clicked()
                    && let Some(n) = &sel
                {
                    todo.push(("workspace", json!({ "operation": "delete", "name": n })));
                    st.selected = None;
                }
            });
            cols[0].add(egui::TextEdit::singleline(&mut st.new_name).hint_text(crate::tl!("Workspace name")));
            cols[0].horizontal_wrapped(|ui| {
                let named = !st.new_name.trim().is_empty();
                if ui.add_enabled(named, egui::Button::new(crate::tl!("Save Current As"))).clicked() {
                    todo.push(("workspace", json!({ "operation": "saveAs", "name": st.new_name.trim() })));
                }
                let renamable = named && sel.as_deref().is_some_and(|n| n != BUILT_IN);
                if ui.add_enabled(renamable, egui::Button::new(crate::tl!("Rename"))).clicked()
                    && let Some(n) = &sel
                {
                    todo.push(("workspace", json!({ "operation": "rename", "name": n, "to": st.new_name.trim() })));
                    st.selected = Some(st.new_name.trim().to_string());
                }
            });

            // Quick Access Toolbar.
            cols[1].strong(crate::tl!("Quick Access Toolbar"));
            let qat = crate::ribbon::qat_ids(app);
            let mut changed: Option<Vec<String>> = None;
            for (i, id) in qat.iter().enumerate() {
                cols[1].horizontal(|ui| {
                    let label = draftcraft_engine::find_command(id).map(|c| crate::i18n::t(c.label).to_string()).unwrap_or_else(|| id.clone());
                    ui.label(label);
                    if ui.small_button("▲").on_hover_text(crate::tl!("Move Up")).clicked() && i > 0 {
                        let mut q = qat.clone();
                        q.swap(i, i - 1);
                        changed = Some(q);
                    }
                    if ui.small_button("×").on_hover_text(crate::tl!("Remove")).clicked() {
                        let mut q = qat.clone();
                        q.remove(i);
                        changed = Some(q);
                    }
                });
            }
            cols[1].horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut st.add_command).hint_text(crate::tl!("Command")).desired_width(90.0));
                if ui.button(crate::tl!("Add")).clicked() && !st.add_command.trim().is_empty() {
                    let mut q = qat.clone();
                    q.push(st.add_command.trim().to_ascii_lowercase());
                    changed = Some(q);
                }
            });
            if cols[1].button(crate::tl!("Reset to Standard")).clicked() {
                todo.push(("ui.cui", json!({ "qat": null })));
            }
            if let Some(q) = changed {
                todo.push(("ui.cui", json!({ "qat": q })));
            }

            // Ribbon tabs.
            cols[2].strong(crate::tl!("Ribbon Tabs"));
            let hidden = app.ui.hidden_tabs.clone();
            for tab in crate::ribbon::tab_names() {
                let mut shown = !hidden.iter().any(|h| h == tab);
                if cols[2].checkbox(&mut shown, crate::i18n::t(tab)).changed() {
                    let mut h: Vec<String> = hidden.iter().filter(|x| *x != tab).cloned().collect();
                    if !shown {
                        h.push(tab.to_string());
                    }
                    todo.push(("ui.cui", json!({ "hiddenTabs": h })));
                }
            }
            cols[2].add_space(6.0);
            if cols[2].button(crate::tl!("Edit Aliases...")).clicked() {
                todo.push(("aliasedit", Value::Null));
            }
        });
        if let Some(e) = &st.error {
            ui.colored_label(t.warn, e);
        }
        ui.label(egui::RichText::new(crate::tl!("Changes apply at once; Save Current As keeps them in a workspace.")).color(t.text_dim));
    });
    for (cmd, p) in todo {
        let r = if p.is_null() {
            app.start(cmd);
            Ok(Value::Null)
        } else {
            app.run(cmd, p)
        };
        st.error = r.err();
        if cmd == "workspace" && st.error.is_none() {
            st.new_name.clear();
        }
        if cmd == "ui.cui" && st.error.is_none() {
            st.add_command.clear();
        }
    }
    if *open {
        ctx.data_mut(|d| d.insert_temp(key, st));
    } else {
        ctx.data_mut(|d| d.remove::<Cui>(key));
    }
}
