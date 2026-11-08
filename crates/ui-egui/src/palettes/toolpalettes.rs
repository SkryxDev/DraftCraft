//! The Tool Palettes window (TOOLPALETTES): named palettes of tools, each a command to start or a
//! block to insert (from this drawing or from a drawing file). The palettes are kept with the
//! preferences. `ui.toolpalettes` manages them programmatically; DesignCenter's Create Tool
//! Palette makes one of a file's blocks.

use egui::vec2;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// Most palettes, and most tools on one palette.
const MAX_PALETTES: usize = 100;
const MAX_TOOLS: usize = 500;
/// Longest palette name or tool label kept.
const MAX_LABEL: usize = 200;

/// One tool on a palette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PaletteTool {
    /// Start a command, as the ribbon does.
    Command { command: String, label: String },
    /// Insert a block; one from a drawing file is added to the drawing first (ADCENTER).
    Block {
        name: String,
        #[serde(default)]
        file: Option<String>,
    },
}

impl PaletteTool {
    fn label(&self) -> String {
        match self {
            Self::Command { label, .. } => crate::i18n::t(label).to_string(),
            Self::Block { name, .. } => name.clone(),
        }
    }
    fn icon(&self) -> Icon {
        match self {
            Self::Command { command, .. } => crate::ribbon::icon_for(command).unwrap_or(Icon::Menu),
            Self::Block { .. } => Icon::Blocks,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub tools: Vec<PaletteTool>,
}

/// The palettes and which one shows.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub palettes: Vec<Palette>,
    pub active: usize,
    /// The command typed in the Add Command field.
    adding: String,
}

fn cmd(command: &str, label: &str) -> PaletteTool {
    PaletteTool::Command { command: command.into(), label: label.into() }
}

impl Default for State {
    /// Draw, Modify and Annotate palettes of common commands.
    fn default() -> Self {
        let palettes = vec![
            Palette {
                name: "Draw".into(),
                tools: vec![
                    cmd("line", "Line"),
                    cmd("pline", "Polyline"),
                    cmd("circle", "Circle"),
                    cmd("arc", "Arc"),
                    cmd("rectang", "Rectangle"),
                    cmd("hatch", "Hatch"),
                ],
            },
            Palette {
                name: "Modify".into(),
                tools: vec![
                    cmd("move", "Move"),
                    cmd("copy", "Copy"),
                    cmd("rotate", "Rotate"),
                    cmd("scale", "Scale"),
                    cmd("trim", "Trim"),
                    cmd("offset", "Offset"),
                ],
            },
            Palette {
                name: "Annotate".into(),
                tools: vec![cmd("dimlinear", "Linear"), cmd("mtext", "Multiline Text"), cmd("mleader", "Multileader"), cmd("table", "Table")],
            },
        ];
        State { palettes, active: 0, adding: String::new() }
    }
}

fn clip(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_LABEL).collect()
}

impl State {
    /// The palettes for the preferences file.
    pub(crate) fn prefs(&self) -> Value {
        json!({ "palettes": self.palettes, "active": self.active })
    }

