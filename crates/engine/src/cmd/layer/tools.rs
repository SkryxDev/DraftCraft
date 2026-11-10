//! LAYTRANS (Layer Translator) and LAYWALK (Layer Walk).
//!
//! LAYTRANS maps layers to other names, the way a drawing is brought to an office's layer
//! standard: every object on a mapped layer moves to its new layer (made from a standards drawing
//! when one is given, else from the old layer's properties), and old layers left empty are purged.
//!
//! LAYWALK shows only the chosen layers, to see what is on them; ending the walk restores which
//! layers were on, unless asked to keep the walk's state.

use std::collections::{BTreeMap, HashSet};

use draftcraft_color::Color;
use serde_json::{Value, json};

use super::merge::{for_each_store, forget_in_viewports, on_layers, protected, xref_layer};
use crate::cmd::blocks::insert_file::read_drawing;
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Result, Session};

/// Most layer mappings one LAYTRANS call applies.
const MAX_MAPPINGS: usize = 10_000;

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("laytrans", "Layer Translator", run_laytrans).menu(&["Tools", "CAD Standards", "Layer Translator..."]).params(
            "{map: {FROM: TO, …} (layer names; FROM may use * and ? wildcards), standards?: DXF/DWG file whose layer definitions \
                 the TO layers take, forceByLayer?: bool (objects moved get ByLayer colour and linetype), purge?: bool (default true: \
                 delete FROM layers left empty)} → {translated: [{from, to, moved}], created: [names], purged: [names]}",
        ),
        CommandSpec::new("laywalk", "Layer Walk", run_laywalk)
            .menu(&["Format", "Layer Tools", "Layer Walk..."])
            .params(
                "{layers?: [names] (show only these; * and ? wildcards), operation?: \"end\" (stop walking), restore?: bool (with end, \
                 default true: turn the layers back on or off as they were)} → {walking: bool, shown: [names]}",
            )
            .noundo(),
    ]
}

/// `*` and `?` wildcard match, without letter case.
fn wild(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect());
    // Iterative match with backtracking to the last `*` (linear in practice; no recursion).
    let (mut pi, mut ni, mut star, mut mark) = (0usize, 0usize, None, 0usize);
    while ni < n.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some(pi);
                mark = ni;
                pi += 1;
            }
            Some(c) if *c == '?' || Some(c) == n.get(ni) => {
                pi += 1;
                ni += 1;
            }
            _ => match star {
                Some(sp) => {
                    pi = sp + 1;
                    mark += 1;
                    ni = mark;
                }
                None => return false,
            },
        }
    }
    p.get(pi..).is_some_and(|rest| rest.iter().all(|c| *c == '*'))
}

fn run_laytrans(s: &mut Session, p: &Value) -> Result<Value> {
    let map = p.get("map").and_then(Value::as_object).ok_or_else(|| bad("laytrans", "`map` gives each layer's new name: {\"OLD\": \"NEW\"}"))?;
    if map.len() > MAX_MAPPINGS {
        return Err(bad("laytrans", format!("at most {MAX_MAPPINGS} mappings")));
    }
    let standards = match str_param(p, "standards").filter(|x| !x.trim().is_empty()) {
        Some(path) => Some(read_drawing(path).map_err(|e| bad("laytrans", e.to_string()))?),
        None => None,
    };
    let force = p.get("forceByLayer").and_then(Value::as_bool).unwrap_or(false);
    let purge = p.get("purge").and_then(Value::as_bool).unwrap_or(true);
    let mut d = s.doc()?.clone();
    // Each existing layer's new name: the first mapping whose FROM matches it.
    let mut plan: BTreeMap<String, String> = BTreeMap::new();
    for (from, to) in map {
        let to =
            to.as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad("laytrans", format!("`{from}` needs a layer name to map to")))?;
        if xref_layer(to) {
            return Err(bad("laytrans", format!("cannot translate to xref-dependent layer \"{to}\"")));
        }
        let mut any = false;
        for l in &d.layers {
            if wild(from, &l.name) && !xref_layer(&l.name) && !l.name.eq_ignore_ascii_case(to) {
                plan.entry(l.name.clone()).or_insert_with(|| to.to_string());
                any = true;
            }
        }
        if !any && !from.contains(['*', '?']) && d.layer(from).is_none() {
            return Err(bad("laytrans", format!("no layer `{from}`")));
        }
    }
    let mut created = Vec::new();
    let mut translated = Vec::new();
    for (from, to) in &plan {
        if d.layer(to).is_none() {
            let layer = match standards.as_ref().and_then(|st| st.layer(to)) {
                Some(l) => l.clone(),
                None => {
                    let mut l = d.layer(from).cloned().unwrap_or_else(|| draftcraft_doc::Layer::new(to));
                    l.name = to.clone();
                    l
                }
            };
            if let Some(lt) = standards.as_ref().and_then(|st| st.linetypes.iter().find(|x| x.name.eq_ignore_ascii_case(&layer.linetype)))
                && !d.linetypes.iter().any(|x| x.name.eq_ignore_ascii_case(&lt.name))
            {
                d.linetypes.push(lt.clone());
            }
            created.push(layer.name.clone());
            d.layers.push(layer);
        }
        let target = d.layer(to).map(|l| l.name.clone()).unwrap_or_else(|| to.clone());
        let set: HashSet<String> = [from.to_ascii_lowercase()].into();
        let moved = for_each_store(&mut d, &set, |store| {
            let hs: Vec<_> = on_layers(store, &set).collect();
            for h in &hs {
                store.modify(*h, |e| {
                    e.common.layer = target.clone();
                    if force {
                        e.common.color = Color::ByLayer;
                        e.common.linetype = "ByLayer".into();
                    }
                });
            }
            hs.len()
        });
        translated.push(json!({ "from": from, "to": target, "moved": moved }));
    }
    let mut purged = Vec::new();
    if purge {
        let current = d.header.str("CLAYER", "0").to_ascii_lowercase();
        let gone: HashSet<String> =
            plan.keys().filter(|n| !protected(n) && n.to_ascii_lowercase() != current).map(|n| n.to_ascii_lowercase()).collect();
        forget_in_viewports(&mut d, &gone);
        d.layers.retain(|l| {
            let drop = gone.contains(&l.name.to_ascii_lowercase());
            if drop {
                purged.push(l.name.clone());
            }
            !drop
        });
    }
    let n = translated.len();
    *s.doc_mut()? = d;
    let message = format!("Translated {n} layer(s); created {}, purged {}.", created.len(), purged.len());
    Ok(json!({ "translated": translated, "created": created, "purged": purged, "message": message }))
}

