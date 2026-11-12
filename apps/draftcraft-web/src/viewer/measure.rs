//! Measuring on the phone: points snap to the drawing (endpoints and corners, midpoints, the
//! nearest point on a line), and the measurement reads as distance, ΔX, ΔY and angle, or as the
//! area and perimeter of the points picked.

use draftcraft_engine::geom::Vec2;
use draftcraft_engine::render::{DisplayList, Kind};

/// What a picked point snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    /// An endpoint, a corner of a polyline or a point object.
    End,
    /// The middle of a straight segment.
    Mid,
    /// The nearest point on a line.
    Near,
    /// Nothing near: the point as picked.
    Free,
}

/// A polyline vertex turning by more than this (radians) is a corner, not a step along a
/// flattened curve.
const CORNER_TURN: f64 = 0.5;

/// Most vertices looked at for one snap: a hostile list cannot stall a tap.
const MAX_SNAP_VERTICES: usize = 20_000_000;

/// The point `p` snaps to within `radius` (drawing units): the closest endpoint or corner, else
/// the closest midpoint, else the nearest point on a line within half the radius, else `p`.
pub fn snap(list: &DisplayList, p: Vec2, radius: f64) -> (Vec2, Snap) {
    if !(p.is_finite() && radius.is_finite() && radius > 0.0) {
        return (p, Snap::Free);
    }
    let r2 = radius * radius;
    let near_r2 = r2 * 0.25;
    let mut best_end: Option<(f64, Vec2)> = None;
    let mut best_mid: Option<(f64, Vec2)> = None;
    let mut best_near: Option<(f64, Vec2)> = None;
    let consider = |best: &mut Option<(f64, Vec2)>, q: Vec2, limit: f64| {
        let d = q.dist(p);
        let d2 = d * d;
        if d2 <= limit && best.is_none_or(|(b, _)| d2 < b) {
            *best = Some((d2, q));
        }
    };
    let mut seen = 0usize;
    for prim in &list.prims {
        let pts = list.points(prim);
        seen = seen.saturating_add(pts.len());
        if seen > MAX_SNAP_VERTICES {
            break;
        }
        match prim.kind {
            Kind::Point => {
                if let Some(q) = pts.first() {
                    consider(&mut best_end, *q, r2);
                }
            }
            Kind::Polyline => {
                let n = pts.len();
                // Corners: the ends and every vertex the line turns sharply at.
                let corner = |i: usize| -> bool {
                    if i == 0 || i + 1 >= n {
                        return true;
                    }
                    match (pts.get(i - 1), pts.get(i), pts.get(i + 1)) {
                        (Some(a), Some(b), Some(c)) => turn(*b - *a, *c - *b) > CORNER_TURN,
                        _ => false,
                    }
                };
                for (i, q) in pts.iter().enumerate() {
                    if corner(i) {
                        consider(&mut best_end, *q, r2);
                    }
                }
                for (i, w) in pts.windows(2).enumerate() {
                    let [a, b] = w else { continue };
                    // A whole straight segment: both its ends are corners.
                    if corner(i) && corner(i + 1) {
                        consider(&mut best_mid, (*a + *b) * 0.5, r2);
                    }
                    consider(&mut best_near, nearest_on_segment(*a, *b, p), near_r2);
                }
            }
            _ => {}
        }
    }
    match (best_end, best_mid, best_near) {
        (Some((_, q)), _, _) => (q, Snap::End),
        (None, Some((_, q)), _) => (q, Snap::Mid),
        (None, None, Some((_, q))) => (q, Snap::Near),
        _ => (p, Snap::Free),
    }
}

/// The angle between two directions, 0..=π (0 for a zero-length one).
fn turn(u: Vec2, v: Vec2) -> f64 {
    let (lu, lv) = (u.len(), v.len());
    if lu <= 0.0 || lv <= 0.0 {
        return 0.0;
    }
    (u.dot(v) / (lu * lv)).clamp(-1.0, 1.0).acos()
}

/// The point of segment `ab` nearest to `p`.
pub fn nearest_on_segment(a: Vec2, b: Vec2, p: Vec2) -> Vec2 {
    let d = b - a;
    let l2 = d.len2();
    if !(l2 > 0.0 && l2.is_finite()) {
        return a;
    }
    let t = ((p - a).dot(d) / l2).clamp(0.0, 1.0);
    a + d * t
}

/// Distance, ΔX, ΔY and the angle from the X axis (degrees, 0..360) from `a` to `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distance {
    pub length: f64,
    pub dx: f64,
    pub dy: f64,
    pub angle_deg: f64,
}

pub fn distance(a: Vec2, b: Vec2) -> Distance {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut angle = dy.atan2(dx).to_degrees();
    if angle < 0.0 {
        angle += 360.0;
    }
    if !angle.is_finite() || angle >= 360.0 {
        angle = 0.0;
    }
    Distance { length: dx.hypot(dy), dx, dy, angle_deg: angle }
}

/// The length along `points`, back to the first one when `closed`.
pub fn perimeter(points: &[Vec2], closed: bool) -> f64 {
    let open: f64 = points.windows(2).map(|w| if let [a, b] = w { a.dist(*b) } else { 0.0 }).sum();
    match (closed, points.first(), points.last()) {
        (true, Some(a), Some(b)) if points.len() > 2 => open + a.dist(*b),
        _ => open,
    }
}

