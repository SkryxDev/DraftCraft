//! BPARAMETER and BACTION: make the block open in the Block Editor dynamic. A parameter is a
//! property a reference to the block can change (a distance, a position); an action says what a
//! change does to the block's objects (stretch them, move them). BSAVE saves them with the block;
//! its references then get grips and custom properties that drive the actions. The objects are
//! made by [`draftcraft_doc::dynblock::author`].
//!
//! Points are given in the Block Editor's coordinates, where the block's base point is the origin.

use draftcraft_doc::dynblock::author::{self, NewAction, NewParameter};
use draftcraft_doc::dynblock::{ParameterKind, ValueSet, dynamic_block};
use draftcraft_doc::{Drawing, Handle};
use draftcraft_geom::Vec2;
use serde_json::{Value, json};

use crate::blockedit::CURRENT_DRAWING;
use crate::cmd::machines::{SelOutcome, SelectPhase, number};
use crate::cmd::{CommandSpec, bad, bool_or, point_value, str_param, targets};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("bparameter", "Parameter", run_bparameter)
            .alias(&["bparam"])
            .params(
                "{type?: linear|point (none: list the block's parameters and actions), base?: [x,y], end?: [x,y] (linear), at?: [x,y] \
                 (point), label? (default Distance1, Position1…), description?, min?, max?, increment?, list?: [numbers] (a linear \
                 parameter's allowed values), fromMidpoint?: bool, remove?: label or graph id (removes a parameter with its actions, or an \
                 action)} → {block, node, handle, label} (list: {block, parameters: [{node, label, type}], actions: [{node, name, type, \
                 parameter}]})",
            )
            .enabled(editing_a_block)
            .interactive(|_| Ok(Box::new(ParamM::default()))),
        CommandSpec::new("baction", "Action", run_baction)
            .alias(&["bac"])
            .params(
                "{type: stretch|move, parameter: label or graph id, handles? (the block's objects it changes; default the selection), \
                 frame?: [[x,y],[x,y]] or polygon (stretch: the points inside it move)} → {block, node, handle, name}",
            )
            .enabled(editing_a_block)
            .interactive(|_| Ok(Box::new(ActionM::default()))),
    ]
}

/// The Block Editor is open on a block (parameters belong to a block, not to the drawing itself).
fn editing_a_block(s: &Session) -> std::result::Result<(), String> {
    match s.state() {
        Ok(st) => match st.block_editor() {
            Some(n) if n.eq_ignore_ascii_case(CURRENT_DRAWING) => {
                Err("Parameters and actions belong to a block: open one in the Block Editor.".into())
            }
            Some(_) => Ok(()),
            None => Err("The Block Editor is not open.".into()),
        },
        Err(e) => Err(e.to_string()),
    }
}

/// The block being edited and its base point (the editor's origin).
fn block(s: &Session, id: &str) -> Result<(String, Vec2)> {
    editing_a_block(s).map_err(|e| bad(id, e))?;
    let st = s.state()?;
    let name = st.block_editor().unwrap_or_default().to_string();
    let base = st.doc.block(&name).map(|b| b.base.xy()).unwrap_or_default();
    Ok((name, base))
}

/// A block the Block Editor started and never saved has no definition yet: give the editor's
/// drawing an empty one to hang the parameters on (BSAVE fills it).
fn ensure_block(s: &mut Session, name: &str) -> Result<()> {
    if s.doc()?.block(name).is_none() {
        let d = s.doc_mut()?;
        d.blocks.insert(name.to_string(), std::sync::Arc::new(draftcraft_doc::Block::new(name)));
    }
    Ok(())
}

fn type_name(k: &ParameterKind) -> &'static str {
    match k {
        ParameterKind::Linear { .. } => "linear",
        ParameterKind::Polar { .. } => "polar",
        ParameterKind::Xy { .. } => "xy",
        ParameterKind::Rotation { .. } => "rotation",
        ParameterKind::Alignment { .. } => "alignment",
        ParameterKind::Flip { .. } => "flip",
        ParameterKind::Point { .. } => "point",
        ParameterKind::Visibility { .. } => "visibility",
        ParameterKind::Lookup { .. } => "lookup",
        ParameterKind::BasePoint { .. } => "basepoint",
        ParameterKind::Other { .. } => "other",
    }
}

