//! Curves as cubic Bézier paths, for writers whose formats draw with Béziers (PDF, SVG): circular
//! and elliptical arcs as cubics of at most a quarter turn each (off the true curve by under
//! 0.03 % of the radius), polylines with bulges as lines and arcs, and non-rational B-splines of
//! degree 2 or 3 exactly, by Bézier decomposition.
//!
//! A path written this way keeps its curves: a reader (or DraftCraft's own PDFIMPORT) gets arcs and
//! splines back, instead of the hundreds of short lines a tessellated curve leaves.

use std::f64::consts::FRAC_PI_2;

use crate::{Arc, Circle, Ellipse, Mat3, Polyline, Segment, Spline, TAU, Vec2};

/// Most cubic pieces one path holds; a longer one is left to tessellation.
const MAX_PIECES: usize = 20_000;

/// One piece of a path, from the end of the previous one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSeg {
    /// A straight segment to the point.
    Line(Vec2),
    /// A cubic Bézier through two control points to the last point.
    Cubic(Vec2, Vec2, Vec2),
}

impl PathSeg {
    /// The point the piece ends at.
    pub fn end(&self) -> Vec2 {
        match self {
            PathSeg::Line(p) | PathSeg::Cubic(_, _, p) => *p,
        }
    }

    fn map(&self, f: impl Fn(Vec2) -> Vec2) -> PathSeg {
        match *self {
            PathSeg::Line(p) => PathSeg::Line(f(p)),
            PathSeg::Cubic(a, b, c) => PathSeg::Cubic(f(a), f(b), f(c)),
        }
    }
}

/// A path: a start point and the pieces that follow it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BezierPath {
    pub start: Vec2,
    pub segs: Vec<PathSeg>,
}

impl BezierPath {
    /// The point the path ends at.
    pub fn end(&self) -> Vec2 {
        self.segs.last().map_or(self.start, PathSeg::end)
    }

    /// The path under an affine transform: exact, as a Bézier curve's image is the curve of its
    /// control points' images.
    pub fn transformed(&self, m: &Mat3) -> BezierPath {
        BezierPath { start: m.apply(self.start), segs: self.segs.iter().map(|s| s.map(|p| m.apply(p))).collect() }
    }

    /// The path with `f` applied to every point (an affine `f` keeps it exact).
    pub fn mapped(&self, f: impl Fn(Vec2) -> Vec2) -> BezierPath {
        BezierPath { start: f(self.start), segs: self.segs.iter().map(|s| s.map(&f)).collect() }
    }

    /// Whether the path has a curved piece (else it is a polyline).
    pub fn has_curves(&self) -> bool {
        self.segs.iter().any(|s| matches!(s, PathSeg::Cubic(..)))
    }

    /// Whether every point of the path is finite.
    pub fn is_finite(&self) -> bool {
        self.start.is_finite()
            && self.segs.iter().all(|s| match s {
                PathSeg::Line(p) => p.is_finite(),
                PathSeg::Cubic(a, b, c) => a.is_finite() && b.is_finite() && c.is_finite(),
            })
    }
}

/// Cubic pieces of the unit-circle arc from angle `a0` turning by `sweep` (negative: clockwise),
/// mapped through `f` (from unit-circle coordinates to the curve's). Each piece turns at most a
/// quarter: its handles are `4/3 · tan(θ/4)` long, the usual best fit.
fn unit_arc(a0: f64, sweep: f64, f: impl Fn(Vec2) -> Vec2, out: &mut Vec<PathSeg>) -> bool {
    if !(a0.is_finite() && sweep.is_finite()) || sweep == 0.0 {
        return false;
    }
    let n = (sweep.abs() / FRAC_PI_2).ceil().clamp(1.0, 8.0) as usize;
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for i in 0..n {
        let (t0, t1) = (a0 + step * i as f64, a0 + step * (i + 1) as f64);
        let (p0, p3) = (Vec2::new(t0.cos(), t0.sin()), Vec2::new(t1.cos(), t1.sin()));
        let p1 = p0 + p0.perp() * k;
        let p2 = p3 - p3.perp() * k;
        out.push(PathSeg::Cubic(f(p1), f(p2), f(p3)));
    }
    true
}

