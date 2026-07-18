//! Integration tests for the detection pipeline against synthetic fixtures
//! generated in code (CLAUDE.md Engine Foundation step 3): wall rectangles
//! form rooms, erased spans form doors. Standard test scale: fpi 4.0
//! (1/4" = 1'-0") at 10 px/ft → 1.8 pts/px.

mod common;

use common::SynthPlan;
use engine_core::{detect_candidates, detect_room, DetectError, DetectParams, Scale};

const PPF: f64 = 10.0;

fn scale() -> Scale {
    Scale::from_fpi(4.0).unwrap()
}

/// (a) A plain enclosed 20×15 ft room detects its true wall-face area and
/// perimeter. The closing pipeline recovers the interior exactly for
/// rectangular rooms; tolerance covers only sub-pixel staircase effects.
#[test]
fn room_area_within_tolerance() {
    let mut plan = SynthPlan::new(30.0, 25.0, PPF);
    plan.walls_rect(5.0, 5.0, 20.0, 15.0, 0.5);
    let det = detect_room(
        &plan.raster(),
        &plan.map(scale()),
        plan.seed_at_ft(15.0, 12.5),
        &DetectParams::default(),
    )
    .unwrap();
    println!(
        "spot-check (a): 20×15 ft room @ fpi 4.0, 10 px/ft → {} SF, {} LF",
        det.area_sf, det.perimeter_lf
    );
    assert!(
        (det.area_sf - 300.0).abs() <= 9.0,
        "area {} not within 3% of 300 SF",
        det.area_sf
    );
    assert!(
        (det.perimeter_lf - 70.0).abs() <= 2.1,
        "perimeter {} not within 3% of 70 LF",
        det.perimeter_lf
    );
    // The simplified rectilinear contour should collapse to few vertices.
    assert!(det.contour.len() >= 4 && det.contour.len() <= 16);
}

/// (b) A 3.0 ft door (≤ the default 3.5 ft closing) between two rooms is
/// sealed: the fill does NOT leak from room A (300 SF) into room B (180 SF).
#[test]
fn door_gap_sealed_at_default() {
    let mut plan = SynthPlan::new(45.0, 25.0, PPF);
    plan.walls_rect(5.0, 5.0, 20.0, 15.0, 0.5); // room A
    plan.walls_rect(25.5, 5.0, 12.0, 15.0, 0.5); // room B, shared wall x∈[25,25.5]
    plan.gap_ft(25.0, 10.0, 0.5, 3.0); // 3 ft door through the shared wall
    let det = detect_room(
        &plan.raster(),
        &plan.map(scale()),
        plan.seed_at_ft(15.0, 12.5),
        &DetectParams::default(),
    )
    .unwrap();
    // Room A ± 4% (staircase + the small door-plane bulge ≤ gap × r ≈ 5.4 SF).
    assert!(
        (det.area_sf - 300.0).abs() <= 12.0,
        "area {} not within 4% of room A's 300 SF",
        det.area_sf
    );
    // And emphatically not A+B — the fill did not pass the door.
    assert!(
        det.area_sf < 350.0,
        "area {} leaked into room B",
        det.area_sf
    );
}

/// (c) An open floor plan — an 8 ft gap in the OUTER wall, wider than the
/// default closing can seal — aborts with the typed not-enclosed error.
#[test]
fn open_floor_plan_errors_not_enclosed() {
    let mut plan = SynthPlan::new(40.0, 30.0, PPF);
    plan.walls_rect(10.0, 10.0, 20.0, 15.0, 0.5);
    plan.gap_ft(9.5, 13.0, 0.5, 8.0); // 8 ft opening in the left outer wall
    let result = detect_room(
        &plan.raster(),
        &plan.map(scale()),
        plan.seed_at_ft(20.0, 17.5),
        &DetectParams::default(),
    );
    assert_eq!(result.unwrap_err(), DetectError::RegionNotEnclosed);
}