/// The parameter a JSON value names: its label (any case) or its graph id.
fn parameter_node(d: &Drawing, name: &str, v: &Value) -> Option<i32> {
    let m = dynamic_block(d, name)?;
    if let Some(n) = v.as_i64().and_then(|n| i32::try_from(n).ok()) {
        return m
            .parameters
            .iter()
            .chain([])
            .find(|p| p.node == n)
            .map(|p| p.node)
            .or_else(|| m.actions.iter().find(|a| a.node == n).map(|a| a.node));
    }
    let t = v.as_str()?.trim();
    m.parameters
        .iter()
        .find(|p| p.kind.label().is_some_and(|l| l.eq_ignore_ascii_case(t)))
        .map(|p| p.node)
        .or_else(|| m.actions.iter().find(|a| a.name.eq_ignore_ascii_case(t)).map(|a| a.node))
}

fn list(d: &Drawing, name: &str) -> Value {
    let m = dynamic_block(d, name).unwrap_or_default();
    let label = |n: i32| m.parameters.iter().find(|p| p.node == n).and_then(|p| p.kind.label()).unwrap_or_default().to_string();
    json!({
        "block": name,
        "parameters": m.parameters.iter().map(|p| json!({ "node": p.node, "label": p.kind.label().unwrap_or(&p.element), "type": type_name(&p.kind) })).collect::<Vec<_>>(),
        "actions": m.actions.iter().map(|a| json!({
            "node": a.node,
            "name": a.name,
            "type": format!("{:?}", a.kind).split([' ', '{']).next().unwrap_or("").to_ascii_lowercase(),
            "parameter": a.connections.first().map(|(n, _)| label(*n)),
        })).collect::<Vec<_>>(),
    })
}

fn num(p: &Value, key: &str) -> Option<f64> {
    p.get(key).and_then(Value::as_f64).filter(|v| v.is_finite())
}

fn run_bparameter(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "bparameter";
    let (name, base) = block(s, ID)?;
    if let Some(r) = p.get("remove") {
        let node = parameter_node(s.doc()?, &name, r).ok_or_else(|| bad(ID, "no such parameter or action"))?;
        let gone = author::remove(s.doc_mut()?, &name, node).map_err(|e| bad(ID, e))?;
        return Ok(json!({ "block": name, "removed": gone }));
    }
    let Some(kind) = str_param(p, "type").map(str::to_ascii_lowercase) else { return Ok(list(s.doc()?, &name)) };
    let pt = |key: &str| -> Result<Vec2> {
        p.get(key).and_then(point_value).map(|q| q + base).ok_or_else(|| bad(ID, format!("`{key}` must be a point [x, y]")))
    };
    let label = str_param(p, "label").unwrap_or_default().to_string();
    let description = str_param(p, "description").unwrap_or_default().to_string();
    let new = match kind.as_str() {
        "linear" | "l" => {
            let mut values = ValueSet::default();
            if let Some(v) = num(p, "min") {
                values.flags |= 1;
                values.minimum = v;
            }
            if let Some(v) = num(p, "max") {
                values.flags |= 2;
                values.maximum = v;
            }
            if let Some(v) = num(p, "increment").filter(|v| *v > 0.0) {
                values.flags |= 4;
                values.increment = v;
            }
            if let Some(l) = p.get("list").and_then(Value::as_array) {
                values.list = l.iter().filter_map(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0).take(author::MAX_SELECTION).collect();
                if !values.list.is_empty() {
                    values.flags |= 8;
                }
            }
            if values.has_minimum() && values.has_maximum() && values.minimum > values.maximum {
                return Err(bad(ID, "`min` is more than `max`"));
            }
            NewParameter::Linear { base: pt("base")?, end: pt("end")?, label, description, values, from_midpoint: bool_or(p, "fromMidpoint", false) }
        }
        "point" | "p" => NewParameter::Point { at: pt("at")?, label, description },
        other => return Err(bad(ID, format!("\"{other}\" parameters can't be made yet (linear or point)"))),
    };
    ensure_block(s, &name)?;
    let added = author::add_parameter(s.doc_mut()?, &name, &new).map_err(|e| bad(ID, e))?;
    Ok(json!({ "block": name, "node": added.node, "handle": added.handle.hex(), "label": added.name }))
}

