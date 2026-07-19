//! Browser adapter (final spec §1.1): the ONLY crate that touches
//! wasm-bindgen. Thin exports over engine-core; all logic lives in
//! plain-Rust functions (natively unit-tested), and the `#[wasm_bindgen]`
//! wrappers only convert types at the boundary.
//!
//! Errors cross the boundary as thrown JS objects `{ code, message }`,
//! where `code` is a stable machine-readable identifier the UI branches
//! on (RegionNotEnclosed is a product behavior — fall back to manual
//! trace — not a stringly-typed exception). The code mapping lives in
//! ONE place ([`WebError::code`]) with exhaustive matches, so a new
//! engine-core error variant cannot ship without a code.
//!
//! Raster handoff is copy-in per call: the Uint8Array is copied into wasm
//! memory for the duration of the call and freed on return; JS keeps its
//! buffer, no views into wasm memory are retained (views invalidate on
//! memory growth — zero-copy belongs to the real tile pipeline later).

use engine_core::detect::{DetectParams, GrayRaster, PixelMap, RasterError};
use engine_core::{
    polygon_area, polyline_length, DetectError, Point, RoomDetection, Scale, ScaleError, Segment,
    SegmentIndex, SnapKind,
};
use wasm_bindgen::prelude::*;

// ---------- error mapping (single source of truth) ----------

#[derive(Debug, PartialEq)]
pub(crate) enum WebError {
    Detect(DetectError),
    Raster(RasterError),
    Scale(ScaleError),
    /// Segment buffer is not [x1,y1,x2,y2,width] × n.
    BadSegments { len: usize },
}

impl WebError {
    /// Stable machine-readable codes. Exhaustive matches: adding a variant
    /// upstream breaks compilation here instead of shipping code-less.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            WebError::Detect(e) => match e {
                DetectError::RegionNotEnclosed => "REGION_NOT_ENCLOSED",
                DetectError::SeedOnWall { .. } => "SEED_ON_WALL",
                DetectError::SeedTrapped { .. } => "SEED_TRAPPED",
                DetectError::SeedOutOfBounds { .. } => "SEED_OUT_OF_BOUNDS",
                DetectError::InvalidPxPerFoot(_) => "INVALID_PX_PER_FOOT",
            },
            WebError::Raster(e) => match e {
                RasterError::Empty
                | RasterError::BufferMismatch { .. }
                | RasterError::TooLarge { .. } => "BAD_RASTER",
            },
            WebError::Scale(e) => match e {
                ScaleError::InvalidFpi(_) => "INVALID_SCALE",
                ScaleError::InvalidKnownLength(_) | ScaleError::DegenerateSpan => {
                    "INVALID_CALIBRATION"
                }
            },
            WebError::BadSegments { .. } => "BAD_SEGMENTS",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            WebError::Detect(e) => e.to_string(),
            WebError::Raster(e) => e.to_string(),
            WebError::Scale(e) => e.to_string(),
            WebError::BadSegments { len } => {
                format!("segment buffer length {len} is not a multiple of 5")
            }
        }
    }
}

impl From<DetectError> for WebError {
    fn from(e: DetectError) -> WebError {
        WebError::Detect(e)
    }
}
impl From<RasterError> for WebError {
    fn from(e: RasterError) -> WebError {
        WebError::Raster(e)
    }
}
impl From<ScaleError> for WebError {
    fn from(e: ScaleError) -> WebError {
        WebError::Scale(e)
    }
}

/// Boundary conversion only — never called from native code paths.
fn to_js(err: WebError) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"code".into(), &err.code().into());
    let _ = js_sys::Reflect::set(&obj, &"message".into(), &err.message().into());
    obj.into()
}

// ---------- plain-Rust core (natively tested) ----------

/// Flat [x0,y0,x1,y1,…] base-unit pairs → Points. A trailing odd element
/// is ignored (chunks of exactly two).
fn to_points(flat: &[f64]) -> Vec<Point> {
    flat.chunks_exact(2)
        .map(|c| Point::new(c[0], c[1]))
        .collect()
}

fn polyline_feet(points: &[f64], fpi: f64) -> Result<f64, WebError> {
    let scale = Scale::from_fpi(fpi)?;
    Ok(scale.points_to_feet(polyline_length(&to_points(points))))
}

