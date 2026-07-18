//! Pure geometry over page-local PDF-point coordinates.

mod area;
mod point;
mod polyline;
mod simplify;

pub use area::polygon_area;
pub use point::Point;
pub use polyline::{distance, polyline_length};
pub use simplify::simplify;
