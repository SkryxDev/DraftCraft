//! Touch gestures on the drawing: one finger pans (or moves the measuring reticle), two fingers
//! pinch-zoom and pan together, a double tap zooms in, a two-finger tap zooms out, and a pan let go
//! while moving keeps gliding.
//!
//! The browser's touches arrive as egui `Event::Touch`; [`Gestures::feed`] turns them into view
//! operations ([`Op`]) in the order they happened, so a fast pinch is followed exactly.

use egui::{Pos2, Vec2};

/// How far (points) a finger may move and still tap.
const TAP_SLOP: f32 = 10.0;
/// Longest touch (seconds) that is a tap.
const TAP_TIME: f64 = 0.35;
/// Longest gap (seconds) and distance (points) between the taps of a double tap.
const DOUBLE_TAP_TIME: f64 = 0.32;
const DOUBLE_TAP_DIST: f32 = 40.0;
/// A pan let go faster than this (points per second) keeps gliding.
const FLING_MIN_SPEED: f32 = 250.0;
/// A pan that stopped this long (seconds) before the finger lifted does not glide.
const FLING_STALE: f64 = 0.08;
/// How fast a glide slows down (per second, exponential).
const FLING_DECAY: f32 = 4.5;
/// Fastest glide start (points per second).
const FLING_MAX_SPEED: f32 = 6000.0;
/// Most fingers followed at once.
const MAX_TOUCHES: usize = 10;

/// One touch event, in screen points, with its time in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub id: u64,
    pub phase: Phase,
    pub pos: Pos2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Move,
    End,
}

/// What a gesture does to the view, or to the measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// Move the drawing by this many points.
    Pan(Vec2),
    /// Zoom by `factor` about `anchor`.
    Zoom { anchor: Pos2, factor: f64 },
    /// A quick single tap.
    Tap(Pos2),
    /// Two quick taps close together.
    DoubleTap(Pos2),
    /// Two fingers tapped together, at their centre.
    TwoFingerTap(Pos2),
    /// One finger is down and dragging (reticle mode): where it is.
    Drag(Pos2),
    /// The dragging finger lifted (reticle mode): where it was.
    DragEnd(Pos2),
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    pos: Pos2,
    start: Pos2,
}

/// Touch state between frames.
#[derive(Debug, Default)]
pub struct Gestures {
    fingers: Vec<Finger>,
    /// When the first finger of this sequence touched, and how many touched at most.
    seq_start: f64,
    seq_max: usize,
    /// A finger of this sequence moved past the tap slop.
    moved: bool,
    last_tap: Option<(f64, Pos2)>,
    /// Single-finger pan speed (points per second) and when it was last measured.
    velocity: Vec2,
    last_move: f64,
    /// A glide in progress (points per second).
    fling: Option<Vec2>,
}

impl Gestures {
    /// Whether any finger is down.
    pub fn active(&self) -> bool {
        !self.fingers.is_empty()
    }

    /// Whether a glide is still moving the view (the app keeps repainting meanwhile).
    pub fn gliding(&self) -> bool {
        self.fling.is_some()
    }

    /// Stop a glide (a new view was set).
    pub fn stop(&mut self) {
        self.fling = None;
    }

    /// Turn this frame's touches into view operations. `reticle`: one finger drags the measuring
    /// reticle instead of panning (and double taps are taps). `dt`: seconds since the last frame,
    /// for the glide.
    pub fn feed(&mut self, touches: &[Touch], now: f64, dt: f64, reticle: bool) -> Vec<Op> {
        let mut ops = Vec::new();
        if let Some(v) = self.fling {
            let dt = dt.clamp(0.0, 0.1) as f32;
            ops.push(Op::Pan(v * dt));
            let v = v * (-FLING_DECAY * dt).exp();
            self.fling = (v.length() > 20.0).then_some(v);
        }
        for t in touches {
            match t.phase {
                Phase::Start => self.start(t, now),
                Phase::Move => self.moved_to(t, now, reticle, &mut ops),
                Phase::End => self.end(t, now, reticle, &mut ops),
            }
        }
        ops
    }

    fn start(&mut self, t: &Touch, now: f64) {
        if self.fingers.iter().any(|f| f.id == t.id) || self.fingers.len() >= MAX_TOUCHES {
            return;
        }
        self.fling = None;
        if self.fingers.is_empty() {
            self.seq_start = now;
            self.seq_max = 0;
            self.moved = false;
            self.velocity = Vec2::ZERO;
        }
        // The fingers already down restart from here: a second finger joining must not count
        // as the first one moving.
        for f in &mut self.fingers {
            f.start = f.pos;
        }
        self.fingers.push(Finger { id: t.id, pos: t.pos, start: t.pos });
        self.seq_max = self.seq_max.max(self.fingers.len());
        self.last_move = now;
    }