fn run_baction(s: &mut Session, p: &Value) -> Result<Value> {
    const ID: &str = "baction";
    let (name, base) = block(s, ID)?;
    let kind = str_param(p, "type").map(str::to_ascii_lowercase).ok_or_else(|| bad(ID, "`type` is required: stretch or move"))?;
    let param = p.get("parameter").ok_or_else(|| bad(ID, "`parameter` is required: a parameter's label"))?;
    let node = parameter_node(s.doc()?, &name, param).ok_or_else(|| bad(ID, "no such parameter in this block"))?;
    let objects: Vec<Handle> = targets(s, p)?;
    let new = match kind.as_str() {
        "stretch" | "s" => {
            let frame: Vec<Vec2> =
                p.get("frame").and_then(Value::as_array).map(|a| a.iter().filter_map(point_value).map(|q| q + base).collect()).unwrap_or_default();
            if frame.len() < 2 {
                return Err(bad(ID, "a stretch action needs `frame`: two corners or a polygon"));
            }
            NewAction::Stretch { parameter: node, frame, objects }
        }
        "move" | "m" => NewAction::Move { parameter: node, objects },
        other => return Err(bad(ID, format!("\"{other}\" actions can't be made yet (stretch or move)"))),
    };
    let added = author::add_action(s.doc_mut()?, &name, &new).map_err(|e| bad(ID, e))?;
    Ok(json!({ "block": name, "node": added.node, "handle": added.handle.hex(), "name": added.name }))
}

fn kw(i: &Input) -> Option<String> {
    match i {
        Input::Keyword(k) | Input::Text(k) => Some(k.trim().to_ascii_lowercase()),
        _ => None,
    }
}

/// Run `f` and report an error at the command line instead of stopping with it.
fn finish(s: &mut Session, r: Result<Value>) -> Result<Step> {
    match r {
        Ok(v) => {
            if let Some(l) = v.get("label").or_else(|| v.get("name")).and_then(Value::as_str) {
                s.echo(format!("{l} added."));
            }
        }
        Err(e) => s.echo(e.to_string()),
    }
    Ok(Step::Done)
}

// ---------------- BPARAMETER prompts ----------------

#[derive(Default)]
struct ParamM {
    kind: Option<&'static str>,
    pts: Vec<Vec2>,
}

impl Interactive for ParamM {
    fn name(&self) -> &'static str {
        "BPARAMETER"
    }
    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match (self.kind, self.pts.len()) {
            (None, _) => Prompt::new("Enter parameter type", Accept::TEXT).kw(&["Linear", "Point"]).default("Linear"),
            (Some("linear"), 0) => Prompt::new("Specify start point", Accept::POINT),
            (Some("linear"), _) => Prompt::new("Specify endpoint", Accept::POINT).base_opt(self.pts.first().copied()),
            (Some(_), _) => Prompt::new("Specify parameter location", Accept::POINT),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        let Some(kind) = self.kind else {
            let k = if matches!(i, Input::Enter) { Some("linear".to_string()) } else { kw(&i) };
            self.kind = match k.as_deref() {
                Some(k) if k.starts_with('l') => Some("linear"),
                Some(k) if k.starts_with('p') => Some("point"),
                _ => {
                    s.echo("Invalid option keyword.");
                    None
                }
            };
            return Ok(Step::Continue);
        };
        let Input::Point(q) = i else { return Ok(Step::Continue) };
        self.pts.push(q);
        match (kind, self.pts.as_slice()) {
            ("linear", [a, b]) => {
                let r = run_bparameter(s, &json!({ "type": "linear", "base": [a.x, a.y], "end": [b.x, b.y] }));
                finish(s, r)
            }
            ("point", [a]) => {
                let r = run_bparameter(s, &json!({ "type": "point", "at": [a.x, a.y] }));
                finish(s, r)
            }
            _ => Ok(Step::Continue),
        }
    }
}

// ---------------- BACTION prompts ----------------

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ActStage {
    #[default]
    Kind,
    Parameter,
    Corner1,
    Corner2,
    Select,
}

