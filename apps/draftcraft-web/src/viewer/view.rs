//! The view: which part of the drawing the screen shows, and how gestures move it.

use draftcraft_engine::geom::{Bounds2, Vec2};
use egui::{Pos2, Rect, pos2};

/// Closest and farthest zoom, in screen points per drawing unit: past these the f32 GPU
/// coordinates break down and nothing useful can be seen.
const MIN_SCALE: f64 = 1e-9;
const MAX_SCALE: f64 = 1e9;

/// Room left around the drawing by [`View::fit`], as a share of the screen.
const FIT_MARGIN: f64 = 0.06;

/// The world point at the screen's centre and the zoom (points per drawing unit), y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub center: Vec2,
    pub scale: f64,
}

impl Default for View {
    fn default() -> Self {
        View { center: Vec2::ZERO, scale: 1.0 }
    }
}

impl View {
    pub fn to_screen(self, rect: Rect, p: Vec2) -> Pos2 {
        let c = rect.center();
        pos2(c.x + ((p.x - self.center.x) * self.scale) as f32, c.y - ((p.y - self.center.y) * self.scale) as f32)
    }

    pub fn to_world(self, rect: Rect, p: Pos2) -> Vec2 {
        let c = rect.center();
        Vec2::new(self.center.x + f64::from(p.x - c.x) / self.scale, self.center.y - f64::from(p.y - c.y) / self.scale)
    }

    /// The world box on screen.
    pub fn world_bounds(self, rect: Rect) -> Bounds2 {
        Bounds2::new(self.to_world(rect, rect.left_bottom()), self.to_world(rect, rect.right_top()))
    }

    /// Drawing units per screen point.
    pub fn units_per_point(self) -> f64 {
        1.0 / self.scale
    }

    /// The view showing all of `bounds` in `rect` with a margin; the current view when `bounds` is
    /// empty or not finite.
    pub fn fit(self, bounds: &Bounds2, rect: Rect) -> View {
        let (w, h) = (f64::from(rect.width()), f64::from(rect.height()));
        if bounds.is_empty() || !bounds.min.is_finite() || !bounds.max.is_finite() || w <= 0.0 || h <= 0.0 {
            return self;
        }
        let (bw, bh) = (bounds.width().max(1e-12), bounds.height().max(1e-12));
        let scale = ((w / bw).min(h / bh) * (1.0 - 2.0 * FIT_MARGIN)).clamp(MIN_SCALE, MAX_SCALE);
        View { center: bounds.center(), scale }
    }

    /// Move the drawing by `d` screen points (it follows the finger).
    pub fn pan(&mut self, d: egui::Vec2) {
        let next = Vec2::new(self.center.x - f64::from(d.x) / self.scale, self.center.y + f64::from(d.y) / self.scale);
        if next.is_finite() {
            self.center = next;
        }
    }

    /// Zoom by `factor` (> 1 closer) keeping the world point under `anchor` where it is.
    pub fn zoom_at(&mut self, rect: Rect, anchor: Pos2, factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let before = self.to_world(rect, anchor);
        let scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        let after = self.to_world(rect, anchor);
        let next = self.center + (before - after);
        if next.is_finite() {
            self.center = next;
        }
    }
}

/// The level-of-detail band of a zoom: curves are flattened to half a point at the band's finest
/// zoom, so one display list serves every zoom in the band (each band is twice the last).
pub fn band(units_per_point: f64) -> i32 {
    if !(units_per_point.is_finite() && units_per_point > 0.0) {
        return 0;
    }
    units_per_point.log2().ceil().clamp(-200.0, 200.0) as i32
}

/// Drawing units per point at the finest zoom of `band`.
pub fn band_units(band: i32) -> f64 {
    2f64.powi(band)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 800.0))
    }

    /// World and screen round-trip, with y up in the world and down on screen.
    #[test]
    fn screen_and_world_round_trip() {
        let v = View { center: Vec2::new(10.0, 20.0), scale: 4.0 };
        let p = v.to_screen(rect(), Vec2::new(12.0, 21.0));
        assert_eq!(p, pos2(208.0, 396.0));
        let w = v.to_world(rect(), p);
        assert!((w.x - 12.0).abs() < 1e-9 && (w.y - 21.0).abs() < 1e-9);
    }

    /// Fitting centres the drawing and leaves the margin on the tighter side.
    #[test]
    fn fit_shows_everything() {
        let b = Bounds2::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));
        let v = View::default().fit(&b, rect());
        assert_eq!(v.center, Vec2::new(50.0, 25.0));
        assert!((v.scale - 4.0 * (1.0 - 2.0 * FIT_MARGIN)).abs() < 1e-9);
        let wb = v.world_bounds(rect());
        assert!(wb.contains_box(&b));
        // Nothing to fit: the view stays.
        assert_eq!(v.fit(&Bounds2::EMPTY, rect()), v);
    }

    /// Zooming keeps the point under the fingers still; hostile factors are ignored.
    #[test]
    fn zoom_keeps_the_anchor_still() {
        let mut v = View { center: Vec2::new(0.0, 0.0), scale: 1.0 };
        let anchor = pos2(300.0, 100.0);
        let before = v.to_world(rect(), anchor);
        v.zoom_at(rect(), anchor, 3.0);
        let after = v.to_world(rect(), anchor);
        assert!(before.dist(after) < 1e-9);
        assert!((v.scale - 3.0).abs() < 1e-12);
        let same = v;
        for f in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            v.zoom_at(rect(), anchor, f);
        }
        assert_eq!(v, same);
        v.zoom_at(rect(), anchor, 1e30);
        assert_eq!(v.scale, MAX_SCALE);
    }

    /// Panning moves the drawing with the finger.
    #[test]
    fn pan_follows_the_finger() {
        let mut v = View { center: Vec2::ZERO, scale: 2.0 };
        let p = v.to_screen(rect(), Vec2::new(5.0, 5.0));
        v.pan(egui::vec2(30.0, -10.0));
        let q = v.to_screen(rect(), Vec2::new(5.0, 5.0));
        assert!((q.x - p.x - 30.0).abs() < 1e-4 && (q.y - p.y + 10.0).abs() < 1e-4);
    }

    #[test]
    fn bands_double() {
        assert_eq!(band(1.0), 0);
        assert_eq!(band(0.3), -1);
        assert_eq!(band(3.0), 2);
        assert_eq!(band(f64::NAN), 0);
        assert_eq!(band(0.0), 0);
        assert!((band_units(-2) - 0.25).abs() < 1e-12);
    }
}
