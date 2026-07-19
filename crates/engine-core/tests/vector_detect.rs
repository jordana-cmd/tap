//! Integration tests for the vector-filtered wall-mask path (addendum §A3.1
//! mask-source revision): synthetic segment sets → rasterize_wall_mask →
//! detect_room_from_mask, plus the first snapping exercise against
//! plan-shaped geometry.

mod common;

use common::{assert_close, SynthSegments};
use engine_core::{
    detect_room, detect_room_from_mask, rasterize_wall_mask, DetectError, DetectParams, Point,
    Scale, SegmentIndex, SnapKind,
};
use proptest::prelude::*;

const PPF: f64 = 10.0;
/// 2 px at 1.8 pts/px — a plausible plotted wall stroke.
const WALL_STROKE_PTS: f64 = 3.6;
/// Sub-pixel hairline — a plausible dimension/grid line.
const HAIRLINE_PTS: f64 = 0.12;

fn scale() -> Scale {
    Scale::from_fpi(4.0).unwrap()
}

/// 30×25 ft sheet with a 20×15 ft room outlined at (5,5). Walls are
/// centerline strokes 0.2 ft thick, so the interior face sits at 5.1 ft and
/// the enclosed area is 19.8 × 14.8 = 293.04 SF.
fn room_sheet() -> SynthSegments {
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.wall_rect_ft(5.0, 5.0, 20.0, 15.0, WALL_STROKE_PTS);
    s
}

#[test]
fn vector_room_detected_from_four_wall_segments() {
    let sheet = room_sheet();
    let (w, h) = sheet.grid();
    let mask = rasterize_wall_mask(&sheet.segments(), &sheet.map(), w, h, 0.0).unwrap();
    let room = detect_room_from_mask(
        &mask,
        &sheet.map(),
        sheet.seed_at_ft(15.0, 12.5),
        &DetectParams::default(),
    )
    .unwrap();
    assert_close(room.area_sf, 293.04, 0.0, 0.03);
    assert_close(room.perimeter_lf, 2.0 * (19.8 + 14.8), 0.0, 0.03);
}

#[test]
fn hairline_annotation_crossing_interior_removed_by_width_filter() {
    let mut sheet = room_sheet();
    // A dimension line clean across the room interior.
    sheet.seg_ft(5.0, 12.5, 25.0, 12.5, HAIRLINE_PTS);
    let (w, h) = sheet.grid();
    let map = sheet.map();
    let params = DetectParams::default();
    let seed = sheet.seed_at_ft(15.0, 8.0);

    // Filtered at 0.18 pts (between hairline and wall): full room.
    let filtered = rasterize_wall_mask(&sheet.segments(), &map, w, h, 0.18).unwrap();
    let full = detect_room_from_mask(&filtered, &map, seed, &params).unwrap();
    assert_close(full.area_sf, 293.04, 0.0, 0.03);

    // Unfiltered (min_width 0): the hairline partitions the fill — the
    // detected region is a fragment. This is eval-01's failure mode,
    // reproduced deliberately to prove the filter is what fixes it.
    let unfiltered = rasterize_wall_mask(&sheet.segments(), &map, w, h, 0.0).unwrap();
    let fragment = detect_room_from_mask(&unfiltered, &map, seed, &params).unwrap();
    assert!(
        fragment.area_sf < 0.6 * full.area_sf,
        "hairline should fragment the unfiltered fill: {} vs {}",
        fragment.area_sf,
        full.area_sf
    );
}

