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
    polygon_area, polyline_length, DetectError, Point, RoomDetection, Scale, ScaleError,
};
use wasm_bindgen::prelude::*;

// ---------- error mapping (single source of truth) ----------

#[derive(Debug, PartialEq)]
pub(crate) enum WebError {
    Detect(DetectError),
    Raster(RasterError),
    Scale(ScaleError),
}

impl WebError {
    /// Stable machine-readable codes. Exhaustive matches: adding a variant
    /// upstream breaks compilation here instead of shipping code-less.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            WebError::Detect(e) => match e {
                DetectError::RegionNotEnclosed => "REGION_NOT_ENCLOSED",
                DetectError::SeedOnWall { .. } => "SEED_ON_WALL",
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
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            WebError::Detect(e) => e.to_string(),
            WebError::Raster(e) => e.to_string(),
            WebError::Scale(e) => e.to_string(),
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
}
