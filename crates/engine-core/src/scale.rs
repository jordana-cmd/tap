use crate::error::ScaleError;
use crate::geom::{distance, Point};

/// PDF points per paper inch.
pub const POINTS_PER_INCH: f64 = 72.0;
/// PDF points² per paper inch² (72²).
pub const POINTS_SQ_PER_SQ_INCH: f64 = 5184.0;

/// Canonical page scale: real-world **feet per paper inch** (`fpi`).
///
/// This is the ONLY stored scale representation. Named scales
/// ("1/4\" = 1'-0\"") are display labels mapped via [`SCALE_PRESETS`];
/// free-text scale parsing is out of scope by decision.
///
/// Contract (binding):
/// - `real_feet = (pdf_points / 72.0) × fpi`
/// - `square_feet = (pdf_points² / 5184.0) × fpi²`
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Scale {
    fpi: f64,
}

impl Scale {
    /// Errors unless `fpi` is finite and > 0.
    pub fn from_fpi(fpi: f64) -> Result<Scale, ScaleError> {
        if fpi.is_finite() && fpi > 0.0 {
            Ok(Scale { fpi })
        } else {
            Err(ScaleError::InvalidFpi(fpi))
        }
    }

    pub fn fpi(self) -> f64 {
        self.fpi
    }

    /// `real_feet = (pdf_points / 72.0) × fpi`
    pub fn points_to_feet(self, pdf_points: f64) -> f64 {
        (pdf_points / POINTS_PER_INCH) * self.fpi
    }

    /// `square_feet = (pdf_points² / 5184.0) × fpi²`
    pub fn points_sq_to_square_feet(self, pdf_points_sq: f64) -> f64 {
        (pdf_points_sq / POINTS_SQ_PER_SQ_INCH) * self.fpi * self.fpi
    }

    /// Inverse of [`Scale::points_to_feet`] (tolerances, hit radii).
    pub fn feet_to_points(self, feet: f64) -> f64 {
        feet / self.fpi * POINTS_PER_INCH
    }
}

impl TryFrom<f64> for Scale {
    type Error = ScaleError;
    fn try_from(fpi: f64) -> Result<Scale, ScaleError> {
        Scale::from_fpi(fpi)
    }
}

impl From<Scale> for f64 {
    fn from(scale: Scale) -> f64 {
        scale.fpi
    }
}

/// Two-point calibration (addendum §A2 `source: 'calibrated'`): the span
/// |a−b| in base units covers `known_feet` real feet, so
/// `fpi = known_feet × 72 / distance(a, b)`.
///
/// Errors: [`ScaleError::InvalidKnownLength`] for non-finite or ≤ 0 feet;
/// [`ScaleError::DegenerateSpan`] when the points are closer than 1 pt
/// (a sub-point span amplifies click error beyond usefulness).
pub fn calibrate_two_point(a: Point, b: Point, known_feet: f64) -> Result<Scale, ScaleError> {
    if !(known_feet.is_finite() && known_feet > 0.0) {
        return Err(ScaleError::InvalidKnownLength(known_feet));
    }
    let span = distance(a, b);
    if !span.is_finite() || span < 1.0 {
        return Err(ScaleError::DegenerateSpan);
    }
    Scale::from_fpi(known_feet * POINTS_PER_INCH / span)
}

/// A named scale as a display label over its canonical `fpi` value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalePreset {
    pub label: &'static str,
    pub fpi: f64,
}