#[derive(Default)]
struct ActionM {
    stage: ActStage,
    kind: &'static str,
    parameter: String,
    frame: Vec<Vec2>,
    sel: SelectPhase,
}

impl ActionM {
    fn run(&self, s: &mut Session, hs: &[Handle]) -> Result<Step> {
        let mut p = json!({ "type": self.kind, "parameter": self.parameter, "handles": hs.iter().map(|h| h.hex()).collect::<Vec<_>>() });
        if self.kind == "stretch" {
            p["frame"] = json!(self.frame.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>());
        }
        let r = run_baction(s, &p);
        s.set_selection(Vec::new());
        finish(s, r)
    }
}

impl Interactive for ActionM {
    fn name(&self) -> &'static str {
        "BACTION"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        let (name, _) = block(s, "baction")?;
        if dynamic_block(s.doc()?, &name).is_none_or(|m| m.parameters.is_empty()) {
            s.echo("Add a parameter first (BPARAMETER): an action follows a parameter.");
            return Ok(Step::Done);
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match self.stage {
            ActStage::Kind => Prompt::new("Enter action type", Accept::TEXT).kw(&["Stretch", "Move"]).default("Stretch"),
            ActStage::Parameter => {
                let first = s
                    .state()
                    .ok()
                    .and_then(|st| dynamic_block(&st.doc, st.block_editor()?))
                    .and_then(|m| m.parameters.iter().find_map(|p| p.kind.label().map(str::to_string)))
                    .unwrap_or_default();
                Prompt::new("Enter the parameter the action follows", Accept::TEXT).default(&first)
            }
            ActStage::Corner1 => Prompt::new("Specify first corner of stretch frame", Accept::POINT),
            ActStage::Corner2 => {
                let p = Prompt::new("Specify opposite corner", Accept::POINT);
                match self.frame.first() {
                    Some(a) => p.corner(*a),
                    None => p,
                }
            }
            ActStage::Select => self.sel.prompt(),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        match self.stage {
            ActStage::Kind => {
                let k = if matches!(i, Input::Enter) { Some("stretch".to_string()) } else { kw(&i) };
                match k.as_deref() {
                    Some(k) if k.starts_with('s') => self.kind = "stretch",
                    Some(k) if k.starts_with('m') => self.kind = "move",
                    _ => {
                        s.echo("Invalid option keyword.");
                        return Ok(Step::Continue);
                    }
                }
                self.stage = ActStage::Parameter;
                Ok(Step::Continue)
            }
            ActStage::Parameter => {
                let typed = match &i {
                    Input::Enter => self.prompt(s).default.unwrap_or_default().to_ascii_lowercase(),
                    other => kw(other).unwrap_or_default(),
                };
                let (name, _) = block(s, "baction")?;
                let found = parameter_node(s.doc()?, &name, &json!(typed))
                    .or_else(|| number(&typed).and_then(|n| parameter_node(s.doc().ok()?, &name, &json!(n as i64))));
                if found.is_none() {
                    s.echo(format!("No parameter \"{typed}\" in this block."));
                    return Ok(Step::Continue);
                }
                self.parameter = typed;
                self.stage = if self.kind == "stretch" { ActStage::Corner1 } else { ActStage::Select };
                if self.stage == ActStage::Select {
                    self.sel = SelectPhase::default();
                }
                Ok(Step::Continue)
            }
            ActStage::Corner1 => {
                if let Input::Point(q) = i {
                    self.frame = vec![q];
                    self.stage = ActStage::Corner2;
                }
                Ok(Step::Continue)
            }
            ActStage::Corner2 => {
                if let Input::Point(q) = i {
                    self.frame.push(q);
                    self.sel = SelectPhase::default();
                    self.stage = ActStage::Select;
                }
                Ok(Step::Continue)
            }
            ActStage::Select => match self.sel.feed(s, &i)? {
                SelOutcome::More => Ok(Step::Continue),
                SelOutcome::Empty => Ok(Step::Done),
                SelOutcome::Done(hs) => self.run(s, &hs),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use draftcraft_doc::EntityKind;
    use serde_json::json;

    use crate::Session;

    /// A 10 × 5 rectangle of lines made into block "PANEL", with a reference, open in the Block
    /// Editor.
    fn session() -> (Session, Vec<String>) {
        let mut s = Session::new();
        let mut hs = Vec::new();
        for (a, b) in [([0, 0], [10, 0]), ([10, 0], [10, 5]), ([10, 5], [0, 5]), ([0, 5], [0, 0])] {
            let r = s.execute("line", &json!({ "points": [a, b] })).unwrap();
            hs.push(r["handles"][0].as_str().or_else(|| r["handle"].as_str()).unwrap().to_string());
        }
        s.execute("block", &json!({ "name": "PANEL", "base": [0, 0], "handles": hs, "retain": false })).unwrap();
        s.execute("insert", &json!({ "name": "PANEL", "at": [100, 0] })).unwrap();
        s.execute("bedit", &json!({ "name": "PANEL" })).unwrap();
        let hs = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
        (s, hs)
    }

    #[test]
    fn a_stretchable_block_made_in_the_block_editor() {
        let (mut s, hs) = session();
        let p = s.execute("bparameter", &json!({ "type": "linear", "base": [0, 0], "end": [10, 0], "label": "Width", "min": 5, "max": 30 })).unwrap();
        assert_eq!(p["label"], "Width");
        let a = s.execute("baction", &json!({ "type": "stretch", "parameter": "width", "frame": [[8, -1], [12, 6]], "handles": hs })).unwrap();
        assert_eq!(a["name"], "Stretch1");
        let l = s.execute("bparameter", &json!({})).unwrap();
        assert_eq!(l["parameters"][0]["label"], "Width");
        assert_eq!(l["actions"][0]["parameter"], "Width");
        s.execute("bclose", &json!({ "save": true })).unwrap();
        // The reference now stretches with its Width property.
        let ins = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
        let d = s.doc_mut().unwrap();
        draftcraft_doc::dynblock::set_property(d, ins, "Width", "20").unwrap();
        assert!(draftcraft_doc::dynblock::set_property(d, ins, "Width", "50").is_ok(), "clamped to the maximum");
        let EntityKind::Insert(i) = &d.entity(ins).unwrap().kind else { panic!("insert") };
        let xs: Vec<f64> = d
            .block(&i.block)
            .unwrap()
            .entities
            .iter()
            .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.a.x.max(l.b.x)) } else { None })
            .collect();
        assert!(xs.iter().any(|x| (x - 30.0).abs() < 1e-9), "{xs:?}");
    }

    #[test]
    fn refused_outside_a_block_and_with_bad_input() {
        let mut s = Session::new();
        assert!(s.execute("bparameter", &json!({ "type": "point", "at": [0, 0] })).is_err(), "no Block Editor");
        let (mut s, hs) = session();
        assert!(s.execute("bparameter", &json!({ "type": "rotation", "base": [0, 0], "end": [1, 0] })).is_err());
        assert!(s.execute("bparameter", &json!({ "type": "linear", "base": [0, 0], "end": [0, 0] })).is_err());
        assert!(s.execute("bparameter", &json!({ "type": "linear", "base": [0, 0], "end": [1, 0], "min": 9, "max": 2 })).is_err());
        assert!(s.execute("baction", &json!({ "type": "move", "parameter": "nope", "handles": hs })).is_err());
        s.execute("bparameter", &json!({ "type": "point", "at": [5, 5] })).unwrap();
        assert!(s.execute("baction", &json!({ "type": "stretch", "parameter": "Position1", "handles": hs })).is_err(), "no frame");
        let r = s.execute("bparameter", &json!({ "remove": "Position1" })).unwrap();
        assert_eq!(r["removed"], json!(["Position1"]));
    }

    #[test]
    fn bparameter_and_baction_at_the_command_line() {
        let (mut s, _) = session();
        s.cmdline("bparameter l 0,0 10,0").unwrap();
        assert!(s.running.is_none());
        s.cmdline("baction s Distance1 8,-1 12,6 all").unwrap();
        s.input(crate::Input::Enter).unwrap();
        let l = s.execute("bparameter", &json!({})).unwrap();
        assert_eq!(l["actions"].as_array().map(Vec::len), Some(1), "{l}");
    }
}
