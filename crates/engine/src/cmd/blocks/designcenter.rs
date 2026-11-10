//! ADCENTER (DesignCenter): look inside another drawing file and bring its blocks, layers,
//! linetypes and styles into this one.
//!
//! Called with only a path it lists what the file holds; with `kind` and `names` it copies those
//! definitions. A name this drawing already has keeps this drawing's definition, as when AutoCAD's
//! DesignCenter adds content. A copied block brings the blocks it references (to
//! [`MAX_BLOCK_DEPTH`] levels) and the layers and linetypes its objects use. The file is read
//! through the host's io hooks, like INSERT of a file.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use draftcraft_doc::{Drawing, EntityKind, MAX_BLOCK_DEPTH};
use serde_json::{Value, json};

use super::insert_file::{adopt, read_drawing};
use crate::cmd::merge::merge_named;
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Result, Session};

/// Most names one listing reports per kind, and one call copies.
const MAX_NAMES: usize = 10_000;

/// The kinds of content DesignCenter shows for a drawing, as `kind` names them.
pub const KINDS: &[&str] = &["blocks", "layers", "linetypes", "textStyles", "dimStyles", "tableStyles", "mleaderStyles", "mlineStyles"];

pub(super) fn spec() -> CommandSpec {
    CommandSpec::new("adcenter", "DesignCenter", run_adcenter).menu(&["Tools", "Palettes", "DesignCenter"]).alias(&["adc", "dc", "dcenter"]).params(
        "{path: DXF/DWG file, kind?: blocks|layers|linetypes|textStyles|dimStyles|tableStyles|mleaderStyles|mlineStyles, \
             names?: [names] (copy these into the drawing; names it has keep its definitions)} → listing {path, blocks: [{name, \
             description, objects}], layers: [names], linetypes, textStyles, dimStyles, tableStyles, mleaderStyles, mlineStyles} | \
             copy {kind, added: [names], kept: [names]}",
    )
}

fn run_adcenter(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").filter(|x| !x.trim().is_empty()).ok_or_else(|| bad("adcenter", "`path` names the drawing file to look in"))?;
    let src = read_drawing(path)?;
    let Some(kind) = str_param(p, "kind") else {
        let mut out = listing(&src);
        out["path"] = json!(path);
        return Ok(out);
    };
    let kind = KINDS
        .iter()
        .find(|k| k.eq_ignore_ascii_case(kind))
        .copied()
        .ok_or_else(|| bad("adcenter", format!("unknown kind \"{kind}\" ({})", KINDS.join(", "))))?;
    let names: Vec<String> = match p.get("names") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).take(MAX_NAMES).map(str::to_string).collect(),
        Some(Value::String(one)) => vec![one.clone()],
        _ => return Err(bad("adcenter", "`names` lists what to copy")),
    };
    if names.is_empty() {
        return Err(bad("adcenter", "`names` lists what to copy"));
    }
    let mut d = s.doc()?.clone();
    let (added, kept) = copy(&mut d, &src, kind, &names)?;
    if !added.is_empty() {
        *s.doc_mut()? = d;
    }
    let message = match (added.len(), kept.len()) {
        (0, _) => format!("Nothing added: the drawing already has {}.", kept.join(", ")),
        (n, 0) => format!("Added {n} item(s): {}.", added.join(", ")),
        (n, _) => format!("Added {n} item(s): {}; kept the drawing's {}.", added.join(", "), kept.join(", ")),
    };
    Ok(json!({ "kind": kind, "added": added, "kept": kept, "message": message }))
}

/// What a drawing holds, by kind. Generated (`*…`) blocks, layout blocks and xrefs aren't listed.
fn listing(src: &Drawing) -> Value {
    let blocks: Vec<Value> = src
        .blocks
        .values()
        .filter(|b| !b.anonymous && !b.name.starts_with('*') && b.xref_path.is_none())
        .take(MAX_NAMES)
        .map(|b| json!({ "name": b.name, "description": b.description, "objects": b.entities.len() }))
        .collect();
    fn names<T>(v: &[T], name: fn(&T) -> &str) -> Vec<&str> {
        v.iter().take(MAX_NAMES).map(name).collect()
    }
    json!({
        "blocks": blocks,
        "layers": names(&src.layers, |l| &l.name),
        "linetypes": names(&src.linetypes, |l| &l.name),
        "textStyles": names(&src.text_styles, |t| &t.name),
        "dimStyles": names(&src.dim_styles, |t| &t.name),
        "tableStyles": names(&src.table_styles, |t| &t.name),
        "mleaderStyles": names(&src.mleader_styles, |t| &t.name),
        "mlineStyles": names(&src.mline_styles, |t| &t.name),
    })
}

