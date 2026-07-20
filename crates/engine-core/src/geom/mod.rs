//! Pure geometry over page-local PDF-point coordinates.

mod area;
mod contains;
mod point;
mod polyline;
mod simplify;

pub use area::{polygon_area, polygon_perimeter};
pub use contains::{point_in_polygon, polygon_contains_polygon};
pub use point::Point;
pub use polyline::{distance, polyline_length};
pub use simplify::simplify;
