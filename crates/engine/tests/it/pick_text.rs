//! Picking (and the hover highlight, which picks the same way) finds dimensions and multileaders
//! anywhere on their words, as plain text is found anywhere inside its box.

use draftcraft_engine::Session;
use draftcraft_engine::doc::{EntityKind, Handle, Space};
use draftcraft_engine::geom::{Bounds2, Vec2};
use draftcraft_engine::render;
use draftcraft_engine::select::pick;
use serde_json::json;

/// The last entity of `kind` and the box (centre, half size along the text, angle) of its words.
fn text_of(s: &Session, kind: &str) -> (Handle, Vec2, Vec2, f64) {
    let d = s.doc().unwrap();
    let e = d.model.iter().filter(|e| format!("{:?}", e.kind).starts_with(kind)).last().unwrap();
    let (pts, angle): (Vec<Vec2>, f64) = match &e.kind {
        EntityKind::Dimension(dm) => {
            let g = render::dimension_in(d, dm);
            (g.text.iter().flatten().chain(g.text_glyphs.iter().flatten().flatten()).copied().collect(), g.text_angle)
        }
        EntityKind::MLeader(m) => {
            let t = m.text.as_ref().unwrap();
            (render::layout_mtext_entity(d, t).strokes.into_iter().flatten().collect(), t.rotation)
        }
        _ => unreachable!(),
    };
    let b = Bounds2::from_points(pts.iter().map(|q| q.rotate(-angle)));
    (e.handle, b.center().rotate(angle), Vec2::new(b.width() / 2.0, b.height() / 2.0), angle)
}

/// A point `(u, v)` half-sizes from the text box's centre, along and across the text.
fn at(c: Vec2, half: Vec2, angle: f64, u: f64, v: f64) -> Vec2 {
    c + Vec2::new(half.x * u, half.y * v).rotate(angle)
}

#[test]
fn dimension_and_multileader_text_is_picked_anywhere_on_its_words() {
    let mut s = Session::new();
    s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [100, 0], "at": [50, 30], "text": "TWO WORDS HERE"})).unwrap();
    let (h, c, half, angle) = text_of(&s, "Dimension");
    let tiny = half.y * 0.01;
    let d = s.doc().unwrap();
    // Between letters, at the ends and the corners of the words: all the dimension.
    for (u, v) in [(0.0, 0.0), (0.33, 0.1), (-0.9, -0.8), (0.9, 0.8), (0.5, -0.5)] {
        assert_eq!(pick(d, &Space::Model, at(c, half, angle, u, v), tiny), Some(h), "({u}, {v})");
    }
    assert_eq!(pick(d, &Space::Model, at(c, half, angle, 0.0, 6.0), tiny), None, "well above the words: nothing");

    s.execute("mleader", &json!({"points": [[200, 0], [220, 20]], "text": "LEADER NOTE"})).unwrap();
    let (h, c, half, angle) = text_of(&s, "MLeader");
    let d = s.doc().unwrap();
    for (u, v) in [(0.0, 0.0), (0.4, 0.2), (-0.8, -0.6)] {
        assert_eq!(pick(d, &Space::Model, at(c, half, angle, u, v), half.y * 0.01), Some(h), "leader text ({u}, {v})");
    }
}

#[test]
fn slanted_dimension_text_is_picked_inside_its_turned_box_only() {
    let mut s = Session::new();
    // Text along the dimension line (not kept horizontal), as in most surveying styles.
    for name in ["DIMTIH", "DIMTOH"] {
        s.execute("setvar", &json!({"name": name, "value": 0})).unwrap();
    }
    s.execute("dimaligned", &json!({"p1": [0, 0], "p2": [100, 100], "at": [40, 60], "text": "SLANTED LABEL"})).unwrap();
    let (h, c, half, angle) = text_of(&s, "Dimension");
    assert!((angle - std::f64::consts::FRAC_PI_4).abs() < 1e-6, "aligned with the 45° line: {angle}");
    let d = s.doc().unwrap();
    let tiny = half.y * 0.01;
    assert_eq!(pick(d, &Space::Model, at(c, half, angle, 0.6, 0.0), tiny), Some(h));
    // Inside the axis-aligned box around the turned words, but off them (and off the lines).
    let off = at(c, half, angle, 0.3, 4.0);
    let aabb = Bounds2::from_points([
        at(c, half, angle, -1.0, -1.0),
        at(c, half, angle, 1.0, -1.0),
        at(c, half, angle, 1.0, 1.0),
        at(c, half, angle, -1.0, 1.0),
    ]);
    assert!(aabb.contains(off), "the probe sits in the loose box");
    assert_ne!(pick(d, &Space::Model, off, tiny), Some(h), "but not on the words");
}