/// Copy the named definitions of `kind` from `src` into `d`. Returns (added, kept) names; a name
/// `src` lacks is an error and nothing is copied.
fn copy(d: &mut Drawing, src: &Drawing, kind: &str, names: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    if kind == "blocks" {
        return copy_blocks(d, src, names);
    }
    fn pick<T: Clone>(ours: &mut Vec<T>, theirs: &[T], names: &[String], name: fn(&T) -> &str) -> Result<(Vec<String>, Vec<String>)> {
        let mut chosen = Vec::new();
        for n in names {
            let t = theirs.iter().find(|t| name(t).eq_ignore_ascii_case(n)).ok_or_else(|| bad("adcenter", format!("the file has no \"{n}\"")))?;
            chosen.push(t.clone());
        }
        let (mut added, mut kept) = (Vec::new(), Vec::new());
        for t in &chosen {
            if merge_named(ours, std::slice::from_ref(t), name) > 0 { added.push(name(t).to_string()) } else { kept.push(name(t).to_string()) }
        }
        Ok((added, kept))
    }
    match kind {
        "layers" => {
            let r = pick(&mut d.layers, &src.layers, names, |l| &l.name)?;
            // A copied layer brings its linetype.
            let lts: Vec<String> = r.0.iter().filter_map(|n| src.layers.iter().find(|l| &l.name == n)).map(|l| l.linetype.clone()).collect();
            add_linetypes(d, src, lts);
            Ok(r)
        }
        "linetypes" => pick(&mut d.linetypes, &src.linetypes, names, |l| &l.name),
        "textStyles" => pick(&mut d.text_styles, &src.text_styles, names, |t| &t.name),
        "dimStyles" => pick(&mut d.dim_styles, &src.dim_styles, names, |t| &t.name),
        "tableStyles" => pick(&mut d.table_styles, &src.table_styles, names, |t| &t.name),
        "mleaderStyles" => pick(&mut d.mleader_styles, &src.mleader_styles, names, |t| &t.name),
        "mlineStyles" => pick(&mut d.mline_styles, &src.mline_styles, names, |t| &t.name),
        _ => Err(bad("adcenter", format!("unknown kind \"{kind}\""))),
    }
}

/// Add the linetypes of `src` named in `names` that `d` lacks.
fn add_linetypes(d: &mut Drawing, src: &Drawing, names: impl IntoIterator<Item = String>) {
    let wanted: BTreeSet<String> = names.into_iter().map(|n| n.to_ascii_uppercase()).collect();
    let theirs: Vec<_> = src.linetypes.iter().filter(|l| wanted.contains(&l.name.to_ascii_uppercase())).cloned().collect();
    merge_named(&mut d.linetypes, &theirs, |l| &l.name);
}

