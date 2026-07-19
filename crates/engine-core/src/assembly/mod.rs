//! Assemblies — the formula engine (addendum: derived quantities).
//!
//! An [`Assembly`] is a reusable recipe: parameters plus formula-driven
//! [`Part`]s. [`apply`]ing it to a measurement yields a [`BillOfMaterials`]
//! whose every line carries the formula that produced it — provenance for
//! derived quantities, not just measured ones. BOMs are DERIVED (invariant
//! 5): recompute from measurement + assembly on read; never persist them
//! as truth — enforced structurally, [`BillOfMaterials`] carries no serde
//! derive.

pub mod expr;

pub use expr::ExprError;

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A unit of measure for a bill-of-materials line item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Unit {
    /// Square feet.
    SF,
    /// Linear feet.
    LF,
    /// Each (a discrete count).
    EA,
    /// Gallons.
    GAL,
    /// Labor hours.
    HR,
    /// Boxes (packaged material).
    BOX,
}

/// Packaging rule applied AFTER waste, to turn a fractional quantity into
/// what you can actually order.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Rounding {
    /// Keep the fractional quantity.
    None,
    /// Round up to the next whole unit (23.4 boxes → 24).
    Ceil,
    /// Round up to the next multiple of `n` (e.g. 8-ft pieces).
    CeilToMultiple(f64),
}

/// What kind of measurement an assembly can be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MeasureKind {
    Area,
    Linear,
    Count,
}

/// A named assembly parameter with an optional default. A parameter with
/// no default is UNBOUND until a caller supplies one — applying then
/// errors rather than guessing (this step uses defaults only).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Parameter {
    pub name: String,
    pub default: Option<f64>,
    pub unit: Unit,
}

/// One material or labor line, driven by a formula over the measurement +
/// parameter variables.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Part {
    pub id: String,
    pub name: String,
    pub unit: Unit,
    /// Expression over `area_sf`, `perimeter_lf`, `length_lf`, `count_ea`,
    /// and the assembly's parameters.
    pub formula: String,
    /// Waste percentage (e.g. `10.0` = +10%), applied to the raw formula
    /// result before rounding. `None` = no waste.
    pub waste_pct: Option<f64>,
    /// Packaging rule applied after waste.
    pub rounding: Rounding,
}

/// A reusable recipe: parameters + formula-driven parts, applicable to
/// measurements of the listed kinds.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Assembly {
    pub id: String,
    pub name: String,
    pub applies_to: Vec<MeasureKind>,
    pub parameters: Vec<Parameter>,
    pub parts: Vec<Part>,
}

/// The driving quantities from a measurement. Only the fields relevant to
/// the measurement's kind are `Some`; a formula referencing an absent one
/// gets a typed `UnknownVariable` (guarded up front by `applies_to`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeasurementInput {
    pub kind: MeasureKind,
    pub area_sf: Option<f64>,
    pub perimeter_lf: Option<f64>,
    pub length_lf: Option<f64>,
    pub count_ea: Option<f64>,
}

impl Default for MeasureKind {
    fn default() -> Self {
        MeasureKind::Area
    }
}

impl MeasurementInput {
    /// An area measurement carries both its area and its perimeter.
    pub fn area(area_sf: f64, perimeter_lf: f64) -> MeasurementInput {
        MeasurementInput {
            kind: MeasureKind::Area,
            area_sf: Some(area_sf),
            perimeter_lf: Some(perimeter_lf),
            ..Default::default()
        }
    }
    /// A linear measurement carries its length.
    pub fn linear(length_lf: f64) -> MeasurementInput {
        MeasurementInput {
            kind: MeasureKind::Linear,
            length_lf: Some(length_lf),
            ..Default::default()
        }
    }
    /// A count measurement carries its quantity.
    pub fn count(count_ea: f64) -> MeasurementInput {
        MeasurementInput {
            kind: MeasureKind::Count,
            count_ea: Some(count_ea),
            ..Default::default()
        }
    }
}

