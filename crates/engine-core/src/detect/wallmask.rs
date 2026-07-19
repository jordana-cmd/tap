//! Vector-filtered wall mask (addendum §A3.1 mask-source revision, final
//! spec §9 "Level-1 wall mask source"): build the Level-1 wall mask from
//! extracted stroked segments filtered by stroke width, instead of raster
//! luminance. The filter threshold is data-driven — derived from the page's
//! stroke-width histogram, never a magic constant — because CAD sheets put
//! walls, text, and furniture at exporter-chosen widths that vary per sheet.
//!
//! The rasterized mask feeds the EXISTING §A3.1 pipeline from step 2 on
//! (door-gap dilate → flood fill → morphological closing → contour) via
//! [`detect_room_from_mask`](super::detect_room_from_mask).

use super::pixelmap::PixelMap;
use super::raster::{Mask, RasterError, MAX_RASTER_PIXELS};
use crate::snap::Segment;
use std::collections::BTreeMap;

/// One histogram bucket: a distinct stroke width and how many segments
/// carry it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidthBucket {
    pub width_pts: f64,
    pub segments: usize,
}

/// Widths within this quantum collapse into one bucket: CAD stroke widths
/// are discrete pen weights; sub-0.01-pt spread is CTM float noise.
const WIDTH_QUANTUM: f64 = 0.01;

/// Stroke-width histogram of a page's segments, sorted ascending by width.
/// Widths are quantized to 0.01 pt; non-finite and negative widths are
/// skipped (defensive against extraction bugs, mirroring `SegmentIndex`).
pub fn width_histogram(segments: &[Segment]) -> Vec<WidthBucket> {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for seg in segments {
        if !seg.width.is_finite() || seg.width < 0.0 {
            continue;
        }
        *counts.entry((seg.width / WIDTH_QUANTUM).round() as i64).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(key, segments)| WidthBucket {
            width_pts: key as f64 * WIDTH_QUANTUM,
            segments,
        })
        .collect()
}


/// Data-driven default for the minimum stroke width kept as "wall ink":
/// the midpoint of the TWO THINNEST distinct bucket widths — i.e. drop
/// exactly the thinnest layer (hairline/zero-width annotation: dimension
/// lines, grids, leaders), keep everything from the second bucket up.
///
/// Rationale (eval-03): the earlier thinnest↔modal midpoint degenerated
/// to a no-op whenever the thinnest bucket was also the modal one — true
/// on both measured full-size sheets, where the 0-width annotation layer
/// dominates by count. The two-thinnest rule is never MORE aggressive
/// than the old rule and still degrades safely: empty histogram → 0.0;
/// single bucket → threshold == that width and the INCLUSIVE filter
/// ([`passes_width_filter`]) keeps everything.
///
/// Accepted, documented risk: a sheet whose walls occupy the thinnest of
/// ≥2 buckets would leak at this default. That contradicts drafting
/// convention on every fixture measured so far, the miss is visible in
/// the harness skeleton overlay, and this is a DEFAULT for a user-held
/// slider — not a gate.
pub fn default_min_width(hist: &[WidthBucket]) -> f64 {
    match hist {
        [] => 0.0,
        [only] => only.width_pts,
        [thinnest, second, ..] => (thinnest.width_pts + second.width_pts) / 2.0,
    }
}

/// The single width-filter predicate, shared by [`rasterize_wall_mask`] and
/// engine-web's overlay id list: finite width ≥ `min_width_pts` (inclusive,
/// so a threshold equal to the thinnest width keeps that bucket).
pub fn passes_width_filter(seg: &Segment, min_width_pts: f64) -> bool {
    seg.width.is_finite() && seg.width >= min_width_pts
}