fn polygon_square_feet(points: &[f64], fpi: f64) -> Result<f64, WebError> {
    let scale = Scale::from_fpi(fpi)?;
    Ok(scale.points_sq_to_square_feet(polygon_area(&to_points(points))))
}

#[derive(Debug)]
struct RoomOut {
    contour: Vec<f64>,
    area_sf: f64,
    perimeter_lf: f64,
}

#[allow(clippy::too_many_arguments)]
fn detect(
    gray: &[u8],
    width: u32,
    height: u32,
    seed_x: u32,
    seed_y: u32,
    fpi: f64,
    px_per_foot: f64,
    wall_threshold: u8,
    door_gap_ft: f64,
) -> Result<RoomOut, WebError> {
    let raster = GrayRaster::new(width, height, gray.to_vec())?;
    let scale = Scale::from_fpi(fpi)?;
    // The harness raster IS the page: PixelMap origin (0,0).
    let map = PixelMap::new(Point::new(0.0, 0.0), scale, px_per_foot)?;
    let params = DetectParams {
        wall_threshold,
        door_gap_ft,
        ..DetectParams::default()
    };
    let RoomDetection {
        contour,
        area_sf,
        perimeter_lf,
    } = engine_core::detect_room(&raster, &map, (seed_x, seed_y), &params)?;
    Ok(RoomOut {
        contour: contour.iter().flat_map(|p| [p.x, p.y]).collect(),
        area_sf,
        perimeter_lf,
    })
}

/// One page's extracted vector geometry: segments + spatial index, built
/// ONCE per page (rebuilding an rstar bulk-load over ~10⁵ segments per
/// click or per pointer-move is the wrong steady-state). Serves the
/// stroke-width histogram, the vector wall-mask detect path, the overlay
/// filter list, and snapping from a single owned copy of the segments.
struct PageGeom {
    index: SegmentIndex,
}

impl PageGeom {
    /// `flat` = [x1,y1,x2,y2,width_pts] × n in base units.
    fn from_flat(flat: &[f64]) -> Result<PageGeom, WebError> {
        if flat.len() % 5 != 0 {
            return Err(WebError::BadSegments { len: flat.len() });
        }
        let segments: Vec<Segment> = flat
            .chunks_exact(5)
            .map(|c| Segment {
                p1: Point::new(c[0], c[1]),
                p2: Point::new(c[2], c[3]),
                width: c[4],
            })
            .collect();
        Ok(PageGeom {
            index: SegmentIndex::build(segments),
        })
    }

    fn segment_count(&self) -> u32 {
        self.index.len() as u32
    }

    fn histogram_json(&self) -> String {
        let entries: Vec<String> = engine_core::width_histogram(self.index.segments())
            .iter()
            .map(|b| format!("{{\"width_pts\":{},\"segments\":{}}}", b.width_pts, b.segments))
            .collect();
        format!("[{}]", entries.join(","))
    }

    fn default_min_width(&self) -> f64 {
        engine_core::default_min_width(&engine_core::width_histogram(self.index.segments()))
    }

