//! Integration tests for the hatch-structure classifier feeding the wall
//! mask (eval-03 §6.A.1): synthetic hatch fields inside walled rooms →
//! classify_hatch → rasterize_wall_mask(…, Some(flags)) →
//! detect_room_from_mask. The flagship case reproduces eval-03's trapped
//! STORAGE seed and proves exclusion recovers it.

mod common;

use common::{assert_close, SynthSegments};
use engine_core::{
    classify_hatch, detect_room_from_mask, rasterize_wall_mask, DetectError, DetectParams,
    HatchParams, Scale,
};
use proptest::prelude::*;

const PPF: f64 = 10.0;
const WALL_STROKE_PTS: f64 = 3.6;

fn scale() -> Scale {
    Scale::from_fpi(4.0).unwrap()
}

/// 30×25 ft sheet, 20×15 ft room at (5,5), hatch field filling the room
/// interior at the given angle/pitch — the eval-03 STORAGE shape.
fn hatched_room(angle_deg: f64, pitch_ft: f64) -> SynthSegments {
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.wall_rect_ft(5.0, 5.0, 20.0, 15.0, WALL_STROKE_PTS);
    s.hatch_field_ft(5.2, 5.2, 19.6, 14.6, angle_deg, 0.5_f64.max(pitch_ft), WALL_STROKE_PTS);
    s
}

fn detect(
    sheet: &SynthSegments,
    flags: Option<&[bool]>,
    params: &DetectParams,
) -> Result<engine_core::RoomDetection, DetectError> {
    let (w, h) = sheet.grid();
    let mask = rasterize_wall_mask(&sheet.segments(), &sheet.map(), w, h, 0.0, flags).unwrap();
    detect_room_from_mask(&mask, &sheet.map(), sheet.seed_at_ft(15.0, 12.5), params)
}

#[test]
fn hatch_field_flagged_walls_kept_at_multiple_angles() {
    for angle in [0.0, 30.0, 45.0, 90.0] {
        let sheet = hatched_room(angle, 0.5);
        let segs = sheet.segments();
        let params = HatchParams::derive(&segs);
        assert!(params.max_pitch_pts > 0.0, "derive found the field at {angle}°");
        let flags = classify_hatch(&segs, &params);
        // The four walls (first four segments) are never hatch.
        assert!(flags[..4].iter().all(|&f| !f), "walls flagged at {angle}°");
        // The field's interior lines classify. Corner-clipped stragglers
        // (short rails failing the overlap gate) may stay unflagged — the
        // conservative direction — so require a decisive majority, not
        // totality.
        let field = flags.len() - 4;
        let flagged = flags.iter().filter(|&&f| f).count();
        assert!(
            flagged * 10 >= field * 6,
            "only {flagged} of {field} hatch lines flagged at {angle}°"
        );
    }
}

#[test]
fn trapped_seed_recovers_when_hatch_excluded() {
    // Eval-03 STORAGE repro: dense hatch at wall stroke traps the seed…
    let sheet = hatched_room(0.0, 0.5);
    let params = DetectParams::default();
    let trapped = detect(&sheet, None, &params);
    assert!(
        matches!(
            trapped,
            Err(DetectError::SeedTrapped { .. }) | Err(DetectError::SeedOnWall { .. })
        ),
        "hatch at wall width should trap: {trapped:?}"
    );

    // …and hatch exclusion recovers the full room.
    let segs = sheet.segments();
    let flags = classify_hatch(&segs, &HatchParams::derive(&segs));
    let room = detect(&sheet, Some(&flags), &params).unwrap();
    // End-rail exemption keeps a couple of field lines near the walls, so
    // the recovered area may inset slightly past the exact 293.04.
    assert_close(room.area_sf, 293.04, 0.0, 0.10);
}

#[test]
fn short_wall_run_parallel_to_neighbors_not_hatch() {
    // Double-line wall false positive: parallel short wall faces (2 rails
    // per wall, two walls 100 pts apart) must never classify.
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.seg_ft(5.0, 5.0, 15.0, 5.0, WALL_STROKE_PTS);
    s.seg_ft(5.0, 5.3, 15.0, 5.3, WALL_STROKE_PTS);
    s.seg_ft(5.0, 12.0, 15.0, 12.0, WALL_STROKE_PTS);
    s.seg_ft(5.0, 12.3, 15.0, 12.3, WALL_STROKE_PTS);
    let segs = s.segments();
    // Even with a permissive manual max_pitch, rail count stays below 5.
    let params = HatchParams {
        max_pitch_pts: 20.0,
        ..HatchParams::derive(&segs)
    };
    let flags = classify_hatch(&segs, &params);
    assert!(flags.iter().all(|&f| !f));
}

