//! Cross-module hand-computed cases: geometry in PDF points → real-world
//! quantities via the fpi scale contract.

mod common;

use engine_core::{format_feet_inches, polygon_area, polyline_length, InchPrecision, Point, Scale};

/// A 72-pt (1 paper inch) square at 1/4" = 1'-0" (fpi 4.0) is a 4 ft × 4 ft
/// room: 5184 pts² → 16 SF.
#[test]
fn area_unit_paper_inch_square() {
    let square = [
        Point::new(0.0, 0.0),
        Point::new(72.0, 0.0),
        Point::new(72.0, 72.0),
        Point::new(0.0, 72.0),
    ];
    let pts_sq = polygon_area(&square);
    assert_eq!(pts_sq, 5184.0);

    let scale = Scale::from_fpi(4.0).unwrap();
    assert_eq!(scale.points_sq_to_square_feet(pts_sq), 16.0);
}

/// Binding fpi contract spot-check: 72 pts @ fpi 4.0 → 4 ft;
/// 5184 pts² @ fpi 4.0 → 16 SF.
#[test]
fn fpi_contract_spot_check() {
    let scale = Scale::from_fpi(4.0).unwrap();
    assert_eq!(scale.points_to_feet(72.0), 4.0);
    assert_eq!(scale.points_sq_to_square_feet(5184.0), 16.0);
}

/// Trace a wall run in points, read its LF through the scale, format it.
#[test]
fn wall_run_length_formats_as_feet_inches() {
    // Two 72-pt segments = 2 paper inches @ engineer 1" = 20' → 40 ft.
    let path = [
        Point::new(0.0, 0.0),
        Point::new(72.0, 0.0),
        Point::new(72.0, 72.0),
    ];
    let scale = Scale::from_fpi(20.0).unwrap();
    let feet = scale.points_to_feet(polyline_length(&path));
    assert_eq!(feet, 40.0);
    assert_eq!(format_feet_inches(feet, InchPrecision::Inch), "40'-0\"");
}
