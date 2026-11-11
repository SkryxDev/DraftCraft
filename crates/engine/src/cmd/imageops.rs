//! IMAGECLIP, PDFCLIP, IMAGEADJUST and PDFADJUST: clip raster images and PDF underlays to a
//! rectangle or polygon and change how they show (brightness, contrast, fade, monochrome). The
//! Image and PDF Underlay contextual ribbon tabs reach them.
//!
//! A boundary is given in drawing coordinates and kept on the image in its own pixel coordinates
//! ([`Image::clip`]), so it moves, scales and rotates with the image. An underlay also keeps it as
//! the file does (inches from its insertion point, [`draftcraft_doc::Underlay::clip`]).

use draftcraft_doc::{EntityKind, Handle, Image, MAX_CLIP_VERTICES, POINTS_PER_INCH};
use draftcraft_geom::Vec2;
use serde_json::{Value, json};

use super::machines::{SelOutcome, SelectPhase};
use super::{CommandSpec, bad, bool_or, has_doc, point_value, str_param, targets};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

/// Which objects a command works on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Underlay,
}

impl Kind {
    fn id(self, clip: bool) -> &'static str {
        match (self, clip) {
            (Kind::Image, true) => "imageclip",
            (Kind::Underlay, true) => "pdfclip",
            (Kind::Image, false) => "imageadjust",
            (Kind::Underlay, false) => "pdfadjust",
        }
    }
    fn wants(self, i: &Image) -> bool {
        (self == Kind::Underlay) == i.underlay.is_some()
    }
    fn nothing(self) -> &'static str {
        match self {
            Kind::Image => "select raster images",
            Kind::Underlay => "select PDF underlays",
        }
    }
}

pub(super) fn specs() -> Vec<CommandSpec> {
    const CLIP: &str = "{handles? | handle? (default the selection), option?: new|on|off|delete (default new with a boundary, else \
                        query), rectangle?: [[x,y],[x,y]] | points?: [[x,y],…] (3 to 4096, drawing coordinates)} → {handles, option} \
                        (query: {clips: [{handle, boundary, enabled}]})";
    vec![
        CommandSpec::new("imageclip", "Image Clip", |s, p| run_clip(s, p, Kind::Image))
            .alias(&["icl"])
            .menu(&["Modify", "Clip", "Image"])
            .params(CLIP)
            .enabled(has_doc)
            .interactive(|_| Ok(Box::new(ClipM::new(Kind::Image)))),
        CommandSpec::new("pdfclip", "PDF Clip", |s, p| run_clip(s, p, Kind::Underlay))
            .menu(&["Modify", "Clip", "PDF Underlay"])
            .params(CLIP)
            .enabled(has_doc)
            .interactive(|_| Ok(Box::new(ClipM::new(Kind::Underlay)))),
        CommandSpec::new("imageadjust", "Image Adjust", |s, p| run_adjust(s, p, Kind::Image))
            .alias(&["iad"])
            .menu(&["Modify", "Object", "Image", "Adjust..."])
            .params("{handles? (default the selection), brightness?: 0-100, contrast?: 0-100, fade?: 0-100} → {handles, brightness, contrast, fade}")
            .enabled(has_doc)
            .interactive(|_| Ok(Box::new(AdjustM::new(Kind::Image)))),
        CommandSpec::new("pdfadjust", "PDF Adjust", |s, p| run_adjust(s, p, Kind::Underlay))
            .params("{handles? (default the selection), contrast?: 0-100, fade?: 0-80, monochrome?: bool, adjustColors?: bool (for the background)} → {handles, contrast, fade, monochrome, adjustColors}")
            .enabled(has_doc)
            .interactive(|_| Ok(Box::new(AdjustM::new(Kind::Underlay)))),
    ]
}