/// One computed line in a bill of materials. Retains the raw formula
/// result, the post-waste quantity, and the packaged final — plus the
/// formula text, so the UI can show WHY the number is what it is.
///
/// NOT serde-serializable: a BOM is DERIVED (invariant 5) and recomputed
/// on read, never persisted as truth.
#[derive(Debug, Clone, PartialEq)]
pub struct LineItem {
    pub part_name: String,
    pub unit: Unit,
    /// The formula result, before waste and rounding.
    pub raw_quantity: f64,
    /// After waste, before rounding (`= raw_quantity` when no waste).
    pub waste_applied: f64,
    /// After waste AND packaging — what you order.
    pub final_quantity: f64,
    /// The source formula that produced `raw_quantity` (provenance).
    pub formula_text: String,
}

/// The full derived bill of materials for one applied assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct BillOfMaterials {
    pub line_items: Vec<LineItem>,
}

/// Applying an assembly failed. Every variant names what went wrong so a
/// bad recipe never silently produces a plausible-but-wrong quantity.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AssemblyError {
    #[error("assembly applies to {applies_to:?} but the measurement is {got:?}")]
    WrongKind {
        applies_to: Vec<MeasureKind>,
        got: MeasureKind,
    },
    #[error("parameter `{0}` has no default and was not supplied")]
    UnboundParameter(String),
    #[error("formula error in part `{part}`: {source}")]
    Formula {
        part: String,
        #[source]
        source: ExprError,
    },
}

/// Round-up helper with an epsilon so an EXACT multiple does not over-round
/// (20 SF at 20-SF boxes → 1 box, not 2; 60 at multiple-of-20 → 60).
fn ceil_eps(v: f64) -> f64 {
    if (v - v.round()).abs() < 1e-9 {
        v.round()
    } else {
        v.ceil()
    }
}