/// Copy the named blocks, the blocks they reference and the layers and linetypes their objects
/// use. A block this drawing already has keeps its definition.
fn copy_blocks(d: &mut Drawing, src: &Drawing, names: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    let mut roots = Vec::new();
    for n in names {
        let key = src.blocks.keys().find(|k| k.eq_ignore_ascii_case(n)).ok_or_else(|| bad("adcenter", format!("the file has no block \"{n}\"")))?;
        roots.push(key.clone());
    }
    // The blocks to bring: the named ones and what they reference, breadth first, to a bounded depth.
    let mut needed: BTreeSet<String> = BTreeSet::new();
    let mut level: Vec<String> = roots.clone();
    for _ in 0..=MAX_BLOCK_DEPTH {
        let mut next = Vec::new();
        for name in level {
            if !needed.insert(name.clone()) {
                continue;
            }
            let Some(b) = src.blocks.get(&name) else { continue };
            for e in b.entities.iter() {
                let refd = match &e.kind {
                    EntityKind::Insert(i) => Some(&i.block),
                    EntityKind::Dimension(dm) => dm.block.as_ref(),
                    _ => None,
                };
                if let Some(r) = refd.and_then(|r| src.blocks.keys().find(|k| k.eq_ignore_ascii_case(r))) {
                    next.push(r.clone());
                }
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    let renames: BTreeMap<String, String> =
        crate::cmd::merge::block_renames(d, src, None).into_iter().filter(|(from, _)| needed.contains(from)).collect();
    let (mut layers, mut linetypes) = (BTreeSet::new(), BTreeSet::new());
    for (from, to) in &renames {
        let Some(b) = src.blocks.get(from) else { continue };
        let mut nb = (**b).clone();
        nb.name = to.clone();
        nb.entities = Default::default();
        for e in b.entities.iter() {
            layers.insert(e.common.layer.to_ascii_uppercase());
            linetypes.insert(e.common.linetype.clone());
            nb.entities.push(adopt(d, src, e, &renames));
        }
        d.blocks.insert(to.clone(), Arc::new(nb));
    }
    let new_layers: Vec<_> = src.layers.iter().filter(|l| layers.contains(&l.name.to_ascii_uppercase())).cloned().collect();
    linetypes.extend(new_layers.iter().map(|l| l.linetype.clone()));
    merge_named(&mut d.layers, &new_layers, |l| &l.name);
    add_linetypes(d, src, linetypes);
    let (mut added, mut kept) = (Vec::new(), Vec::new());
    for r in roots {
        if renames.contains_key(&r) { added.push(r) } else { kept.push(r) }
    }
    Ok((added, kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use draftcraft_doc::{Block, Circle, Entity, Handle, Insert, Layer};
    use draftcraft_geom::Vec3;

    fn source() -> Drawing {
        let mut d = Drawing::new_metric();
        let mut parts = Layer::new("PARTS");
        parts.linetype = "DASHED".into();
        d.layers.push(parts);
        d.linetypes.push(draftcraft_doc::Linetype::simple("DASHED", "", &[0.5, -0.25]));
        let mut nut = Block::new("NUT");
        let mut c = Entity::new(Handle(0x50), EntityKind::Circle(Circle { center: Vec3::ZERO, radius: 1.0 }));
        c.common.layer = "PARTS".into();
        nut.entities.push(c);
        d.blocks.insert("NUT".into(), Arc::new(nut));
        let mut bolt = Block::new("BOLT");
        let ins = Insert {
            block: "NUT".into(),
            insert: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        };
        bolt.entities.push(Entity::new(Handle(0x51), EntityKind::Insert(ins)));
        d.blocks.insert("BOLT".into(), Arc::new(bolt));
        d
    }

    #[test]
    fn a_block_brings_what_it_references() {
        let src = source();
        let mut d = Drawing::new_metric();
        let (added, kept) = copy(&mut d, &src, "blocks", &["bolt".into()]).unwrap();
        assert_eq!((added, kept), (vec!["BOLT".to_string()], vec![]));
        assert!(d.block("NUT").is_some(), "the nested block comes along");
        assert!(d.layers.iter().any(|l| l.name == "PARTS"), "the layer its objects use");
        assert!(d.linetypes.iter().any(|l| l.name == "DASHED"), "and that layer's linetype");
        // A second copy keeps the drawing's definitions.
        let (added, kept) = copy(&mut d, &src, "blocks", &["BOLT".into()]).unwrap();
        assert!(added.is_empty());
        assert_eq!(kept, vec!["BOLT".to_string()]);
    }

    #[test]
    fn listing_and_missing_names() {
        let src = source();
        let l = listing(&src);
        let blocks: Vec<&str> = l["blocks"].as_array().unwrap().iter().filter_map(|b| b["name"].as_str()).collect();
        assert!(blocks.contains(&"BOLT") && blocks.contains(&"NUT"));
        assert!(l["layers"].as_array().unwrap().iter().any(|n| n == "PARTS"));
        let mut d = Drawing::new_metric();
        assert!(copy(&mut d, &src, "layers", &["NOPE".into()]).is_err());
        let (added, _) = copy(&mut d, &src, "layers", &["parts".into()]).unwrap();
        assert_eq!(added, vec!["PARTS".to_string()]);
        assert!(d.linetypes.iter().any(|l| l.name == "DASHED"));
    }
}
