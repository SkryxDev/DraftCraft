//! Making a block dynamic: adding parameters and actions to a block (BPARAMETER and BACTION in
//! the Block Editor).
//!
//! They are written as the same objects [`dynamic_block`](super::dynamic_block) reads: the block
//! record's extension dictionary gets an `ACAD_ENHANCEDBLOCK` evaluation graph, and each parameter,
//! its grips and each action become members of it, with the groups the reader and
//! [`set_property`](super::set_property) use. So a reference to the block gets its grips and custom
//! properties in DraftCraft at once, and the objects are kept and written with the drawing.
//!
//! What can be made so far: a linear (distance) parameter and a point parameter, and stretch and
//! move actions on them.

use std::sync::Arc;

use draftcraft_geom::{Vec2, Vec3};

use super::{ParameterKind, ValueSet, dynamic_block};
use crate::foreign::ForeignObject;
use crate::{Drawing, Handle, RawTag};

/// Most objects one action may act on.
pub const MAX_SELECTION: usize = 10_000;
/// Longest name, label or description kept.
const MAX_TEXT: usize = 255;

fn tag(code: i32, value: impl Into<String>) -> RawTag {
    RawTag { code, value: value.into() }
}

fn text(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect()
}

fn real(v: f64) -> String {
    if v.is_finite() { format!("{v}") } else { "0".into() }
}

fn point(tags: &mut Vec<RawTag>, x: i32, p: Vec3) {
    tags.extend([tag(x, real(p.x)), tag(x + 10, real(p.y)), tag(x + 20, real(p.z))]);
}

/// The definition of a new parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum NewParameter {
    /// A distance between two points (block coordinates); `values` limits it.
    Linear { base: Vec2, end: Vec2, label: String, description: String, values: ValueSet, from_midpoint: bool },
    /// A position (block coordinates).
    Point { at: Vec2, label: String, description: String },
}

/// The definition of a new action on parameter `parameter` (its graph id).
#[derive(Clone, Debug, PartialEq)]
pub enum NewAction {
    /// Moves the points of `objects` inside `frame` (two corners or a polygon, block coordinates)
    /// by the change of the parameter's end point.
    Stretch { parameter: i32, frame: Vec<Vec2>, objects: Vec<Handle> },
    /// Moves `objects` by the change of the parameter's end point (a point parameter's position).
    Move { parameter: i32, objects: Vec<Handle> },
}

/// What was added: the member's graph id, object handle and name.
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    pub node: i32,
    pub handle: Handle,
    pub name: String,
}

/// The record key of block `name` in the kept foreign data.
fn record_key(name: &str) -> (String, String) {
    ("BLOCK_RECORD".to_string(), name.trim().to_ascii_uppercase())
}

fn find(d: &Drawing, h: Handle) -> Option<&ForeignObject> {
    d.foreign.objects.iter().find(|o| o.handle == h)
}

fn find_mut(d: &mut Drawing, h: Handle) -> Option<&mut ForeignObject> {
    Arc::make_mut(&mut d.foreign).objects.iter_mut().find(|o| o.handle == h)
}

/// The entry `key` of dictionary object `dict`.
fn entry(o: &ForeignObject, key: &str) -> Option<Handle> {
    let mut it = o.tags.iter();
    while let Some(t) = it.next() {
        if t.code == 3 && t.value.eq_ignore_ascii_case(key) {
            return it.next().filter(|t| matches!(t.code, 350 | 360)).and_then(|t| Handle::parse_hex(&t.value));
        }
    }
    None
}

/// The handle the kept data knows block record `key` by: the file's, or for a block made here a
/// new one, which the writer turns into the written record's handle like a file's.
fn record_handle(d: &mut Drawing, key: &(String, String)) -> Handle {
    if let Some(h) = d.foreign.targets.iter().find(|(_, k)| *k == key).map(|(h, _)| *h) {
        return h;
    }
    let h = d.new_handle();
    Arc::make_mut(&mut d.foreign).targets.insert(h, key.clone());
    h
}