impl Arc {
    /// The arc counter-clockwise from its start, as cubic pieces.
    pub fn bezier(&self) -> Option<BezierPath> {
        let (c, r) = (self.center, self.radius);
        let mut segs = Vec::new();
        unit_arc(self.start, self.sweep(), |p| c + p * r, &mut segs).then_some(())?;
        Some(BezierPath { start: self.start_point(), segs })
    }
}

impl Circle {
    /// The circle counter-clockwise from angle 0 (where its tessellation starts), as four cubics.
    pub fn bezier(&self) -> Option<BezierPath> {
        Arc { center: self.center, radius: self.radius, start: 0.0, end: TAU }.bezier()
    }
}

impl Ellipse {
    /// The elliptical arc counter-clockwise from its start parameter, as cubic pieces: the
    /// circular pieces under the affine map taking the unit circle to the ellipse (exact as such).
    pub fn bezier(&self) -> Option<BezierPath> {
        let (c, u, v) = (self.center, self.major, self.minor());
        let f = |p: Vec2| c + u * p.x + v * p.y;
        let mut segs = Vec::new();
        unit_arc(self.start, self.sweep(), f, &mut segs).then_some(())?;
        Some(BezierPath { start: self.at_param(self.start), segs })
    }
}

impl Segment {
    /// The pieces of this segment from its start (see [`Segment::start`]).
    fn bezier_into(&self, out: &mut Vec<PathSeg>) -> bool {
        match self {
            Segment::Line(l) => {
                out.push(PathSeg::Line(l.b));
                true
            }
            Segment::Arc { arc, ccw } => {
                let (c, r) = (arc.center, arc.radius);
                let sweep = arc.sweep();
                if *ccw { unit_arc(arc.start, sweep, |p| c + p * r, out) } else { unit_arc(arc.end, -sweep, |p| c + p * r, out) }
            }
        }
    }

    /// The segment as a path from its start.
    pub fn bezier(&self) -> Option<BezierPath> {
        let mut segs = Vec::new();
        self.bezier_into(&mut segs).then_some(())?;
        Some(BezierPath { start: self.start(), segs })
    }
}

impl Polyline {
    /// The polyline's lines and arcs as one path (closed back to the start when closed).
    pub fn bezier(&self) -> Option<BezierPath> {
        let segs = self.segments();
        let first = segs.first()?;
        let mut out = Vec::new();
        for s in &segs {
            if !s.bezier_into(&mut out) || out.len() > MAX_PIECES {
                return None;
            }
        }
        Some(BezierPath { start: first.start(), segs: out })
    }
}

