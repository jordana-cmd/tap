//! Polygon containment — does one polygon fully enclose another?
//!
//! Used by the deduct workflow: a deduction (column, cooler, drain footprint)
//! must sit inside the area it subtracts from. Containment is scale-independent
//! (page-point coordinates), so these are pure geometry over [`Point`].

use super::Point;

/// Points nearer than this (in PDF points) count as coincident / on-edge.
/// Loose enough to absorb float error from drawn geometry, tight enough that a
/// real gap between a deduct and a wall never reads as "on the boundary".
const EPS_ON: f64 = 1e-7;
/// Relative parallel-rejection epsilon (mirrors the snap module's policy).
const EPS_PARALLEL: f64 = 1e-12;
/// Strict-interior slack for a *proper* crossing: an intersection whose
/// parameter is within this of an endpoint is a touch, not a crossing.
const EPS_CROSS: f64 = 1e-9;

/// Closest point on segment a–b to `p` (projection clamped to the segment).
fn project(p: Point, a: Point, b: Point) -> Point {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}

fn on_segment(p: Point, a: Point, b: Point) -> bool {
    let q = project(p, a, b);
    (p.x - q.x).hypot(p.y - q.y) <= EPS_ON
}

/// Do segments a1–a2 and b1–b2 cross *transversally* — meeting at a point
/// strictly interior to both? Endpoint touches, collinear overlaps, and
/// parallel pairs are deliberately NOT crossings, so a deduct sharing a wall
/// or a vertex with its parent is still "inside", not "poking out".
fn proper_cross(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let d1x = a2.x - a1.x;
    let d1y = a2.y - a1.y;
    let d2x = b2.x - b1.x;
    let d2y = b2.y - b1.y;
    let cross = d1x * d2y - d1y * d2x;
    if cross.abs() <= EPS_PARALLEL * d1x.hypot(d1y) * d2x.hypot(d2y) {
        return false;
    }
    let sx = b1.x - a1.x;
    let sy = b1.y - a1.y;
    let t = (sx * d2y - sy * d2x) / cross;
    let u = (sx * d1y - sy * d1x) / cross;
    let strict = |v: f64| v > EPS_CROSS && v < 1.0 - EPS_CROSS;
    strict(t) && strict(u)
}