/// (d) A 12 ft equipment door leaks at the default door_gap_ft = 3.5 (the
/// fill spans rooms A + B + doorway), and seals when the slider parameter is
/// raised to 13.0 — re-run is cheap (addendum §A3.1 step 2).
#[test]
fn equipment_door_leaks_then_seals() {
    let mut plan = SynthPlan::new(60.0, 30.0, PPF);
    plan.walls_rect(5.0, 5.0, 30.0, 20.0, 0.5); // room A, 600 SF
    plan.walls_rect(35.5, 5.0, 20.0, 20.0, 0.5); // room B, 400 SF
    plan.gap_ft(35.0, 9.0, 0.5, 12.0); // 12 ft equipment opening
    let raster = plan.raster();
    let map = plan.map(scale());
    let seed = plan.seed_at_ft(20.0, 15.0);

    // Default: 2r closes only 3.6 ft — the 12 ft opening stays open.
    let leaked = detect_room(&raster, &map, seed, &DetectParams::default()).unwrap();
    println!(
        "spot-check (d): 12 ft door @ door_gap_ft 3.5 → {} SF (leak)",
        leaked.area_sf
    );
    let both_rooms = 600.0 + 400.0 + 6.0; // A + B + the 12'×0.5' doorway
    assert!(
        (leaked.area_sf - both_rooms).abs() <= 30.0,
        "default door gap should leak: area {} vs A+B {}",
        leaked.area_sf,
        both_rooms
    );

    // Raised slider: door_gap_ft = 13 → 2r covers the 12 ft opening.
    let sealed = detect_room(
        &raster,
        &map,
        seed,
        &DetectParams {
            door_gap_ft: 13.0,
            ..DetectParams::default()
        },
    )
    .unwrap();
    println!(
        "spot-check (d): 12 ft door @ door_gap_ft 13.0 → {} SF (sealed)",
        sealed.area_sf
    );
    assert!(
        sealed.area_sf >= 570.0 && sealed.area_sf <= 660.0,
        "raised door gap should seal to room A: area {}",
        sealed.area_sf
    );
}

/// (e) Level 2 on a multi-room synthetic sheet: exactly the three real rooms
/// survive; the 20 SF closet fails min_area_sf, the corridor (81% of sheet)
/// fails max_area_frac, and the outside strip touches the raster border.
#[test]
fn ccl_candidates_filtered_and_ordered() {
    // 130×100 ft sheet; building interior (3.5,3.5)–(126.5,96.5) behind a
    // 1 ft wall, leaving a 3.5 ft outside strip whose outer band survives
    // dilation and touches the raster border (border-exclusion case). The
    // corridor (building interior minus room boxes ≈ 10,625 SF) is ≈ 82% of
    // the 13,000 SF sheet → excluded by max_area_frac. All wall-to-wall
    // gaps are ≥ 4 ft so they survive the default 3.6 ft closing.
    let mut plan = SynthPlan::new(130.0, 100.0, PPF);
    plan.walls_rect(3.5, 3.5, 123.0, 93.0, 1.0);
    plan.walls_rect(8.0, 8.0, 20.0, 15.0, 0.5); // R1: 300 SF
    plan.walls_rect(33.0, 8.0, 16.0, 15.0, 0.5); // R2: 240 SF
    plan.walls_rect(54.0, 8.0, 15.0, 10.0, 0.5); // R3: 150 SF
    plan.walls_rect(74.0, 8.0, 5.0, 4.0, 0.5); // closet: 20 SF < 40 → excluded

    let candidates =
        detect_candidates(&plan.raster(), &plan.map(scale()), &DetectParams::default());
    let areas: Vec<f64> = candidates.iter().map(|c| c.area_sf).collect();
    println!("spot-check (e): candidate areas (sorted desc) = {areas:?}");
    assert_eq!(
        candidates.len(),
        3,
        "expected exactly R1, R2, R3; got areas {areas:?}"
    );
    // Sorted by area descending, each within 3%.
    for (candidate, expected) in candidates.iter().zip([300.0, 240.0, 150.0]) {
        assert!(
            (candidate.area_sf - expected).abs() <= expected * 0.03,
            "area {} not within 3% of {expected}",
            candidate.area_sf
        );
    }

    // Centroids land inside their rooms (base units: 1 ft = 18 pts at fpi 4).
    let ft = 18.0;
    let in_box = |p: engine_core::Point, x0: f64, y0: f64, x1: f64, y1: f64| {
        p.x > x0 * ft && p.x < x1 * ft && p.y > y0 * ft && p.y < y1 * ft
    };
    assert!(in_box(candidates[0].centroid, 8.0, 8.0, 28.0, 23.0));
    assert!(in_box(candidates[1].centroid, 33.0, 8.0, 49.0, 23.0));
    assert!(in_box(candidates[2].centroid, 54.0, 8.0, 69.0, 18.0));

    // Bboxes contain their contours.
    for candidate in &candidates {
        let (min, max) = candidate.bbox;
        for p in &candidate.contour {
            assert!(
                p.x >= min.x - 1e-9
                    && p.x <= max.x + 1e-9
                    && p.y >= min.y - 1e-9
                    && p.y <= max.y + 1e-9,
                "contour point {p:?} outside bbox {min:?}..{max:?}"
            );
        }
    }
}
