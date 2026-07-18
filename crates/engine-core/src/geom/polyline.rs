use super::Point;

/// Euclidean distance between two points, in PDF points.
pub fn distance(a: Point, b: Point) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// Sum of segment lengths along the path, in PDF points.
/// Fewer than 2 points → 0.0.
pub fn polyline_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| distance(w[0], w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_3_4_5_triangle() {
        assert_eq!(distance(Point::new(0.0, 0.0), Point::new(3.0, 4.0)), 5.0);
    }

    #[test]
    fn distance_coincident_is_zero() {
        let p = Point::new(17.25, -3.5);
        assert_eq!(distance(p, p), 0.0);
    }

    #[test]
    fn polyline_length_open_square_path() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert_eq!(polyline_length(&pts), 30.0);
    }

    #[test]
    fn polyline_length_under_two_points_is_zero() {
        assert_eq!(polyline_length(&[]), 0.0);
        assert_eq!(polyline_length(&[Point::new(5.0, 5.0)]), 0.0);
    }
}