/// Block `name`'s evaluation graph, made (with the extension dictionary that holds it) when the
/// block has none.
fn graph(d: &mut Drawing, name: &str) -> Result<Handle, String> {
    if d.block(name).is_none() {
        return Err(format!("Block \"{name}\" not found."));
    }
    let key = record_key(name);
    let xdict = match d.foreign.records.get(&key).and_then(|x| x.xdict).filter(|h| find(d, *h).is_some()) {
        Some(h) => h,
        None => {
            let owner = record_handle(d, &key);
            let h = d.new_handle();
            Arc::make_mut(&mut d.foreign).objects.push(ForeignObject {
                handle: h,
                kind: "DICTIONARY".into(),
                tags: vec![tag(5, h.hex()), tag(330, owner.hex()), tag(100, "AcDbDictionary"), tag(280, "1"), tag(281, "1")],
            });
            Arc::make_mut(&mut d.foreign).records.entry(key).or_default().xdict = Some(h);
            h
        }
    };
    let existing = find(d, xdict)
        .and_then(|o| entry(o, "ACAD_ENHANCEDBLOCK"))
        .filter(|g| find(d, *g).is_some_and(|o| o.kind.eq_ignore_ascii_case("ACAD_EVALUATION_GRAPH")));
    if let Some(g) = existing {
        return Ok(g);
    }
    let g = d.new_handle();
    Arc::make_mut(&mut d.foreign).objects.push(ForeignObject {
        handle: g,
        kind: "ACAD_EVALUATION_GRAPH".into(),
        tags: vec![tag(5, g.hex()), tag(330, xdict.hex()), tag(100, "AcDbEvalGraph"), tag(96, "0"), tag(97, "0")],
    });
    if let Some(dict) = find_mut(d, xdict) {
        dict.tags.push(tag(3, "ACAD_ENHANCEDBLOCK"));
        dict.tags.push(tag(360, g.hex()));
    }
    Ok(g)
}

/// The next free graph id: one more than any the block's graph uses.
fn next_node(d: &Drawing, name: &str) -> i32 {
    let Some(m) = dynamic_block(d, name) else { return 0 };
    let used = m.parameters.iter().map(|p| p.node).chain(m.actions.iter().map(|a| a.node)).chain(m.grip_objects.iter().map(|g| g.0));
    let grips = m.parameters.iter().flat_map(|p| p.grips.iter().copied());
    used.chain(grips).max().map_or(0, |n| n.saturating_add(1))
}

/// A name not used by another member of the block's graph: `base` followed by a number.
fn unique_name(d: &Drawing, name: &str, base: &str) -> String {
    let m = dynamic_block(d, name).unwrap_or_default();
    let taken = |s: &str| {
        m.parameters.iter().any(|p| p.element.eq_ignore_ascii_case(s) || p.kind.label().is_some_and(|l| l.eq_ignore_ascii_case(s)))
            || m.actions.iter().any(|a| a.name.eq_ignore_ascii_case(s))
    };
    (1..10_000).map(|i| format!("{base}{i}")).find(|s| !taken(s)).unwrap_or_else(|| format!("{base}{}", m.parameters.len() + m.actions.len() + 1))
}

/// Add member `tags` (kind `kind`) to graph `g`.
fn add_member(d: &mut Drawing, g: Handle, kind: &str, mut tags: Vec<RawTag>) -> Handle {
    let h = d.new_handle();
    tags.insert(0, tag(330, g.hex()));
    tags.insert(0, tag(5, h.hex()));
    Arc::make_mut(&mut d.foreign).objects.push(ForeignObject { handle: h, kind: kind.into(), tags });
    if let Some(o) = find_mut(d, g) {
        o.tags.push(tag(360, h.hex()));
    }
    rebuild_graph(d, g);
    h
}

