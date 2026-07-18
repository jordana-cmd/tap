//! Exact snap math and the epsilon policy.

use crate::Point;

/// Relative epsilon for parallel/collinear rejection: segment pairs with
/// `|cross(d1, d2)| ≤ EPS_PARALLEL × |d1| × |d2|` produce no intersection
/// point. This covers duplicates and collinear overlaps (no unique point
/// exists — endpoints are served by the endpoint pass, interiors by the
/// projection pass) and prevents catastrophically amplified far-away
/// "intersections" from near-parallel pairs.
pub(crate) const EPS_PARALLEL: f64 = 1e-12;

/// Slack on the intersection parameters: `t, u ∈ [−EPS_T, 1 + EPS_T]`
/// counts as on-segment, so a T-junction endpoint lying exactly on another
/// segment is a valid intersection.
pub(crate) const EPS_T: f64 = 1e-9;

/// Closest point on segment a–b to `p` (projection clamped to the segment).
/// Degenerate segment (a == b) → a.
pub(crate) fn project_onto_segment(p: Point, a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}

/// Proper intersection point of segments a1–a2 and b1–b2, or None for
/// parallel / collinear / off-segment pairs per the epsilon policy above.
pub(crate) fn segment_intersection(a1: Point, a2: Point, b1: Point, b2: Point) -> Option<Point> {
    let d1x = a2.x - a1.x;
    let d1y = a2.y - a1.y;
    let d2x = b2.x - b1.x;
    let d2y = b2.y - b1.y;
    let cross = d1x * d2y - d1y * d2x;
    if cross.abs() <= EPS_PARALLEL * d1x.hypot(d1y) * d2x.hypot(d2y) {
        return None;
    }
    let sx = b1.x - a1.x;
    let sy = b1.y - a1.y;
    let t = (sx * d2y - sy * d2x) / cross;
    let u = (sx * d1y - sy * d1x) / cross;
    let on_segment = |v: f64| (-EPS_T..=1.0 + EPS_T).contains(&v);
    if !on_segment(t) || !on_segment(u) {
        return None;
    }
    Some(Point::new(a1.x + t * d1x, a1.y + t * d1y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_of_crossing_segments() {
        let p = segment_intersection(
            Point::new(0.0, 10.0),
            Point::new(100.0, 10.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 100.0),
        )
        .unwrap();
        assert_eq!(p, Point::new(50.0, 10.0));
    }

    #[test]
    fn parallel_and_collinear_yield_none() {
        let a1 = Point::new(0.0, 0.0);
        let a2 = Point::new(100.0, 0.0);
        assert!(
            segment_intersection(a1, a2, Point::new(0.0, 3.0), Point::new(100.0, 3.0)).is_none()
        );
        assert!(
            segment_intersection(a1, a2, Point::new(40.0, 0.0), Point::new(140.0, 0.0)).is_none()
        );
        assert!(segment_intersection(a1, a2, a1, a2).is_none()); // duplicate
    }

    #[test]
    fn nonintersecting_span_yields_none() {
        // Lines cross at (50, 10) but the second segment stops at y = 5.
        assert!(segment_intersection(
            Point::new(0.0, 10.0),
            Point::new(100.0, 10.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 5.0),
        )
        .is_none());
    }

    #[test]
    fn t_junction_endpoint_counts_as_intersection() {
        let p = segment_intersection(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 80.0),
        )
        .unwrap();
        assert_eq!(p, Point::new(50.0, 0.0));
    }

    #[test]
    fn projection_clamps_and_handles_degenerate() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(100.0, 0.0);
        assert_eq!(
            project_onto_segment(Point::new(30.0, 4.0), a, b),
            Point::new(30.0, 0.0)
        );
        assert_eq!(project_onto_segment(Point::new(-10.0, 4.0), a, b), a); // clamped
        assert_eq!(project_onto_segment(Point::new(6.0, 6.0), a, a), a); // degenerate
    }
}