/// The images (or underlays) among `hs`, once each.
fn pick(s: &Session, hs: &[Handle], kind: Kind) -> Vec<Handle> {
    let Ok(d) = s.doc() else { return Vec::new() };
    let mut seen = std::collections::HashSet::new();
    hs.iter()
        .copied()
        .filter(|h| matches!(d.entity(*h).map(|e| &e.kind), Some(EntityKind::Image(i)) if kind.wants(i)))
        .filter(|h| seen.insert(*h))
        .collect()
}

fn hexes(hs: &[Handle]) -> Vec<String> {
    hs.iter().map(|h| h.hex()).collect()
}

/// A drawing point as image pixel coordinates (from the top-left pixel centre, y down), with
/// the same point in the image's own frame (a: along U, b: along V, in pixels from the corner).
fn to_pixels(i: &Image, q: Vec2) -> Option<(Vec2, Vec2)> {
    let (u, v) = (i.u.xy(), i.v.xy());
    let det = u.x * v.y - u.y * v.x;
    if !det.is_finite() || det.abs() < 1e-300 {
        return None;
    }
    let r = q - i.insert.xy();
    let a = (r.x * v.y - r.y * v.x) / det;
    let b = (u.x * r.y - u.y * r.x) / det;
    (a.is_finite() && b.is_finite()).then(|| (Vec2::new(a - 0.5, i.size.y - b - 0.5), Vec2::new(a, b)))
}

/// An image's boundary outline in drawing coordinates.
fn outline(i: &Image) -> Vec<Vec2> {
    let pts: Vec<Vec2> = match i.clip.as_slice() {
        [a, b] => vec![*a, Vec2::new(b.x, a.y), *b, Vec2::new(a.x, b.y)],
        p => p.to_vec(),
    };
    pts.iter().map(|p| i.insert.xy() + i.u.xy() * (p.x + 0.5) + i.v.xy() * (i.size.y - p.y - 0.5)).collect()
}

fn boundary(p: &Value, id: &str) -> Result<(Vec<Vec2>, bool)> {
    if let Some(r) = p.get("rectangle") {
        let pts: Vec<Vec2> = r.as_array().map(|a| a.iter().filter_map(point_value).collect()).unwrap_or_default();
        let [a, b] = pts.as_slice() else { return Err(bad(id, "`rectangle` needs two finite corners [[x,y],[x,y]]")) };
        if (a.x - b.x).abs() <= 1e-12 || (a.y - b.y).abs() <= 1e-12 {
            return Err(bad(id, "the clipping rectangle must have a width and a height"));
        }
        return Ok((vec![*a, *b], true));
    }
    let pts = p.get("points").and_then(Value::as_array).ok_or_else(|| bad(id, "a boundary is required: `rectangle` or `points`"))?;
    if !(3..=MAX_CLIP_VERTICES).contains(&pts.len()) {
        return Err(bad(id, format!("a polygonal boundary needs 3 to {MAX_CLIP_VERTICES} points")));
    }
    let pts: Vec<Vec2> =
        pts.iter().map(|v| point_value(v).ok_or_else(|| bad(id, "every boundary point must be a finite [x, y]"))).collect::<Result<_>>()?;
    Ok((pts, false))
}

