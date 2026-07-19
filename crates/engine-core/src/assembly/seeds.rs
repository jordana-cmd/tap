//! Seed assemblies for real trades — shipped builders that double as
//! worked examples and test fixtures. Defaults are typical, editable
//! values (this is model + math; the authoring UI comes later).

use super::{Assembly, MeasureKind, Parameter, Part, Rounding, Unit};

fn param(name: &str, default: f64, unit: Unit) -> Parameter {
    Parameter { name: name.to_string(), default: Some(default), unit }
}

fn part(id: &str, name: &str, unit: Unit, formula: &str, waste: Option<f64>, r: Rounding) -> Part {
    Part {
        id: id.to_string(),
        name: name.to_string(),
        unit,
        formula: formula.to_string(),
        waste_pct: waste,
        rounding: r,
    }
}

/// Commercial flooring: material SF with waste, adhesive by coverage rate,
/// cove base from perimeter, transition strips, and labor hours per SF.
/// Applies to an AREA measurement (uses `area_sf` and `perimeter_lf`).
pub fn commercial_flooring() -> Assembly {
    Assembly {
        id: "commercial_flooring".into(),
        name: "Commercial Flooring".into(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![
            param("waste_pct", 10.0, Unit::EA),
            param("box_coverage_sf", 20.0, Unit::SF),
            param("adhesive_coverage_sf_per_gal", 150.0, Unit::SF),
            param("cove_base_waste_pct", 5.0, Unit::EA),
            param("transition_lf", 0.0, Unit::LF), // set per job
            param("labor_sf_per_hour", 200.0, Unit::SF),
        ],
        parts: vec![
            part("material", "Flooring material", Unit::SF, "area_sf", Some(10.0), Rounding::None),
            part("boxes", "Flooring boxes", Unit::BOX,
                 "area_sf / box_coverage_sf", Some(10.0), Rounding::Ceil),
            part("adhesive", "Adhesive", Unit::GAL,
                 "area_sf / adhesive_coverage_sf_per_gal", None, Rounding::Ceil),
            part("cove_base", "Cove base", Unit::LF,
                 "perimeter_lf", Some(5.0), Rounding::None),
            part("transitions", "Transition strips", Unit::LF,
                 "transition_lf", None, Rounding::None),
            part("labor", "Labor", Unit::HR,
                 "area_sf / labor_sf_per_hour", None, Rounding::None),
        ],
    }
}

/// Epoxy coating: primer gallons by coverage, epoxy gallons by coverage ×
/// number of coats, and labor hours. Applies to an AREA measurement.
pub fn epoxy_coating() -> Assembly {
    Assembly {
        id: "epoxy_coating".into(),
        name: "Epoxy Coating".into(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![
            param("coats", 2.0, Unit::EA),
            param("coverage_sf_per_gal", 250.0, Unit::SF),
            param("primer_coverage_sf_per_gal", 300.0, Unit::SF),
            param("waste_pct", 5.0, Unit::EA),
            param("labor_sf_per_hour", 150.0, Unit::SF),
        ],
        parts: vec![
            part("primer", "Primer", Unit::GAL,
                 "area_sf / primer_coverage_sf_per_gal", Some(5.0), Rounding::Ceil),
            part("epoxy", "Epoxy", Unit::GAL,
                 "area_sf * coats / coverage_sf_per_gal", Some(5.0), Rounding::Ceil),
            part("labor", "Labor", Unit::HR,
                 "area_sf * coats / labor_sf_per_hour", None, Rounding::None),
        ],
    }
}