#[test]
fn a_dimension_drawn_from_its_block_is_picked_on_the_block_text() {
    // As AutoCAD saves dimensions: the geometry in an anonymous block, here with the text moved
    // well away from where the dimension's own geometry would set it.
    let mut s = Session::new();
    s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [100, 0], "at": [50, 30]})).unwrap();
    s.execute("mtext", &json!({"at": [400, 400], "text": "MOVED TEXT", "height": 5})).unwrap();
    let d = s.doc_mut().unwrap();
    let text = d.model.iter().find(|e| matches!(e.kind, EntityKind::MText(_))).unwrap().as_ref().clone();
    let dim = d.model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    d.model.remove(text.handle);
    let mut entities = draftcraft_engine::doc::EntityStore::new();
    entities.push(text);
    let block = draftcraft_engine::doc::Block {
        xref_overlay: false,
        xref_status: Default::default(),
        name: "*D1".into(),
        annotative: false,
        base: Default::default(),
        entities,
        description: String::new(),
        anonymous: true,
        xref_path: None,
        explodable: true,
        units: 0,
        array: None,
    };
    d.blocks.insert("*D1".into(), std::sync::Arc::new(block));
    d.model.modify(dim, |e| {
        if let EntityKind::Dimension(dm) = &mut e.kind {
            dm.block = Some("*D1".into());
        }
    });
    let d = s.doc().unwrap();
    let m = render::layout_mtext_entity(
        d,
        match &d.block("*D1").unwrap().entities.iter().next().unwrap().kind {
            EntityKind::MText(t) => t,
            _ => unreachable!(),
        },
    );
    let mid = m.bounds.center();
    assert_eq!(pick(d, &Space::Model, mid, 0.01), Some(dim), "on the block's text");
    assert_eq!(pick(d, &Space::Model, Vec2::new(50.0, 33.0), 0.01), None, "not where the dimension would have set it");
}

/// The case reported from the field: text turned 40° set just above a diagonal line. The text's
/// axis-aligned bounds cover the line and a wide empty corner below it; picks there must find the
/// line (or nothing), not the text, at any pickbox size. Text is hit inside its own turned box.
#[test]
fn turned_text_is_picked_inside_its_turned_box_not_its_bounds() {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [100, 84]]})).unwrap();
    for (kind, params) in [
        ("text", json!({"at": [20, 22], "height": 5, "text": "dlocd", "rotation": 40})),
        ("mtext", json!({"at": [20, 22], "height": 5, "text": "dlocd", "rotation": 40, "justify": "BL"})),
    ] {
        s.execute(kind, &params).unwrap();
        let d = s.doc().unwrap();
        let line = d.model.iter().find(|e| matches!(e.kind, EntityKind::Line(_))).unwrap().handle;
        let text = d.model.iter().last().unwrap().handle;
        let a = 40f64.to_radians();
        let along = |u: f64, v: f64| Vec2::new(20.0, 22.0) + Vec2::new(u, v).rotate(a);
        for ap in [0.05, 0.5, 1.5] {
            // On the letters, in the middle of the word: the text.
            assert_eq!(pick(d, &Space::Model, along(10.0, 2.5), ap), Some(text), "{kind} on its words (aperture {ap})");
            // On the line where it runs under the text's bounds: the line, not the text.
            let on_line = Vec2::new(30.0, 25.2);
            assert!(draftcraft_engine::doc::entity_bounds(d, d.entity(text).unwrap(), 0).contains(on_line), "{kind}: probe in the loose bounds");
            assert_eq!(pick(d, &Space::Model, on_line, ap), Some(line), "{kind} the line under the text's bounds (aperture {ap})");
            // The empty bottom-right corner of the text's bounds: nothing.
            assert!(draftcraft_engine::doc::entity_bounds(d, d.entity(text).unwrap(), 0).contains(Vec2::new(35.0, 23.0)));
            assert_eq!(pick(d, &Space::Model, Vec2::new(35.0, 23.0), ap), None, "{kind} the empty corner (aperture {ap})");
        }
        s.execute("erase", &json!({"handles": [text.0]})).unwrap();
    }
}