fn run_clip(s: &mut Session, p: &Value, kind: Kind) -> Result<Value> {
    let id = kind.id(true);
    let hs = pick(s, &targets(s, p)?, kind);
    if hs.is_empty() {
        return Err(bad(id, kind.nothing()));
    }
    let has_boundary = p.get("rectangle").is_some() || p.get("points").is_some();
    let option = str_param(p, "option").map(str::to_ascii_lowercase).unwrap_or_else(|| if has_boundary { "new".into() } else { "query".into() });
    match option.as_str() {
        "new" | "n" => {
            let (world, rect) = boundary(p, id)?;
            let mut changes = Vec::with_capacity(hs.len());
            {
                let d = s.doc()?;
                for h in &hs {
                    let Some(EntityKind::Image(i)) = d.entity(*h).map(|e| &e.kind) else { continue };
                    // A rectangle stays two corners only while the image is square to the axes.
                    let square = i.u.y.abs() < 1e-12 && i.v.x.abs() < 1e-12;
                    let pts: Vec<Vec2> = match (rect, world.as_slice()) {
                        (true, [a, b]) if !square => vec![*a, Vec2::new(b.x, a.y), *b, Vec2::new(a.x, b.y)],
                        _ => world.clone(),
                    };
                    let mapped: Vec<(Vec2, Vec2)> = pts
                        .iter()
                        .map(|q| to_pixels(i, *q))
                        .collect::<Option<_>>()
                        .ok_or_else(|| bad(id, "an image with a zero size can't be clipped"))?;
                    changes.push((*h, mapped));
                }
            }
            for (h, mapped) in changes {
                s.doc_mut()?
                    .modify_entity(h, |e| {
                        if let EntityKind::Image(i) = &mut e.kind {
                            i.clip = mapped.iter().map(|m| m.0).collect();
                            i.clipping = true;
                            i.display |= 4;
                            if let Some(u) = &mut i.underlay {
                                u.clip = mapped.iter().map(|m| m.1 * (1.0 / POINTS_PER_INCH)).collect();
                            }
                        }
                    })
                    .map_err(|e| bad(id, e.to_string()))?;
            }
        }
        "on" | "off" => {
            let on = option == "on";
            let d = s.doc()?;
            if !hs.iter().all(|h| matches!(d.entity(*h).map(|e| &e.kind), Some(EntityKind::Image(i)) if i.clip.len() >= 2)) {
                return Err(bad(id, "no clipping boundary on the selected objects"));
            }
            for h in &hs {
                s.doc_mut()?
                    .modify_entity(*h, |e| {
                        if let EntityKind::Image(i) = &mut e.kind {
                            i.clipping = on;
                            i.display = if on { i.display | 4 } else { i.display & !4 };
                        }
                    })
                    .map_err(|e| bad(id, e.to_string()))?;
            }
        }
        "delete" | "d" => {
            for h in &hs {
                s.doc_mut()?
                    .modify_entity(*h, |e| {
                        if let EntityKind::Image(i) = &mut e.kind {
                            i.clip.clear();
                            i.clipping = false;
                            i.display &= !4;
                            if let Some(u) = &mut i.underlay {
                                u.clip.clear();
                            }
                        }
                    })
                    .map_err(|e| bad(id, e.to_string()))?;
            }
        }
        "query" | "?" => {
            let d = s.doc()?;
            let clips: Vec<Value> = hs
                .iter()
                .filter_map(|h| match &d.entity(*h)?.kind {
                    EntityKind::Image(i) => Some(json!({
                        "handle": h.hex(),
                        "boundary": (i.clip.len() >= 2).then(|| outline(i).iter().map(|p| [p.x, p.y]).collect::<Vec<_>>()),
                        "enabled": i.clipping && i.clip.len() >= 2,
                    })),
                    _ => None,
                })
                .collect();
            return Ok(json!({ "clips": clips }));
        }
        o => return Err(bad(id, format!("unknown option \"{o}\" (new, on, off, delete or query)"))),
    }
    Ok(json!({ "handles": hexes(&hs), "option": option }))
}

/// A percentage parameter: `None` when absent, an error outside 0..=`max`.
fn pct(p: &Value, key: &str, max: u8, id: &str) -> Result<Option<u8>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .or_else(|| v.as_str().and_then(|t| t.trim().parse().ok()))
            .filter(|x| x.is_finite() && (0.0..=f64::from(max)).contains(x))
            .map(|x| Some(x.round() as u8))
            .ok_or_else(|| bad(id, format!("`{key}` must be a number from 0 to {max}"))),
    }
}

