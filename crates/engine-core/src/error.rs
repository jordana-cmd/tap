/// Errors constructing a [`crate::Scale`].
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ScaleError {
    #[error("scale fpi must be finite and > 0, got {0}")]
    InvalidFpi(f64),
    #[error("calibration length must be finite and > 0 feet, got {0}")]
    InvalidKnownLength(f64),
    #[error("calibration span is degenerate (points closer than 1 pt)")]
    DegenerateSpan,
}