/// Write graph `g`'s node list as AutoCAD's files hold it, from the members it lists: the next
/// free id twice (96, 97), then each member as a node, its id (91, its expression's 90), flags
/// 32 (93), the id after it (95), the member (360) and four unused links (92 -1). The DWG writer
/// encodes the nodes from these groups; a bare list of members would be written as an empty
/// graph, and the block would open as a plain block.
fn rebuild_graph(d: &mut Drawing, g: Handle) {
    let Some(graph) = find(d, g) else { return };
    let members: Vec<Handle> = graph.tags.iter().filter(|t| t.code == 360).filter_map(|t| Handle::parse_hex(&t.value)).collect();
    let node_of = |h: Handle| -> Option<i64> {
        let o = find(d, h)?;
        let mut tags = o.tags.iter().skip_while(|t| !(t.code == 100 && t.value == "AcDbEvalExpr"));
        tags.find(|t| t.code == 90).and_then(|t| t.value.trim().parse().ok())
    };
    let nodes: Vec<(i64, Handle)> = members.into_iter().filter_map(|h| Some((node_of(h)?, h))).collect();
    let free = nodes.iter().map(|n| n.0).max().map_or(0, |n| n.saturating_add(1));
    let Some(graph) = find_mut(d, g) else { return };
    let head = graph.tags.iter().position(|t| t.code == 100 && t.value == "AcDbEvalGraph").map_or(graph.tags.len(), |i| i + 1);
    graph.tags.truncate(head);
    graph.tags.extend([tag(96, free.to_string()), tag(97, free.to_string())]);
    for (i, (id, h)) in nodes.iter().enumerate() {
        let after = nodes.get(i + 1).map_or(free, |n| n.0);
        graph.tags.extend([tag(91, id.to_string()), tag(93, "32"), tag(95, after.to_string()), tag(360, h.hex())]);
        graph.tags.extend(std::iter::repeat_with(|| tag(92, "-1")).take(4));
    }
}

fn head(node: i32, element: &str) -> Vec<RawTag> {
    vec![tag(100, "AcDbEvalExpr"), tag(90, node.to_string()), tag(100, "AcDbBlockElement"), tag(300, element)]
}

/// Add parameter `p` to block `name`. Its label defaults to "Distance1", "Position1"…
pub fn add_parameter(d: &mut Drawing, name: &str, p: &NewParameter) -> Result<Added, String> {
    let finite = |v: Vec2| v.x.is_finite() && v.y.is_finite();
    match p {
        NewParameter::Linear { base, end, .. } if !finite(*base) || !finite(*end) => return Err("Invalid point.".into()),
        NewParameter::Linear { base, end, .. } if (*end - *base).len() < 1e-9 => return Err("The two points must differ.".into()),
        NewParameter::Point { at, .. } if !finite(*at) => return Err("Invalid point.".into()),
        _ => {}
    }
    let g = graph(d, name)?;
    let node = next_node(d, name);
    let (grip1, grip2) = (node.saturating_add(1), node.saturating_add(2));
    let (kind, element, label, tags, grips): (&str, &str, String, Vec<RawTag>, Vec<(i32, &str)>) = match p {
        NewParameter::Linear { base, end, label, description, values, from_midpoint } => {
            let label = if label.trim().is_empty() { unique_name(d, name, "Distance") } else { text(label) };
            let mut t = head(node, "Linear");
            t.extend([tag(100, "AcDbBlockParameter"), tag(280, "1"), tag(281, "0"), tag(100, "AcDbBlock2PtParameter")]);
            point(&mut t, 1010, base.to3(0.0));
            point(&mut t, 1011, end.to3(0.0));
            t.extend([tag(170, "2"), tag(91, grip1.to_string()), tag(91, grip2.to_string()), tag(177, if *from_midpoint { "1" } else { "0" })]);
            t.extend([tag(100, "AcDbBlockLinearParameter"), tag(305, label.clone()), tag(306, text(description)), tag(307, "")]);
            t.extend([
                tag(96, values.flags.to_string()),
                tag(141, real(values.minimum)),
                tag(142, real(values.maximum)),
                tag(143, real(values.increment)),
            ]);
            let list: Vec<f64> = values.list.iter().copied().filter(|v| v.is_finite()).take(MAX_SELECTION).collect();
            t.push(tag(175, list.len().to_string()));
            t.extend(list.iter().map(|v| tag(144, real(*v))));
            ("BLOCKLINEARPARAMETER", "Linear", label, t, vec![(grip1, "Grip"), (grip2, "Grip")])
        }
        NewParameter::Point { at, label, description } => {
            let label = if label.trim().is_empty() { unique_name(d, name, "Position") } else { text(label) };
            let mut t = head(node, "Point");
            t.extend([tag(100, "AcDbBlockParameter"), tag(280, "1"), tag(281, "0"), tag(100, "AcDbBlock1PtParameter")]);
            point(&mut t, 1010, at.to3(0.0));
            t.push(tag(93, grip1.to_string()));
            t.extend([tag(100, "AcDbBlockPointParameter"), tag(303, label.clone()), tag(304, text(description))]);
            ("BLOCKPOINTPARAMETER", "Point", label, t, vec![(grip1, "Grip")])
        }
    };
    let label_taken =
        dynamic_block(d, name).is_some_and(|m| m.parameters.iter().any(|q| q.kind.label().is_some_and(|l| l.eq_ignore_ascii_case(&label))));
    if label_taken {
        return Err(format!("The block already has a parameter named \"{label}\"."));
    }
    let _ = element;
    let handle = add_member(d, g, kind, tags);
    let grip_kind = if kind == "BLOCKLINEARPARAMETER" { "BLOCKLINEARGRIP" } else { "BLOCKXYGRIP" };
    for (id, el) in grips {
        let mut t = head(id, el);
        t.extend([tag(100, "AcDbBlockGrip"), tag(91, node.to_string())]);
        add_member(d, g, grip_kind, t);
    }
    Ok(Added { node, handle, name: label })
}

