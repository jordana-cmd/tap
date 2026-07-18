use super::{distance, Point};

/// Douglas-Peucker polyline simplification; `epsilon` in PDF points.
///
/// First and last points are always retained and output vertices are a
/// subset of the input, in original order. Fewer than 3 points → input
/// unchanged. Implemented iteratively (explicit stack, never recursive),
/// per the addendum's §A3.1 discipline.
pub fn simplify(points: &[Point], epsilon: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;

    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        if last <= first + 1 {
            continue;
        }
        let mut max_dist = 0.0;
        let mut max_idx = first;
        for (i, p) in points.iter().enumerate().take(last).skip(first + 1) {
            let d = segment_distance(*p, points[first], points[last]);
            if d > max_dist {
                max_dist = d;
                max_idx = i;
            }
        }
        if max_dist > epsilon {
            keep[max_idx] = true;
            stack.push((first, max_idx));
            stack.push((max_idx, last));
        }
    }

    points
        .iter()
        .zip(&keep)
        .filter(|(_, &k)| k)
        .map(|(p, _)| *p)
        .collect()
}

/// Distance from `p` to the segment a–b (projection clamped to the segment,
/// so every dropped vertex is within epsilon of the *simplified polyline*,
/// not just the infinite line). Degenerate segment → point distance.
fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return distance(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    distance(p, Point::new(a.x + t * dx, a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_drops_near_collinear_midpoint() {
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.1),
            Point::new(10.0, 0.0),
        ];
        assert_eq!(
            simplify(&pts, 1.0),
            vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)]
        );
    }

    #[test]
    fn simplify_keeps_corner_above_epsilon() {
        // Corner (10,0) is 10/√2 ≈ 7.07 from the chord (0,0)–(10,10).
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];
        assert_eq!(simplify(&pts, 1.0), pts.to_vec());
    }

    #[test]
    fn simplify_coincident_endpoints_use_point_distance() {
        // First and last points coincide → degenerate chord; the interior
        // point's deviation is its plain distance to that point (≈7.07 > 1).
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 5.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(simplify(&pts, 1.0), pts.to_vec());
    }

    #[test]
    fn simplify_short_input_unchanged() {
        let empty: [Point; 0] = [];
        assert_eq!(simplify(&empty, 1.0), Vec::<Point>::new());
        let one = [Point::new(1.0, 2.0)];
        assert_eq!(simplify(&one, 1.0), one.to_vec());
        let two = [Point::new(1.0, 2.0), Point::new(3.0, 4.0)];
        assert_eq!(simplify(&two, 1.0), two.to_vec());
    }
}