    /// Restore saved palettes; malformed ones keep the current palettes.
    pub(crate) fn load_prefs(&mut self, v: &Value) {
        let Some(p) = v.get("palettes").and_then(|p| serde_json::from_value::<Vec<Palette>>(p.clone()).ok()) else { return };
        let mut palettes: Vec<Palette> = p.into_iter().take(MAX_PALETTES).collect();
        for pal in &mut palettes {
            pal.name = clip(&pal.name);
            pal.tools.truncate(MAX_TOOLS);
        }
        self.palettes = palettes;
        self.active =
            v.get("active").and_then(Value::as_u64).and_then(|a| usize::try_from(a).ok()).unwrap_or(0).min(self.palettes.len().saturating_sub(1));
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.palettes.iter().position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// `ui.toolpalettes`: `{operation: list|newPalette|deletePalette|rename|add|remove|activate, palette, to?, command?, label?, block?,
    /// blocks?, file?, index?}`.
    pub(crate) fn command(&mut self, p: &Value) -> Result<Value, String> {
        let s = |k: &str| p.get(k).and_then(Value::as_str).map(clip);
        let op = s("operation").unwrap_or_else(|| "list".into());
        let palette = |st: &Self| -> Result<usize, String> {
            match s("palette") {
                Some(n) => st.find(&n).ok_or_else(|| format!("no tool palette \"{n}\"")),
                None => (st.active < st.palettes.len()).then_some(st.active).ok_or_else(|| "there are no tool palettes".to_string()),
            }
        };
        match op.as_str() {
            "list" => {}
            "newPalette" => {
                let name = s("palette").filter(|n| !n.trim().is_empty()).ok_or("`palette` names the new palette")?;
                if self.palettes.len() >= MAX_PALETTES {
                    return Err(format!("at most {MAX_PALETTES} tool palettes"));
                }
                // A taken name gets a number.
                let mut unique = name.clone();
                let mut i = 2;
                while self.find(&unique).is_some() && i < 10_000 {
                    unique = format!("{name} ({i})");
                    i += 1;
                }
                let file = s("file").filter(|f| !f.is_empty());
                let tools: Vec<PaletteTool> = p
                    .get("blocks")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter().filter_map(Value::as_str).take(MAX_TOOLS).map(|n| PaletteTool::Block { name: clip(n), file: file.clone() }).collect()
                    })
                    .unwrap_or_default();
                self.palettes.push(Palette { name: unique, tools });
                self.active = self.palettes.len() - 1;
            }
            "deletePalette" => {
                let i = palette(self)?;
                self.palettes.remove(i);
                self.active = self.active.min(self.palettes.len().saturating_sub(1));
            }
            "rename" => {
                let i = palette(self)?;
                let to = s("to").filter(|n| !n.trim().is_empty()).ok_or("`to` is the new name")?;
                if self.find(&to).is_some_and(|j| j != i) {
                    return Err(format!("a tool palette is already named \"{to}\""));
                }
                if let Some(pal) = self.palettes.get_mut(i) {
                    pal.name = to;
                }
            }
            "activate" => self.active = palette(self)?,
            "add" => {
                let i = palette(self)?;
                let tool = match (s("command"), s("block")) {
                    (Some(c), _) => {
                        let c = c.trim().to_ascii_lowercase();
                        let label = match draftcraft_engine::find_command(&c) {
                            Some(spec) => spec.label.to_string(),
                            None if crate::menus::is_ui_command(&c) => c.clone(),
                            None => return Err(format!("unknown command \"{c}\"")),
                        };
                        PaletteTool::Command { label: s("label").unwrap_or(label), command: c }
                    }
                    (None, Some(b)) => PaletteTool::Block { name: b, file: s("file").filter(|f| !f.is_empty()) },
                    (None, None) => return Err("`command` or `block` is the tool to add".into()),
                };
                let pal = self.palettes.get_mut(i).ok_or("no such palette")?;
                if pal.tools.len() >= MAX_TOOLS {
                    return Err(format!("at most {MAX_TOOLS} tools on a palette"));
                }
                pal.tools.push(tool);
            }
            "remove" => {
                let i = palette(self)?;
                let pal = self.palettes.get_mut(i).ok_or("no such palette")?;
                let at = p
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|x| usize::try_from(x).ok())
                    .filter(|x| *x < pal.tools.len())
                    .ok_or("`index` is the tool's position, from 0")?;
                pal.tools.remove(at);
            }
            other => return Err(format!("unknown operation \"{other}\" (list, newPalette, deletePalette, rename, activate, add, remove)")),
        }
        Ok(json!({ "palettes": self.palettes, "active": self.active }))
    }
}

/// Open the palette window (TOOLPALETTES).
pub(crate) fn open(app: &mut CadApp) -> Result<Value, String> {
    crate::docking::open_on_demand(app, crate::docking::Panel::ToolPalettes, 240.0)
}

/// Run a tool: start its command, or insert its block (copied from its file first).
fn use_tool(app: &mut CadApp, tool: &PaletteTool) {
    match tool {
        PaletteTool::Command { command, .. } => app.start(command),
        PaletteTool::Block { name, file } => {
            if let Some(f) = file
                && app.session.doc().is_ok_and(|d| d.block(name).is_none())
                && let Err(e) = app.run("adcenter", json!({ "path": f, "kind": "blocks", "names": [name] }))
            {
                app.set_status(e);
                return;
            }
            app.cmdline("-insert");
            if let Err(e) = app.session.input(draftcraft_engine::Input::Text(name.clone())) {
                app.set_status(e.to_string());
            }
        }
    }
}