/// Text is hit around its letters, not around an estimate of the full font height and a fixed
/// width per character: above lower-case letters and past the end of the word is off the text
/// (a line drawn there is what the cursor finds).
#[test]
fn text_is_picked_around_its_letters_not_a_full_height_estimate() {
    let mut s = Session::new();
    for rotation in [0.0, 40.0] {
        s.execute("text", &json!({"at": [0, 0], "height": 5, "text": "zoom e", "rotation": rotation})).unwrap();
        let d = s.doc().unwrap();
        let text = d.model.iter().last().unwrap().handle;
        let a = f64::to_radians(rotation);
        let at = |u: f64, v: f64| Vec2::new(u, v).rotate(a);
        // On the letters.
        assert_eq!(pick(d, &Space::Model, at(3.0, 1.5), 0.05), Some(text), "on 'z' at {rotation}°");
        // Inside the old estimate (5 high), above the lower-case letters.
        assert_eq!(pick(d, &Space::Model, at(8.0, 4.7), 0.05), None, "above the letters at {rotation}°");
        s.execute("erase", &json!({"handles": [text.0]})).unwrap();
    }
}

/// The case reported from the field: a dimension was found with the cursor well away from its
/// lines and not with the cursor near them. It was hit on the outline of its axis-aligned bounds,
/// whose edges run through empty space (through the defining points). Now it is hit on what it
/// draws and inside the box around its words, by picks and by crossing windows alike.
#[test]
fn a_dimension_is_picked_on_its_lines_not_its_bounds_outline() {
    let mut s = Session::new();
    // Defining points well apart from the dimension line, as on a drawn part.
    s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [100, 0], "at": [50, 30]})).unwrap();
    s.execute("dimlinear", &json!({"p1": [200, 0], "p2": [200, 80], "at": [170, 40]})).unwrap();
    let d = s.doc().unwrap();
    let dims: Vec<_> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Dimension(_))).collect();
    let (h, v) = (dims[0].handle, dims[1].handle);
    let ap = 0.5;
    let bounds = |dim| draftcraft_engine::doc::entity_bounds(d, d.entity(dim).unwrap(), 0);
    let (bh, bv) = (bounds(h), bounds(v));
    for (dim, probes_off, probes_on) in [
        // Horizontal: the bounds' bottom edge runs by the defining points, between them; the
        // extension lines and the dimension line are hit.
        (h, [Vec2::new(50.0, bh.min.y), Vec2::new(30.0, bh.min.y)], [Vec2::new(0.2, 15.0), Vec2::new(70.0, 30.2)]),
        // Vertical (as the 5.00 one at the left of the sample): the bounds' right edge.
        (v, [Vec2::new(bv.max.x, 20.0), Vec2::new(bv.max.x, 60.0)], [Vec2::new(185.0, 0.2), Vec2::new(169.8, 20.0)]),
    ] {
        for p in probes_off {
            assert_eq!(pick(d, &Space::Model, p, ap), None, "empty space on the bounds' outline at {p:?}");
            // A crossing window touching only that outline doesn't take it either.
            let bx = Bounds2::new(p - Vec2::new(1.0, 1.0), p + Vec2::new(1.0, 1.0));
            assert!(!draftcraft_engine::select::select_window(d, &Space::Model, bx, true).contains(&dim), "crossing at {p:?}");
        }
        for p in probes_on {
            assert_eq!(pick(d, &Space::Model, p, ap), Some(dim), "near its lines at {p:?}");
        }
    }
    // Its words are still hit anywhere inside their box, and a crossing window across the edge
    // of that box (touching no line) takes it.
    let d = s.doc().unwrap();
    let words = |dim| {
        let e = d.entity(dim).unwrap();
        let EntityKind::Dimension(dm) = &e.kind else { unreachable!() };
        let g = render::dimension_in(d, dm);
        Bounds2::from_points(g.text.iter().flatten().chain(g.text_glyphs.iter().flatten().flatten()).copied())
    };
    // Level words: their box is axis-aligned.
    let tb = words(h);
    assert_eq!(pick(d, &Space::Model, tb.center(), 0.01), Some(h), "between the letters");
    let mid_y = tb.center().y;
    let bx = Bounds2::new(Vec2::new(tb.min.x - 0.5, mid_y - 0.05), Vec2::new(tb.min.x + 0.5, mid_y + 0.05));
    assert_eq!(draftcraft_engine::select::select_window(d, &Space::Model, bx, true), vec![h], "crossing the words' box");
}