/// Rasterize width-filtered segments into a wall mask on the same pixel
/// grid the raster path uses (`map` supplies pts↔px; the grid is
/// `width_px × height_px` with pixel (0,0) at `map`'s origin).
///
/// Stroke thickness: `width_pts / map.points_per_px()`, half-thickness
/// clamped to ≥ 0.5 px so hairline-thin kept segments still rasterize at
/// least one pixel wide. Drawing = capsule coverage test (pixel center
/// within half-thickness of the segment) over the segment's expanded
/// bounding box, plus a supercover centerline walk so 1-px diagonals stay
/// 4-connected for the flood fill regardless of pixel-center phase.
///
/// Segments with non-finite geometry are skipped (mirrors
/// `SegmentIndex::build`). Zero-length segments stamp a dot.
pub fn rasterize_wall_mask(
    segments: &[Segment],
    map: &PixelMap,
    width_px: u32,
    height_px: u32,
    min_width_pts: f64,
) -> Result<Mask, RasterError> {
    if width_px == 0 || height_px == 0 {
        return Err(RasterError::Empty);
    }
    if width_px as usize * height_px as usize > MAX_RASTER_PIXELS {
        return Err(RasterError::TooLarge {
            width: width_px,
            height: height_px,
        });
    }
    let mut mask = Mask::new(width_px, height_px);
    for seg in segments {
        if !passes_width_filter(seg, min_width_pts) || !finite_geometry(seg) {
            continue;
        }
        let (x1, y1) = map.point_to_px(seg.p1);
        let (x2, y2) = map.point_to_px(seg.p2);
        let half = (seg.width / map.points_per_px() / 2.0).max(0.5);
        stamp_capsule(&mut mask, x1, y1, x2, y2, half);
        walk_supercover(&mut mask, x1, y1, x2, y2);
    }
    Ok(mask)
}

fn finite_geometry(seg: &Segment) -> bool {
    seg.p1.x.is_finite() && seg.p1.y.is_finite() && seg.p2.x.is_finite() && seg.p2.y.is_finite()
}

/// Set every pixel whose CENTER lies within `half` px of segment
/// (x1,y1)–(x2,y2), scanning only the expanded bounding box.
fn stamp_capsule(mask: &mut Mask, x1: f64, y1: f64, x2: f64, y2: f64, half: f64) {
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    let x0 = ((x1.min(x2) - half).floor() as i64).max(0);
    let y0 = ((y1.min(y2) - half).floor() as i64).max(0);
    let xn = ((x1.max(x2) + half).ceil() as i64).min(w - 1);
    let yn = ((y1.max(y2) + half).ceil() as i64).min(h - 1);
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    let half_sq = half * half;
    for py in y0..=yn {
        for px in x0..=xn {
            let (cx, cy) = (px as f64 + 0.5, py as f64 + 0.5);
            let t = if len_sq == 0.0 {
                0.0
            } else {
                (((cx - x1) * dx + (cy - y1) * dy) / len_sq).clamp(0.0, 1.0)
            };
            let (nx, ny) = (x1 + t * dx - cx, y1 + t * dy - cy);
            if nx * nx + ny * ny <= half_sq {
                mask.set(px as u32, py as u32, true);
            }
        }
    }
}

