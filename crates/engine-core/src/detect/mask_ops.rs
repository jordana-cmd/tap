use super::raster::{GrayRaster, Mask};

/// Wall mask: luminance strictly below `wall_threshold` → wall (`true`).
pub fn threshold_mask(raster: &GrayRaster, wall_threshold: u8) -> Mask {
    let data = raster
        .as_slice()
        .iter()
        .map(|&lum| lum < wall_threshold)
        .collect();
    Mask::from_raw(raster.width(), raster.height(), data)
}

/// Dilation radius that closes a door opening of `door_gap_ft`:
/// `ceil((door_gap_ft / 2) × px_per_foot)` — ceil so `2r` always covers the gap.
pub fn door_gap_radius_px(door_gap_ft: f64, px_per_foot: f64) -> u32 {
    ((door_gap_ft / 2.0) * px_per_foot).ceil().max(0.0) as u32
}

/// Binary dilation with a square (Chebyshev) structuring element of the given
/// radius, as two separable 1-D passes. O(set pixels × radius) per pass.
///
/// Square rather than disc: identical gap-closing on axis-aligned door
/// openings, exact corner preservation under the closing pipeline for
/// rectangular rooms, and no per-pixel disc scan.
pub fn dilate(mask: &Mask, radius_px: u32) -> Mask {
    if radius_px == 0 {
        return mask.clone();
    }
    let w = mask.width() as usize;
    let h = mask.height() as usize;
    let r = radius_px as usize;
    let src = mask.as_slice();

    // Horizontal pass: each set pixel paints [x-r, x+r] in its row.
    let mut hor = vec![false; w * h];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut hor[y * w..(y + 1) * w];
        for (x, &set) in row.iter().enumerate() {
            if set {
                let lo = x.saturating_sub(r);
                let hi = (x + r).min(w - 1);
                for o in &mut out[lo..=hi] {
                    *o = true;
                }
            }
        }
    }

    // Vertical pass over the horizontal result.
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            if hor[y * w + x] {
                let lo = y.saturating_sub(r);
                let hi = (y + r).min(h - 1);
                for yy in lo..=hi {
                    out[yy * w + x] = true;
                }
            }
        }
    }
    Mask::from_raw(mask.width(), mask.height(), out)
}

/// The dilate-back half of the morphological closing (addendum §A3.1
/// implementation refinement): expand the fill by the same radius used to
/// dilate the walls, then remove original wall pixels. Recovers the room to
/// its true wall faces instead of the r-inset interior the fill sees.
pub fn close_region(fill: &Mask, walls: &Mask, radius_px: u32) -> Mask {
    let mut closed = dilate(fill, radius_px);
    for (c, &wall) in closed.as_mut_slice().iter_mut().zip(walls.as_slice()) {
        if wall {
            *c = false;
        }
    }
    closed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_boundary_values() {
        let r = GrayRaster::new(2, 1, vec![199, 200]).unwrap();
        let m = threshold_mask(&r, 200);
        assert!(m.get(0, 0));
        assert!(!m.get(1, 0));
    }

    #[test]
    fn door_gap_radius_formula() {
        // 3.5 ft at 10 px/ft: ceil(17.5) = 18.
        assert_eq!(door_gap_radius_px(3.5, 10.0), 18);
        assert_eq!(door_gap_radius_px(0.0, 10.0), 0);
        // 12 ft equipment door needs door_gap_ft ≥ 12 to seal: r = 60 covers 120 px.
        assert_eq!(door_gap_radius_px(12.0, 10.0), 60);
    }

    #[test]
    fn dilate_zero_radius_is_identity() {
        let mut m = Mask::new(4, 4);
        m.set(1, 2, true);
        assert_eq!(dilate(&m, 0), m);
    }

    #[test]
    fn dilate_radius_one_makes_3x3_block() {
        let mut m = Mask::new(5, 5);
        m.set(2, 2, true);
        let d = dilate(&m, 1);
        assert_eq!(d.count(), 9);
        for y in 1..=3 {
            for x in 1..=3 {
                assert!(d.get(x, y));
            }
        }
    }

    #[test]
    fn dilate_clamps_at_edges() {
        let mut m = Mask::new(3, 3);
        m.set(0, 0, true);
        let d = dilate(&m, 2);
        // Would extend to negative coords; clamped to the raster.
        assert_eq!(d.count(), 9);
    }

    #[test]
    fn close_region_recovers_full_interior() {
        // 9×9: wall ring at x/y ∈ {2, 6}, interior 3×3 at (3..=5)².
        let mut walls = Mask::new(9, 9);
        for i in 2..=6 {
            walls.set(i, 2, true);
            walls.set(i, 6, true);
            walls.set(2, i, true);
            walls.set(6, i, true);
        }
        let dilated = dilate(&walls, 1);
        // After dilation the fillable interior shrinks to the single pixel (4,4).
        let mut fill = Mask::new(9, 9);
        fill.set(4, 4, true);
        let closed = close_region(&fill, &walls, 1);
        // Closing recovers the full 3×3 interior exactly, and nothing more.
        assert_eq!(closed.count(), 9);
        for y in 3..=5 {
            for x in 3..=5 {
                assert!(closed.get(x, y));
            }
        }
        // Sanity: the dilated ring really did exclude the interior band.
        assert!(dilated.get(3, 3));
    }
}