/// Display-label presets. Architectural "N\" = 1'-0\"" means N paper inches
/// per real foot → `fpi = 1 / N` (`1/4" = 1'-0"` → 4.0 feet per paper inch;
/// `3" = 1'-0"` → 1/3). Engineer "1\" = N'" → `fpi = N`.
pub const SCALE_PRESETS: &[ScalePreset] = &[
    // Architectural
    ScalePreset {
        label: "1/16\" = 1'-0\"",
        fpi: 16.0,
    },
    ScalePreset {
        label: "3/32\" = 1'-0\"",
        fpi: 32.0 / 3.0,
    },
    ScalePreset {
        label: "1/8\" = 1'-0\"",
        fpi: 8.0,
    },
    ScalePreset {
        label: "3/16\" = 1'-0\"",
        fpi: 16.0 / 3.0,
    },
    ScalePreset {
        label: "1/4\" = 1'-0\"",
        fpi: 4.0,
    },
    ScalePreset {
        label: "3/8\" = 1'-0\"",
        fpi: 8.0 / 3.0,
    },
    ScalePreset {
        label: "1/2\" = 1'-0\"",
        fpi: 2.0,
    },
    ScalePreset {
        label: "3/4\" = 1'-0\"",
        fpi: 4.0 / 3.0,
    },
    ScalePreset {
        label: "1\" = 1'-0\"",
        fpi: 1.0,
    },
    ScalePreset {
        label: "1 1/2\" = 1'-0\"",
        fpi: 2.0 / 3.0,
    },
    ScalePreset {
        label: "3\" = 1'-0\"",
        fpi: 1.0 / 3.0,
    },
    // Engineer
    ScalePreset {
        label: "1\" = 10'",
        fpi: 10.0,
    },
    ScalePreset {
        label: "1\" = 20'",
        fpi: 20.0,
    },
    ScalePreset {
        label: "1\" = 30'",
        fpi: 30.0,
    },
    ScalePreset {
        label: "1\" = 40'",
        fpi: 40.0,
    },
    ScalePreset {
        label: "1\" = 50'",
        fpi: 50.0,
    },
    ScalePreset {
        label: "1\" = 60'",
        fpi: 60.0,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_from_fpi_rejects_invalid() {
        for bad in [0.0, -4.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = Scale::from_fpi(bad).unwrap_err();
            assert!(matches!(err, ScaleError::InvalidFpi(_)));
        }
    }

    #[test]
    fn scale_quarter_inch_one_paper_inch_is_4ft() {
        let s = Scale::from_fpi(4.0).unwrap();
        assert_eq!(s.points_to_feet(72.0), 4.0);
    }

    #[test]
    fn scale_engineer_20_half_inch_is_10ft() {
        let s = Scale::from_fpi(20.0).unwrap();
        assert_eq!(s.points_to_feet(36.0), 10.0);
    }

    #[test]
    fn scale_sf_formula_5184pts2_at_fpi4_is_16sf() {
        let s = Scale::from_fpi(4.0).unwrap();
        assert_eq!(s.points_sq_to_square_feet(5184.0), 16.0);
    }

    #[test]
    fn scale_preset_anchors() {
        let quarter = SCALE_PRESETS
            .iter()
            .find(|p| p.label == "1/4\" = 1'-0\"")
            .expect("architectural anchor preset present");
        assert_eq!(quarter.fpi, 4.0);

        let engineer20 = SCALE_PRESETS
            .iter()
            .find(|p| p.label == "1\" = 20'")
            .expect("engineer anchor preset present");
        assert_eq!(engineer20.fpi, 20.0);

        // Every preset must itself be a valid Scale.
        for preset in SCALE_PRESETS {
            assert!(Scale::from_fpi(preset.fpi).is_ok(), "{}", preset.label);
        }
    }

    #[test]
    fn scale_fpi_accessor_roundtrips() {
        assert_eq!(Scale::from_fpi(4.0).unwrap().fpi(), 4.0);
    }

    #[test]
    fn calibrate_72pt_span_4ft_gives_fpi_4() {
        // 72 pts = 1 paper inch covering 4 real feet → 1/4" = 1'-0".
        let s = calibrate_two_point(Point::new(100.0, 100.0), Point::new(172.0, 100.0), 4.0)
            .unwrap();
        assert_eq!(s.fpi(), 4.0);
    }

    #[test]
    fn calibrate_engineer_span() {
        // 36 pts covering 10 ft → fpi 20 (1" = 20').
        let s = calibrate_two_point(Point::new(0.0, 0.0), Point::new(0.0, 36.0), 10.0).unwrap();
        assert_eq!(s.fpi(), 20.0);
    }

    #[test]
    fn calibrate_rejects_bad_feet_and_degenerate_span() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(72.0, 0.0);
        for bad in [0.0, -3.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                calibrate_two_point(a, b, bad),
                Err(ScaleError::InvalidKnownLength(_))
            ));
        }
        assert_eq!(
            calibrate_two_point(a, Point::new(0.5, 0.0), 4.0),
            Err(ScaleError::DegenerateSpan)
        );
        assert_eq!(calibrate_two_point(a, a, 4.0), Err(ScaleError::DegenerateSpan));
    }

    #[test]
    fn scale_f64_conversions_enforce_validation() {
        let s = Scale::try_from(4.0).unwrap();
        assert_eq!(f64::from(s), 4.0);
        assert!(Scale::try_from(-1.0).is_err());
    }
}
