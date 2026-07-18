use thiserror::Error;

/// Hard cap on raster size (4096×4096) per the final spec's memory-budget
/// thinking. Worst-case transient memory at this cap:
///
/// | buffer                       | type       | size    |
/// |------------------------------|------------|---------|
/// | `GrayRaster`                 | `u8`       | 16 MiB  |
/// | wall mask                    | `Vec<bool>`| 16 MiB  |
/// | dilated mask (+1 pass temp)  | `Vec<bool>`| 32 MiB  |
/// | fill / closed-region mask    | `Vec<bool>`| 16 MiB  |
/// | CCL labels (Level 2 only)    | `u32`      | 64 MiB  |
///
/// Peak ≈ 80 MiB (Level 1) / ≈ 145 MiB (Level 2), all freed on return; a
/// typical Level-1 click region (1200×1200 px at 8–12 px/ft) is ≈ 7 MiB.
/// The final spec budgets 700 MB for the tile cache alone. Bit-packed masks
/// (8× smaller) are a future optimization if wasm-memory telemetry warrants.
pub const MAX_RASTER_PIXELS: usize = 16_777_216;

/// Errors constructing a [`GrayRaster`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RasterError {
    #[error("raster dimensions must be nonzero")]
    Empty,
    #[error("buffer length {len} != width × height = {expected}")]
    BufferMismatch { len: usize, expected: usize },
    #[error("raster {width}×{height} exceeds the {MAX_RASTER_PIXELS}-pixel cap")]
    TooLarge { width: u32, height: u32 },
}

/// Grayscale raster: row-major u8 luminance, pixel (0,0) at the top-left —
/// the same orientation as base-unit space (invariant 4). `engine-web`
/// supplies real rasters; tests generate synthetic ones in code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayRaster {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl GrayRaster {
    /// Validates nonzero dimensions, `data.len() == width × height`, and the
    /// [`MAX_RASTER_PIXELS`] cap.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<GrayRaster, RasterError> {
        if width == 0 || height == 0 {
            return Err(RasterError::Empty);
        }
        let expected = width as usize * height as usize;
        if expected > MAX_RASTER_PIXELS {
            return Err(RasterError::TooLarge { width, height });
        }
        if data.len() != expected {
            return Err(RasterError::BufferMismatch {
                len: data.len(),
                expected,
            });
        }
        Ok(GrayRaster {
            width,
            height,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Panics if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> u8 {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) out of bounds"
        );
        self.data[y as usize * self.width as usize + x as usize]
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

/// Binary mask over the same pixel grid (`true` = wall or filled, per context).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<bool>,
}

impl Mask {
    /// All-false mask.
    pub fn new(width: u32, height: u32) -> Mask {
        Mask {
            width,
            height,
            data: vec![false; width as usize * height as usize],
        }
    }

    pub(crate) fn from_raw(width: u32, height: u32, data: Vec<bool>) -> Mask {
        debug_assert_eq!(data.len(), width as usize * height as usize);
        Mask {
            width,
            height,
            data,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Panics if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> bool {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) out of bounds"
        );
        self.data[y as usize * self.width as usize + x as usize]
    }

    /// Panics if out of bounds.
    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) out of bounds"
        );
        self.data[y as usize * self.width as usize + x as usize] = value;
    }

    /// Number of set pixels.
    pub fn count(&self) -> usize {
        self.data.iter().filter(|&&b| b).count()
    }

    /// Logical NOT of every pixel.
    pub fn invert(&self) -> Mask {
        Mask {
            width: self.width,
            height: self.height,
            data: self.data.iter().map(|&b| !b).collect(),
        }
    }

    pub(crate) fn as_slice(&self) -> &[bool] {
        &self.data
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [bool] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_new_rejects_bad_dims() {
        assert_eq!(GrayRaster::new(0, 5, vec![]), Err(RasterError::Empty));
        assert_eq!(GrayRaster::new(5, 0, vec![]), Err(RasterError::Empty));
        assert_eq!(
            GrayRaster::new(2, 2, vec![0; 3]),
            Err(RasterError::BufferMismatch {
                len: 3,
                expected: 4
            })
        );
        assert_eq!(
            GrayRaster::new(8192, 8192, vec![]),
            Err(RasterError::TooLarge {
                width: 8192,
                height: 8192
            })
        );
    }

    #[test]
    fn raster_accessors_roundtrip() {
        let r = GrayRaster::new(2, 2, vec![10, 20, 30, 40]).unwrap();
        assert_eq!((r.width(), r.height()), (2, 2));
        assert_eq!(r.get(1, 1), 40);
    }

    #[test]
    fn mask_set_get_count_invert() {
        let mut m = Mask::new(3, 2);
        m.set(2, 1, true);
        assert!(m.get(2, 1));
        assert_eq!(m.count(), 1);
        let inv = m.invert();
        assert!(!inv.get(2, 1));
        assert_eq!(inv.count(), 5);
    }
}
