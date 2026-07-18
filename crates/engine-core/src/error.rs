/// Errors constructing a [`crate::Scale`].
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ScaleError {
    #[error("scale fpi must be finite and > 0, got {0}")]
    InvalidFpi(f64),
}