    fn moved_to(&mut self, t: &Touch, now: f64, reticle: bool, ops: &mut Vec<Op>) {
        if !t.pos.x.is_finite() || !t.pos.y.is_finite() {
            return;
        }
        let Some(i) = self.fingers.iter().position(|f| f.id == t.id) else { return };
        let (c0, s0) = spread(&self.fingers);
        let old = self.fingers.get(i).map(|f| f.pos).unwrap_or(t.pos);
        if let Some(f) = self.fingers.get_mut(i) {
            f.pos = t.pos;
            if (t.pos - f.start).length() > TAP_SLOP {
                self.moved = true;
            }
        }
        let (c1, s1) = spread(&self.fingers);
        if self.fingers.len() == 1 {
            if reticle {
                if self.moved {
                    ops.push(Op::Drag(t.pos));
                }
                return;
            }
            let d = t.pos - old;
            let dt = (now - self.last_move).max(1e-3) as f32;
            // Smooth the speed over the last few moves.
            self.velocity = self.velocity * 0.4 + (d / dt) * 0.6;
            self.last_move = now;
            ops.push(Op::Pan(d));
        } else {
            ops.push(Op::Pan(c1 - c0));
            if s0 > 1.0 && s1 > 1.0 {
                ops.push(Op::Zoom { anchor: c1, factor: f64::from(s1 / s0) });
            }
        }
    }

    fn end(&mut self, t: &Touch, now: f64, reticle: bool, ops: &mut Vec<Op>) {
        let Some(i) = self.fingers.iter().position(|f| f.id == t.id) else { return };
        let lifted = self.fingers.remove(i);
        // The fingers still down restart from here.
        for f in &mut self.fingers {
            f.start = f.pos;
        }
        if !self.fingers.is_empty() {
            return;
        }
        let quick = now - self.seq_start <= TAP_TIME;
        match (self.seq_max, self.moved, quick) {
            (1, false, true) => {
                let double =
                    !reticle && self.last_tap.is_some_and(|(at, p)| now - at <= DOUBLE_TAP_TIME && (p - lifted.pos).length() <= DOUBLE_TAP_DIST);
                if double {
                    self.last_tap = None;
                    ops.push(Op::DoubleTap(lifted.pos));
                } else {
                    self.last_tap = Some((now, lifted.pos));
                    ops.push(Op::Tap(lifted.pos));
                }
            }
            (2, false, true) => ops.push(Op::TwoFingerTap(lifted.pos)),
            (1, true, _) if reticle => ops.push(Op::DragEnd(lifted.pos)),
            (1, true, _) => {
                let v = self.velocity;
                let fresh = now - self.last_move <= FLING_STALE;
                if fresh && v.length() >= FLING_MIN_SPEED && v.x.is_finite() && v.y.is_finite() {
                    let speed = v.length().min(FLING_MAX_SPEED);
                    self.fling = Some(v.normalized() * speed);
                }
            }
            _ => {}
        }
    }
}

