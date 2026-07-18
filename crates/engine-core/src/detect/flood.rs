use super::raster::Mask;
use super::DetectError;

/// Scanline flood fill (iterative — explicit span stack, never recursive)
/// from `seed` across non-wall pixels, 4-connected.
///
/// Aborts with [`DetectError::RegionNotEnclosed`] the moment any filled span
/// touches a raster-edge pixel — fall back to manual trace, never guess
/// (addendum §A3.1 step 3). On success every filled pixel is strictly
/// interior.
pub fn flood_fill(walls: &Mask, seed: (u32, u32)) -> Result<Mask, DetectError> {
    let (w, h) = (walls.width(), walls.height());
    let (sx, sy) = seed;
    if sx >= w || sy >= h {
        return Err(DetectError::SeedOutOfBounds { x: sx, y: sy });
    }
    if walls.get(sx, sy) {
        return Err(DetectError::SeedOnWall { x: sx, y: sy });
    }

    let wu = w as usize;
    let wall = walls.as_slice();
    let mut filled = vec![false; wu * h as usize];
    let mut stack: Vec<(u32, u32)> = vec![(sx, sy)];

    while let Some((x, y)) = stack.pop() {
        let row = y as usize * wu;
        if filled[row + x as usize] || wall[row + x as usize] {
            continue;
        }
        // Expand to the maximal open span containing (x, y).
        let mut lx = x;
        while lx > 0 && !wall[row + (lx - 1) as usize] {
            lx -= 1;
        }
        let mut rx = x;
        while rx + 1 < w && !wall[row + (rx + 1) as usize] {
            rx += 1;
        }
        // Boundary abort: the region is open to the raster edge.
        if y == 0 || y == h - 1 || lx == 0 || rx == w - 1 {
            return Err(DetectError::RegionNotEnclosed);
        }
        for xx in lx..=rx {
            filled[row + xx as usize] = true;
        }
        // Seed one point per open run in the rows above and below.
        for ny in [y - 1, y + 1] {
            let nrow = ny as usize * wu;
            let mut xx = lx;
            while xx <= rx {
                if !wall[nrow + xx as usize] && !filled[nrow + xx as usize] {
                    stack.push((xx, ny));
                    while xx <= rx && !wall[nrow + xx as usize] {
                        xx += 1;
                    }
                } else {
                    xx += 1;
                }
            }
        }
    }
    Ok(Mask::from_raw(w, h, filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ring of walls at x/y ∈ {1, 5} inside a 7×7 mask.
    fn ring_mask() -> Mask {
        let mut m = Mask::new(7, 7);
        for i in 1..=5 {
            m.set(i, 1, true);
            m.set(i, 5, true);
            m.set(1, i, true);
            m.set(5, i, true);
        }
        m
    }

    #[test]
    fn flood_fill_fills_enclosed_rect() {
        let filled = flood_fill(&ring_mask(), (3, 3)).unwrap();
        assert_eq!(filled.count(), 9); // 3×3 interior
        for y in 2..=4 {
            for x in 2..=4 {
                assert!(filled.get(x, y));
            }
        }
    }

    #[test]
    fn flood_fill_scanline_handles_concave_region() {
        // 9×9 ring at {1, 7} with a divider descending from the top:
        // column x=4, y = 2..=4. The fill must wrap under it.
        let mut m = Mask::new(9, 9);
        for i in 1..=7 {
            m.set(i, 1, true);
            m.set(i, 7, true);
            m.set(1, i, true);
            m.set(7, i, true);
        }
        for y in 2..=4 {
            m.set(4, y, true);
        }
        let filled = flood_fill(&m, (2, 2)).unwrap();
        // 5×5 interior minus 3 divider pixels.
        assert_eq!(filled.count(), 22);
        assert!(filled.get(6, 2)); // reached the far arm around the divider
    }

    #[test]
    fn flood_fill_seed_on_wall_errors() {
        assert_eq!(
            flood_fill(&ring_mask(), (1, 3)),
            Err(DetectError::SeedOnWall { x: 1, y: 3 })
        );
    }

    #[test]
    fn flood_fill_seed_out_of_bounds_errors() {
        assert_eq!(
            flood_fill(&ring_mask(), (99, 0)),
            Err(DetectError::SeedOutOfBounds { x: 99, y: 0 })
        );
    }

    #[test]
    fn flood_fill_open_region_hits_boundary() {
        // No walls at all: the first span reaches both x edges.
        let empty = Mask::new(7, 7);
        assert_eq!(
            flood_fill(&empty, (3, 3)),
            Err(DetectError::RegionNotEnclosed)
        );
    }

    #[test]
    fn flood_fill_vertical_corridor_hits_top_boundary() {
        // Walls only in columns 1 and 3: spans in column 2 climb to row 0.
        let mut m = Mask::new(5, 7);
        for y in 0..7 {
            m.set(1, y, true);
            m.set(3, y, true);
        }
        assert_eq!(flood_fill(&m, (2, 3)), Err(DetectError::RegionNotEnclosed));
    }
}
