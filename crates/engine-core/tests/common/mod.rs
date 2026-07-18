//! Shared helpers for integration and property tests.
#![allow(dead_code)]

use engine_core::{distance, Point};

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