/// The centre of the fingers and their mean distance from it.
fn spread(fingers: &[Finger]) -> (Pos2, f32) {
    if fingers.is_empty() {
        return (Pos2::ZERO, 0.0);
    }
    let n = fingers.len() as f32;
    let sum = fingers.iter().fold(Vec2::ZERO, |s, f| s + f.pos.to_vec2());
    let c = (sum / n).to_pos2();
    let r = fingers.iter().map(|f| (f.pos - c).length()).sum::<f32>() / n;
    (c, r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn t(id: u64, phase: Phase, x: f32, y: f32) -> Touch {
        Touch { id, phase, pos: pos2(x, y) }
    }

    fn pans(ops: &[Op]) -> Vec2 {
        ops.iter().fold(Vec2::ZERO, |s, o| if let Op::Pan(d) = o { s + *d } else { s })
    }

    fn zoom(ops: &[Op]) -> f64 {
        ops.iter().fold(1.0, |z, o| if let Op::Zoom { factor, .. } = o { z * factor } else { z })
    }

    /// One finger drags the drawing along.
    #[test]
    fn one_finger_pans() {
        let mut g = Gestures::default();
        g.feed(&[t(1, Phase::Start, 100.0, 100.0)], 0.0, 0.016, false);
        let ops = g.feed(&[t(1, Phase::Move, 130.0, 90.0)], 0.5, 0.016, false);
        assert_eq!(pans(&ops), egui::vec2(30.0, -10.0));
        // Lifted after stopping: no glide.
        g.feed(&[t(1, Phase::End, 130.0, 90.0)], 1.0, 0.016, false);
        assert!(!g.gliding());
    }

    /// Two fingers spreading apart zoom in by the ratio of their spread.
    #[test]
    fn pinch_zooms() {
        let mut g = Gestures::default();
        g.feed(&[t(1, Phase::Start, 100.0, 100.0), t(2, Phase::Start, 200.0, 100.0)], 0.0, 0.016, false);
        let ops = g.feed(&[t(1, Phase::Move, 50.0, 100.0), t(2, Phase::Move, 250.0, 100.0)], 0.1, 0.016, false);
        assert!((zoom(&ops) - 2.0).abs() < 1e-6, "{ops:?}");
        assert!(pans(&ops).length() < 1e-4);
        // Lifting does not tap.
        let ops = g.feed(&[t(1, Phase::End, 50.0, 100.0), t(2, Phase::End, 250.0, 100.0)], 0.2, 0.016, false);
        assert!(ops.is_empty());
    }

    /// Two quick taps zoom in; a two-finger tap zooms out; a slow touch is no tap.
    #[test]
    fn taps() {
        let mut g = Gestures::default();
        g.feed(&[t(1, Phase::Start, 10.0, 10.0)], 0.0, 0.0, false);
        assert_eq!(g.feed(&[t(1, Phase::End, 10.0, 10.0)], 0.1, 0.0, false), vec![Op::Tap(pos2(10.0, 10.0))]);
        g.feed(&[t(2, Phase::Start, 14.0, 12.0)], 0.2, 0.0, false);
        assert_eq!(g.feed(&[t(2, Phase::End, 14.0, 12.0)], 0.25, 0.0, false), vec![Op::DoubleTap(pos2(14.0, 12.0))]);
        g.feed(&[t(3, Phase::Start, 10.0, 10.0), t(4, Phase::Start, 50.0, 10.0)], 1.0, 0.0, false);
        let ops = g.feed(&[t(3, Phase::End, 10.0, 10.0), t(4, Phase::End, 50.0, 10.0)], 1.1, 0.0, false);
        assert!(matches!(ops.as_slice(), [Op::TwoFingerTap(_)]));
        g.feed(&[t(5, Phase::Start, 10.0, 10.0)], 2.0, 0.0, false);
        assert!(g.feed(&[t(5, Phase::End, 10.0, 10.0)], 3.0, 0.0, false).is_empty());
    }

    /// In reticle mode one finger drags the reticle and taps never double.
    #[test]
    fn reticle_mode_drags_instead_of_panning() {
        let mut g = Gestures::default();
        g.feed(&[t(1, Phase::Start, 10.0, 10.0)], 0.0, 0.0, true);
        let ops = g.feed(&[t(1, Phase::Move, 60.0, 10.0)], 0.1, 0.0, true);
        assert_eq!(ops, vec![Op::Drag(pos2(60.0, 10.0))]);
        assert_eq!(g.feed(&[t(1, Phase::End, 60.0, 10.0)], 0.2, 0.0, true), vec![Op::DragEnd(pos2(60.0, 10.0))]);
        for k in 0u32..2 {
            let at = 1.0 + f64::from(k) * 0.1;
            g.feed(&[t(2 + u64::from(k), Phase::Start, 5.0, 5.0)], at, 0.0, true);
            let ops = g.feed(&[t(2 + u64::from(k), Phase::End, 5.0, 5.0)], at + 0.05, 0.0, true);
            assert_eq!(ops, vec![Op::Tap(pos2(5.0, 5.0))]);
        }
    }

    /// A pan let go while moving glides on and slows down to a stop.
    #[test]
    fn fling_glides_and_stops() {
        let mut g = Gestures::default();
        g.feed(&[t(1, Phase::Start, 0.0, 0.0)], 0.0, 0.016, false);
        for k in 1..=5 {
            g.feed(&[t(1, Phase::Move, 20.0 * k as f32, 0.0)], 0.016 * f64::from(k), 0.016, false);
        }
        g.feed(&[t(1, Phase::End, 100.0, 0.0)], 0.09, 0.016, false);
        assert!(g.gliding());
        let mut total = 0.0;
        for _ in 0..600 {
            total += pans(&g.feed(&[], 1.0, 0.016, false)).x;
        }
        assert!(!g.gliding());
        assert!(total > 50.0 && total.is_finite());
    }

    /// Unknown fingers, repeated starts and non-finite positions are ignored.
    #[test]
    fn hostile_touches_are_ignored() {
        let mut g = Gestures::default();
        assert!(g.feed(&[t(9, Phase::Move, 1.0, 1.0), t(9, Phase::End, 1.0, 1.0)], 0.0, 0.0, false).is_empty());
        g.feed(&[t(1, Phase::Start, 0.0, 0.0), t(1, Phase::Start, 5.0, 5.0)], 0.0, 0.0, false);
        let ops = g.feed(&[t(1, Phase::Move, f32::NAN, 0.0)], 0.1, 0.0, false);
        assert!(ops.is_empty());
        for id in 0..50 {
            g.feed(&[t(100 + id, Phase::Start, 0.0, 0.0)], 0.2, 0.0, false);
        }
        assert!(g.fingers.len() <= MAX_TOUCHES);
    }
}
