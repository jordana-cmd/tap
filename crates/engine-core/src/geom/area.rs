use super::Point;

/// Unsigned shoelace area of a polygon, in PDF points².
///
/// Auto-closes an open ring (last→first), so a repeated first point is
/// harmless. Fewer than 3 points → 0.0; collinear input → 0.0.
pub fn polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut twice_area = 0.0;
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        twice_area += a.x * b.y - b.x * a.y;
    }
    twice_area.abs() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_right_triangle_is_6() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(0.0, 3.0),
        ];
        assert_eq!(polygon_area(&pts), 6.0);
    }

    #[test]
    fn area_l_shape_hand_computed() {
        // 6×4 rectangle minus a 2×2 notch at the top-right corner = 24 − 4 = 20.
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(6.0, 0.0),
            Point::new(6.0, 2.0),
            Point::new(4.0, 2.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
        ];
        assert_eq!(polygon_area(&pts), 20.0);
    }

    #[test]
    fn area_cw_equals_ccw() {
        let ccw = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
        ];
        let cw: Vec<Point> = ccw.iter().rev().copied().collect();
        assert_eq!(polygon_area(&ccw), 12.0);
        assert_eq!(polygon_area(&cw), 12.0);
    }

    #[test]
    fn area_collinear_is_zero() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(10.0, 10.0),
        ];
        assert_eq!(polygon_area(&pts), 0.0);
    }

    #[test]
    fn area_open_ring_auto_closes() {
        let open = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
        ];
        let closed = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 3.0),
            Point::new(0.0, 3.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(polygon_area(&open), polygon_area(&closed));
    }

    #[test]
    fn area_under_three_points_is_zero() {
        assert_eq!(polygon_area(&[]), 0.0);
        assert_eq!(polygon_area(&[Point::new(1.0, 1.0)]), 0.0);
        assert_eq!(
            polygon_area(&[Point::new(1.0, 1.0), Point::new(2.0, 2.0)]),
            0.0
        );
    }
}
