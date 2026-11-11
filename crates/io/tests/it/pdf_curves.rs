//! PDF plots keep curves: arcs, circles, ellipses, splines and bulged polylines are written as
//! Bézier paths (not hundreds of short lines), and PDFIMPORT reads circular ones back as arcs,
//! circles and polylines with arc segments, so a DraftCraft PDF comes back as editable geometry.

use draftcraft_doc::{Arc, Circle, Common, Drawing, Ellipse, EntityKind, Line, LwPolyline, Space};
use draftcraft_geom::{PolyVertex, Spline, Vec2, Vec3};
use draftcraft_io::{pdf_read, plotter};
use serde_json::json;

fn add(d: &mut Drawing, k: EntityKind) {
    d.add(&Space::Model, Common::default(), k).unwrap();
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    add(&mut d, EntityKind::Circle(Circle { center: Vec3::new(50.0, 50.0, 0.0), radius: 20.0 }));
    add(&mut d, EntityKind::Arc(Arc { center: Vec3::new(120.0, 50.0, 0.0), radius: 15.0, start: 0.2, end: 2.6 }));
    add(&mut d, EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(200.0, 0.0, 0.0) }));
    let v = |x: f64, y: f64, bulge: f64| PolyVertex { bulge, ..PolyVertex::new(Vec2::new(x, y)) };
    let slot = LwPolyline {
        vertices: vec![v(20.0, 100.0, 0.0), v(80.0, 100.0, 1.0), v(80.0, 120.0, 0.0), v(20.0, 120.0, 1.0)],
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    add(&mut d, EntityKind::LwPolyline(slot));
    add(
        &mut d,
        EntityKind::Ellipse(Ellipse {
            center: Vec3::new(160.0, 110.0, 0.0),
            major: Vec3::new(25.0, 0.0, 0.0),
            ratio: 0.5,
            start: 0.0,
            end: std::f64::consts::TAU,
        }),
    );
    let spline = Spline::from_control(
        vec![Vec2::new(100.0, 140.0), Vec2::new(120.0, 170.0), Vec2::new(150.0, 170.0), Vec2::new(170.0, 140.0), Vec2::new(190.0, 150.0)],
        3,
    );
    add(&mut d, EntityKind::Spline(spline));
    d
}

fn plot(d: &Drawing) -> Vec<u8> {
    plotter::plot(d, &Space::Model, &json!({"compress": false})).unwrap()
}

/// How many times the page content uses path operator `op` (`m`, `l`, `c`).
fn ops(bytes: &[u8], op: &str) -> usize {
    String::from_utf8_lossy(bytes).split_whitespace().filter(|t| *t == op).count()
}

#[test]
fn curves_are_written_as_bezier_curves_not_short_lines() {
    let bytes = plot(&drawing());
    let curves = ops(&bytes, "c");
    // Circle 4, arc 2, slot 2 × 2, ellipse 4, spline 2 cubic pieces.
    assert!(curves >= 16, "{curves} curve operators");
    // The line, the slot's two straight sides and the plot area's clip rectangle; tessellated
    // curves would be hundreds.
    let lines = ops(&bytes, "l");
    assert!(lines <= 8, "{lines} line operators: curves were tessellated");
}

#[test]
fn a_dashed_curve_is_still_drawn_in_dashes() {
    let mut d = Drawing::new_metric();
    d.linetypes.push(draftcraft_doc::Linetype::simple("DASHED", "Dashed", &[5.0, -2.5]));
    let common = Common { linetype: "DASHED".into(), ..Common::default() };
    d.add(&Space::Model, common, EntityKind::Circle(Circle { center: Vec3::new(0.0, 0.0, 0.0), radius: 100.0 })).unwrap();
    let bytes = plot(&d);
    // Dashes are short lines, as before: the whole circle isn't written over them.
    assert_eq!(ops(&bytes, "c"), 0);
    assert!(ops(&bytes, "m") > 10);
}

#[test]
fn pdfimport_reads_a_draftcraft_pdf_back_as_arcs_circles_and_lines() {
    let bytes = plot(&drawing());
    let (shapes, _) = pdf_read::import(&bytes, 0, pdf_read::ImportOptions::default()).unwrap();
    let count = |f: fn(&EntityKind) -> bool| shapes.iter().filter(|s| f(&s.kind)).count();
    assert_eq!(count(|k| matches!(k, EntityKind::Circle(_))), 1, "the circle");
    assert_eq!(count(|k| matches!(k, EntityKind::Arc(_))), 1, "the arc");
    assert_eq!(count(|k| matches!(k, EntityKind::Line(_))), 1, "the line");
    // The slot: one closed polyline, two straight sides and two half-circle ends.
    let slot = shapes
        .iter()
        .find_map(|s| match &s.kind {
            EntityKind::LwPolyline(p) if p.vertices.iter().any(|v| v.bulge.abs() > 0.5) => Some(p),
            _ => None,
        })
        .expect("the slot as a polyline with arcs");
    assert!(slot.closed);
    assert_eq!(slot.vertices.len(), 4, "{:?}", slot.vertices);
    assert!(slot.vertices.iter().filter(|v| (v.bulge.abs() - 1.0).abs() < 1e-3).count() == 2);
    // The ellipse and the spline stay curves (splines), not lines.
    assert_eq!(count(|k| matches!(k, EntityKind::Spline(_))), 2);
    // The circle's size survives the plot scale: its radius relative to the arc's.
    let r = |f: fn(&EntityKind) -> Option<f64>| shapes.iter().find_map(|s| f(&s.kind)).unwrap();
    let rc = r(|k| if let EntityKind::Circle(c) = k { Some(c.radius) } else { None });
    let ra = r(|k| if let EntityKind::Arc(a) = k { Some(a.radius) } else { None });
    assert!((rc / ra - 20.0 / 15.0).abs() < 1e-3, "{rc} / {ra}");
    let ang = shapes.iter().find_map(|s| if let EntityKind::Arc(a) = &s.kind { Some((a.start, a.end)) } else { None }).unwrap();
    assert!((ang.0 - 0.2).abs() < 1e-3 && (ang.1 - 2.6).abs() < 1e-3, "{ang:?}");
}
