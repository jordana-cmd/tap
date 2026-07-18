//! Property tests over the public geometry, scale, and formatting API.

mod common;

use common::{assert_close, parse_feet_inches, polyline_distance};
use engine_core::{
    calibrate_two_point, distance, format_feet_inches, polygon_area, polygon_perimeter,
    polyline_length, simplify, InchPrecision, Point, Scale,
};
use proptest::prelude::*;

fn coord() -> impl Strategy<Value = f64> {
    -1.0e6..1.0e6
}

fn point() -> impl Strategy<Value = Point> {
    (coord(), coord()).prop_map(|(x, y)| Point::new(x, y))
}

fn points(min: usize) -> impl Strategy<Value = Vec<Point>> {
    prop::collection::vec(point(), min..40)
}

fn precision() -> impl Strategy<Value = InchPrecision> {
    prop_oneof![
        Just(InchPrecision::Inch),
        Just(InchPrecision::Half),
        Just(InchPrecision::Quarter),
        Just(InchPrecision::Eighth),
        Just(InchPrecision::Sixteenth),
    ]
}

proptest! {
    // ---- distance ----

    #[test]
    fn prop_distance_symmetric(a in point(), b in point()) {
        prop_assert_eq!(distance(a, b), distance(b, a));
    }

    #[test]
    fn prop_distance_nonnegative_and_self_zero(a in point(), b in point()) {
        prop_assert!(distance(a, b) >= 0.0);
        prop_assert_eq!(distance(a, a), 0.0);
    }

    #[test]
    fn prop_triangle_inequality(a in point(), b in point(), c in point()) {
        let direct = distance(a, c);
        let via = distance(a, b) + distance(b, c);
        prop_assert!(direct <= via + 1e-6 + 1e-12 * via);
    }

    // ---- polyline length ----

    #[test]
    fn prop_length_translation_invariant(pts in points(2), dx in coord(), dy in coord()) {
        let moved: Vec<Point> =
            pts.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
        assert_close(polyline_length(&moved), polyline_length(&pts), 1e-4, 1e-9);
    }

    #[test]
    fn prop_length_concat_additive(a in points(1), b in points(1)) {
        let joined: Vec<Point> = a.iter().chain(b.iter()).copied().collect();
        let expected = polyline_length(&a)
            + polyline_length(&b)
            + distance(*a.last().unwrap(), b[0]);
        assert_close(polyline_length(&joined), expected, 1e-6, 1e-12);
    }

    // ---- shoelace area ----

    #[test]
    fn prop_area_translation_invariant(pts in points(3), dx in coord(), dy in coord()) {
        let moved: Vec<Point> =
            pts.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
        assert_close(polygon_area(&moved), polygon_area(&pts), 1.0, 1e-9);
    }

    #[test]
    fn prop_area_scale_quadratic(pts in points(3), k in 0.1..10.0f64) {
        let scaled: Vec<Point> =
            pts.iter().map(|p| Point::new(p.x * k, p.y * k)).collect();
        assert_close(polygon_area(&scaled), k * k * polygon_area(&pts), 1e-6, 1e-9);
    }

    #[test]
    fn prop_perimeter_translation_invariant_and_scales_linearly(
        pts in points(2), dx in coord(), dy in coord(), k in 0.1..10.0f64,
    ) {
        let moved: Vec<Point> =
            pts.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
        assert_close(polygon_perimeter(&moved), polygon_perimeter(&pts), 1e-4, 1e-9);
        let scaled: Vec<Point> =
            pts.iter().map(|p| Point::new(p.x * k, p.y * k)).collect();
        assert_close(polygon_perimeter(&scaled), k * polygon_perimeter(&pts), 1e-6, 1e-9);
    }

    #[test]
    fn prop_area_reversal_invariant(pts in points(3)) {
        let reversed: Vec<Point> = pts.iter().rev().copied().collect();
        assert_close(polygon_area(&reversed), polygon_area(&pts), 1e-6, 1e-12);
    }

    // ---- Douglas-Peucker simplify ----

    #[test]
    fn prop_simplify_vertices_subset_of_input(pts in points(0), eps in 0.0..100.0f64) {
        let out = simplify(&pts, eps);
        for p in &out {
            prop_assert!(pts.iter().any(|q| q == p));
        }
    }

    #[test]
    fn prop_simplify_endpoints_preserved(pts in points(1), eps in 0.0..100.0f64) {
        let out = simplify(&pts, eps);
        prop_assert_eq!(out.first(), pts.first());
        prop_assert_eq!(out.last(), pts.last());
    }

    #[test]
    fn prop_simplify_deviation_bounded(pts in points(3), eps in 0.0..100.0f64) {
        let out = simplify(&pts, eps);
        prop_assert!(out.len() >= 2);
        for p in &pts {
            let d = polyline_distance(*p, &out);
            prop_assert!(d <= eps + 1e-6, "point deviates {d} > eps {eps}");
        }
    }

    #[test]
    fn prop_simplify_idempotent(pts in points(0), eps in 0.0..100.0f64) {
        let once = simplify(&pts, eps);
        let twice = simplify(&once, eps);
        prop_assert_eq!(once, twice);
    }

    // ---- fpi scale contract ----

    #[test]
    fn prop_scale_roundtrip(fpi in 0.01..1000.0f64, pts in -1.0e6..1.0e6f64) {
        let s = Scale::from_fpi(fpi).unwrap();
        assert_close(s.feet_to_points(s.points_to_feet(pts)), pts, 1e-9, 1e-12);
    }

    #[test]
    fn prop_scale_sf_consistent_with_linear(fpi in 0.01..1000.0f64, d in 0.0..1.0e6f64) {
        let s = Scale::from_fpi(fpi).unwrap();
        let linear = s.points_to_feet(d);
        assert_close(s.points_sq_to_square_feet(d * d), linear * linear, 1e-9, 1e-9);
    }

    #[test]
    fn prop_calibrate_roundtrip(
        ax in -1.0e5..1.0e5f64, ay in -1.0e5..1.0e5f64,
        dx in 1.0..5000.0f64, dy in 0.0..5000.0f64,
        feet in 0.1..1000.0f64,
    ) {
        let a = Point::new(ax, ay);
        let b = Point::new(ax + dx, ay + dy); // span ≥ 1 pt guaranteed by dx
        let scale = calibrate_two_point(a, b, feet).unwrap();
        // Measuring the calibration span through the derived scale returns
        // the known length.
        assert_close(scale.points_to_feet(distance(a, b)), feet, 1e-9, 1e-12);
    }

    // ---- feet-inches formatting ----

    #[test]
    fn prop_format_error_within_half_step(feet in 0.0..100_000.0f64, prec in precision()) {
        let formatted = format_feet_inches(feet, prec);
        let parsed = parse_feet_inches(&formatted);
        let half_step_ft = 1.0 / (prec.denominator() as f64 * 12.0) / 2.0;
        prop_assert!(
            (parsed - feet).abs() <= half_step_ft + 1e-9,
            "{feet} formatted as {formatted} (parsed {parsed}), off by more than half a step"
        );
    }
}