/// Apply an assembly to a measurement, producing a [`BillOfMaterials`].
///
/// Per line the order is fixed and explicit:
/// `raw = eval(formula)` → `waste_applied = raw × (1 + waste_pct/100)`
/// → `final_quantity = round(waste_applied)` per the part's [`Rounding`].
/// Waste is applied ONCE per part, to the raw result, before rounding; it
/// does not compound across parts. Formulas parse and evaluate here — the
/// recompute-on-read path (invariant 5); nothing is cached as truth.
pub fn apply(
    assembly: &Assembly,
    input: &MeasurementInput,
) -> Result<BillOfMaterials, AssemblyError> {
    if !assembly.applies_to.contains(&input.kind) {
        return Err(AssemblyError::WrongKind {
            applies_to: assembly.applies_to.clone(),
            got: input.kind,
        });
    }

    // Variable context: present drivers + every parameter (via its default).
    let mut vars: BTreeMap<String, f64> = BTreeMap::new();
    for (name, val) in [
        ("area_sf", input.area_sf),
        ("perimeter_lf", input.perimeter_lf),
        ("length_lf", input.length_lf),
        ("count_ea", input.count_ea),
    ] {
        if let Some(v) = val {
            vars.insert(name.to_string(), v);
        }
    }
    for p in &assembly.parameters {
        let v = p
            .default
            .ok_or_else(|| AssemblyError::UnboundParameter(p.name.clone()))?;
        vars.insert(p.name.clone(), v);
    }

    let mut line_items = Vec::with_capacity(assembly.parts.len());
    for part in &assembly.parts {
        let ast = expr::parse(&part.formula).map_err(|source| AssemblyError::Formula {
            part: part.name.clone(),
            source,
        })?;
        let raw = ast.eval_num(&vars).map_err(|source| AssemblyError::Formula {
            part: part.name.clone(),
            source,
        })?;
        let waste_applied = match part.waste_pct {
            Some(pct) => raw * (1.0 + pct / 100.0),
            None => raw,
        };
        let final_quantity = match part.rounding {
            Rounding::None => waste_applied,
            Rounding::Ceil => ceil_eps(waste_applied),
            Rounding::CeilToMultiple(n) => ceil_eps(waste_applied / n) * n,
        };
        line_items.push(LineItem {
            part_name: part.name.clone(),
            unit: part.unit,
            raw_quantity: raw,
            waste_applied,
            final_quantity,
            formula_text: part.formula.clone(),
        });
    }
    Ok(BillOfMaterials { line_items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, unit: Unit, formula: &str, waste: Option<f64>, r: Rounding) -> Part {
        Part {
            id: name.to_lowercase().replace(' ', "_"),
            name: name.to_string(),
            unit,
            formula: formula.to_string(),
            waste_pct: waste,
            rounding: r,
        }
    }

    #[test]
    fn waste_then_rounding_order() {
        // raw 123.75 → ×1.10 = 136.125 → ceil = 137.
        let a = Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Area],
            parameters: vec![],
            parts: vec![part("Boxes", Unit::BOX, "area_sf / 20", Some(10.0), Rounding::Ceil)],
        };
        let bom = apply(&a, &MeasurementInput::area(2475.0, 0.0)).unwrap();
        let li = &bom.line_items[0];
        assert_eq!(li.raw_quantity, 123.75);
        assert!((li.waste_applied - 136.125).abs() < 1e-9);
        assert_eq!(li.final_quantity, 137.0);
        assert_eq!(li.formula_text, "area_sf / 20");
    }

    #[test]
    fn rounding_boundaries() {
        let mk = |formula: &str, r: Rounding| Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Area],
            parameters: vec![],
            parts: vec![part("P", Unit::BOX, formula, None, r)],
        };
        let final_of = |a: &Assembly, sf: f64| {
            apply(a, &MeasurementInput::area(sf, 0.0)).unwrap().line_items[0].final_quantity
        };
        // Exactly 20 SF at 20-SF boxes → 1 box, not 2.
        let boxes = mk("area_sf / 20", Rounding::Ceil);
        assert_eq!(final_of(&boxes, 20.0), 1.0);
        assert_eq!(final_of(&boxes, 20.0001), 2.0);
        assert_eq!(final_of(&boxes, 40.0), 2.0);
        // CeilToMultiple: an exact multiple stays put.
        let piece = mk("area_sf", Rounding::CeilToMultiple(8.0));
        assert_eq!(final_of(&piece, 16.0), 16.0); // exact multiple
        assert_eq!(final_of(&piece, 17.0), 24.0); // up to next 8
    }

    #[test]
    fn wrong_kind_and_unbound_parameter() {
        let flooring_like = Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Area],
            parameters: vec![],
            parts: vec![part("P", Unit::SF, "area_sf", None, Rounding::None)],
        };
        assert!(matches!(
            apply(&flooring_like, &MeasurementInput::linear(10.0)),
            Err(AssemblyError::WrongKind { .. })
        ));

        let unbound = Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Area],
            parameters: vec![Parameter { name: "k".into(), default: None, unit: Unit::EA }],
            parts: vec![part("P", Unit::SF, "area_sf * k", None, Rounding::None)],
        };
        assert_eq!(
            apply(&unbound, &MeasurementInput::area(10.0, 0.0)),
            Err(AssemblyError::UnboundParameter("k".into()))
        );
    }

    #[test]
    fn formula_error_surfaces_with_part_name() {
        let a = Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Area],
            parameters: vec![],
            parts: vec![part("Bad", Unit::SF, "area_sf / 0", None, Rounding::None)],
        };
        match apply(&a, &MeasurementInput::area(10.0, 0.0)) {
            Err(AssemblyError::Formula { part, source }) => {
                assert_eq!(part, "Bad");
                assert_eq!(source, ExprError::DivideByZero);
            }
            other => panic!("expected Formula error, got {other:?}"),
        }
    }

    #[test]
    fn unknown_variable_when_driver_absent() {
        // A part referencing perimeter_lf on a Count measurement (no perimeter).
        let a = Assembly {
            id: "t".into(),
            name: "t".into(),
            applies_to: vec![MeasureKind::Count],
            parameters: vec![],
            parts: vec![part("P", Unit::LF, "perimeter_lf", None, Rounding::None)],
        };
        match apply(&a, &MeasurementInput::count(5.0)) {
            Err(AssemblyError::Formula { source, .. }) => {
                assert_eq!(source, ExprError::UnknownVariable("perimeter_lf".into()));
            }
            other => panic!("expected UnknownVariable, got {other:?}"),
        }
    }
}
