//! Deterministic detection layer (addendum §A3.1–A3.2): thresholded wall
//! masks, door-gap dilation, scanline flood fill, marching-squares contours,
//! and connected-component candidate detection. Pure raster math — no model
//! calls, no browser APIs; `engine-web` supplies real rasters and the
//! pixel↔base-unit mapping.
//!
//! Level 1 pipeline (per §A3.1 plus its implementation-refinement note):
//! threshold → dilate walls by the door-gap radius → flood fill from the
//! click (boundary abort) → dilate the fill back by the same radius and
//! subtract original walls (morphological closing) → marching squares →
//! base units → Douglas-Peucker → shoelace SF + perimeter LF.

mod ccl;
mod contour;
mod flood;
mod mask_ops;
mod pixelmap;
mod raster;
mod wallmask;

pub use contour::trace_contour;
pub use flood::flood_fill;
pub use mask_ops::{close_region, dilate, door_gap_radius_px, threshold_mask};
pub use pixelmap::{choose_px_per_foot, PixelMap, TARGET_PX_PER_FOOT};
pub use raster::{GrayRaster, Mask, RasterError, MAX_RASTER_PIXELS};
pub use wallmask::{
    default_min_width, passes_width_filter, rasterize_wall_mask, width_histogram, WidthBucket,
};

use crate::geom::{polygon_area, polygon_perimeter, simplify, Point};

