//! CONIC: a conic arc (ellipse, parabola or hyperbola piece) from its two end points, the apex
//! where its end tangents meet, and its shape: a rho value or a point it passes through. It is
//! drawn as an exact rational quadratic SPLINE, which DXF and DWG keep and AutoCAD opens.

use super::{
    Accept, EngineError, EntityKind, Input, Interactive, Prompt, Result, Session, Spline, Step, Value, Vec2, bad, json, line, number, point_param,
    point_req,
};

/// Rho of a parabola, CONIC's default shape.
pub(super) const DEFAULT_RHO: f64 = 0.5;

/// "ellipse", "parabola" or "hyperbola": the kind of conic a rho makes.
pub(super) fn conic_type(rho: f64) -> &'static str {
    if (rho - 0.5).abs() < 1e-9 {
        "parabola"
    } else if rho < 0.5 {
        "ellipse"
    } else {
        "hyperbola"
    }
}

fn rho_value(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|r| r.is_finite() && *r > 0.0 && *r < 1.0)
        .ok_or_else(|| bad("conic", "`rho` must be a number between 0 and 1 (0.5 is a parabola)"))
}

pub(super) fn run_conic(s: &mut Session, p: &Value) -> Result<Value> {
    let start = point_req("conic", p, "start")?;
    let apex = point_req("conic", p, "apex")?;
    let end = point_req("conic", p, "end")?;
    let rho = match (p.get("rho").filter(|v| !v.is_null()), point_param(p, "through")) {
        (Some(_), Some(_)) => return Err(bad("conic", "give `rho` or `through`, not both")),
        (Some(r), None) => rho_value(r)?,
        (None, Some(t)) => Spline::conic_rho_through(start, apex, end, t)
            .ok_or_else(|| bad("conic", "`through` must lie inside the triangle of `start`, `apex` and `end`"))?,
        (None, None) => DEFAULT_RHO,
    };
    let sp = Spline::conic(start, apex, end, rho).ok_or_else(|| bad("conic", "`start`, `apex` and `end` must not lie on one line"))?;
    let h = s.add_entity(EntityKind::Spline(sp))?;
    Ok(json!({ "handle": h.hex(), "rho": rho, "type": conic_type(rho) }))
}

/// CONIC's prompts: start point, apex, end point, then rho or a point on the curve.
pub(super) struct ConicM {
    pts: Vec<Vec2>,
    rho: f64,
}

impl Default for ConicM {
    fn default() -> Self {
        ConicM { pts: Vec::new(), rho: DEFAULT_RHO }
    }
}

impl ConicM {
    fn add(&self, s: &mut Session, rho: f64) -> Result<Step> {
        let [start, apex, end] = self.pts[..] else { return Ok(Step::Continue) };
        let sp = Spline::conic(start, apex, end, rho).ok_or_else(|| EngineError::Other("The three points lie on one line.".into()))?;
        s.add_entity(EntityKind::Spline(sp))?;
        Ok(Step::Done)
    }
}

impl Interactive for ConicM {
    fn name(&self) -> &'static str {
        "CONIC"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.pts[..] {
            [] => Prompt::new("Specify start point of conic", Accept::POINT),
            [a] => Prompt::new("Specify apex (where the end tangents meet)", Accept::POINT).base(a),
            [_, b] => Prompt::new("Specify end point of conic", Accept::POINT).base(b),
            _ => Prompt::new("Specify rho or a point on the conic", Accept::POINT_OR_NUMBER).default(format!("{:.4}", self.rho)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.pts.len(), i) {
            (0..=1, Input::Point(p)) => self.pts.push(p),
            (2, Input::Point(p)) => {
                let [start, apex] = self.pts[..] else { return Ok(Step::Continue) };
                if Spline::conic(start, apex, p, DEFAULT_RHO).is_none() {
                    return Err(EngineError::Other("The end point must not lie on the line through the start point and apex.".into()));
                }
                self.pts.push(p);
            }
            (3, Input::Point(p)) => {
                let [start, apex, end] = self.pts[..] else { return Ok(Step::Continue) };
                let rho = Spline::conic_rho_through(start, apex, end, p)
                    .ok_or_else(|| EngineError::Other("The point must lie inside the triangle of the start point, apex and end point.".into()))?;
                return self.add(s, rho);
            }
            (3, Input::Text(t)) => {
                let rho = number(&t)
                    .filter(|r| r.is_finite() && *r > 0.0 && *r < 1.0)
                    .ok_or_else(|| EngineError::Other("Rho must be between 0 and 1 (0.5 is a parabola).".into()))?;
                return self.add(s, rho);
            }
            (3, Input::Enter) => return self.add(s, self.rho),
            (_, Input::Enter) => return Ok(Step::Done),
            _ => {}
        }
        Ok(Step::Continue)
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        let curve = |start, apex, end, rho| Spline::conic(start, apex, end, rho).map(EntityKind::Spline);
        match self.pts[..] {
            [a] => vec![line(a, c)],
            [a, b] => [Some(line(a, b)), Some(line(b, c)), curve(a, b, c, self.rho)].into_iter().flatten().collect(),
            [a, b, e] => {
                let rho = Spline::conic_rho_through(a, b, e, c).unwrap_or(self.rho);
                [Some(line(a, b)), Some(line(b, e)), curve(a, b, e, rho)].into_iter().flatten().collect()
            }
            _ => Vec::new(),
        }
    }
}