#[test]
fn door_gap_between_wall_segments_closed_by_dilation() {
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    // Top wall split around a 3-ft door opening at x ∈ [12, 15].
    s.seg_ft(5.0, 5.0, 12.0, 5.0, WALL_STROKE_PTS);
    s.seg_ft(15.0, 5.0, 25.0, 5.0, WALL_STROKE_PTS);
    s.seg_ft(25.0, 5.0, 25.0, 20.0, WALL_STROKE_PTS);
    s.seg_ft(25.0, 20.0, 5.0, 20.0, WALL_STROKE_PTS);
    s.seg_ft(5.0, 20.0, 5.0, 5.0, WALL_STROKE_PTS);
    let (w, h) = s.grid();
    let map = s.map();
    let seed = s.seed_at_ft(15.0, 12.5);
    let mask = rasterize_wall_mask(&s.segments(), &map, w, h, 0.0).unwrap();

    // Default door_gap 3.5 ft seals the 3-ft opening.
    let room = detect_room_from_mask(&mask, &map, seed, &DetectParams::default()).unwrap();
    assert_close(room.area_sf, 293.04, 0.0, 0.04);

    // door_gap 0.5 ft cannot seal it — the fill escapes to the sheet edge.
    let leaky = DetectParams {
        door_gap_ft: 0.5,
        ..DetectParams::default()
    };
    assert_eq!(
        detect_room_from_mask(&mask, &map, seed, &leaky),
        Err(DetectError::RegionNotEnclosed)
    );
}

#[test]
fn vector_and_raster_paths_agree_on_identical_geometry() {
    // Raster: wall band OUTSIDE the 20×15 interior (walls_rect semantics).
    let mut plan = common::SynthPlan::new(30.0, 25.0, PPF);
    plan.walls_rect(5.0, 5.0, 20.0, 15.0, 0.2);
    let raster_room = detect_room(
        &plan.raster(),
        &plan.map(scale()),
        plan.seed_at_ft(15.0, 12.5),
        &DetectParams::default(),
    )
    .unwrap();

    // Vector: centerline rect offset outward 0.1 ft so the interior stroke
    // face also lands at exactly 5.0 ft → identical enclosed geometry.
    let mut sheet = SynthSegments::new(30.0, 25.0, scale(), PPF);
    sheet.wall_rect_ft(4.9, 4.9, 20.2, 15.2, WALL_STROKE_PTS);
    let (w, h) = sheet.grid();
    let mask = rasterize_wall_mask(&sheet.segments(), &sheet.map(), w, h, 0.0).unwrap();
    let vector_room = detect_room_from_mask(
        &mask,
        &sheet.map(),
        sheet.seed_at_ft(15.0, 12.5),
        &DetectParams::default(),
    )
    .unwrap();

    assert_close(vector_room.area_sf, raster_room.area_sf, 0.0, 0.02);
    assert_close(vector_room.perimeter_lf, raster_room.perimeter_lf, 0.0, 0.02);
}

#[test]
fn mask_path_error_parity() {
    let sheet = room_sheet();
    let (w, h) = sheet.grid();
    let map = sheet.map();
    let params = DetectParams::default();
    let mask = rasterize_wall_mask(&sheet.segments(), &map, w, h, 0.0).unwrap();

    // Seed on a (dilated) wall pixel.
    assert!(matches!(
        detect_room_from_mask(&mask, &map, sheet.seed_at_ft(5.0, 5.0), &params),
        Err(DetectError::SeedOnWall { .. })
    ));
    // Seed outside the raster.
    assert!(matches!(
        detect_room_from_mask(&mask, &map, (w + 5, 0), &params),
        Err(DetectError::SeedOutOfBounds { .. })
    ));
    // Open geometry: one lone wall segment never encloses the seed.
    let mut open = SynthSegments::new(30.0, 25.0, scale(), PPF);
    open.seg_ft(5.0, 5.0, 25.0, 5.0, WALL_STROKE_PTS);
    let open_mask = rasterize_wall_mask(&open.segments(), &map, w, h, 0.0).unwrap();
    assert_eq!(
        detect_room_from_mask(&open_mask, &map, sheet.seed_at_ft(15.0, 12.5), &params),
        Err(DetectError::RegionNotEnclosed)
    );
}