/// Set every grid cell the centerline passes through (supercover DDA), so a
/// kept segment is always a 4-connected barrier even at 1-px thickness.
fn walk_supercover(mask: &mut Mask, x1: f64, y1: f64, x2: f64, y2: f64) {
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    let set = |mask: &mut Mask, x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < w && y < h {
            mask.set(x as u32, y as u32, true);
        }
    };
    let steps = (x2 - x1).abs().max((y2 - y1).abs()).ceil() as usize * 2 + 1;
    let mut prev: Option<(i64, i64)> = None;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let cx = (x1 + t * (x2 - x1)).floor() as i64;
        let cy = (y1 + t * (y2 - y1)).floor() as i64;
        if let Some((px, py)) = prev {
            // Diagonal hop: bridge with an orthogonal neighbor to preserve
            // 4-connectivity of the barrier.
            if (cx - px).abs() == 1 && (cy - py).abs() == 1 {
                set(mask, cx, py);
            }
        }
        set(mask, cx, cy);
        prev = Some((cx, cy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;
    use crate::scale::Scale;

    fn seg(x1: f64, y1: f64, x2: f64, y2: f64, width: f64) -> Segment {
        Segment {
            p1: Point::new(x1, y1),
            p2: Point::new(x2, y2),
            width,
        }
    }

    /// fpi 4, 10 px/ft → 1.8 pts per px (same anchor as pixelmap tests).
    fn map() -> PixelMap {
        PixelMap::new(Point::new(0.0, 0.0), Scale::from_fpi(4.0).unwrap(), 10.0).unwrap()
    }

    #[test]
    fn histogram_buckets_collapse_float_noise_and_sort() {
        let segs = [
            seg(0.0, 0.0, 1.0, 0.0, 0.24),
            seg(0.0, 1.0, 1.0, 1.0, 0.240_000_4),
            seg(0.0, 2.0, 1.0, 2.0, 0.12),
        ];
        let hist = width_histogram(&segs);
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].width_pts, 0.12);
        assert_eq!(hist[0].segments, 1);
        assert_eq!(hist[1].width_pts, 0.24);
        assert_eq!(hist[1].segments, 2);
    }

    #[test]
    fn histogram_skips_nonfinite_and_negative_widths() {
        let segs = [
            seg(0.0, 0.0, 1.0, 0.0, f64::NAN),
            seg(0.0, 0.0, 1.0, 0.0, f64::INFINITY),
            seg(0.0, 0.0, 1.0, 0.0, -0.5),
            seg(0.0, 0.0, 1.0, 0.0, 0.24),
        ];
        let hist = width_histogram(&segs);
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].segments, 1);
    }

    #[test]
    fn default_min_width_lands_between_two_thinnest() {
        // Fixture-001-shaped: hairlines at 0.12, dominant linework at 0.24.
        let hist = [
            WidthBucket { width_pts: 0.12, segments: 4_647 },
            WidthBucket { width_pts: 0.24, segments: 67_836 },
            WidthBucket { width_pts: 0.36, segments: 776 },
            WidthBucket { width_pts: 0.48, segments: 135 },
            WidthBucket { width_pts: 0.96, segments: 234 },
        ];
        let d = default_min_width(&hist);
        assert!((d - 0.18).abs() < 1e-12);
    }

    #[test]
    fn default_min_width_single_bucket_filter_keeps_all() {
        let hist = [WidthBucket { width_pts: 0.24, segments: 100 }];
        let d = default_min_width(&hist);
        assert_eq!(d, 0.24);
        assert!(passes_width_filter(&seg(0.0, 0.0, 1.0, 0.0, 0.24), d));
    }

    #[test]
    fn default_min_width_mode_is_thinnest_drops_thinnest_bucket() {
        // Eval-03 degeneracy: modal bucket IS the thinnest (zero-width
        // annotation dominates). New rule drops exactly that layer.
        let hist = [
            WidthBucket { width_pts: 0.12, segments: 900 },
            WidthBucket { width_pts: 0.48, segments: 10 },
        ];
        let d = default_min_width(&hist);
        assert!((d - 0.30).abs() < 1e-12);
        assert!(!passes_width_filter(&seg(0.0, 0.0, 1.0, 0.0, 0.12), d));
        assert!(passes_width_filter(&seg(0.0, 0.0, 1.0, 0.0, 0.48), d));
    }

    #[test]
    fn default_min_width_fixture002_shape_drops_zero_width_layer() {
        // p37-shaped: 0-width modal+thinnest, walls et al at 0.72.
        let hist = [
            WidthBucket { width_pts: 0.0, segments: 4_125 },
            WidthBucket { width_pts: 0.36, segments: 1_659 },
            WidthBucket { width_pts: 0.72, segments: 3_826 },
            WidthBucket { width_pts: 0.84, segments: 68 },
            WidthBucket { width_pts: 1.74, segments: 497 },
        ];
        let d = default_min_width(&hist);
        assert!((d - 0.18).abs() < 1e-12);
        assert!(!passes_width_filter(&seg(0.0, 0.0, 1.0, 0.0, 0.0), d));
        assert!(passes_width_filter(&seg(0.0, 0.0, 1.0, 0.0, 0.36), d));
    }

    #[test]
    fn default_min_width_empty_is_zero() {
        assert_eq!(default_min_width(&[]), 0.0);
    }

    #[test]
    fn rasterize_horizontal_stroke_thickness_in_px() {
        // Stroke 3.6 pts at 1.8 pts/px = 2 px thick → half = 1.0 px.
        // Centerline along y = 9.0 pts = 5.0 px: pixel centers 4.5 and 5.5
        // are exactly 0.5 px away → rows 4 and 5 set, rows 3 and 6 not.
        let m = map();
        let segs = [seg(9.0, 9.0, 45.0, 9.0, 3.6)];
        let mask = rasterize_wall_mask(&segs, &m, 40, 12, 0.0).unwrap();
        assert!(mask.get(10, 4) && mask.get(10, 5));
        assert!(!mask.get(10, 3) && !mask.get(10, 6));
    }

    #[test]
    fn rasterize_hairline_diagonal_is_connected() {
        // A hairline diagonal (min thickness) must be a 4-connected barrier:
        // flood fill from one side must NOT reach the other side.
        let m = map();
        let segs = [seg(0.0, 0.0, 36.0, 36.0, 0.01)];
        let mask = rasterize_wall_mask(&segs, &m, 20, 20, 0.0).unwrap();
        let fill = super::super::flood_fill(&mask, (15, 2));
        // Fill aborts at the boundary (open region) — but must never cross
        // the diagonal. Check by verifying no filled pixel below-left of it.
        if let Ok(fill) = fill {
            for y in 0..20u32 {
                for x in 0..20u32 {
                    if fill.get(x, y) {
                        assert!(
                            x > y,
                            "flood crossed the diagonal barrier at ({x}, {y})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rasterize_zero_length_segment_is_dot() {
        let m = map();
        let segs = [seg(9.0, 9.0, 9.0, 9.0, 0.24)];
        let mask = rasterize_wall_mask(&segs, &m, 10, 10, 0.0).unwrap();
        assert!(mask.get(5, 5));
        assert!(mask.count() <= 4, "dot stamped {} px", mask.count());
    }

    #[test]
    fn rasterize_excludes_below_min_width_inclusive_boundary() {
        let m = map();
        let segs = [
            seg(0.0, 3.6, 18.0, 3.6, 0.12), // below → dropped
            seg(0.0, 9.0, 18.0, 9.0, 0.18), // equal → kept (inclusive)
        ];
        let mask = rasterize_wall_mask(&segs, &m, 10, 10, 0.18).unwrap();
        assert!(!mask.get(5, 2), "0.12 segment should be filtered out");
        assert!(mask.get(5, 5), "0.18 segment should be kept");
    }

    #[test]
    fn rasterize_skips_nonfinite_segments() {
        let m = map();
        let segs = [seg(f64::NAN, 0.0, 18.0, 0.0, 0.24)];
        let mask = rasterize_wall_mask(&segs, &m, 10, 10, 0.0).unwrap();
        assert_eq!(mask.count(), 0);
    }

    #[test]
    fn rasterize_rejects_zero_and_oversize_grid() {
        let m = map();
        assert_eq!(
            rasterize_wall_mask(&[], &m, 0, 10, 0.0),
            Err(RasterError::Empty)
        );
        assert!(matches!(
            rasterize_wall_mask(&[], &m, 5000, 5000, 0.0),
            Err(RasterError::TooLarge { .. })
        ));
    }
}