impl Spline {
    /// A non-rational spline of degree 2 or 3 as its cubic Bézier pieces, exactly (degree 2
    /// raised to 3). `None` for rational splines, other degrees and broken knot vectors, which
    /// are left to tessellation.
    pub fn bezier(&self) -> Option<BezierPath> {
        if self.is_rational() || !(2..=3).contains(&self.degree) {
            return None;
        }
        let (pieces, _) = self.bezier_pieces()?;
        if pieces.len() > MAX_PIECES {
            return None;
        }
        let mut start = None;
        let mut segs = Vec::with_capacity(pieces.len());
        for piece in pieces {
            let mut q = piece;
            while q.len() < 4 {
                q = crate::nurbs::elevate_bezier(&q);
            }
            // Non-rational: every weight is 1, so the homogeneous points are the points.
            let [a, b, c, d] = q.get(..4)?.iter().map(|h| h.0).collect::<Vec<_>>().try_into().ok()?;
            start.get_or_insert(a);
            segs.push(PathSeg::Cubic(b, c, d));
        }
        let path = BezierPath { start: start?, segs };
        path.is_finite().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PolyVertex;

    /// The point at `t` on a cubic.
    fn at(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f64) -> Vec2 {
        let s = 1.0 - t;
        p0 * (s * s * s) + p1 * (3.0 * s * s * t) + p2 * (3.0 * s * t * t) + p3 * (t * t * t)
    }

    /// Every point sampled along the path's cubics, with its lines' ends.
    fn samples(p: &BezierPath) -> Vec<Vec2> {
        let mut out = vec![p.start];
        let mut cur = p.start;
        for s in &p.segs {
            match *s {
                PathSeg::Line(q) => out.push(q),
                PathSeg::Cubic(a, b, c) => out.extend((1..=16).map(|i| at(cur, a, b, c, f64::from(i) / 16.0))),
            }
            cur = s.end();
        }
        out
    }

    #[test]
    fn a_circle_is_four_cubics_on_the_circle() {
        let c = Circle::new(Vec2::new(3.0, -2.0), 5.0);
        let p = c.bezier().unwrap();
        assert_eq!(p.segs.len(), 4);
        assert!(p.start.near(Vec2::new(8.0, -2.0), 1e-12));
        assert!(p.end().near(p.start, 1e-9));
        for q in samples(&p) {
            assert!((q.dist(c.center) - 5.0).abs() < 5.0 * 3e-4, "{q:?}");
        }
    }

    #[test]
    fn arcs_turn_their_way_and_end_where_they_end() {
        let arc = Arc::new(Vec2::ZERO, 2.0, 0.3, 2.9);
        let p = arc.bezier().unwrap();
        assert!(p.start.near(arc.start_point(), 1e-12) && p.end().near(arc.end_point(), 1e-9));
        assert_eq!(p.segs.len(), 2);
        let cw = Segment::Arc { arc, ccw: false }.bezier().unwrap();
        assert!(cw.start.near(arc.end_point(), 1e-12) && cw.end().near(arc.start_point(), 1e-9));
        for q in samples(&cw) {
            assert!((q.len() - 2.0).abs() < 2.0 * 3e-4);
        }
    }

    #[test]
    fn an_ellipse_arc_stays_on_the_ellipse() {
        let e = Ellipse { center: Vec2::new(1.0, 1.0), major: Vec2::new(4.0, 3.0), ratio: 0.4, start: 0.5, end: 4.0 };
        let p = e.bezier().unwrap();
        assert!(p.start.near(e.at_param(0.5), 1e-12) && p.end().near(e.at_param(4.0), 1e-9));
        // In the ellipse's own frame the samples sit on the unit circle.
        let (u, v) = (e.major, e.minor());
        for q in samples(&p) {
            let d = q - e.center;
            let (x, y) = (d.dot(u) / u.dot(u), d.dot(v) / v.dot(v));
            assert!(((x * x + y * y).sqrt() - 1.0).abs() < 3e-4, "{q:?}");
        }
    }

    #[test]
    fn a_bulged_polyline_is_lines_and_arcs() {
        let pl = Polyline {
            vertices: vec![
                PolyVertex { bulge: 1.0, ..PolyVertex::new(Vec2::new(0.0, 0.0)) },
                PolyVertex::new(Vec2::new(10.0, 0.0)),
                PolyVertex::new(Vec2::new(10.0, 5.0)),
            ],
            closed: false,
        };
        let p = pl.bezier().unwrap();
        assert!(p.has_curves());
        assert_eq!(p.segs.last(), Some(&PathSeg::Line(Vec2::new(10.0, 5.0))));
        // The half circle below the chord (bulge 1, counter-clockwise from (0,0) to (10,0)).
        assert!(samples(&p).iter().any(|q| q.near(Vec2::new(5.0, -5.0), 1e-3)));
    }

    #[test]
    fn a_cubic_spline_is_its_bezier_pieces_exactly() {
        let s =
            Spline::from_control(vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 3.0), Vec2::new(4.0, 3.0), Vec2::new(6.0, 0.0), Vec2::new(9.0, 1.0)], 3);
        let p = s.bezier().unwrap();
        assert_eq!(p.segs.len(), 2);
        let pts = s.tessellate(1e-4);
        for q in samples(&p) {
            let d = pts.windows(2).map(|w| crate::Line::new(w[0], w[1]).dist(q)).fold(f64::INFINITY, f64::min);
            assert!(d < 1e-3, "{q:?} off the spline by {d}");
        }
        let mut rational = s.clone();
        rational.weights = vec![1.0, 2.0, 1.0, 1.0, 1.0];
        assert!(rational.bezier().is_none(), "rational splines stay tessellated");
    }

    #[test]
    fn hostile_input_gives_none_not_a_panic() {
        assert!(Arc { center: Vec2::new(f64::NAN, 0.0), radius: 1.0, start: f64::NAN, end: 1.0 }.bezier().is_none());
        assert!(Spline::from_control(vec![Vec2::ZERO], 3).bezier().is_none());
        assert!(Polyline { vertices: Vec::new(), closed: true }.bezier().is_none());
    }
}