/// Add action `a` to block `name`. Its objects must be the block's.
pub fn add_action(d: &mut Drawing, name: &str, a: &NewAction) -> Result<Added, String> {
    let m = dynamic_block(d, name).ok_or("Add a parameter first: an action follows a parameter.")?;
    let (param, objects) = match a {
        NewAction::Stretch { parameter, objects, .. } | NewAction::Move { parameter, objects } => (*parameter, objects),
    };
    let p = m.parameters.iter().find(|p| p.node == param).ok_or("No such parameter.")?;
    let block = d.block(name).ok_or_else(|| format!("Block \"{name}\" not found."))?;
    let mut seen = std::collections::HashSet::new();
    // In the Block Editor the block's objects are the editor drawing's model space.
    let objects: Vec<Handle> = objects
        .iter()
        .copied()
        .filter(|h| (block.entities.get(*h).is_some() || d.model.get(*h).is_some()) && seen.insert(*h))
        .take(MAX_SELECTION)
        .collect();
    if objects.is_empty() {
        return Err("Select the block's objects the action changes.".into());
    }
    let (at, connections): (Vec3, [&str; 2]) = match &p.kind {
        ParameterKind::Linear { end, .. } => (*end, ["EndXDelta", "EndYDelta"]),
        ParameterKind::Point { at, .. } => (*at, ["XDelta", "YDelta"]),
        _ => return Err("Actions can be added to linear and point parameters.".into()),
    };
    let (kind, base_name) = match a {
        NewAction::Stretch { .. } => ("BLOCKSTRETCHACTION", "Stretch"),
        NewAction::Move { .. } => ("BLOCKMOVEACTION", "Move"),
    };
    let action_name = unique_name(d, name, base_name);
    let g = graph(d, name)?;
    let node = next_node(d, name);
    let mut t = head(node, &action_name);
    t.extend([tag(100, "AcDbBlockAction"), tag(70, "0"), tag(71, objects.len().to_string())]);
    t.extend(objects.iter().map(|h| tag(330, h.hex())));
    point(&mut t, 1010, at);
    match a {
        NewAction::Stretch { frame, .. } => {
            let frame: Vec<Vec2> = frame.iter().copied().filter(|q| q.x.is_finite() && q.y.is_finite()).take(4096).collect();
            if frame.len() < 2 {
                return Err("A stretch action needs a frame: two corners or a polygon.".into());
            }
            t.extend([
                tag(100, "AcDbBlockStretchAction"),
                tag(92, param.to_string()),
                tag(301, connections[0]),
                tag(93, param.to_string()),
                tag(302, connections[1]),
            ]);
            t.push(tag(72, frame.len().to_string()));
            for q in &frame {
                t.extend([tag(1011, real(q.x)), tag(1021, real(q.y))]);
            }
            t.extend([tag(73, "0"), tag(75, "0"), tag(140, "1"), tag(141, "0")]);
        }
        NewAction::Move { .. } => {
            t.extend([
                tag(100, "AcDbBlockMoveAction"),
                tag(92, param.to_string()),
                tag(301, connections[0]),
                tag(93, param.to_string()),
                tag(302, connections[1]),
            ]);
            t.extend([tag(140, "1"), tag(141, "0")]);
        }
    }
    let handle = add_member(d, g, kind, t);
    Ok(Added { node, handle, name: action_name })
}

