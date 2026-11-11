//! Picking against what is drawn, for every kind of object: a click on what an object draws
//! finds it, and a click well away from anything it draws (outside its fills and the boxes of its
//! words) finds nothing. Hit testing that runs along anything else (the outline of the object's
//! bounds, an estimate of its text, a stale shape) shows up here as a pick in empty space.

use draftcraft_engine::Session;
use draftcraft_engine::doc::{Handle, Space};
use draftcraft_engine::geom::{Bounds2, Vec2};
use draftcraft_engine::render::{self, Kind};
use draftcraft_engine::select::pick;
use serde_json::{Value, json};

/// The pick aperture (drawing units) the audit clicks with; the objects are tens of units across.
const AP: f64 = 0.25;

fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).len()
}

fn in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let s = |p1: Vec2, p2: Vec2, p3: Vec2| (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
    let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// What `h` draws: its strokes (lines, curves, point markers, infinite lines as long segments),
/// its filled triangles, and the boxes around the strokes of its words.
struct Drawn {
    strokes: Vec<Vec<Vec2>>,
    tris: Vec<[Vec2; 3]>,
    /// Filled areas picked on their frame only (a wipeout), not inside.
    masks: Vec<[Vec2; 3]>,
    words: Option<Bounds2>,
}

fn drawn(s: &Session, h: Handle) -> Drawn {
    let d = s.doc().unwrap();
    let e = d.entity(h).unwrap();
    let build = |text: bool| {
        render::build_entities(d, std::iter::once(e.as_ref()), &render::Options { tolerance: 0.005, text, linetypes: false, ..Default::default() })
    };
    let (full, bare) = (build(true), build(false));
    let key = |q: Vec2| (q.x.to_bits(), q.y.to_bits());
    let bare_pts: std::collections::HashSet<_> = bare.prims.iter().flat_map(|p| bare.points(p).iter().map(|q| key(*q))).collect();
    let mut out = Drawn { strokes: Vec::new(), tris: Vec::new(), masks: Vec::new(), words: None };
    for p in &full.prims {
        let pts = full.points(p);
        let text = !pts.iter().all(|q| bare_pts.contains(&key(*q)));
        match p.kind {
            Kind::Tris | Kind::Mask => {
                let to = if matches!(p.kind, Kind::Mask) { &mut out.masks } else { &mut out.tris };
                to.extend(pts.as_chunks::<3>().0.iter().copied());
            }
            Kind::Infinite { ray } => {
                let (b, dir) = (pts[0], pts[1]);
                out.strokes.push(vec![if ray { b } else { b - dir * 1e4 }, b + dir * 1e4]);
            }
            Kind::Point => out.strokes.push(vec![pts[0], pts[0]]),
            _ => out.strokes.push(pts.to_vec()),
        }
        if text {
            let b = Bounds2::from_points(pts.iter().copied());
            out.words = Some(out.words.map_or(b, |w| w.union(&b)));
        }
    }
    out
}

/// Where picking `h` disagrees with what it draws: on-stroke clicks that miss it, and clicks in
/// empty space that find it.
fn audit(s: &Session, h: Handle) -> Vec<String> {
    let d = s.doc().unwrap();
    let dr = drawn(s, h);
    let mut bad = Vec::new();
    // On what it draws: a sample of segment midpoints and triangle centres.
    let mut on: Vec<Vec2> = Vec::new();
    for st in &dr.strokes {
        if st.len() == 2 && (st[1] - st[0]).len() > 1e3 {
            on.push(st[0] + (st[1] - st[0]) * 0.5);
            continue;
        }
        for w in st.windows(2) {
            on.push(w[0] + (w[1] - w[0]) * 0.5);
        }
        if st.len() == 2 && st[0] == st[1] {
            on.push(st[0]);
        }
    }
    on.extend(dr.tris.iter().filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).abs() > 1e-6).map(|t| (t[0] + t[1] + t[2]) * (1.0 / 3.0)));
    let step = (on.len() / 150).max(1);
    for p in on.iter().step_by(step) {
        if pick(d, &Space::Model, *p, AP) != Some(h) {
            bad.push(format!("misses its own drawing at ({:.2}, {:.2})", p.x, p.y));
        }
    }
    // Away from it: a grid over what it draws (infinite lines left out), each point farther
    // than 4 apertures from every stroke and outside its fills and words.
    let finite: Vec<Vec2> = dr.strokes.iter().filter(|st| !(st.len() == 2 && (st[1] - st[0]).len() > 1e3)).flatten().copied().collect();
    let b = Bounds2::from_points(finite.iter().copied().chain(dr.tris.iter().chain(&dr.masks).flatten().copied()));
    let b = if b.is_empty() { Bounds2::new(Vec2::new(-20.0, -20.0), Vec2::new(20.0, 20.0)) } else { b.expand(5.0) };
    let n = 50;
    for i in 0..=n {
        for j in 0..=n {
            let p = Vec2::new(b.min.x + b.width() * f64::from(i) / f64::from(n), b.min.y + b.height() * f64::from(j) / f64::from(n));
            let near = dr.strokes.iter().any(|st| {
                if st.len() == 1 || (st.len() == 2 && st[0] == st[1]) {
                    return (st[0] - p).len() < 4.0 * AP;
                }
                st.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) < 4.0 * AP)
            });
            // In a fill, or within 4 apertures of its edge.
            let filled = dr
                .tris
                .iter()
                .chain(&dr.masks)
                .any(|t| in_triangle(p, t[0], t[1], t[2]) || (0..3).any(|k| dist_to_segment(p, t[k], t[(k + 1) % 3]) < 4.0 * AP));
            let worded = dr.words.is_some_and(|w| w.expand(4.0 * AP).contains(p));
            if !near && !filled && !worded && pick(d, &Space::Model, p, AP) == Some(h) {
                bad.push(format!("found in empty space at ({:.2}, {:.2})", p.x, p.y));
            }
        }
    }
    bad
}