/// First snapping exercise against plan-shaped geometry: a room outline
/// with an annotation line crossing one wall. Base units: 1 ft = 18 pts.
#[test]
fn snap_on_synthetic_plan_geometry() {
    let mut sheet = room_sheet();
    // Vertical annotation crossing the top wall (y = 90 pts) mid-span.
    sheet.seg_ft(14.0, 4.5, 14.0, 6.0, HAIRLINE_PTS);
    let index = SegmentIndex::build(sheet.segments());

    // Corner (5,5) ft = (90,90) pts: endpoint wins.
    let s = index.snap(Point::new(91.0, 89.0), 5.0).unwrap();
    assert_eq!(s.kind, SnapKind::Endpoint);
    assert_eq!((s.point.x, s.point.y), (90.0, 90.0));

    // Mid-span of the top wall, no endpoint in tolerance: projection.
    let s = index.snap(Point::new(200.0, 88.0), 5.0).unwrap();
    assert_eq!(s.kind, SnapKind::Projection);
    assert_close(s.point.y, 90.0, 1e-9, 0.0);
    assert_close(s.point.x, 200.0, 1e-9, 0.0);

    // Near where the annotation crosses the wall (252, 90), with both
    // segments' endpoints out of tolerance: intersection.
    let s = index.snap(Point::new(253.0, 89.0), 3.0).unwrap();
    assert!(matches!(s.kind, SnapKind::Intersection { .. }));
    assert_close(s.point.x, 252.0, 1e-9, 0.0);
    assert_close(s.point.y, 90.0, 1e-9, 0.0);
}

proptest! {
    /// Nudge invariance (eval-02 queue A1 fix): ANY raw-free click inside
    /// the room — including clicks inside the dilation inset that used to
    /// fail SEED_ON_WALL — succeeds and yields the identical region.
    #[test]
    fn prop_nudge_invariant_over_interior_seeds(
        sx in 5.3..24.7f64,
        sy in 5.3..19.7f64,
    ) {
        let sheet = room_sheet();
        let (w, h) = sheet.grid();
        let map = sheet.map();
        let params = DetectParams::default();
        let mask = rasterize_wall_mask(&sheet.segments(), &map, w, h, 0.18).unwrap();
        let reference =
            detect_room_from_mask(&mask, &map, sheet.seed_at_ft(15.0, 12.5), &params).unwrap();
        let seeded =
            detect_room_from_mask(&mask, &map, sheet.seed_at_ft(sx, sy), &params).unwrap();
        prop_assert_eq!(seeded, reference);
    }

    /// The wall mask is monotone in the width filter: raising min_width can
    /// only remove pixels, never add them (filtered ⊆ unfiltered).
    #[test]
    fn prop_mask_monotone_in_min_width(
        segs in proptest::collection::vec(
            (0.0..500.0f64, 0.0..400.0f64, 0.0..500.0f64, 0.0..400.0f64, 0.01..1.2f64),
            1..25,
        ),
        t1 in 0.0..1.3f64,
        dt in 0.0..1.3f64,
    ) {
        let scale = Scale::from_fpi(4.0).unwrap();
        let map = engine_core::PixelMap::new(Point::new(0.0, 0.0), scale, 10.0).unwrap();
        let segments: Vec<engine_core::Segment> = segs
            .iter()
            .map(|&(x1, y1, x2, y2, w)| engine_core::Segment {
                p1: Point::new(x1, y1),
                p2: Point::new(x2, y2),
                width: w,
            })
            .collect();
        let t2 = t1 + dt;
        let loose = rasterize_wall_mask(&segments, &map, 60, 50, t1).unwrap();
        let strict = rasterize_wall_mask(&segments, &map, 60, 50, t2).unwrap();
        for y in 0..50u32 {
            for x in 0..60u32 {
                prop_assert!(
                    !strict.get(x, y) || loose.get(x, y),
                    "pixel ({x},{y}) set at min_width {t2} but not at {t1}"
                );
            }
        }
    }
}
