//! ARC's "hold Ctrl to switch direction": the UI writes `Session::ctrl_down` every frame and
//! the command reads it. Two cases, and they flip differently on purpose:
//!
//! * Continue (the tangent comes from the previous curve): negating the start tangent makes
//!   `arc_sed` pick the mirror arc through the same two endpoints.
//! * Center,Start,End (the two picks fix the chord): swapping the start/end angles walks the
//!   other way around the same circle.
//!
//! Both should land on the same circle with its two halves swapped, so the assertions below
//! are the same: same center, same radius, and the start/end angles are exchanged.
//!
//! The other ARC variants AutoCAD lets Ctrl reverse are below `every_variant_…`: each one is
//! drawn with and without Ctrl, and the rubber band shown while Ctrl is held must be the arc
//! the click then creates.

use draftcraft_engine::doc::EntityKind;
use draftcraft_engine::geom::Vec2;
use draftcraft_engine::{Input, Session};

/// `(center, radius, start angle, end angle)` of the last arc in the model.
fn last_arc(s: &Session) -> (Vec2, f64, f64, f64) {
    match &s.doc().unwrap().model.last().unwrap().kind {
        EntityKind::Arc(a) => (a.center.xy(), a.radius, a.start, a.end),
        k => panic!("expected an arc, got {k:?}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// CCW sweep from `start` to `end`, both normalised to `[0, TAU)`.
fn sweep(start: f64, end: f64) -> f64 {
    (end - start).rem_euclid(std::f64::consts::TAU)
}

/// Draw a line from the origin to (10,0), then `arc` + Enter continues from its end at (10,0)
/// with the outgoing tangent +X. The end point is picked at (20,5).
fn run_continue(ctrl: bool) -> (Vec2, f64, f64, f64) {
    let mut s = Session::new();
    s.cmdline("line").unwrap();
    s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
    s.input(Input::Point(Vec2::new(10.0, 0.0))).unwrap();
    s.input(Input::Enter).unwrap();
    s.cmdline("arc").unwrap();
    // Enter at the first point prompt continues the last curve.
    s.input(Input::Enter).unwrap();
    assert!(s.prompt_text().contains("hold Ctrl"), "Continue prompt should advertise Ctrl: {}", s.prompt_text());
    s.ctrl_down = ctrl;
    s.input(Input::Point(Vec2::new(20.0, 5.0))).unwrap();
    last_arc(&s)
}

/// ARC Center,Start,End: center (0,0), start (5,0) at 0°, end (0,5) at 90°.
fn run_cse(ctrl: bool) -> (Vec2, f64, f64, f64) {
    let mut s = Session::new();
    s.cmdline("arc c").unwrap();
    s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
    s.input(Input::Point(Vec2::new(5.0, 0.0))).unwrap();
    assert!(s.prompt_text().contains("hold Ctrl"), "Cse end-point prompt should advertise Ctrl: {}", s.prompt_text());
    s.ctrl_down = ctrl;
    s.input(Input::Point(Vec2::new(0.0, 5.0))).unwrap();
    last_arc(&s)
}

#[test]
fn continue_ctrl_flips_the_arc_through_the_same_endpoints() {
    let plain = run_continue(false);
    let flipped = run_continue(true);
    assert!(plain.0.dist(flipped.0) < 1e-9, "center moved: {:?} vs {:?}", plain.0, flipped.0);
    assert!(near(plain.1, flipped.1), "radius changed: {} vs {}", plain.1, flipped.1);
    assert!(
        near(plain.2, flipped.3) && near(plain.3, flipped.2),
        "Ctrl must swap start/end angles: plain ({}, {}) vs flipped ({}, {})",
        plain.2,
        plain.3,
        flipped.2,
        flipped.3
    );
    let total = sweep(plain.2, plain.3) + sweep(flipped.2, flipped.3);
    assert!(
        near(total, std::f64::consts::TAU),
        "the two arcs must be the two halves of the circle: {} + {} != TAU",
        sweep(plain.2, plain.3),
        sweep(flipped.2, flipped.3)
    );
}

#[test]
fn cse_ctrl_reverses_by_swapping_the_endpoint_angles() {
    let plain = run_cse(false);
    let flipped = run_cse(true);
    assert!(plain.0.dist(flipped.0) < 1e-9, "center moved: {:?} vs {:?}", plain.0, flipped.0);
    assert!(near(plain.1, flipped.1), "radius changed: {} vs {}", plain.1, flipped.1);
    assert!(
        near(plain.2, flipped.3) && near(plain.3, flipped.2),
        "Ctrl must swap start/end angles: plain ({}, {}) vs flipped ({}, {})",
        plain.2,
        plain.3,
        flipped.2,
        flipped.3
    );
    let total = sweep(plain.2, plain.3) + sweep(flipped.2, flipped.3);
    assert!(near(total, std::f64::consts::TAU), "sum of sweeps must be TAU");
}

/// Sanity: without Ctrl the Continue arc uses the tangent as-is (center above the chord).
#[test]
fn no_ctrl_is_the_old_behavior() {
    let plain = run_continue(false);
    assert!(plain.0.y > 0.0, "tangent as-is puts the center above the chord: {:?}", plain.0);
}

// ---------------- every ARC variant whose direction Ctrl switches ----------------

/// Same angle modulo a full turn.
fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(std::f64::consts::TAU);
    d < 1e-9 || d > std::f64::consts::TAU - 1e-9
}

/// Types each input at the command line, holding Ctrl (or not) for the last one, and returns
/// the arc it made plus, when the last input is a point, the arc previewed at that point first.
fn draw(inputs: &[&str], ctrl: bool) -> ((Vec2, f64, f64, f64), Option<(Vec2, f64, f64, f64)>) {
    let mut s = Session::new();
    s.execute("setvar", &serde_json::json!({ "name": "OSMODE", "value": 0 })).unwrap();
    let (last, first) = inputs.split_last().unwrap();
    for t in first {
        s.cmdline(t).unwrap();
    }
    assert!(s.prompt_text().contains("hold Ctrl"), "{inputs:?}: the last prompt should advertise Ctrl: {}", s.prompt_text());
    s.ctrl_down = ctrl;
    let previewed = last.split_once(',').map(|(x, y)| {
        let c = Vec2::new(x.parse().unwrap(), y.parse().unwrap());
        let arcs: Vec<_> = s
            .preview(c)
            .into_iter()
            .filter_map(|e| match e.kind {
                EntityKind::Arc(a) => Some((a.center.xy(), a.radius, a.start, a.end)),
                _ => None,
            })
            .collect();
        assert_eq!(arcs.len(), 1, "{inputs:?}: one arc in the rubber band");
        arcs[0]
    });
    s.cmdline(last).unwrap();
    assert!(s.current_prompt().is_none(), "{inputs:?}: ARC should be done");
    (last_arc(&s), previewed)
}

fn same_arc(a: (Vec2, f64, f64, f64), b: (Vec2, f64, f64, f64)) -> bool {
    a.0.dist(b.0) < 1e-9 && near(a.1, b.1) && same_angle(a.2, b.2) && same_angle(a.3, b.3)
}

/// How a Ctrl arc relates to the plain one.
enum Flip {
    /// Same circle, other way round: start and end angles swap.
    Swap,
    /// Same sweep, clockwise from the same start point.
    ClockwiseFromStart,
    /// Same two endpoints and radius, the center mirrored across the chord.
    Mirror(Vec2),
}

#[test]
fn every_variant_ctrl_switches_direction_in_preview_and_result() {
    // Start (5,0), center (0,0) and the point (0,5) a quarter turn counter-clockwise.
    let cases: &[(&str, &[&str], Flip)] = &[
        ("start, center, end", &["arc", "5,0", "c", "0,0", "0,5"], Flip::Swap),
        ("start, end, center", &["arc", "5,0", "e", "0,5", "0,0"], Flip::Swap),
        ("center, start, end", &["arc", "c", "0,0", "5,0", "0,5"], Flip::Swap),
        ("start, center, picked angle", &["arc", "5,0", "c", "0,0", "a", "0,5"], Flip::Swap),
        ("center, start, picked angle", &["arc", "c", "0,0", "5,0", "a", "0,5"], Flip::Swap),
        ("start, center, typed angle", &["arc", "5,0", "c", "0,0", "a", "90"], Flip::ClockwiseFromStart),
        ("center, start, typed angle", &["arc", "c", "0,0", "5,0", "a", "90"], Flip::ClockwiseFromStart),
        ("start, center, chord length", &["arc", "5,0", "c", "0,0", "l", "5"], Flip::ClockwiseFromStart),
        ("center, start, chord length", &["arc", "c", "0,0", "5,0", "l", "5"], Flip::ClockwiseFromStart),
        ("start, end, angle", &["arc", "5,0", "e", "0,5", "a", "90"], Flip::Mirror(Vec2::new(5.0, 5.0))),
        ("start, end, radius", &["arc", "5,0", "e", "0,5", "r", "5"], Flip::Mirror(Vec2::new(5.0, 5.0))),
    ];
    for (name, inputs, flip) in cases {
        let (plain, plain_preview) = draw(inputs, false);
        let (flipped, flipped_preview) = draw(inputs, true);
        for (arc, preview) in [(plain, plain_preview), (flipped, flipped_preview)] {
            if let Some(p) = preview {
                assert!(same_arc(arc, p), "{name}: the rubber band {p:?} is not the arc drawn {arc:?}");
            }
        }
        assert!(plain.0.dist(Vec2::ZERO) < 1e-9 && near(plain.1, 5.0), "{name}: plain arc {plain:?}");
        assert!(same_angle(plain.2, 0.0), "{name}: plain arc starts at the start point: {plain:?}");
        assert!(!same_arc(plain, flipped), "{name}: Ctrl changed nothing: {plain:?}");
        match flip {
            Flip::Swap => assert!(
                same_arc(flipped, (plain.0, plain.1, plain.3, plain.2)),
                "{name}: Ctrl must walk the other way round the same circle: {plain:?} vs {flipped:?}"
            ),
            Flip::ClockwiseFromStart => {
                let sw = sweep(plain.2, plain.3);
                assert!(
                    same_arc(flipped, (plain.0, plain.1, plain.2 - sw, plain.2)),
                    "{name}: Ctrl must sweep the same angle clockwise from the start: {plain:?} vs {flipped:?}"
                );
            }
            Flip::Mirror(c) => {
                assert!(flipped.0.dist(*c) < 1e-9 && near(flipped.1, plain.1), "{name}: mirrored center expected: {flipped:?}");
                // Counter-clockwise from the end point (0,5) back to the start point (5,0).
                let end = Vec2::new(0.0, 5.0) - *c;
                let start = Vec2::new(5.0, 0.0) - *c;
                assert!(same_angle(flipped.2, end.y.atan2(end.x)) && same_angle(flipped.3, start.y.atan2(start.x)), "{name}: {flipped:?}");
            }
        }
    }
}

/// ARC's three-point form and the tangent direction variant have no Ctrl in AutoCAD; holding it
/// changes nothing there.
#[test]
fn ctrl_leaves_three_point_and_direction_arcs_alone() {
    for inputs in [&["arc", "5,0", "3.5355339,3.5355339", "0,5"][..], &["arc", "5,0", "e", "0,5", "d", "90"][..]] {
        let mut a = Session::new();
        let mut b = Session::new();
        b.ctrl_down = true;
        for t in inputs {
            a.cmdline(t).unwrap();
            b.cmdline(t).unwrap();
        }
        assert!(same_arc(last_arc(&a), last_arc(&b)), "{inputs:?}");
    }
}