/// A drawing with one object made by `cmd` (`helpers` first, erased after), and that object.
fn one(helpers: &[(&str, Value)], cmd: &str, params: Value) -> (Session, Handle) {
    let mut s = Session::new();
    for (c, p) in helpers {
        s.execute(c, p).unwrap();
    }
    let before: Vec<Handle> = s.doc().unwrap().model.iter().map(|e| e.handle).collect();
    s.execute(cmd, &params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    let h = s.doc().unwrap().model.iter().map(|e| e.handle).find(|h| !before.contains(h)).unwrap_or_else(|| panic!("{cmd} drew nothing"));
    if !before.is_empty() {
        let hexes: Vec<String> = before.iter().map(|h| h.hex()).collect();
        s.execute("erase", &json!({ "handles": hexes })).unwrap();
    }
    (s, h)
}

fn cases() -> Vec<(&'static str, Session, Handle)> {
    let rect = || ("rectang", json!({"p1": [0, 0], "p2": [40, 20]}));
    let circ = || ("circle", json!({"center": [0, 0], "radius": 20}));
    let mut v = Vec::new();
    let mut add = |name: &'static str, (s, h): (Session, Handle)| v.push((name, s, h));
    add("line", one(&[], "line", json!({"points": [[0, 0], [50, 20]]})));
    add("circle", one(&[], "circle", json!({"center": [0, 0], "radius": 20})));
    add("arc", one(&[], "arc", json!({"center": [0, 0], "radius": 20, "start": 30, "end": 200})));
    add("ellipse", one(&[], "ellipse", json!({"center": [0, 0], "major": [30, 0], "ratio": 0.4})));
    add("elliptical arc", one(&[], "ellipse", json!({"center": [0, 0], "major": [30, 0], "ratio": 0.4, "start": 0, "end": 210})));
    add("polyline with arcs", one(&[], "pline", json!({"vertices": [{"p": [0, 0]}, {"p": [30, 0], "bulge": 0.6}, {"p": [30, 20]}, {"p": [0, 25]}]})));
    add("wide polyline", one(&[], "pline", json!({"vertices": [[0, 0], [30, 0], [30, 20]], "width": 2})));
    add("tapered polyline", one(&[], "pline", json!({"vertices": [{"p": [0, 0], "startWidth": 0, "endWidth": 4}, {"p": [30, 0]}]})));
    add("rectangle", one(&[], "rectang", json!({"p1": [0, 0], "p2": [40, 20]})));
    add("polygon", one(&[], "polygon", json!({"sides": 6, "center": [0, 0], "radius": 20})));
    add("donut", one(&[], "donut", json!({"center": [0, 0], "inside": 10, "outside": 14})));
    add("revision cloud", one(&[], "revcloud", json!({"p1": [0, 0], "p2": [40, 20]})));
    add("spline", one(&[], "spline", json!({"fit": [[0, 0], [10, 15], [25, -5], [40, 10]]})));
    add("closed spline", one(&[], "spline", json!({"fit": [[0, 0], [20, 15], [40, 0], [20, -10]], "closed": true})));
    add("point", one(&[], "point", json!({"at": [5, 5]})));
    add("xline", one(&[], "xline", json!({"base": [0, 0], "through": [10, 5]})));
    add("ray", one(&[], "ray", json!({"base": [0, 0], "through": [10, 5]})));
    add("2D solid", one(&[], "solid", json!({"points": [[0, 0], [20, 0], [0, 20], [20, 20]]})));
    add("triangular solid", one(&[], "solid", json!({"points": [[0, 0], [30, 0], [0, 30]]})));
    add("text", one(&[], "text", json!({"at": [0, 0], "height": 5, "text": "HELLO WORLD"})));
    add("turned text", one(&[], "text", json!({"at": [0, 0], "height": 5, "text": "TURNED", "rotation": 35})));
    add("mtext", one(&[], "mtext", json!({"at": [0, 20], "height": 4, "text": "LINE ONE\\PLINE TWO"})));
    add("linear dimension", one(&[], "dimlinear", json!({"p1": [0, 0], "p2": [60, 0], "at": [30, 20]})));
    add("vertical dimension", one(&[], "dimlinear", json!({"p1": [0, 0], "p2": [0, 60], "at": [-20, 30]})));
    add("aligned dimension", one(&[], "dimaligned", json!({"p1": [0, 0], "p2": [40, 30], "at": [10, 30]})));
    add("radius dimension", one(&[circ()], "dimradius", json!({"center": [0, 0], "point": [14.14, 14.14]})));
    add("diameter dimension", one(&[circ()], "dimdiameter", json!({"center": [0, 0], "point": [14.14, 14.14]})));
    add("angular dimension", one(&[], "dimangular", json!({"vertex": [0, 0], "p1": [30, 0], "p2": [0, 30], "at": [20, 20]})));
    add("ordinate dimension", one(&[], "dimordinate", json!({"feature": [10, 10], "leader": [30, 30]})));
    add("leader", one(&[], "leader", json!({"points": [[0, 0], [20, 20], [30, 20]], "text": "NOTE"})));
    add("multileader", one(&[], "mleader", json!({"points": [[0, 0], [20, 20]], "text": "NOTE"})));
    add("pattern hatch", one(&[rect()], "hatch", json!({"points": [[20, 10]], "pattern": "ANSI31", "scale": 4, "associative": false})));
    add("solid hatch", one(&[rect()], "hatch", json!({"points": [[20, 10]], "pattern": "SOLID", "associative": false})));
    add("wipeout", one(&[], "wipeout", json!({"points": [[0, 0], [30, 0], [30, 20], [0, 20]]})));
    add("multiline", one(&[], "mline", json!({"points": [[0, 0], [40, 0], [40, 30]], "scale": 4})));
    add("table", one(&[], "table", json!({"at": [0, 0], "rows": 2, "cols": 2})));
    v
}

#[test]
fn every_kind_of_object_is_picked_on_what_it_draws_and_nowhere_else() {
    let mut report = Vec::new();
    for (name, mut s, h) in cases() {
        // Alone (every object tested), then among enough others that picking goes through the
        // spatial index, which finds candidates by their boxes.
        for indexed in [false, true] {
            if indexed {
                let far: Vec<Value> = (0..400).map(|i| json!([10_000 + i * 3, 10_000])).collect();
                for p in far {
                    s.execute("point", &json!({ "at": p })).unwrap();
                }
            }
            let bad = audit(&s, h);
            if !bad.is_empty() {
                let how = if indexed { "indexed" } else { "alone" };
                report.push(format!(
                    "{name} ({how}): {} wrong picks, e.g. {}",
                    bad.len(),
                    bad.iter().take(3).cloned().collect::<Vec<_>>().join("; ")
                ));
            }
        }
    }
    assert!(report.is_empty(), "\n{}", report.join("\n"));
}