fn run_adjust(s: &mut Session, p: &Value, kind: Kind) -> Result<Value> {
    let id = kind.id(false);
    let hs = pick(s, &targets(s, p)?, kind);
    if hs.is_empty() {
        return Err(bad(id, kind.nothing()));
    }
    let brightness = if kind == Kind::Image { pct(p, "brightness", 100, id)? } else { None };
    let contrast = pct(p, "contrast", 100, id)?;
    let fade = pct(p, "fade", if kind == Kind::Image { 100 } else { 80 }, id)?;
    let mono = p.get("monochrome").map(|_| bool_or(p, "monochrome", false));
    let adjust = p.get("adjustColors").map(|_| bool_or(p, "adjustColors", false));
    let changes = brightness.is_some() || contrast.is_some() || fade.is_some() || (kind == Kind::Underlay && (mono.is_some() || adjust.is_some()));
    if changes {
        for h in &hs {
            s.doc_mut()?
                .modify_entity(*h, |e| {
                    if let EntityKind::Image(i) = &mut e.kind {
                        if let Some(v) = brightness {
                            i.brightness = v;
                        }
                        if let Some(v) = contrast {
                            i.contrast = v;
                        }
                        if let Some(v) = fade {
                            i.fade = v;
                        }
                        if let Some(u) = &mut i.underlay {
                            if let Some(m) = mono {
                                u.monochrome = m;
                            }
                            if let Some(a) = adjust {
                                u.adjust_for_background = a;
                            }
                        }
                    }
                })
                .map_err(|e| bad(id, e.to_string()))?;
        }
    }
    // The first object's values: what the command shows (and every object has after a change).
    let d = s.doc()?;
    let Some(EntityKind::Image(i)) = hs.first().and_then(|h| d.entity(*h)).map(|e| &e.kind) else { return Err(bad(id, kind.nothing())) };
    let mut out = json!({ "handles": hexes(&hs), "contrast": i.contrast, "fade": i.fade });
    match &i.underlay {
        Some(u) => {
            out["monochrome"] = json!(u.monochrome);
            out["adjustColors"] = json!(u.adjust_for_background);
        }
        None => out["brightness"] = json!(i.brightness),
    }
    Ok(out)
}

fn kw(i: &Input) -> Option<String> {
    match i {
        Input::Keyword(k) | Input::Text(k) => Some(k.trim().to_ascii_lowercase()),
        _ => None,
    }
}

// ---------------- IMAGECLIP / PDFCLIP prompts ----------------

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum ClipStage {
    #[default]
    Select,
    Option,
    Boundary,
    Corner2,
    Polygon,
}

struct ClipM {
    kind: Kind,
    stage: ClipStage,
    sel: SelectPhase,
    hs: Vec<Handle>,
    pts: Vec<Vec2>,
}

impl ClipM {
    fn new(kind: Kind) -> Self {
        ClipM { kind, stage: ClipStage::Select, sel: SelectPhase::default(), hs: Vec::new(), pts: Vec::new() }
    }
    fn selected(&mut self, s: &mut Session, hs: &[Handle]) -> Step {
        self.hs = pick(s, hs, self.kind);
        if self.hs.is_empty() {
            s.echo(match self.kind {
                Kind::Image => "No raster images selected.",
                Kind::Underlay => "No PDF underlays selected.",
            });
            return Step::Done;
        }
        self.stage = ClipStage::Option;
        Step::Continue
    }
    fn apply(&mut self, s: &mut Session, mut p: Value) -> Result<Step> {
        p["handles"] = json!(hexes(&self.hs));
        if let Err(e) = run_clip(s, &p, self.kind) {
            s.echo(e.to_string());
        }
        s.set_selection(Vec::new());
        Ok(Step::Done)
    }
}