/// Components smaller than this many pixels are dropped before the accurate
/// per-component closing pass — pure raster noise, cheaper to skip early.
/// The authoritative size filter is `DetectParams::min_area_sf`, applied to
/// the closed contour's shoelace area.
const NOISE_FLOOR_PX: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum DetectError {
    /// Fall back to manual trace — never guess (addendum §A3.1 step 3).
    #[error("region not enclosed: flood fill reached the raster boundary")]
    RegionNotEnclosed,
    #[error("seed ({x}, {y}) is on a wall pixel")]
    SeedOnWall { x: u32, y: u32 },
    #[error("seed ({x}, {y}) is outside the raster")]
    SeedOutOfBounds { x: u32, y: u32 },
    #[error("px_per_foot must be finite and > 0, got {0}")]
    InvalidPxPerFoot(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectParams {
    /// Luminance strictly below this is a wall pixel (addendum: ~200).
    pub wall_threshold: u8,
    /// Door-opening width the pipeline closes — the user-slider parameter;
    /// industrial plans have 12' equipment doors, re-run is cheap.
    pub door_gap_ft: f64,
    /// Douglas-Peucker epsilon in base units (addendum: ε ≈ 1.5).
    pub simplify_epsilon_pts: f64,
    /// Level 2: candidates below this square footage are discarded.
    pub min_area_sf: f64,
    /// Level 2: candidates above this fraction of the raster are discarded.
    pub max_area_frac: f64,
}

impl Default for DetectParams {
    fn default() -> Self {
        DetectParams {
            wall_threshold: 200,
            door_gap_ft: 3.5,
            simplify_epsilon_pts: 1.5,
            min_area_sf: 40.0,
            max_area_frac: 0.8,
        }
    }
}

/// A Level-1 detected room. Geometry in base units — editable exactly like a
/// hand-traced shape (invariant 4 is what makes that true).
#[derive(Debug, Clone, PartialEq)]
pub struct RoomDetection {
    /// Simplified polygon (closed implicitly, first vertex not repeated).
    pub contour: Vec<Point>,
    /// Shoelace on the simplified polygon — never pixel count.
    pub area_sf: f64,
    /// Polygon perimeter: the cove-base / wall-base LF for free.
    pub perimeter_lf: f64,
}

/// A Level-2 region proposal for the AI labeling call and review queue.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomCandidate {
    pub centroid: Point,
    pub area_sf: f64,
    /// Min / max corners in base units.
    pub bbox: (Point, Point),
    /// Simplified contour in base units.
    pub contour: Vec<Point>,
}

/// Level 1 — click-to-room (addendum §A3.1 with the closing refinement),
/// raster mask source (luminance threshold): the fallback path for scans.
pub fn detect_room(
    raster: &GrayRaster,
    map: &PixelMap,
    seed_px: (u32, u32),
    params: &DetectParams,
) -> Result<RoomDetection, DetectError> {
    let walls = threshold_mask(raster, params.wall_threshold);
    detect_room_from_mask(&walls, map, seed_px, params)
}

/// Level 1 from a pre-built wall mask (addendum §A3.1 mask-source revision):
/// the primary path for CAD PDFs feeds a vector-filtered mask from
/// [`rasterize_wall_mask`] here. Runs §A3.1 steps 2–5 unchanged.
/// `params.wall_threshold` is unused on this path — the mask is already
/// binary by construction.
pub fn detect_room_from_mask(
    walls: &Mask,
    map: &PixelMap,
    seed_px: (u32, u32),
    params: &DetectParams,
) -> Result<RoomDetection, DetectError> {
    let radius = door_gap_radius_px(params.door_gap_ft, map.px_per_foot());
    let dilated = dilate(walls, radius);
    let fill = flood_fill(&dilated, seed_px)?;
    let closed = close_region(&fill, walls, radius);
    let (contour, area_sf, perimeter_lf) = measure_region(&closed, (0, 0), map, params);
    Ok(RoomDetection {
        contour,
        area_sf,
        perimeter_lf,
    })
}

/// Level 2 — auto-detect candidates (addendum §A3.2): full-page mask →
/// dilate → invert → CCL → filter (area window, border-touching) → per-
/// component closing → contour. Sorted by area descending.
pub fn detect_candidates(
    raster: &GrayRaster,
    map: &PixelMap,
    params: &DetectParams,
) -> Vec<RoomCandidate> {
    let walls = threshold_mask(raster, params.wall_threshold);
    let radius = door_gap_radius_px(params.door_gap_ft, map.px_per_foot());
    let dilated = dilate(&walls, radius);
    let rooms = dilated.invert();
    let (labels, components) = ccl::label_components(&rooms);
    let raster_sf = map.px_area_to_sf(f64::from(raster.width()) * f64::from(raster.height()));

    let mut out = Vec::new();
    for comp in &components {
        if comp.touches_border || comp.pixels < NOISE_FLOOR_PX {
            continue;
        }
        // Closing window: component bbox expanded by the radius, clamped.
        let wx0 = comp.min_x.saturating_sub(radius);
        let wy0 = comp.min_y.saturating_sub(radius);
        let wx1 = (comp.max_x + radius).min(raster.width() - 1);
        let wy1 = (comp.max_y + radius).min(raster.height() - 1);
        let (ww, wh) = (wx1 - wx0 + 1, wy1 - wy0 + 1);

        let mut comp_mask = Mask::new(ww, wh);
        let mut wall_win = Mask::new(ww, wh);
        for y in 0..wh {
            for x in 0..ww {
                let (gx, gy) = (wx0 + x, wy0 + y);
                let idx = gy as usize * raster.width() as usize + gx as usize;
                if labels[idx] == comp.label {
                    comp_mask.set(x, y, true);
                }
                if walls.get(gx, gy) {
                    wall_win.set(x, y, true);
                }
            }
        }
        let closed = close_region(&comp_mask, &wall_win, radius);
        let (contour, area_sf, _) = measure_region(&closed, (wx0, wy0), map, params);
        if area_sf < params.min_area_sf || area_sf > params.max_area_frac * raster_sf {
            continue;
        }

        // Centroid and bbox from the closed region's pixels.
        let (mut sum_x, mut sum_y, mut n) = (0.0_f64, 0.0_f64, 0_usize);
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0_u32, 0_u32);
        for y in 0..wh {
            for x in 0..ww {
                if closed.get(x, y) {
                    let (gx, gy) = (wx0 + x, wy0 + y);
                    n += 1;
                    sum_x += f64::from(gx) + 0.5;
                    sum_y += f64::from(gy) + 0.5;
                    min_x = min_x.min(gx);
                    min_y = min_y.min(gy);
                    max_x = max_x.max(gx);
                    max_y = max_y.max(gy);
                }
            }
        }
        if n == 0 {
            continue;
        }
        let centroid = map.px_to_point(sum_x / n as f64, sum_y / n as f64);
        let bbox = (
            map.px_to_point(f64::from(min_x), f64::from(min_y)),
            map.px_to_point(f64::from(max_x + 1), f64::from(max_y + 1)),
        );
        out.push(RoomCandidate {
            centroid,
            area_sf,
            bbox,
            contour,
        });
    }
    out.sort_by(|a, b| {
        b.area_sf
            .partial_cmp(&a.area_sf)
            .expect("detected areas are finite")
    });
    out
}

/// Contour → base units → simplify → (polygon, SF, LF).
fn measure_region(
    region: &Mask,
    offset_px: (u32, u32),
    map: &PixelMap,
    params: &DetectParams,
) -> (Vec<Point>, f64, f64) {
    let raw = trace_contour(region);
    let pts: Vec<Point> = raw
        .iter()
        .map(|&(x, y)| map.px_to_point(x + f64::from(offset_px.0), y + f64::from(offset_px.1)))
        .collect();
    let contour = simplify(&pts, params.simplify_epsilon_pts);
    let area_sf = map.scale().points_sq_to_square_feet(polygon_area(&contour));
    let perimeter_lf = map.scale().points_to_feet(polygon_perimeter(&contour));
    (contour, area_sf, perimeter_lf)
}
