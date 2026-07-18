use super::raster::MAX_RASTER_PIXELS;
use super::DetectError;
use crate::scale::{Scale, POINTS_PER_INCH};
use crate::Point;

/// Addendum §A3.1 target: raster resolution scales with drawing scale
/// (px_per_foot in this range), not fixed DPI.
pub const TARGET_PX_PER_FOOT: core::ops::RangeInclusive<f64> = 8.0..=12.0;

/// Maps a raster's pixel grid onto page base units (PDF points).
///
/// Pixel (x, y) spans the unit square [x, x+1)×[y, y+1) in pixel space with
/// its center at (x + 0.5, y + 0.5); `points_per_px = 72 / (fpi × px_per_foot)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelMap {
    origin: Point,
    scale: Scale,
    px_per_foot: f64,
}

impl PixelMap {
    /// `origin` is the base-unit position of pixel (0, 0)'s top-left corner.
    /// Errors unless `px_per_foot` is finite and > 0.
    pub fn new(origin: Point, scale: Scale, px_per_foot: f64) -> Result<PixelMap, DetectError> {
        if px_per_foot.is_finite() && px_per_foot > 0.0 {
            Ok(PixelMap {
                origin,
                scale,
                px_per_foot,
            })
        } else {
            Err(DetectError::InvalidPxPerFoot(px_per_foot))
        }
    }

    pub fn scale(&self) -> Scale {
        self.scale
    }

    pub fn px_per_foot(&self) -> f64 {
        self.px_per_foot
    }

    /// PDF points per pixel: one real foot spans `72 / fpi` points on paper.
    pub fn points_per_px(&self) -> f64 {
        (POINTS_PER_INCH / self.scale.fpi()) / self.px_per_foot
    }

    /// Fractional pixel coordinates → base units.
    pub fn px_to_point(&self, px: f64, py: f64) -> Point {
        let ppp = self.points_per_px();
        Point::new(self.origin.x + px * ppp, self.origin.y + py * ppp)
    }

    /// Base units → fractional pixel coordinates.
    pub fn point_to_px(&self, p: Point) -> (f64, f64) {
        let ppp = self.points_per_px();
        ((p.x - self.origin.x) / ppp, (p.y - self.origin.y) / ppp)
    }

    /// Pixel count → square feet. For cheap FILTERING only — reported areas
    /// come from the shoelace on the simplified contour, never pixel count.
    pub fn px_area_to_sf(&self, pixels: f64) -> f64 {
        pixels / (self.px_per_foot * self.px_per_foot)
    }
}

/// Pick px_per_foot for a region: 10.0 (mid [`TARGET_PX_PER_FOOT`]), reduced
/// only as far as needed to keep `region × px_per_foot²` within
/// [`MAX_RASTER_PIXELS`] — the memory cap wins over the target range.
pub fn choose_px_per_foot(region_w_ft: f64, region_h_ft: f64) -> f64 {
    let area_ft2 = (region_w_ft * region_h_ft).max(f64::MIN_POSITIVE);
    let fits_cap = (MAX_RASTER_PIXELS as f64 / area_ft2).sqrt();
    10.0_f64.min(fits_cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> PixelMap {
        PixelMap::new(Point::new(0.0, 0.0), Scale::from_fpi(4.0).unwrap(), 10.0).unwrap()
    }

    #[test]
    fn pixelmap_points_per_px_and_roundtrip() {
        // fpi 4.0: one foot = 18 pts on paper; at 10 px/ft → 1.8 pts/px.
        let m = map();
        assert_eq!(m.points_per_px(), 1.8);
        let p = m.px_to_point(100.0, 50.0);
        assert_eq!(p, Point::new(180.0, 90.0));
        let (px, py) = m.point_to_px(p);
        assert!((px - 100.0).abs() < 1e-9 && (py - 50.0).abs() < 1e-9);
    }

    #[test]
    fn pixelmap_rejects_bad_px_per_foot() {
        let scale = Scale::from_fpi(4.0).unwrap();
        for bad in [0.0, -10.0, f64::NAN, f64::INFINITY] {
            assert!(PixelMap::new(Point::new(0.0, 0.0), scale, bad).is_err());
        }
    }

    #[test]
    fn pixelmap_px_area_to_sf() {
        // 100 px² at 10 px/ft = 1 SF.
        assert_eq!(map().px_area_to_sf(100.0), 1.0);
    }

    #[test]
    fn choose_px_per_foot_in_target_range_and_cap_wins() {
        // Ordinary room region → the 10 px/ft target.
        let ppf = choose_px_per_foot(20.0, 15.0);
        assert_eq!(ppf, 10.0);
        assert!(TARGET_PX_PER_FOOT.contains(&ppf));

        // A 10,000×10,000 ft region cannot fit at 8 px/ft — the cap wins.
        let ppf = choose_px_per_foot(10_000.0, 10_000.0);
        assert!(ppf < *TARGET_PX_PER_FOOT.start());
        let pixels = (10_000.0 * ppf) * (10_000.0 * ppf);
        assert!(pixels <= MAX_RASTER_PIXELS as f64 * 1.000_001);
    }
}
