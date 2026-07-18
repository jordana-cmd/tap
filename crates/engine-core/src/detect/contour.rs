use super::raster::Mask;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// Marching-squares outer contour of a single 4-connected region of set
/// pixels, walked with the region on the left.
///
/// Vertices are pixel-corner lattice points in fractional pixel coordinates
/// (pixel (x, y) spans [x, x+1)×[y, y+1)), so the shoelace of the returned
/// loop equals the region's pixel count exactly. The loop is closed
/// implicitly (first vertex not repeated). Saddle cells — two diagonal pixels
/// set — are resolved by continuing around the pixel the incoming edge hugs,
/// which keeps 4-connected foreground components separate. Interior holes
/// (e.g. columns inside a room) are not traced — the outer contour encloses
/// them; hole deductions are a later feature. Empty mask → empty vec; a mask
/// with several components traces the one containing the topmost-leftmost
/// set pixel.
pub fn trace_contour(region: &Mask) -> Vec<(f64, f64)> {
    let (w, h) = (region.width(), region.height());
    let mut start_px = None;
    'scan: for y in 0..h {
        for x in 0..w {
            if region.get(x, y) {
                start_px = Some((x, y));
                break 'scan;
            }
        }
    }
    let Some((sx, sy)) = start_px else {
        return Vec::new();
    };

    let filled = |px: i64, py: i64| -> bool {
        px >= 0 && py >= 0 && px < w as i64 && py < h as i64 && region.get(px as u32, py as u32)
    };
    // Pixel on the left / right of the edge leaving corner (cx, cy) toward d,
    // in screen coordinates (x right, y down).
    let left_px = |cx: i64, cy: i64, d: Dir| -> (i64, i64) {
        match d {
            Dir::Right => (cx, cy - 1),
            Dir::Left => (cx - 1, cy),
            Dir::Down => (cx, cy),
            Dir::Up => (cx - 1, cy - 1),
        }
    };
    let right_px = |cx: i64, cy: i64, d: Dir| -> (i64, i64) {
        match d {
            Dir::Right => (cx, cy),
            Dir::Left => (cx - 1, cy - 1),
            Dir::Down => (cx - 1, cy),
            Dir::Up => (cx, cy - 1),
        }
    };
    let valid = |cx: i64, cy: i64, d: Dir| -> bool {
        let (lx, ly) = left_px(cx, cy, d);
        let (rx, ry) = right_px(cx, cy, d);
        filled(lx, ly) && !filled(rx, ry)
    };
    let step = |cx: i64, cy: i64, d: Dir| -> (i64, i64) {
        match d {
            Dir::Right => (cx + 1, cy),
            Dir::Left => (cx - 1, cy),
            Dir::Down => (cx, cy + 1),
            Dir::Up => (cx, cy - 1),
        }
    };

    // The topmost-leftmost set pixel's top edge is boundary; region-on-left
    // walks it leftward starting from the pixel's top-right corner.
    let start = ((sx + 1) as i64, sy as i64, Dir::Left);
    debug_assert!(valid(start.0, start.1, start.2));

    let mut out = Vec::new();
    let (mut cx, mut cy, mut dir) = start;
    let max_steps = 4 * (w as usize + 1) * (h as usize + 1);
    for _ in 0..max_steps {
        out.push((cx as f64, cy as f64));
        let hugged = left_px(cx, cy, dir);
        let (nx, ny) = step(cx, cy, dir);
        let valid_dirs: Vec<Dir> = [Dir::Up, Dir::Right, Dir::Down, Dir::Left]
            .into_iter()
            .filter(|&d| valid(nx, ny, d))
            .collect();
        let next_dir = match valid_dirs.as_slice() {
            [d] => *d,
            // Saddle: keep hugging the pixel the incoming edge ran along.
            [_, _] => *valid_dirs
                .iter()
                .find(|&&d| left_px(nx, ny, d) == hugged)
                .expect("saddle always offers a continuation along the hugged pixel"),
            _ => unreachable!("boundary corner must offer 1 or 2 continuations"),
        };
        (cx, cy, dir) = (nx, ny, next_dir);
        if (cx, cy, dir) == start {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{polygon_area, polygon_perimeter, Point};

    fn as_points(contour: &[(f64, f64)]) -> Vec<Point> {
        contour.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn contour_empty_mask_is_empty() {
        assert!(trace_contour(&Mask::new(4, 4)).is_empty());
    }

    #[test]
    fn contour_square_region_exact() {
        // 3×3 block at (1,1) in a 5×5 mask.
        let mut m = Mask::new(5, 5);
        for y in 1..=3 {
            for x in 1..=3 {
                m.set(x, y, true);
            }
        }
        let pts = as_points(&trace_contour(&m));
        assert_eq!(polygon_area(&pts), 9.0); // exact pixel count
        assert_eq!(polygon_perimeter(&pts), 12.0);
    }

    #[test]
    fn contour_l_shape_exact() {
        // L: (0,0),(0,1),(0,2),(1,2),(2,2) → 5 px, perimeter 12.
        let mut m = Mask::new(4, 4);
        for y in 0..=2 {
            m.set(0, y, true);
        }
        m.set(1, 2, true);
        m.set(2, 2, true);
        let pts = as_points(&trace_contour(&m));
        assert_eq!(polygon_area(&pts), 5.0);
        assert_eq!(polygon_perimeter(&pts), 12.0);
    }

    #[test]
    fn contour_saddle_prefers_incoming_region() {
        // Diagonal pixels (0,0) and (1,1): two 4-connected components meeting
        // at a saddle corner. The trace must stay on the first component.
        let mut m = Mask::new(2, 2);
        m.set(0, 0, true);
        m.set(1, 1, true);
        let pts = as_points(&trace_contour(&m));
        assert_eq!(pts.len(), 4);
        assert_eq!(polygon_area(&pts), 1.0); // one pixel, not two
    }

    #[test]
    fn contour_border_touching_region_is_traced() {
        // Region flush with the raster edge: virtual empty border handles it.
        let mut m = Mask::new(3, 3);
        for y in 0..3 {
            for x in 0..3 {
                m.set(x, y, true);
            }
        }
        let pts = as_points(&trace_contour(&m));
        assert_eq!(polygon_area(&pts), 9.0);
    }
}
