//! Shared helpers for integration and property tests.
#![allow(dead_code)]

use engine_core::detect::PixelMap;
use engine_core::{distance, GrayRaster, Point, Scale, Segment};

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

/// Synthetic segment-set builder (vector twin of [`SynthPlan`]): segments
/// drawn in FEET, stored in base units (PDF points) with explicit stroke
/// widths in points. Door openings are expressed by emitting split wall
/// segments (`wall_rect_ft` has no gaps; compose with `seg_ft`).
pub struct SynthSegments {
    width_ft: f64,
    height_ft: f64,
    px_per_foot: f64,
    scale: Scale,
    segments: Vec<Segment>,
}

impl SynthSegments {
    pub fn new(width_ft: f64, height_ft: f64, scale: Scale, px_per_foot: f64) -> SynthSegments {
        SynthSegments {
            width_ft,
            height_ft,
            px_per_foot,
            scale,
            segments: Vec::new(),
        }
    }

    /// Feet → base units (PDF points): `ft × 72 / fpi`.
    fn pts(&self, ft: f64) -> f64 {
        ft * 72.0 / self.scale.fpi()
    }

    /// One stroked segment from (x1,y1) to (x2,y2) in feet, stroke width in
    /// POINTS (the unit extraction reports).
    pub fn seg_ft(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke_pts: f64) {
        let s = Segment {
            p1: Point::new(self.pts(x1), self.pts(y1)),
            p2: Point::new(self.pts(x2), self.pts(y2)),
            width: stroke_pts,
        };
        self.segments.push(s);
    }

    /// Four wall segments outlining the rectangle [x, x+w] × [y, y+h] ft.
    pub fn wall_rect_ft(&mut self, x: f64, y: f64, w: f64, h: f64, stroke_pts: f64) {
        self.seg_ft(x, y, x + w, y, stroke_pts);
        self.seg_ft(x + w, y, x + w, y + h, stroke_pts);
        self.seg_ft(x + w, y + h, x, y + h, stroke_pts);
        self.seg_ft(x, y + h, x, y, stroke_pts);
    }

    /// Fill the rectangle [x, x+w] × [y, y+h] ft with a hatch field of
    /// parallel lines at `angle_deg` (0 = horizontal), spaced `pitch_ft`
    /// apart, clipped to the rectangle.
    pub fn hatch_field_ft(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        angle_deg: f64,
        pitch_ft: f64,
        stroke_pts: f64,
    ) {
        let ang = angle_deg.to_radians();
        let (ux, uy) = (ang.cos(), ang.sin());
        let (nx, ny) = (-uy, ux);
        // Offsets along the normal that cover the rectangle's corners.
        let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
        let rhos: Vec<f64> = corners.iter().map(|&(cx, cy)| cx * nx + cy * ny).collect();
        let (rho_min, rho_max) = (
            rhos.iter().cloned().fold(f64::INFINITY, f64::min),
            rhos.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        let ss: Vec<f64> = corners.iter().map(|&(cx, cy)| cx * ux + cy * uy).collect();
        let (s_min, s_max) = (
            ss.iter().cloned().fold(f64::INFINITY, f64::min),
            ss.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        let mut rho = rho_min + pitch_ft;
        while rho < rho_max {
            // Line at offset rho spanning [s_min, s_max], Liang-Barsky
            // clipped to the rectangle (correct for any angle).
            let (ax, ay) = (rho * nx + s_min * ux, rho * ny + s_min * uy);
            let (bx, by) = (rho * nx + s_max * ux, rho * ny + s_max * uy);
            let (dx, dy) = (bx - ax, by - ay);
            let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
            let mut visible = true;
            for (p, q) in [
                (-dx, ax - x),
                (dx, x + w - ax),
                (-dy, ay - y),
                (dy, y + h - ay),
            ] {
                if p == 0.0 {
                    if q < 0.0 {
                        visible = false;
                    }
                } else {
                    let r = q / p;
                    if p < 0.0 {
                        t0 = t0.max(r);
                    } else {
                        t1 = t1.min(r);
                    }
                }
            }
            if visible && t0 < t1 {
                self.seg_ft(ax + t0 * dx, ay + t0 * dy, ax + t1 * dx, ay + t1 * dy, stroke_pts);
            }
            rho += pitch_ft;
        }
    }

    pub fn segments(&self) -> Vec<Segment> {
        self.segments.clone()
    }

    pub fn map(&self) -> PixelMap {
        PixelMap::new(Point::new(0.0, 0.0), self.scale, self.px_per_foot).unwrap()
    }

    /// Pixel grid covering the declared sheet extent.
    pub fn grid(&self) -> (u32, u32) {
        (
            (self.width_ft * self.px_per_foot).round() as u32,
            (self.height_ft * self.px_per_foot).round() as u32,
        )
    }

    pub fn seed_at_ft(&self, x: f64, y: f64) -> (u32, u32) {
        (
            (x * self.px_per_foot).round() as u32,
            (y * self.px_per_foot).round() as u32,
        )
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