/// The area enclosed by `points` (closed back to the first one), never negative.
pub fn area(points: &[Vec2]) -> f64 {
    let Some(first) = points.first() else { return 0.0 };
    if points.len() < 3 {
        return 0.0;
    }
    // Relative to the first point, so far-off coordinates keep their precision.
    let rel: Vec<Vec2> = points.iter().map(|p| *p - *first).collect();
    let twice: f64 = rel.iter().zip(rel.iter().cycle().skip(1)).map(|(a, b)| a.cross(*b)).sum();
    (twice / 2.0).abs()
}

/// The name of the drawing's units (`INSUNITS`), to write after a length; empty when unitless.
pub fn unit_suffix(insunits: i64) -> &'static str {
    match insunits {
        1 => "in",
        2 => "ft",
        3 => "mi",
        4 => "mm",
        5 => "cm",
        6 => "m",
        7 => "km",
        8 => "µin",
        9 => "mil",
        10 => "yd",
        11 => "Å",
        12 => "nm",
        13 => "µm",
        14 => "dm",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draftcraft_engine::render::DPrim;

    fn list(polylines: &[&[(f64, f64)]]) -> DisplayList {
        let mut l = DisplayList::default();
        for pl in polylines {
            let start = l.verts.len() as u32;
            l.verts.extend(pl.iter().map(|(x, y)| Vec2::new(*x, *y)));
            l.prims.push(DPrim {
                handle: draftcraft_engine::doc::Handle(1),
                color: draftcraft_engine::doc::color::Rgb(255, 255, 255),
                aci7: true,
                lw: 0.0,
                transparency: 0,
                kind: Kind::Polyline,
                start,
                len: pl.len() as u32,
            });
        }
        l
    }

    /// Endpoints and corners win over midpoints, midpoints over the nearest point on a line.
    #[test]
    fn snaps_in_order_of_preference() {
        let l = list(&[&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]]);
        assert_eq!(snap(&l, Vec2::new(9.5, 0.4), 1.0), (Vec2::new(10.0, 0.0), Snap::End));
        assert_eq!(snap(&l, Vec2::new(5.3, 0.2), 1.0), (Vec2::new(5.0, 0.0), Snap::Mid));
        assert_eq!(snap(&l, Vec2::new(2.5, 0.3), 1.0), (Vec2::new(2.5, 0.0), Snap::Near));
        assert_eq!(snap(&l, Vec2::new(5.0, 5.0), 1.0), (Vec2::new(5.0, 5.0), Snap::Free));
    }

    /// The steps of a flattened curve are not corners and have no midpoints.
    #[test]
    fn flattened_curves_have_no_false_corners() {
        let arc: Vec<(f64, f64)> =
            (0..=32).map(|i| f64::from(i) / 32.0 * std::f64::consts::FRAC_PI_2).map(|a| (10.0 * a.cos(), 10.0 * a.sin())).collect();
        let l = list(&[&arc]);
        let mid = Vec2::new(10.0 * 0.25f64.cos(), 10.0 * 0.25f64.sin());
        let (q, kind) = snap(&l, mid + Vec2::new(0.05, 0.05), 1.0);
        assert_eq!(kind, Snap::Near);
        assert!(q.dist(mid) < 0.2);
        assert_eq!(snap(&l, Vec2::new(10.2, 0.1), 1.0).1, Snap::End);
    }

    #[test]
    fn hostile_input_does_not_snap() {
        let l = list(&[&[(0.0, 0.0), (1.0, 0.0)]]);
        assert_eq!(snap(&l, Vec2::new(f64::NAN, 0.0), 1.0).1, Snap::Free);
        assert_eq!(snap(&l, Vec2::new(0.0, 0.0), f64::INFINITY).1, Snap::Free);
        assert_eq!(snap(&l, Vec2::new(0.0, 0.0), -1.0).1, Snap::Free);
        assert_eq!(nearest_on_segment(Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::ZERO), Vec2::new(1.0, 1.0));
    }

    #[test]
    fn distance_angle_area_perimeter() {
        let d = distance(Vec2::new(1.0, 1.0), Vec2::new(4.0, 5.0));
        assert_eq!((d.length, d.dx, d.dy), (5.0, 3.0, 4.0));
        assert!((d.angle_deg - 53.130_102_354).abs() < 1e-6);
        assert!((distance(Vec2::ZERO, Vec2::new(0.0, -2.0)).angle_deg - 270.0).abs() < 1e-9);
        assert_eq!(distance(Vec2::ZERO, Vec2::ZERO).angle_deg, 0.0);
        let sq = [Vec2::new(1e9, 1e9), Vec2::new(1e9 + 2.0, 1e9), Vec2::new(1e9 + 2.0, 1e9 + 3.0), Vec2::new(1e9, 1e9 + 3.0)];
        assert_eq!(area(&sq), 6.0);
        assert_eq!(perimeter(&sq, true), 10.0);
        assert_eq!(perimeter(&sq, false), 7.0);
        assert_eq!(area(&sq[..2]), 0.0);
        assert_eq!(perimeter(&[], true), 0.0);
        assert_eq!(unit_suffix(4), "mm");
        assert_eq!(unit_suffix(-3), "");
    }
}