/// Is `p` inside `poly` (a boundary point counts as inside)? Even-odd ray
/// casting along +x, with an explicit on-edge pre-check so points exactly on
/// the boundary are stable regardless of ray-grazing ambiguity. Fewer than 3
/// vertices → false.
pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        if on_segment(p, poly[i], poly[(i + 1) % n]) {
            return true;
        }
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = poly[i];
        let pj = poly[j];
        if (pi.y > p.y) != (pj.y > p.y) {
            let x_int = pi.x + (p.y - pi.y) / (pj.y - pi.y) * (pj.x - pi.x);
            if p.x < x_int {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Does `outer` fully contain `inner`? True iff every vertex of `inner` is
/// inside-or-on `outer` **and** no edge of `inner` properly crosses an edge of
/// `outer`. The edge-crossing check is what makes concave parents correct: an
/// inner edge that bows out through a concave notch has all vertices inside yet
/// crosses the boundary, so it is (rightly) not contained. Auto-closes both
/// rings (last→first). Fewer than 3 vertices on either side → false.
pub fn polygon_contains_polygon(outer: &[Point], inner: &[Point]) -> bool {
    let (no, ni) = (outer.len(), inner.len());
    if no < 3 || ni < 3 {
        return false;
    }
    if !inner.iter().all(|&v| point_in_polygon(v, outer)) {
        return false;
    }
    for i in 0..ni {
        let a1 = inner[i];
        let a2 = inner[(i + 1) % ni];
        for k in 0..no {
            if proper_cross(a1, a2, outer[k], outer[(k + 1) % no]) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(coords: &[(f64, f64)]) -> Vec<Point> {
        coords.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    // 10×10 square at the origin.
    fn square() -> Vec<Point> {
        pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])
    }

    #[test]
    fn point_inside_and_outside() {
        let sq = square();
        assert!(point_in_polygon(Point::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(15.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(-1.0, 5.0), &sq));
    }

    #[test]
    fn point_on_boundary_counts_as_inside() {
        let sq = square();
        assert!(point_in_polygon(Point::new(0.0, 5.0), &sq)); // on left edge
        assert!(point_in_polygon(Point::new(10.0, 10.0), &sq)); // corner
        assert!(point_in_polygon(Point::new(5.0, 0.0), &sq)); // on bottom edge
    }

    #[test]
    fn square_contains_smaller_square() {
        let inner = pts(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)]);
        assert!(polygon_contains_polygon(&square(), &inner));
    }

    #[test]
    fn square_does_not_contain_overlapping_square() {
        // Straddles the right wall: two vertices in, two out; edges cross.
        let straddle = pts(&[(7.0, 3.0), (13.0, 3.0), (13.0, 7.0), (7.0, 7.0)]);
        assert!(!polygon_contains_polygon(&square(), &straddle));
    }

    #[test]
    fn square_does_not_contain_disjoint_square() {
        let away = pts(&[(20.0, 20.0), (24.0, 20.0), (24.0, 24.0), (20.0, 24.0)]);
        assert!(!polygon_contains_polygon(&square(), &away));
    }

    #[test]
    fn shared_boundary_is_contained() {
        // Inner flush into the bottom-left corner, sharing two edges with the
        // parent. On-boundary vertices + collinear edges ⇒ still contained.
        let flush = pts(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
        assert!(polygon_contains_polygon(&square(), &flush));
    }

    /// Concave L-room: a 10×10 square with the top-right 5×5 quadrant removed.
    ///   (0,0)→(10,0)→(10,5)→(5,5)→(5,10)→(0,10)
    fn l_room() -> Vec<Point> {
        pts(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (5.0, 5.0),
            (5.0, 10.0),
            (0.0, 10.0),
        ])
    }

    #[test]
    fn concave_room_contains_column_in_the_solid_leg() {
        // A 1×1 column in the lower-left solid region.
        let column = pts(&[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)]);
        assert!(polygon_contains_polygon(&l_room(), &column));
    }

    #[test]
    fn concave_room_rejects_column_in_the_notch() {
        // A column in the removed top-right quadrant: all vertices are OUTSIDE
        // the L — rejected by the vertex test alone.
        let in_notch = pts(&[(6.0, 6.0), (9.0, 6.0), (9.0, 9.0), (6.0, 9.0)]);
        assert!(!polygon_contains_polygon(&l_room(), &in_notch));
    }

    #[test]
    fn concave_room_rejects_bridge_through_the_notch() {
        // A triangle whose three vertices all sit in solid parts of the L
        // (left column and bottom-right leg), but whose hypotenuse (1,8)→(9,3)
        // passes THROUGH the notch, crossing the concave wall x=5 at (5, 5.5).
        // Vertices-inside is true; the edge-crossing check is what rejects it —
        // the case a naive point-in-polygon-only test gets wrong.
        let bridge = pts(&[(1.0, 8.0), (9.0, 3.0), (1.0, 3.0)]);
        // Sanity: every vertex is inside the L…
        assert!(bridge.iter().all(|&v| point_in_polygon(v, &l_room())));
        // …yet the polygon is not contained.
        assert!(!polygon_contains_polygon(&l_room(), &bridge));
    }

    #[test]
    fn polygon_contains_itself() {
        assert!(polygon_contains_polygon(&square(), &square()));
    }

    #[test]
    fn degenerate_inputs_are_not_contained() {
        let sq = square();
        assert!(!polygon_contains_polygon(&sq, &pts(&[(1.0, 1.0), (2.0, 2.0)])));
        assert!(!polygon_contains_polygon(&pts(&[(0.0, 0.0)]), &sq));
        assert!(!polygon_contains_polygon(&[], &sq));
    }
}
