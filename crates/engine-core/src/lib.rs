//! Deterministic measurement kernel for the takeoff engine.
//!
//! All geometry is stored in base units = **PDF points (1/72 paper inch),
//! page-local, top-left origin** (addendum §A0 invariant 4). Zoom, DPR, tile
//! resolution, and export resolution are display transforms only and never
//! touch stored data.
//!
//! This crate contains zero browser APIs (final spec §0 Rule 1). All platform
//! I/O goes through the traits in [`traits`], implemented by `engine-web`.

pub mod detect;
pub mod error;
pub mod geom;
pub mod scale;
pub mod snap;
pub mod traits;
pub mod units;

pub use detect::{
    default_min_width, detect_candidates, detect_room, detect_room_from_mask,
    passes_width_filter, rasterize_wall_mask, width_histogram, DetectError, DetectParams,
    GrayRaster, Mask, PixelMap, RoomCandidate, RoomDetection, WidthBucket,
};
pub use error::ScaleError;
pub use geom::{distance, polygon_area, polygon_perimeter, polyline_length, simplify, Point};
pub use scale::{
    calibrate_two_point, Scale, ScalePreset, POINTS_PER_INCH, POINTS_SQ_PER_SQ_INCH, SCALE_PRESETS,
};
pub use snap::{CollinearChain, Segment, SegmentId, SegmentIndex, Snap, SnapKind};
pub use units::{format_feet_inches, InchPrecision};
