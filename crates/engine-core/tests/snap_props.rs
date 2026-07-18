//! Property tests for snapping: results are honest about distance, lie on
//! the geometry they claim, and None means nothing was in range.

mod common;

use common::segment_distance;
use engine_core::{distance, Point, Segment, SegmentIndex, SnapKind};
use proptest::prelude::*;

fn coord() -> impl Strategy<Value = f64> {
    0.0..3000.0
}

fn segment() -> impl Strategy<Value = Segment> {
    (coord(), coord(), coord(), coord(), 0.1..10.0f64).prop_map(|(x1, y1, x2, y2, width)| Segment {
        p1: Point::new(x1, y1),
        p2: Point::new(x2, y2),
        width,
    })
}

fn segments() -> impl Strategy<Value = Vec<Segment>> {
    prop::collection::vec(segment(), 0..50)
}

proptest! {
    #[test]
    fn prop_snap_within_tolerance(
        segs in segments(), cx in coord(), cy in coord(), tol in 0.1..50.0f64,
    ) {
        let index = SegmentIndex::build(segs);
        let cursor = Point::new(cx, cy);
        if let Some(snap) = index.snap(cursor, tol) {
            prop_assert!(distance(snap.point, cursor) <= tol + 1e-9);
        }
    }

    #[test]
    fn prop_snap_point_on_claimed_geometry(
        segs in segments(), cx in coord(), cy in coord(), tol in 0.1..50.0f64,
    ) {
        let index = SegmentIndex::build(segs);
        let cursor = Point::new(cx, cy);
        let Some(snap) = index.snap(cursor, tol) else { return Ok(()); };
        let seg = index.segment(snap.segment).unwrap();
        match snap.kind {
            SnapKind::Endpoint => {
                prop_assert!(snap.point == seg.p1 || snap.point == seg.p2);
            }
            SnapKind::Projection => {
                prop_assert!(segment_distance(snap.point, seg.p1, seg.p2) <= 1e-6);
            }
            SnapKind::Intersection { other } => {
                let other_seg = index.segment(other).unwrap();
                prop_assert!(segment_distance(snap.point, seg.p1, seg.p2) <= 1e-6);
                prop_assert!(
                    segment_distance(snap.point, other_seg.p1, other_seg.p2) <= 1e-6
                );
                prop_assert!(snap.segment.0 < other.0); // ids ordered
            }
        }
    }

    #[test]
    fn prop_none_iff_nothing_in_tolerance(
        segs in segments(), cx in coord(), cy in coord(), tol in 0.1..50.0f64,
    ) {
        let index = SegmentIndex::build(segs.clone());
        let cursor = Point::new(cx, cy);
        let brute_min = segs
            .iter()
            .map(|s| segment_distance(cursor, s.p1, s.p2))
            .fold(f64::INFINITY, f64::min);
        // Asymmetric slack keeps exact-boundary cases from flaking.
        match index.snap(cursor, tol) {
            Some(_) => prop_assert!(brute_min <= tol + 1e-9),
            None => prop_assert!(brute_min > tol - 1e-6),
        }
    }

    #[test]
    fn prop_endpoint_priority_matches_bruteforce(
        segs in segments(), cx in coord(), cy in coord(), tol in 0.1..50.0f64,
    ) {
        let index = SegmentIndex::build(segs.clone());
        let cursor = Point::new(cx, cy);
        let brute_endpoint = segs
            .iter()
            .flat_map(|s| [s.p1, s.p2])
            .map(|p| distance(cursor, p))
            .fold(f64::INFINITY, f64::min);
        if brute_endpoint <= tol {
            let snap = index.snap(cursor, tol).unwrap();
            prop_assert_eq!(snap.kind, SnapKind::Endpoint);
            prop_assert!((distance(cursor, snap.point) - brute_endpoint).abs() <= 1e-9);
        }
    }
}