/// Remove parameter or action `node` from block `name` (with a parameter's grips and the actions
/// that follow it).
pub fn remove(d: &mut Drawing, name: &str, node: i32) -> Result<Vec<String>, String> {
    let m = dynamic_block(d, name).ok_or("The block has no parameters or actions.")?;
    let mut gone: Vec<(Handle, String)> = Vec::new();
    if let Some(p) = m.parameters.iter().find(|p| p.node == node) {
        gone.push((p.handle, p.kind.label().unwrap_or(&p.element).to_string()));
        gone.extend(m.grip_objects.iter().filter(|(id, _)| p.grips.contains(id)).map(|(_, h)| (*h, String::new())));
        gone.extend(m.actions.iter().filter(|a| a.connections.iter().any(|(n, _)| *n == node)).map(|a| (a.handle, a.name.clone())));
    } else if let Some(a) = m.actions.iter().find(|a| a.node == node) {
        gone.push((a.handle, a.name.clone()));
    } else {
        return Err("No such parameter or action.".into());
    }
    let hs: std::collections::HashSet<Handle> = gone.iter().map(|g| g.0).collect();
    Arc::make_mut(&mut d.foreign).objects.retain(|o| !hs.contains(&o.handle));
    let hexes: std::collections::HashSet<String> = hs.iter().map(|h| h.hex()).collect();
    let mut graphs = Vec::new();
    for o in Arc::make_mut(&mut d.foreign).objects.iter_mut().filter(|o| o.kind.eq_ignore_ascii_case("ACAD_EVALUATION_GRAPH")) {
        let before = o.tags.len();
        o.tags.retain(|t| !(t.code == 360 && Handle::parse_hex(&t.value).is_some_and(|h| hexes.contains(&h.hex()))));
        if o.tags.len() != before {
            graphs.push(o.handle);
        }
    }
    for g in graphs {
        rebuild_graph(d, g);
    }
    Ok(gone.into_iter().map(|g| g.1).filter(|n| !n.is_empty()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, EntityKind, Insert};

    /// A drawing with block "B" (a 10 × 5 rectangle's four lines) and a reference to it.
    fn drawing() -> (Drawing, Vec<Handle>, Handle) {
        let mut d = Drawing::new_metric();
        let mut b = Block::new("B");
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 5.0), Vec2::new(0.0, 5.0)];
        let mut hs = Vec::new();
        for i in 0..4 {
            let h = d.new_handle();
            let (a, c) = (pts[i], pts[(i + 1) % 4]);
            let line = crate::Line { a: a.to3(0.0), b: c.to3(0.0) };
            b.entities.push(crate::Entity::new(h, EntityKind::Line(line)));
            hs.push(h);
        }
        d.blocks.insert("B".into(), Arc::new(b));
        let ins = d.new_handle();
        d.model.push(crate::Entity::new(ins, EntityKind::Insert(Insert { block: "B".into(), ..Insert::default() })));
        (d, hs, ins)
    }

    #[test]
    fn a_linear_parameter_with_a_stretch_makes_a_working_dynamic_block() {
        let (mut d, hs, ins) = drawing();
        let p = NewParameter::Linear {
            base: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            label: String::new(),
            description: "Width".into(),
            values: ValueSet::default(),
            from_midpoint: false,
        };
        let added = add_parameter(&mut d, "B", &p).unwrap();
        assert_eq!(added.name, "Distance1");
        let frame = vec![Vec2::new(8.0, -1.0), Vec2::new(12.0, 6.0)];
        let act = add_action(&mut d, "B", &NewAction::Stretch { parameter: added.node, frame, objects: hs.clone() }).unwrap();
        assert_eq!(act.name, "Stretch1");
        let m = dynamic_block(&d, "B").expect("the block is dynamic");
        assert_eq!((m.parameters.len(), m.actions.len()), (1, 1));
        assert_eq!(m.parameters[0].grips.len(), 2);
        assert_eq!(m.actions[0].selection, hs);
        // The reference has a Distance1 property and grips; setting it stretches the right side.
        let (_, props) = super::super::custom_properties(&d, ins).expect("custom properties");
        assert!(props.iter().any(|p| p.name == "Distance1"), "{props:?}");
        assert!(!super::super::grips(&d, ins).is_empty());
        super::super::set_property(&mut d, ins, "Distance1", "16").unwrap();
        let EntityKind::Insert(i) = &d.entity(ins).unwrap().kind else { panic!("insert") };
        let shown = d.block(&i.block).unwrap();
        let xs: Vec<f64> =
            shown.entities.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.a.x.max(l.b.x)) } else { None }).collect();
        assert!(xs.iter().any(|x| (x - 16.0).abs() < 1e-9), "{xs:?}");
    }

    #[test]
    fn a_point_parameter_with_a_move_and_removing_them() {
        let (mut d, hs, _) = drawing();
        let p =
            add_parameter(&mut d, "B", &NewParameter::Point { at: Vec2::new(5.0, 5.0), label: "Top".into(), description: String::new() }).unwrap();
        let mv = add_action(&mut d, "B", &NewAction::Move { parameter: p.node, objects: vec![hs[2]] }).unwrap();
        assert_eq!(mv.name, "Move1");
        // Same label twice, no objects, unknown parameter.
        assert!(add_parameter(&mut d, "B", &NewParameter::Point { at: Vec2::ZERO, label: "top".into(), description: String::new() }).is_err());
        assert!(add_action(&mut d, "B", &NewAction::Move { parameter: p.node, objects: vec![Handle(0xFFFFF)] }).is_err());
        assert!(add_action(&mut d, "B", &NewAction::Move { parameter: 999, objects: hs.clone() }).is_err());
        let gone = remove(&mut d, "B", p.node).unwrap();
        assert_eq!(gone, ["Top", "Move1"]);
        let m = dynamic_block(&d, "B").unwrap();
        assert!(m.parameters.is_empty() && m.actions.is_empty() && m.grip_objects.is_empty());
        assert!(add_parameter(&mut d, "NOPE", &NewParameter::Point { at: Vec2::ZERO, label: String::new(), description: String::new() }).is_err());
    }

    /// The graph lists its members as AutoCAD's files do, one node each (id, flags, the next id,
    /// the member, four links), and keeps the list right when a member goes.
    #[test]
    fn the_graph_lists_each_member_as_a_node() {
        let (mut d, hs, _) = drawing();
        let p =
            add_parameter(&mut d, "B", &NewParameter::Point { at: Vec2::new(5.0, 5.0), label: "Top".into(), description: String::new() }).unwrap();
        let mv = add_action(&mut d, "B", &NewAction::Move { parameter: p.node, objects: vec![hs[2]] }).unwrap();
        let graph = |d: &Drawing| d.foreign.objects.iter().find(|o| o.kind == "ACAD_EVALUATION_GRAPH").unwrap().tags.clone();
        let nodes = |tags: &[RawTag]| -> Vec<(String, String, String)> {
            let mut out = Vec::new();
            for (i, t) in tags.iter().enumerate().filter(|(_, t)| t.code == 91) {
                let at = |k: usize| tags.get(i + k).map(|t| (t.code, t.value.clone()));
                assert_eq!(at(1), Some((93, "32".into())));
                let Some((95, next)) = at(2) else { panic!("95 after 91") };
                let Some((360, member)) = at(3) else { panic!("360 after 95") };
                assert!((4..8).all(|k| at(k) == Some((92, "-1".into()))));
                out.push((t.value.clone(), next, member));
            }
            out
        };
        let tags = graph(&d);
        let listed = nodes(&tags);
        // The point parameter, its grip and the move.
        assert_eq!(listed.len(), 3, "{tags:?}");
        assert_eq!(listed.last().map(|n| n.2.clone()), Some(mv.handle.hex()));
        let free = tags.iter().find(|t| t.code == 96).map(|t| t.value.clone()).unwrap();
        assert_eq!(listed.last().map(|n| n.1.clone()), Some(free.clone()), "the last node's next is the next free id");
        assert_eq!(tags.iter().filter(|t| t.code == 360).count(), 3);
        remove(&mut d, "B", mv.node).unwrap();
        let tags = graph(&d);
        assert_eq!(nodes(&tags).len(), 2, "{tags:?}");
        assert_eq!(tags.iter().filter(|t| t.code == 360).count(), 2);
        assert!(dynamic_block(&d, "B").is_some_and(|m| m.actions.is_empty() && m.parameters.len() == 1));
    }
}