#[test]
fn diagonal_hatch_crossing_wall_keeps_wall() {
    // 45° hatch overrunning the room: the horizontal/vertical walls are a
    // different angle family entirely and can never join the comb.
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.wall_rect_ft(5.0, 5.0, 20.0, 15.0, WALL_STROKE_PTS);
    s.hatch_field_ft(4.0, 4.0, 22.0, 17.0, 45.0, 0.5, WALL_STROKE_PTS);
    let segs = s.segments();
    let flags = classify_hatch(&segs, &HatchParams::derive(&segs));
    assert!(flags[..4].iter().all(|&f| !f), "walls must survive diagonal hatch");
    assert!(flags[4..].iter().filter(|&&f| f).count() > 20, "field must classify");
}

#[test]
fn no_hatch_sheet_is_noop() {
    // Rooms + a few annotation lines, no hatch anywhere: flags all false
    // and the exclusion mask is byte-identical to the plain mask.
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.wall_rect_ft(5.0, 5.0, 20.0, 15.0, WALL_STROKE_PTS);
    s.seg_ft(5.0, 12.5, 25.0, 12.5, 0.12);
    s.seg_ft(2.0, 2.0, 28.0, 2.0, 0.12);
    let segs = s.segments();
    let params = HatchParams::derive(&segs);
    let flags = classify_hatch(&segs, &params);
    assert!(flags.iter().all(|&f| !f));
    let (w, h) = s.grid();
    let plain = rasterize_wall_mask(&segs, &s.map(), w, h, 0.0, None).unwrap();
    let excl = rasterize_wall_mask(&segs, &s.map(), w, h, 0.0, Some(&flags)).unwrap();
    assert_eq!(plain, excl);
}

#[test]
fn equal_room_row_rails_not_hatch() {
    // Five equal-width rooms in a row: double-line walls at a large regular
    // module. The paired faces (0.3 ft apart) break pitch regularity, and
    // the module pitch far exceeds any hatch-derived max pitch.
    let mut s = SynthSegments::new(60.0, 25.0, scale(), PPF);
    for i in 0..6 {
        let x = 5.0 + i as f64 * 9.0;
        s.seg_ft(x, 5.0, x, 20.0, WALL_STROKE_PTS);
        s.seg_ft(x + 0.3, 5.0, x + 0.3, 20.0, WALL_STROKE_PTS);
    }
    // Give the classifier a real hatch elsewhere so max_pitch derives from
    // hatch pitch, not from wall pairs.
    s.hatch_field_ft(30.0, 21.0, 20.0, 3.0, 0.0, 0.5, WALL_STROKE_PTS);
    let segs = s.segments();
    let params = HatchParams::derive(&segs);
    let flags = classify_hatch(&segs, &params);
    // The twelve wall faces (first 12 segments) never classify.
    assert!(flags[..12].iter().all(|&f| !f));
}

#[test]
fn stair_tread_comb_dropped_room_still_encloses() {
    // A stairwell: room walls + 8 regular treads. Treads may classify as
    // hatch (they are not boundaries); the room must still detect cleanly
    // with exclusion on — and MUST NOT leak (walls intact).
    let mut s = SynthSegments::new(30.0, 25.0, scale(), PPF);
    s.wall_rect_ft(5.0, 5.0, 10.0, 15.0, WALL_STROKE_PTS);
    for i in 1..=8 {
        s.seg_ft(5.2, 5.0 + i as f64 * 1.0, 14.8, 5.0 + i as f64 * 1.0, WALL_STROKE_PTS);
    }
    let segs = s.segments();
    let flags = classify_hatch(&segs, &HatchParams::derive(&segs));
    assert!(flags[..4].iter().all(|&f| !f), "stairwell walls kept");
    let (w, h) = s.grid();
    let mask = rasterize_wall_mask(&segs, &s.map(), w, h, 0.0, Some(&flags)).unwrap();
    let room = detect_room_from_mask(
        &mask,
        &s.map(),
        s.seed_at_ft(10.0, 16.0),
        &DetectParams::default(),
    )
    .unwrap();
    // 10×15 ft interior at stroke faces ≈ 9.8×14.8 = 145.04 SF; treads
    // near the seed may partially remain (end rails) — generous band, but
    // decisively bigger than one tread slice and no larger than the room.
    assert!(room.area_sf > 60.0 && room.area_sf < 150.0, "{}", room.area_sf);
}

proptest! {
    /// Exclusion only removes ink: the mask with hatch flags is a subset
    /// of the mask without them, for any random flag vector.
    #[test]
    fn prop_hatch_exclusion_mask_is_subset(flag_bits in proptest::collection::vec(any::<bool>(), 24)) {
        let sheet = hatched_room(0.0, 1.0);
        let segs = sheet.segments();
        let mut flags = vec![false; segs.len()];
        for (i, b) in flag_bits.iter().enumerate() {
            if i < flags.len() { flags[i] = *b; }
        }
        let (w, h) = sheet.grid();
        let plain = rasterize_wall_mask(&segs, &sheet.map(), w, h, 0.0, None).unwrap();
        let excl = rasterize_wall_mask(&segs, &sheet.map(), w, h, 0.0, Some(&flags)).unwrap();
        for y in 0..h {
            for x in 0..w {
                prop_assert!(!excl.get(x, y) || plain.get(x, y));
            }
        }
    }
}
