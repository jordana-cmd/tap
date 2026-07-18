/// Rounding step for feet-inches display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InchPrecision {
    Inch,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

impl InchPrecision {
    /// Denominator of the rounding step, in fractions of an inch.
    pub fn denominator(self) -> u64 {
        match self {
            Self::Inch => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
        }
    }
}

/// Format decimal feet as feet-inches, e.g. `24.5` @ `Inch` → `24'-6"`,
/// `10.375` @ `Sixteenth` → `10'-4 1/2"`.
///
/// Rounds to the nearest step of `precision`; fractions are reduced
/// (8/16 → 1/2) and rounding carries (11.999' @ `Inch` → `12'-0"`).
/// Negative input formats as `-` + the absolute value (measurements are
/// non-negative; defined behavior beats a panic). Non-finite input
/// formats as `0'-0"`.
pub fn format_feet_inches(feet: f64, precision: InchPrecision) -> String {
    let sign = if feet.is_sign_negative() && feet != 0.0 && !feet.is_nan() {
        "-"
    } else {
        ""
    };
    let denom = precision.denominator();
    let units_per_foot = 12 * denom;
    // Saturating cast: NaN → 0, huge → u64::MAX.
    let total_units = (feet.abs() * units_per_foot as f64).round() as u64;

    let ft = total_units / units_per_foot;
    let rem = total_units % units_per_foot;
    let inches = rem / denom;
    let mut num = rem % denom;
    let mut den = denom;
    if num != 0 {
        // denom is a power of two, so halving reduces fully.
        while num % 2 == 0 {
            num /= 2;
            den /= 2;
        }
        format!("{sign}{ft}'-{inches} {num}/{den}\"")
    } else {
        format!("{sign}{ft}'-{inches}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_24_5_ft_inch_precision() {
        assert_eq!(format_feet_inches(24.5, InchPrecision::Inch), "24'-6\"");
    }

    #[test]
    fn format_carry_rounds_up_a_foot() {
        assert_eq!(format_feet_inches(11.9999, InchPrecision::Inch), "12'-0\"");
    }

    #[test]
    fn format_reduced_fraction() {
        // 10' 4.5" — 8/16 reduces to 1/2.
        assert_eq!(
            format_feet_inches(10.375, InchPrecision::Sixteenth),
            "10'-4 1/2\""
        );
        // 0.375" = 6/16, reduces to 3/8.
        assert_eq!(
            format_feet_inches(0.03125, InchPrecision::Sixteenth),
            "0'-0 3/8\""
        );
    }

    #[test]
    fn format_zero() {
        assert_eq!(format_feet_inches(0.0, InchPrecision::Inch), "0'-0\"");
        assert_eq!(format_feet_inches(0.0, InchPrecision::Sixteenth), "0'-0\"");
    }

    #[test]
    fn format_exact_foot_no_fraction() {
        assert_eq!(format_feet_inches(7.0, InchPrecision::Sixteenth), "7'-0\"");
    }

    #[test]
    fn format_negative_prefixes_sign() {
        assert_eq!(format_feet_inches(-24.5, InchPrecision::Inch), "-24'-6\"");
    }

    #[test]
    fn format_non_finite_is_zero() {
        assert_eq!(format_feet_inches(f64::NAN, InchPrecision::Inch), "0'-0\"");
    }

    #[test]
    fn format_all_precisions_round_correctly() {
        // 5.041666… ft = 5' 0.5"
        let half_inch_ft = 5.0 + 0.5 / 12.0;
        assert_eq!(
            format_feet_inches(half_inch_ft, InchPrecision::Inch),
            "5'-1\""
        );
        assert_eq!(
            format_feet_inches(half_inch_ft, InchPrecision::Half),
            "5'-0 1/2\""
        );
        assert_eq!(
            format_feet_inches(half_inch_ft, InchPrecision::Quarter),
            "5'-0 1/2\""
        );
        assert_eq!(
            format_feet_inches(half_inch_ft, InchPrecision::Eighth),
            "5'-0 1/2\""
        );
    }
}