/// The window's body: the palette tabs down the side, the active palette's tools.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let mut st = std::mem::replace(&mut app.side_palettes.tool_palettes, State { palettes: Vec::new(), active: 0, adding: String::new() });
    let mut run: Option<PaletteTool> = None;
    let mut action: Option<Value> = None;
    ui.add_space(4.0);
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    // The palette tabs.
    ui.horizontal_wrapped(|ui| {
        ui.add_space(4.0);
        for (i, pal) in st.palettes.iter().enumerate() {
            let r = ui.selectable_label(i == st.active, crate::i18n::t(&pal.name));
            if r.clicked() {
                st.active = i;
            }
            r.context_menu(|ui| {
                if ui.button(crate::tl!("Delete Palette")).clicked() {
                    action = Some(json!({ "operation": "deletePalette", "palette": pal.name }));
                    ui.close();
                }
            });
        }
        if icons::button(ui, Icon::Plus, 20.0, crate::tl!("New Palette"), false).clicked() {
            action = Some(json!({ "operation": "newPalette", "palette": crate::i18n::t("New Palette") }));
        }
    });
    ui.separator();
    let active = st.active;
    egui::ScrollArea::vertical().id_salt("tool-palette").auto_shrink([false, false]).show(ui, |ui| {
        let Some(pal) = st.palettes.get(active) else {
            ui.label(egui::RichText::new(crate::tl!("No tool palettes.")).color(t.text_dim));
            return;
        };
        if pal.tools.is_empty() {
            ui.label(
                egui::RichText::new(crate::tl!("This palette is empty: add a command below, or make a palette from DesignCenter.")).color(t.text_dim),
            );
        }
        for (i, tool) in pal.tools.iter().enumerate() {
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(4.0);
                    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                    icons::paint(ui.painter(), rect, tool.icon(), false);
                    ui.add(egui::Button::new(tool.label()).frame(false))
                })
                .inner;
            if r.clicked() {
                run = Some(tool.clone());
            }
            r.context_menu(|ui| {
                if ui.button(crate::tl!("Delete Tool")).clicked() {
                    action = Some(json!({ "operation": "remove", "palette": pal.name, "index": i }));
                    ui.close();
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let field = egui::TextEdit::singleline(&mut st.adding).hint_text(crate::tl!("Command")).desired_width(110.0);
            let r = ui.add(field);
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(crate::tl!("Add Command")).clicked() || enter) && !st.adding.trim().is_empty() {
                action = Some(json!({ "operation": "add", "palette": pal.name, "command": st.adding.trim() }));
            }
        });
    });
    if let Some(a) = action {
        match st.command(&a) {
            Ok(_) => st.adding.clear(),
            Err(e) => app.set_status(e),
        }
    }
    app.side_palettes.tool_palettes = st;
    if let Some(tool) = run {
        use_tool(app, &tool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_are_managed_and_kept() {
        let mut st = State::default();
        st.command(&json!({"operation": "newPalette", "palette": "Parts", "blocks": ["BOLT", "NUT"], "file": "/p/parts.dwg"})).unwrap();
        assert_eq!(st.palettes[st.active].name, "Parts");
        assert_eq!(st.palettes[st.active].tools.len(), 2);
        st.command(&json!({"operation": "newPalette", "palette": "parts"})).unwrap();
        assert_eq!(st.palettes[st.active].name, "parts (2)", "a taken name gets a number");
        st.command(&json!({"operation": "add", "command": "LINE"})).unwrap();
        assert!(st.command(&json!({"operation": "add", "command": "nosuchcommand"})).is_err());
        st.command(&json!({"operation": "rename", "to": "Mine"})).unwrap();
        st.command(&json!({"operation": "remove", "palette": "Mine", "index": 0})).unwrap();
        assert!(st.command(&json!({"operation": "remove", "palette": "Mine", "index": 0})).is_err());
        let mut back = State::default();
        back.load_prefs(&st.prefs());
        assert_eq!(back.palettes, st.palettes);
        assert_eq!(back.active, st.active);
        // Malformed preferences keep what there is.
        back.load_prefs(&json!({"palettes": 5}));
        assert_eq!(back.palettes, st.palettes);
    }
}