    /// Ids (positions in the flat build array) of segments passing the
    /// width filter — the harness's wall-skeleton overlay.
    fn passing_ids(&self, min_width_pts: f64) -> Vec<u32> {
        self.index
            .segments()
            .iter()
            .enumerate()
            .filter(|(_, s)| engine_core::passes_width_filter(s, min_width_pts))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Vector-mask Level-1 detect: rasterize width-filtered segments onto
    /// the canvas pixel grid, then run the shared §A3.1 pipeline.
    #[allow(clippy::too_many_arguments)]
    fn detect_vector(
        &self,
        width_px: u32,
        height_px: u32,
        seed_x: u32,
        seed_y: u32,
        fpi: f64,
        px_per_foot: f64,
        min_width_pts: f64,
        door_gap_ft: f64,
    ) -> Result<RoomOut, WebError> {
        let scale = Scale::from_fpi(fpi)?;
        // Same convention as the raster path: the canvas IS the page.
        let map = PixelMap::new(Point::new(0.0, 0.0), scale, px_per_foot)?;
        let mask = engine_core::rasterize_wall_mask(
            self.index.segments(),
            &map,
            width_px,
            height_px,
            min_width_pts,
        )?;
        let params = DetectParams {
            door_gap_ft,
            ..DetectParams::default()
        };
        let RoomDetection {
            contour,
            area_sf,
            perimeter_lf,
        } = engine_core::detect_room_from_mask(&mask, &map, (seed_x, seed_y), &params)?;
        Ok(RoomOut {
            contour: contour.iter().flat_map(|p| [p.x, p.y]).collect(),
            area_sf,
            perimeter_lf,
        })
    }

    /// Snap in BASE UNITS (tolerance already divided by zoom on the JS
    /// side). None → no snap within tolerance (or empty index).
    fn snap_json(&self, x: f64, y: f64, tolerance_pts: f64) -> Option<String> {
        let snap = self.index.snap(Point::new(x, y), tolerance_pts)?;
        let (kind, other) = match snap.kind {
            SnapKind::Endpoint => ("endpoint", None),
            SnapKind::Intersection { other } => ("intersection", Some(other.0)),
            SnapKind::Projection => ("projection", None),
        };
        let other_field = other.map_or(String::new(), |o| format!(",\"other\":{o}"));
        Some(format!(
            "{{\"x\":{},\"y\":{},\"kind\":\"{}\",\"segment\":{}{}}}",
            snap.point.x, snap.point.y, kind, snap.segment.0, other_field
        ))
    }
}

fn calibrate(x1: f64, y1: f64, x2: f64, y2: f64, known_feet: f64) -> Result<f64, WebError> {
    let scale =
        engine_core::calibrate_two_point(Point::new(x1, y1), Point::new(x2, y2), known_feet)?;
    Ok(scale.fpi())
}

fn presets_json() -> String {
    let entries: Vec<String> = engine_core::SCALE_PRESETS
        .iter()
        .map(|p| {
            format!(
                "{{\"label\":{:?},\"fpi\":{}}}",
                p.label,
                p.fpi
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

// ---------- wasm exports (3-line wrappers) ----------

/// Length of an OPEN polyline in real feet. `points`: flat [x0,y0,…] in
/// base units (PDF points). Close the ring yourself (repeat the first
/// point) if you want a perimeter.
#[wasm_bindgen]
pub fn measure_polyline(points: &[f64], fpi: f64) -> Result<f64, JsValue> {
    polyline_feet(points, fpi).map_err(to_js)
}

/// Shoelace area of a polygon in square feet (auto-closes the ring).
#[wasm_bindgen]
pub fn measure_polygon_area(points: &[f64], fpi: f64) -> Result<f64, JsValue> {
    polygon_square_feet(points, fpi).map_err(to_js)
}

/// Two-point calibration: the span (x1,y1)–(x2,y2) in BASE UNITS covers
/// `known_feet` real feet; returns the derived fpi. Throws
/// `{ code: "INVALID_CALIBRATION" | "INVALID_SCALE", message }`.
#[wasm_bindgen]
pub fn calibrate_two_point(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    known_feet: f64,
) -> Result<f64, JsValue> {
    calibrate(x1, y1, x2, y2, known_feet).map_err(to_js)
}

/// The canonical named-scale preset table as JSON
/// `[{label, fpi}, …]` — display labels only; fpi is the representation.
#[wasm_bindgen]
pub fn scale_presets_json() -> String {
    presets_json()
}

#[wasm_bindgen]
pub struct RoomResult {
    contour: Vec<f64>,
    area_sf: f64,
    perimeter_lf: f64,
}

#[wasm_bindgen]
impl RoomResult {
    /// Simplified contour as flat base-unit pairs (a fresh Float64Array
    /// owned by JS).
    #[wasm_bindgen(getter)]
    pub fn contour(&self) -> Vec<f64> {
        self.contour.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn area_sf(&self) -> f64 {
        self.area_sf
    }

    #[wasm_bindgen(getter)]
    pub fn perimeter_lf(&self) -> f64 {
        self.perimeter_lf
    }
}

/// Click-to-room flood fill on a grayscale raster (row-major u8 luminance,
/// top-left origin). Throws `{ code, message }`; REGION_NOT_ENCLOSED is
/// the fall-back-to-manual-trace branch, not a crash.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn detect_room(
    gray: &[u8],
    width: u32,
    height: u32,
    seed_x: u32,
    seed_y: u32,
    fpi: f64,
    px_per_foot: f64,
    wall_threshold: u8,
    door_gap_ft: f64,
) -> Result<RoomResult, JsValue> {
    detect(
        gray,
        width,
        height,
        seed_x,
        seed_y,
        fpi,
        px_per_foot,
        wall_threshold,
        door_gap_ft,
    )
    .map(|r| RoomResult {
        contour: r.contour,
        area_sf: r.area_sf,
        perimeter_lf: r.perimeter_lf,
    })
    .map_err(to_js)
}

/// Per-page vector geometry handle (addendum §A3.1 mask-source revision +
/// §A3.3). Construct once per page from the extraction's flat
/// `[x1,y1,x2,y2,width_pts] × n` Float64Array (base units, top-left
/// origin); call `.free()` before replacing it. An empty array is valid —
/// the scanned-PDF degrade path (detect throws, snap returns null).
#[wasm_bindgen]
pub struct PageGeometry {
    inner: PageGeom,
}

#[wasm_bindgen]
impl PageGeometry {
    /// Throws `{ code: "BAD_SEGMENTS" }` if `flat.length % 5 != 0`.
    #[wasm_bindgen(constructor)]
    pub fn new(flat: &[f64]) -> Result<PageGeometry, JsValue> {
        PageGeom::from_flat(flat)
            .map(|inner| PageGeometry { inner })
            .map_err(to_js)
    }

    /// Total segments in the id space (including non-finite inert entries).
    #[wasm_bindgen(getter)]
    pub fn segment_count(&self) -> u32 {
        self.inner.segment_count()
    }

    /// Stroke-width histogram as JSON `[{width_pts, segments}, …]`,
    /// ascending by width.
    pub fn histogram_json(&self) -> String {
        self.inner.histogram_json()
    }

    /// Data-driven default for the min-width filter (midpoint between the
    /// thinnest and the modal stroke width; 0 when no segments).
    pub fn default_min_width(&self) -> f64 {
        self.inner.default_min_width()
    }

    /// Ids of segments passing the width filter — drives the harness's
    /// wall-skeleton overlay (ids index the flat construction array × 5).
    pub fn wall_segment_ids(&self, min_width_pts: f64) -> Vec<u32> {
        self.inner.passing_ids(min_width_pts)
    }

    /// Click-to-room on the VECTOR wall mask: width-filtered segments
    /// rasterized onto the canvas grid, then the same dilate/flood/close
    /// pipeline as the raster path. Same error codes as `detect_room`.
    #[allow(clippy::too_many_arguments)]
    pub fn detect_room_vector(
        &self,
        width_px: u32,
        height_px: u32,
        seed_x: u32,
        seed_y: u32,
        fpi: f64,
        px_per_foot: f64,
        min_width_pts: f64,
        door_gap_ft: f64,
    ) -> Result<RoomResult, JsValue> {
        self.inner
            .detect_vector(
                width_px,
                height_px,
                seed_x,
                seed_y,
                fpi,
                px_per_foot,
                min_width_pts,
                door_gap_ft,
            )
            .map(|r| RoomResult {
                contour: r.contour,
                area_sf: r.area_sf,
                perimeter_lf: r.perimeter_lf,
            })
            .map_err(to_js)
    }

    /// Snap a base-unit cursor within a base-unit tolerance (JS converts
    /// the addendum's `10 / zoom` screen tolerance before calling).
    /// Returns JSON `{x, y, kind, segment[, other]}` or null.
    pub fn snap_json(&self, x: f64, y: f64, tolerance_pts: f64) -> Option<String> {
        self.inner.snap_json(x, y, tolerance_pts)
    }
}

// ---------- native tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_feet_72pts_at_fpi4_is_4ft() {
        assert_eq!(polyline_feet(&[0.0, 0.0, 72.0, 0.0], 4.0).unwrap(), 4.0);
    }

    #[test]
    fn polygon_square_feet_paper_inch_square() {
        let square = [0.0, 0.0, 72.0, 0.0, 72.0, 72.0, 0.0, 72.0];
        assert_eq!(polygon_square_feet(&square, 4.0).unwrap(), 16.0);
    }

    #[test]
    fn trailing_odd_element_ignored() {
        assert_eq!(
            polyline_feet(&[0.0, 0.0, 72.0, 0.0, 99.0], 4.0).unwrap(),
            4.0
        );
    }

    #[test]
    fn invalid_scale_maps_to_code() {
        let err = polyline_feet(&[0.0, 0.0, 1.0, 1.0], 0.0).unwrap_err();
        assert_eq!(err.code(), "INVALID_SCALE");
        assert!(err.message().contains("finite"));
    }

    #[test]
    fn calibrate_works_and_maps_codes() {
        // 72-pt span over 4 ft → fpi 4.0.
        assert_eq!(calibrate(0.0, 0.0, 72.0, 0.0, 4.0).unwrap(), 4.0);
        assert_eq!(
            calibrate(0.0, 0.0, 72.0, 0.0, -1.0).unwrap_err().code(),
            "INVALID_CALIBRATION"
        );
        assert_eq!(
            calibrate(0.0, 0.0, 0.1, 0.0, 4.0).unwrap_err().code(),
            "INVALID_CALIBRATION"
        );
    }

    #[test]
    fn presets_json_is_valid_and_anchored() {
        let json = presets_json();
        assert!(json.starts_with('[') && json.ends_with(']'));
        assert!(json.contains("\"label\":\"1/4\\\" = 1'-0\\\"\",\"fpi\":4"));
        assert!(json.contains("\"fpi\":20"));
    }

    /// 200×200 px synthetic: white field, black wall ring enclosing a
    /// 100×100 px interior (10×10 ft at 10 px/ft) — margins exceed the
    /// default 18 px dilation radius.
    fn room_raster() -> Vec<u8> {
        let mut gray = vec![255_u8; 200 * 200];
        let mut wall = |x0: usize, y0: usize, x1: usize, y1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    gray[y * 200 + x] = 0;
                }
            }
        };
        wall(45, 45, 155, 50); // top
        wall(45, 150, 155, 155); // bottom
        wall(45, 50, 50, 150); // left
        wall(150, 50, 155, 150); // right
        gray
    }