fn run_laywalk(s: &mut Session, p: &Value) -> Result<Value> {
    if str_param(p, "operation").is_some_and(|o| o.eq_ignore_ascii_case("end")) {
        let restore = p.get("restore").and_then(Value::as_bool).unwrap_or(true);
        let saved = s.laywalk.take();
        if restore && let Some(saved) = saved {
            let d = s.doc_mut()?;
            for (name, on) in saved {
                if let Some(l) = d.layer_mut(&name) {
                    l.on = on;
                }
            }
        }
        return Ok(json!({ "walking": false, "shown": [], "message": "Layer walk ended." }));
    }
    let patterns: Vec<String> = match p.get("layers") {
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).take(MAX_MAPPINGS).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    if s.laywalk.is_none() {
        let saved = s.doc()?.layers.iter().map(|l| (l.name.clone(), l.on)).collect();
        s.laywalk = Some(saved);
    }
    let d = s.doc_mut()?;
    let mut shown = Vec::new();
    for l in &mut d.layers {
        // No layers chosen: everything shows, as when the walk starts.
        l.on = patterns.is_empty() || patterns.iter().any(|pat| wild(pat, &l.name));
        if l.on {
            shown.push(l.name.clone());
        }
    }
    let message = format!("Layer walk: {} layer(s) shown.", shown.len());
    Ok(json!({ "walking": true, "shown": shown, "message": message }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use draftcraft_doc::{Entity, EntityKind, Handle, Layer, Line};
    use draftcraft_geom::Vec3;

    fn session() -> Session {
        let mut s = Session::new();
        let d = s.doc_mut().unwrap();
        for n in ["WALLS-OLD", "DOORS-OLD", "TEXT"] {
            d.layers.push(Layer::new(n));
        }
        let mut e = Entity::new(Handle(0x900), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) }));
        e.common.layer = "WALLS-OLD".into();
        d.model.push(e);
        s
    }

    #[test]
    fn wildcards() {
        assert!(wild("*-old", "WALLS-OLD"));
        assert!(wild("W?LLS*", "walls-old"));
        assert!(!wild("*-new", "WALLS-OLD"));
        assert!(wild("*", ""));
        assert!(!wild("a", ""));
    }

    #[test]
    fn layers_are_translated_and_purged() {
        let mut s = session();
        let r = s.execute("laytrans", &json!({"map": {"WALLS-OLD": "A-WALL", "DOORS-*": "A-DOOR"}, "forceByLayer": true})).unwrap();
        assert_eq!(r["created"], json!(["A-DOOR", "A-WALL"]));
        let d = s.doc().unwrap();
        assert!(d.layer("WALLS-OLD").is_none() && d.layer("DOORS-OLD").is_none(), "{r}");
        assert!(d.model.iter().all(|e| e.common.layer == "A-WALL"));
        assert!(s.execute("laytrans", &json!({"map": {"NOPE": "X"}})).is_err());
        assert!(s.execute("laytrans", &json!({"map": {"TEXT": "x|y"}})).is_err(), "xref layers are not targets");
    }

    #[test]
    fn a_walk_shows_only_its_layers_then_restores() {
        let mut s = session();
        s.doc_mut().unwrap().layer_mut("TEXT").unwrap().on = false;
        let r = s.execute("laywalk", &json!({"layers": ["*-OLD"]})).unwrap();
        assert_eq!(r["shown"], json!(["WALLS-OLD", "DOORS-OLD"]));
        assert!(!s.doc().unwrap().layer("0").unwrap().on);
        s.execute("laywalk", &json!({"layers": "TEXT"})).unwrap();
        s.execute("laywalk", &json!({"operation": "end"})).unwrap();
        let d = s.doc().unwrap();
        assert!(d.layer("0").unwrap().on && !d.layer("TEXT").unwrap().on, "the layers are as they were before the walk");
    }
}
