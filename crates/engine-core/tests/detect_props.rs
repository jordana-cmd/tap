//! Property tests for the detection layer. Raster-heavy properties run
//! fewer cases than the proptest default to keep suite time sane.

mod common;

use common::SynthPlan;
use engine_core::detect::{close_region, dilate, flood_fill, threshold_mask, trace_contour, Mask};
use engine_core::{detect_room, DetectParams, GrayRaster, Point, Scale};
use proptest::prelude::*;

const PPF: f64 = 10.0;

fn scale() -> Scale {
    Scale::from_fpi(4.0).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Detected area is invariant under raster translation of the same room.
    #[test]
    fn prop_detected_area_translation_invariant(
        dx in 0_u32..=20, dy in 0_u32..=20,
    ) {
        let (ox, oy) = (5.0 + f64::from(dx) * 0.1, 5.0 + f64::from(dy) * 0.1);
        let mut plan = SynthPlan::new(40.0, 30.0, PPF);
        plan.walls_rect(ox, oy, 20.0, 15.0, 0.5);
        let det = detect_room(
            &plan.raster(),
            &plan.map(scale()),
            plan.seed_at_ft(ox + 10.0, oy + 7.5),
            &DetectParams::default(),
        ).unwrap();
        // Same room wherever it sits: exact wall-face recovery ± staircase.
        prop_assert!((det.area_sf - 300.0).abs() <= 3.0,
            "area {} at offset ({ox}, {oy})", det.area_sf);
    }

    /// Random enclosed room dimensions are recovered within 3%.
    #[test]
    fn prop_room_dims_recover_area(w in 10.0..40.0f64, h in 10.0..40.0f64) {
        let mut plan = SynthPlan::new(w + 10.0, h + 10.0, PPF);
        plan.walls_rect(5.0, 5.0, w, h, 0.5);
        let det = detect_room(
            &plan.raster(),
            &plan.map(scale()),
            plan.seed_at_ft(5.0 + w / 2.0, 5.0 + h / 2.0),
            &DetectParams::default(),
        ).unwrap();
        let expected = w * h;
        prop_assert!((det.area_sf - expected).abs() <= expected * 0.03,
            "area {} vs expected {expected}", det.area_sf);
    }

    /// The fill never overlaps the dilated walls; the closed region never
    /// overlaps the original walls and contains the fill.
    #[test]
    fn prop_fill_disjoint_from_walls(w in 10.0..20.0f64, h in 10.0..20.0f64) {
        let mut plan = SynthPlan::new(w + 10.0, h + 10.0, PPF);
        plan.walls_rect(5.0, 5.0, w, h, 0.5);
        let raster = plan.raster();
        let walls = threshold_mask(&raster, 200);
        let radius = 18; // door_gap_ft 3.5 at 10 px/ft
        let dilated = dilate(&walls, radius);
        let fill = flood_fill(&dilated, plan.seed_at_ft(5.0 + w / 2.0, 5.0 + h / 2.0)).unwrap();
        let closed = close_region(&fill, &walls, radius);
        for y in 0..walls.height() {
            for x in 0..walls.width() {
                prop_assert!(!(fill.get(x, y) && dilated.get(x, y)));
                prop_assert!(!(closed.get(x, y) && walls.get(x, y)));
                prop_assert!(!fill.get(x, y) || closed.get(x, y)); // fill ⊆ closed
            }
        }
    }

    /// Raising the threshold never removes wall pixels.
    #[test]
    fn prop_threshold_monotonic(
        data in prop::collection::vec(0_u8..=255, 256),
        t1 in 0_u8..=255, t2 in 0_u8..=255,
    ) {
        let (lo, hi) = (t1.min(t2), t1.max(t2));
        let raster = GrayRaster::new(16, 16, data).unwrap();
        let m_lo = threshold_mask(&raster, lo);
        let m_hi = threshold_mask(&raster, hi);
        for y in 0..16 {
            for x in 0..16 {
                prop_assert!(!m_lo.get(x, y) || m_hi.get(x, y));
            }
        }
    }

    /// Dilation is extensive and monotonic in the radius.
    #[test]
    fn prop_dilate_superset_and_monotonic(
        bits in prop::collection::vec(any::<bool>(), 256),
        r1 in 0_u32..4, r2 in 0_u32..4,
    ) {
        let (lo, hi) = (r1.min(r2), r1.max(r2));
        let mut m = Mask::new(16, 16);
        for (i, &b) in bits.iter().enumerate() {
            if b { m.set(i as u32 % 16, i as u32 / 16, true); }
        }
        let d_lo = dilate(&m, lo);
        let d_hi = dilate(&m, hi);
        for y in 0..16 {
            for x in 0..16 {
                prop_assert!(!m.get(x, y) || d_lo.get(x, y));      // extensive
                prop_assert!(!d_lo.get(x, y) || d_hi.get(x, y));   // monotone
            }
        }
    }

    /// The traced contour's shoelace equals the pixel count exactly for
    /// rectangular regions (corner-lattice marching squares is exact).
    #[test]
    fn prop_contour_area_matches_pixel_count(
        x0 in 0_u32..6, y0 in 0_u32..6, w in 1_u32..10, h in 1_u32..10,
    ) {
        let mut m = Mask::new(16, 16);
        for y in y0..(y0 + h).min(16) {
            for x in x0..(x0 + w).min(16) {
                m.set(x, y, true);
            }
        }
        let pts: Vec<Point> = trace_contour(&m)
            .iter()
            .map(|&(px, py)| Point::new(px, py))
            .collect();
        let area = engine_core::polygon_area(&pts);
        prop_assert!((area - m.count() as f64).abs() < 1e-9,
            "shoelace {area} vs pixel count {}", m.count());
    }
}
