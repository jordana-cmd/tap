//! Shared helpers for integration and property tests.
#![allow(dead_code)]

use engine_core::detect::PixelMap;
use engine_core::{distance, GrayRaster, Point, Scale};

/// Assert |a − b| ≤ abs_tol + rel_tol × max(|a|, |b|).
#[track_caller]
pub fn assert_close(a: f64, b: f64, abs_tol: f64, rel_tol: f64) {
    let tol = abs_tol + rel_tol * a.abs().max(b.abs());
    assert!(
        (a - b).abs() <= tol,
        "expected {b}, got {a} (|diff| = {}, tol = {tol})",
        (a - b).abs()
    );
}

/// Distance from `p` to segment a–b (projection clamped to the segment).
pub fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return distance(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    distance(p, Point::new(a.x + t * dx, a.y + t * dy))
}

/// Min distance from `p` to a polyline (≥ 2 points).
pub fn polyline_distance(p: Point, polyline: &[Point]) -> f64 {
    polyline
        .windows(2)
        .map(|w| segment_distance(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Synthetic plan-sheet builder: a white (255) raster drawn in FEET at a
/// fixed px_per_foot; walls are dark (0) bands, door gaps are erased back to
/// white. Coordinates snap to whole pixels via rounding, so dimensions in
/// 0.1-ft steps at 10 px/ft are exact.
pub struct SynthPlan {
    width_px: u32,
    height_px: u32,
    px_per_foot: f64,
    data: Vec<u8>,
}

impl SynthPlan {
    pub fn new(width_ft: f64, height_ft: f64, px_per_foot: f64) -> SynthPlan {
        let width_px = (width_ft * px_per_foot).round() as u32;
        let height_px = (height_ft * px_per_foot).round() as u32;
        SynthPlan {
            width_px,
            height_px,
            px_per_foot,
            data: vec![255; width_px as usize * height_px as usize],
        }
    }

    fn px(&self, ft: f64) -> i64 {
        (ft * self.px_per_foot).round() as i64
    }

    /// Paint the rectangle [x, x+w) × [y, y+h) (in feet) with `value`.
    pub fn fill_rect_ft(&mut self, x: f64, y: f64, w: f64, h: f64, value: u8) {
        let (x0, y0) = (self.px(x).max(0), self.px(y).max(0));
        let (x1, y1) = (
            self.px(x + w).min(self.width_px as i64),
            self.px(y + h).min(self.height_px as i64),
        );
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.data[yy as usize * self.width_px as usize + xx as usize] = value;
            }
        }
    }

    /// Draw a room: walls of thickness `t` ft around the interior rectangle
    /// [x, x+w) × [y, y+h) ft (dark outer box, then the interior cleared).
    /// Draw outer walls before inner partitions — the clear pass erases
    /// anything previously drawn inside the interior.
    pub fn walls_rect(&mut self, x: f64, y: f64, w: f64, h: f64, t: f64) {
        self.fill_rect_ft(x - t, y - t, w + 2.0 * t, h + 2.0 * t, 0);
        self.fill_rect_ft(x, y, w, h, 255);
    }

    /// Punch a door opening (erase walls back to white) in the given rect.
    pub fn gap_ft(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.fill_rect_ft(x, y, w, h, 255);
    }

    pub fn seed_at_ft(&self, x: f64, y: f64) -> (u32, u32) {
        (self.px(x) as u32, self.px(y) as u32)
    }

    pub fn raster(&self) -> GrayRaster {
        GrayRaster::new(self.width_px, self.height_px, self.data.clone()).unwrap()
    }

    pub fn map(&self, scale: Scale) -> PixelMap {
        PixelMap::new(Point::new(0.0, 0.0), scale, self.px_per_foot).unwrap()
    }
}

/// Test-only inverse of `format_feet_inches` ("F'-I n/d\"" → decimal feet).
pub fn parse_feet_inches(s: &str) -> f64 {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1.0, r),
        None => (1.0, s),
    };
    let rest = rest.strip_suffix('"').expect("trailing inch mark");
    let (ft_s, in_s) = rest.split_once("'-").expect("feet-inches separator");
    let feet: f64 = ft_s.parse().unwrap();
    let inches = match in_s.split_once(' ') {
        Some((whole, frac)) => {
            let (num, den) = frac.split_once('/').expect("fraction");
            whole.parse::<f64>().unwrap()
                + num.parse::<f64>().unwrap() / den.parse::<f64>().unwrap()
        }
        None => in_s.parse::<f64>().unwrap(),
    };
    sign * (feet + inches / 12.0)
}