    #[test]
    fn detect_recovers_room_area() {
        let out = detect(&room_raster(), 200, 200, 100, 100, 4.0, 10.0, 200, 3.5).unwrap();
        assert!((out.area_sf - 100.0).abs() <= 3.0, "area {}", out.area_sf);
        assert!(
            (out.perimeter_lf - 40.0).abs() <= 1.5,
            "perimeter {}",
            out.perimeter_lf
        );
        assert!(out.contour.len() >= 8); // ≥ 4 vertices, flat pairs
    }

    #[test]
    fn detect_error_codes_map_one_to_one() {
        let gray = room_raster();
        // Seed on a wall pixel.
        let err = detect(&gray, 200, 200, 47, 100, 4.0, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "SEED_ON_WALL");
        // Seed outside the raster.
        let err = detect(&gray, 200, 200, 999, 100, 4.0, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "SEED_OUT_OF_BOUNDS");
        // Seed outside the room: fill reaches the boundary.
        let err = detect(&gray, 200, 200, 10, 10, 4.0, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "REGION_NOT_ENCLOSED");
        // Seed in a fully dilation-closed sliver: a second wall ring 20 px
        // (2 ft) inside the first leaves no fillable pixel between them.
        let mut sliver = room_raster();
        let mut wall = |x0: usize, y0: usize, x1: usize, y1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    sliver[y * 200 + x] = 0;
                }
            }
        };
        wall(70, 70, 130, 75);
        wall(70, 125, 130, 130);
        wall(70, 75, 75, 125);
        wall(125, 75, 130, 125);
        let err = detect(&sliver, 200, 200, 60, 100, 4.0, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "SEED_TRAPPED");
        // Buffer length mismatch.
        let err = detect(&gray, 300, 300, 100, 100, 4.0, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "BAD_RASTER");
        // Bad px_per_foot.
        let err = detect(&gray, 200, 200, 100, 100, 4.0, 0.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "INVALID_PX_PER_FOOT");
        // Bad scale.
        let err = detect(&gray, 200, 200, 100, 100, f64::NAN, 10.0, 200, 3.5).unwrap_err();
        assert_eq!(err.code(), "INVALID_SCALE");
    }

    /// Flat-form square room: 10×10 ft interior at fpi 4 (1 ft = 18 pts),
    /// stroke centerlines on the interior faces ± half stroke. Walls at
    /// 3.6 pts (2 px at 10 ppf), one hairline dim line crossing inside.
    fn room_flat() -> Vec<f64> {
        let (a, b) = (90.0, 270.0); // 5 ft and 15 ft in pts
        vec![
            a, a, b, a, 3.6, // top
            b, a, b, b, 3.6, // right
            b, b, a, b, 3.6, // bottom
            a, b, a, a, 3.6, // left
            // Hairline annotation mid-room, crossing THROUGH the right wall
            // (so its endpoint is not coincident with the intersection).
            a, 180.0, 280.0, 180.0, 0.12,
        ]
    }

    #[test]
    fn page_geom_rejects_len_not_multiple_of_five() {
        // (match, not unwrap_err — SegmentIndex has no Debug impl)
        let Err(err) = PageGeom::from_flat(&[1.0, 2.0, 3.0]) else {
            panic!("expected BAD_SEGMENTS");
        };
        assert_eq!(err.code(), "BAD_SEGMENTS");
        assert!(err.message().contains("multiple of 5"));
    }

    #[test]
    fn page_geom_histogram_json_and_default_width() {
        let g = PageGeom::from_flat(&room_flat()).unwrap();
        assert_eq!(g.segment_count(), 5);
        assert_eq!(
            g.histogram_json(),
            "[{\"width_pts\":0.12,\"segments\":1},{\"width_pts\":3.6,\"segments\":4}]"
        );
        // Midpoint of thinnest (0.12) and modal (3.6).
        assert!((g.default_min_width() - 1.86).abs() < 1e-12);
    }

    #[test]
    fn page_geom_detect_vector_recovers_synthetic_room() {
        let g = PageGeom::from_flat(&room_flat()).unwrap();
        // 200×200 px canvas at 10 ppf; seed off the (filtered-out) hairline.
        let out = g
            .detect_vector(200, 200, 100, 75, 4.0, 10.0, 1.86, 3.5)
            .unwrap();
        // Interior 9.8×9.8 ft (centerline strokes inset one pixel).
        assert!((out.area_sf - 96.04).abs() <= 3.0, "area {}", out.area_sf);

        // Unfiltered, the hairline partitions the room: fragment.
        let frag = g
            .detect_vector(200, 200, 100, 75, 4.0, 10.0, 0.0, 3.5)
            .unwrap();
        assert!(
            frag.area_sf < 0.6 * out.area_sf,
            "expected fragment, got {}",
            frag.area_sf
        );
    }

    #[test]
    fn page_geom_passing_ids_inclusive_boundary() {
        let g = PageGeom::from_flat(&room_flat()).unwrap();
        assert_eq!(g.passing_ids(0.0).len(), 5);
        assert_eq!(g.passing_ids(0.12).len(), 5); // inclusive
        assert_eq!(g.passing_ids(1.86), vec![0, 1, 2, 3]);
        assert!(g.passing_ids(99.0).is_empty());
    }

    #[test]
    fn page_geom_snap_json_endpoint_and_none() {
        let g = PageGeom::from_flat(&room_flat()).unwrap();
        // Near the (90, 90) corner → endpoint on some wall segment.
        let json = g.snap_json(91.0, 89.0, 5.0).unwrap();
        assert!(json.contains("\"kind\":\"endpoint\""));
        assert!(json.contains("\"x\":90") && json.contains("\"y\":90"));
        // Intersection of hairline and right wall at (270, 180): cursor
        // near it but away from endpoints.
        let json = g.snap_json(268.0, 179.0, 4.0).unwrap();
        assert!(json.contains("\"kind\":\"intersection\""), "{json}");
        assert!(json.contains("\"other\":"), "{json}");
        // Far from everything → None; empty index → None.
        assert!(g.snap_json(500.0, 500.0, 5.0).is_none());
        let empty = PageGeom::from_flat(&[]).unwrap();
        assert!(empty.snap_json(90.0, 90.0, 5.0).is_none());
    }
}