impl Interactive for ClipM {
    fn name(&self) -> &'static str {
        match self.kind {
            Kind::Image => "IMAGECLIP",
            Kind::Underlay => "PDFCLIP",
        }
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.sel = SelectPhase::begin(s);
        self.sel.single = true;
        if self.sel.done {
            let hs = self.sel.picked.clone();
            return Ok(self.selected(s, &hs));
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            ClipStage::Select => match self.kind {
                Kind::Image => Prompt::new("Select image to clip", Accept::SELECT),
                Kind::Underlay => Prompt::new("Select PDF to clip", Accept::SELECT),
            },
            ClipStage::Option => Prompt::new("Enter image clipping option", Accept::TEXT).kw(&["ON", "OFF", "Delete", "New boundary"]).default("New"),
            ClipStage::Boundary => {
                Prompt::new("Specify first corner point or", Accept::POINT).kw(&["Polygonal", "Rectangular"]).default("Rectangular")
            }
            ClipStage::Corner2 => {
                let p = Prompt::new("Specify opposite corner point", Accept::POINT);
                match self.pts.first() {
                    Some(a) => p.corner(*a),
                    None => p,
                }
            }
            ClipStage::Polygon => {
                let msg = if self.pts.is_empty() { "Specify first point" } else { "Specify next point" };
                let p = Prompt::new(msg, Accept::POINT).base_opt(self.pts.last().copied());
                if self.pts.is_empty() { p } else { p.kw(&["Undo"]) }
            }
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        match self.stage {
            ClipStage::Select => match self.sel.feed(s, &i)? {
                SelOutcome::More => Ok(Step::Continue),
                SelOutcome::Empty => Ok(Step::Done),
                SelOutcome::Done(hs) => Ok(self.selected(s, &hs)),
            },
            ClipStage::Option => {
                let k = if matches!(i, Input::Enter) { Some("new".to_string()) } else { kw(&i) };
                match k.as_deref() {
                    Some("on") => self.apply(s, json!({ "option": "on" })),
                    Some("off") => self.apply(s, json!({ "option": "off" })),
                    Some(k) if k.starts_with('d') => self.apply(s, json!({ "option": "delete" })),
                    Some(k) if k.starts_with('n') => {
                        self.stage = ClipStage::Boundary;
                        Ok(Step::Continue)
                    }
                    _ => {
                        s.echo("Invalid option keyword.");
                        Ok(Step::Continue)
                    }
                }
            }
            ClipStage::Boundary => match (&i, kw(&i).as_deref()) {
                (Input::Point(p), _) => {
                    self.pts = vec![*p];
                    self.stage = ClipStage::Corner2;
                    Ok(Step::Continue)
                }
                (_, Some(k)) if k.starts_with('p') => {
                    self.pts.clear();
                    self.stage = ClipStage::Polygon;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            ClipStage::Corner2 => match (i, self.pts.first().copied()) {
                (Input::Point(b), Some(a)) => self.apply(s, json!({ "option": "new", "rectangle": [[a.x, a.y], [b.x, b.y]] })),
                _ => Ok(Step::Continue),
            },
            ClipStage::Polygon => match i {
                Input::Point(p) if self.pts.len() < MAX_CLIP_VERTICES => {
                    self.pts.push(p);
                    Ok(Step::Continue)
                }
                Input::Point(_) => {
                    s.echo(format!("A boundary has at most {MAX_CLIP_VERTICES} points."));
                    Ok(Step::Continue)
                }
                Input::Keyword(_) | Input::Text(_) => {
                    self.pts.pop();
                    Ok(Step::Continue)
                }
                Input::Enter if self.pts.len() >= 3 => {
                    let pts: Vec<[f64; 2]> = self.pts.iter().map(|p| [p.x, p.y]).collect();
                    self.apply(s, json!({ "option": "new", "points": pts }))
                }
                Input::Enter => {
                    s.echo("A polygonal boundary needs at least 3 points.");
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
        }
    }
}

// ---------------- IMAGEADJUST / PDFADJUST prompts ----------------

struct AdjustM {
    kind: Kind,
    sel: SelectPhase,
    hs: Vec<Handle>,
    /// The setting being asked for, once chosen.
    setting: Option<&'static str>,
}

impl AdjustM {
    fn new(kind: Kind) -> Self {
        AdjustM { kind, sel: SelectPhase::default(), hs: Vec::new(), setting: None }
    }
    fn settings(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Image => &["Brightness", "Contrast", "Fade"],
            Kind::Underlay => &["Contrast", "Fade", "Monochrome"],
        }
    }
}

impl Interactive for AdjustM {
    fn name(&self) -> &'static str {
        match self.kind {
            Kind::Image => "IMAGEADJUST",
            Kind::Underlay => "PDFADJUST",
        }
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.sel = SelectPhase::begin(s);
        if self.sel.done {
            self.hs = pick(s, &self.sel.picked.clone(), self.kind);
            if self.hs.is_empty() {
                s.echo(self.kind.nothing());
                return Ok(Step::Done);
            }
        }
        Ok(Step::Continue)
    }
    fn prompt(&self, s: &Session) -> Prompt {
        if self.hs.is_empty() {
            return self.sel.prompt();
        }
        let current = |key: &str| -> String {
            let v = run_adjust_query(s, &self.hs, self.kind);
            v.get(key).map(|x| x.to_string()).unwrap_or_default()
        };
        match self.setting {
            None => Prompt::new("Enter adjustment option", Accept::TEXT).kw(self.settings()),
            Some("monochrome") => {
                Prompt::new("Monochrome?", Accept::TEXT).kw(&["Yes", "No"]).default(if current("monochrome") == "true" { "Yes" } else { "No" })
            }
            Some(k) => Prompt::new(format!("Enter {k} value (0-100)"), Accept::NUMBER).default(current(k)),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if matches!(i, Input::Cancel) {
            return Ok(Step::Cancel);
        }
        if self.hs.is_empty() {
            return match self.sel.feed(s, &i)? {
                SelOutcome::More => Ok(Step::Continue),
                SelOutcome::Empty => Ok(Step::Done),
                SelOutcome::Done(hs) => {
                    self.hs = pick(s, &hs, self.kind);
                    if self.hs.is_empty() {
                        s.echo(self.kind.nothing());
                        return Ok(Step::Done);
                    }
                    Ok(Step::Continue)
                }
            };
        }
        let Some(setting) = self.setting else {
            let k = kw(&i).unwrap_or_default();
            let chosen = ["brightness", "contrast", "fade", "monochrome"].into_iter().find(|n| !k.is_empty() && n.starts_with(&k));
            match chosen.filter(|n| self.settings().iter().any(|o| o.eq_ignore_ascii_case(n))) {
                Some(n) => self.setting = Some(n),
                None if matches!(i, Input::Enter) => return Ok(Step::Done),
                None => s.echo("Invalid option keyword."),
            }
            return Ok(Step::Continue);
        };
        let value = match (&i, setting) {
            (Input::Enter, _) => return Ok(Step::Done),
            (_, "monochrome") => json!(kw(&i).is_some_and(|k| k.starts_with('y'))),
            (_, _) => match kw(&i).and_then(|t| t.parse::<f64>().ok()) {
                Some(v) => json!(v),
                None => {
                    s.echo("Requires a number from 0 to 100.");
                    return Ok(Step::Continue);
                }
            },
        };
        let mut p = json!({ "handles": hexes(&self.hs) });
        p[setting] = value;
        if let Err(e) = run_adjust(s, &p, self.kind) {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        Ok(Step::Done)
    }
}

/// The adjustment values of the first of `hs`, without changing anything.
fn run_adjust_query(s: &Session, hs: &[Handle], kind: Kind) -> Value {
    let Ok(d) = s.doc() else { return Value::Null };
    let Some(EntityKind::Image(i)) = hs.first().and_then(|h| d.entity(*h)).map(|e| &e.kind) else { return Value::Null };
    match (&i.underlay, kind) {
        (Some(u), _) => json!({ "contrast": i.contrast, "fade": i.fade, "monochrome": u.monochrome }),
        (None, _) => json!({ "brightness": i.brightness, "contrast": i.contrast, "fade": i.fade }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_image(underlay: bool) -> (Session, Handle) {
        let mut s = Session::new();
        let img = Image {
            insert: draftcraft_geom::Vec3::new(10.0, 20.0, 0.0),
            size: Vec2::new(100.0, 50.0),
            path: "a.png".into(),
            underlay: underlay.then(|| draftcraft_doc::Underlay { page: "1".into(), ..Default::default() }),
            ..Image::default()
        };
        let h = s.add_entity(EntityKind::Image(img)).unwrap();
        (s, h)
    }

    fn image(s: &Session, h: Handle) -> Image {
        match &s.doc().unwrap().entity(h).unwrap().kind {
            EntityKind::Image(i) => i.clone(),
            _ => panic!("not an image"),
        }
    }

    #[test]
    fn a_rectangle_clips_in_pixels_and_turns_off_and_goes() {
        let (mut s, h) = session_with_image(false);
        let r = s.execute("imageclip", &json!({ "handle": h.hex(), "rectangle": [[20, 30], [40, 60]] })).unwrap();
        assert_eq!(r["option"], "new");
        let i = image(&s, h);
        // (20, 30) is 10 right and 10 up from the corner: pixel (9.5, 39.5).
        assert_eq!(i.clip, vec![Vec2::new(9.5, 39.5), Vec2::new(29.5, 9.5)]);
        assert!(i.clipping && i.display & 4 != 0);
        let q = s.execute("imageclip", &json!({ "handle": h.hex() })).unwrap();
        assert_eq!(q["clips"][0]["boundary"][0], json!([20.0, 30.0]));
        s.execute("imageclip", &json!({ "handle": h.hex(), "option": "off" })).unwrap();
        assert!(!image(&s, h).clipping);
        s.execute("imageclip", &json!({ "handle": h.hex(), "option": "delete" })).unwrap();
        assert!(image(&s, h).clip.is_empty());
        // An image isn't an underlay, and a bad boundary changes nothing.
        assert!(s.execute("pdfclip", &json!({ "handle": h.hex(), "rectangle": [[0, 0], [1, 1]] })).is_err());
        assert!(s.execute("imageclip", &json!({ "handle": h.hex(), "rectangle": [[0, 0], [0, 1]] })).is_err());
        assert!(s.execute("imageclip", &json!({ "handle": h.hex(), "points": [[0, 0], [1, 1]] })).is_err());
    }

    #[test]
    fn an_underlay_clip_is_also_kept_in_inches() {
        let (mut s, h) = session_with_image(true);
        s.execute("pdfclip", &json!({ "handle": h.hex(), "points": [[10, 20], [82, 20], [82, 56]] })).unwrap();
        let i = image(&s, h);
        let u = i.underlay.unwrap();
        assert_eq!(u.clip, vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.5)]);
        assert_eq!(i.clip.len(), 3);
    }

    #[test]
    fn adjust_sets_and_reports_the_values() {
        let (mut s, h) = session_with_image(false);
        let r = s.execute("imageadjust", &json!({ "handle": h.hex(), "brightness": 70, "fade": 10 })).unwrap();
        assert_eq!((r["brightness"].as_u64(), r["contrast"].as_u64(), r["fade"].as_u64()), (Some(70), Some(50), Some(10)));
        assert!(s.execute("imageadjust", &json!({ "handle": h.hex(), "contrast": 101 })).is_err());
        assert!(s.execute("imageadjust", &json!({ "handle": h.hex(), "contrast": "lots" })).is_err());
        let (mut s, h) = session_with_image(true);
        let r = s.execute("pdfadjust", &json!({ "handle": h.hex(), "monochrome": true, "fade": 40 })).unwrap();
        assert_eq!((r["monochrome"].as_bool(), r["fade"].as_u64()), (Some(true), Some(40)));
        assert!(s.execute("pdfadjust", &json!({ "handle": h.hex(), "fade": 90 })).is_err(), "an underlay fades up to 80");
    }

    #[test]
    fn imageclip_prompts_for_a_rectangle() {
        let (mut s, h) = session_with_image(false);
        s.set_selection(vec![h]);
        s.start("imageclip").unwrap();
        s.input(Input::Enter).unwrap();
        s.input(Input::Point(Vec2::new(20.0, 30.0))).unwrap();
        s.input(Input::Point(Vec2::new(40.0, 60.0))).unwrap();
        assert!(s.running.is_none());
        assert_eq!(image(&s, h).clip.len(), 2);
    }
}
