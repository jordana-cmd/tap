//! The project cost stack — cost → price, matched exactly to the MCFC quoting
//! tool (`reference/mcfc-quoting-tool.jsx` `calc`). Pure arithmetic, no
//! measurement concepts and no browser APIs. Applied PER SCOPE (Base Bid and
//! each alternate), so alternates price standalone.
//!
//! Like a [`crate::BillOfMaterials`], a [`PriceBreakdown`] is DERIVED (invariant
//! 5): recompute from the inputs on read, never persist it. Only the inputs are
//! stored.

#[cfg(feature = "serde")]
use serde::Deserialize;

/// How markup turns cost into price — the three modes the quoting tool ships.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub enum PricingMode {
    /// Rob's formula: `price = cost × (1 + profit% + legacy_adder%)`.
    Legacy { profit_pct: f64, legacy_adder: f64 },
    /// Exact margin: `price = cost / (1 − margin%)`.
    Margin { margin_pct: f64 },
    /// Direct: `price = sqft × price_per_sf`, margin computed backward.
    Sqft { price_per_sf: f64 },
}

/// Everything the cost stack needs for one scope. `materials` already includes
/// consumables (the phases 1–3 BOM total). Flat adds (insurance/equipment/
/// permits) are the caller's call to include or zero — the harness puts them on
/// the Base Bid only.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct CostInputs {
    pub materials: f64,
    pub crew: f64,
    pub hours: f64,
    pub wage: f64,
    pub payroll_tax_pct: f64,
    pub overhead_rate: f64, // $ per labor-hour
    pub insurance: f64,
    pub equipment: f64,
    pub permits: f64,
    pub sqft: f64, // for target-$/SF mode and the per-SF outputs
    pub mode: PricingMode,
    pub discount_pct: f64,
    /// Card-processing rate (e.g. 3.5) when the fee is on; `None` when off.
    pub cc_pct: Option<f64>,
}

/// The full derived breakdown. Never persisted (invariant 5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceBreakdown {
    pub materials: f64,
    pub labor: f64,
    pub payroll_tax: f64,
    pub overhead: f64,
    pub insurance: f64,
    pub equipment: f64,
    pub permits: f64,
    pub cost: f64,
    pub markup: f64,
    pub discount: f64,
    pub cc: f64,
    pub price: f64,
    pub profit: f64,
    pub margin_pct: f64,
    pub price_per_sf: f64,
    pub cost_per_sf: f64,
}

/// Compute the cost stack for one scope. Matches the quoting tool's `calc` line
/// for line: cost is the sum of the seven components; markup/discount depend on
/// the mode; the card fee applies to the post-discount subtotal.
pub fn price(inputs: &CostInputs) -> PriceBreakdown {
    let labor = inputs.wage * inputs.crew * inputs.hours;
    let payroll_tax = labor * inputs.payroll_tax_pct / 100.0;
    let overhead = inputs.overhead_rate * inputs.crew * inputs.hours;
    let cost = inputs.materials
        + labor
        + payroll_tax
        + inputs.insurance
        + overhead
        + inputs.equipment
        + inputs.permits;

    let (markup, discount) = match inputs.mode {
        PricingMode::Legacy { profit_pct, legacy_adder } => (
            cost * (profit_pct / 100.0 + legacy_adder / 100.0),
            -cost * inputs.discount_pct / 100.0,
        ),
        PricingMode::Margin { margin_pct } => {
            let pp = margin_pct / 100.0;
            let markup = if pp >= 1.0 { 0.0 } else { cost / (1.0 - pp) - cost };
            (markup, -cost * inputs.discount_pct / 100.0)
        }
        PricingMode::Sqft { price_per_sf } => {
            let target = inputs.sqft * price_per_sf;
            (target - cost, -target * inputs.discount_pct / 100.0)
        }
    };

    let subtotal = cost + markup + discount;
    let cc = inputs.cc_pct.map_or(0.0, |rate| subtotal * rate / 100.0);
    let price = subtotal + cc;
    let profit = price - cost;
    let margin_pct = if price > 0.0 { profit / price * 100.0 } else { 0.0 };

    PriceBreakdown {
        materials: inputs.materials,
        labor,
        payroll_tax,
        overhead,
        insurance: inputs.insurance,
        equipment: inputs.equipment,
        permits: inputs.permits,
        cost,
        markup,
        discount,
        cc,
        price,
        profit,
        margin_pct,
        price_per_sf: if inputs.sqft > 0.0 { price / inputs.sqft } else { 0.0 },
        cost_per_sf: if inputs.sqft > 0.0 { cost / inputs.sqft } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::{apply, seeds, MeasurementInput};

    fn cent(x: f64) -> f64 {
        (x * 100.0).round() / 100.0
    }

    fn base(materials: f64, sqft: f64, mode: PricingMode) -> CostInputs {
        CostInputs {
            materials,
            crew: 0.0,
            hours: 0.0,
            wage: 0.0,
            payroll_tax_pct: 0.0,
            overhead_rate: 0.0,
            insurance: 0.0,
            equipment: 0.0,
            permits: 0.0,
            sqft,
            mode,
            discount_pct: 0.0,
            cc_pct: None,
        }
    }

    #[test]
    fn full_quote_matches_the_tool_to_the_cent() {
        // Complete quote — Epoxy + High Wear Urethane @ 5,000 SF, no add-ons,
        // crew 3 · 24 hrs · $27.50, legacy mode 30% + 12.85%, no
        // insurance/equipment/permits/discount/card. Materials come from the
        // seeded catalog (system + coating consumables), so this guards the
        // WHOLE pipeline — measurement → materials → consumables → price.
        let m = MeasurementInput::area(5000.0, 0.0);
        let sys = seeds::mcfc_systems().into_iter().find(|a| a.id == "epoxy_hw").unwrap();
        let materials = apply(&sys, &m).unwrap().materials_total
            + seeds::stack_consumables(&[sys.consumable_profile_id.clone()], &m).unwrap().materials_total;
        assert_eq!(cent(materials), 5788.72, "materials");

        let b = price(&CostInputs {
            crew: 3.0,
            hours: 24.0,
            wage: 27.5,
            payroll_tax_pct: 7.65,
            overhead_rate: 52.99,
            mode: PricingMode::Legacy { profit_pct: 30.0, legacy_adder: 12.85 },
            ..base(materials, 5000.0, PricingMode::Legacy { profit_pct: 30.0, legacy_adder: 12.85 })
        });

        assert_eq!(cent(b.labor), 1980.00, "labor");
        assert_eq!(cent(b.payroll_tax), 151.47, "payroll tax");
        assert_eq!(cent(b.overhead), 3815.28, "overhead");
        assert_eq!(cent(b.cost), 11735.47, "cost");
        assert_eq!(cent(b.markup), 5028.65, "markup");
        assert_eq!(cent(b.price), 16764.12, "PRICE");
        assert_eq!(cent(b.profit), 5028.65, "profit");
        assert_eq!((b.margin_pct * 10.0).round() / 10.0, 30.0, "margin %");
        assert_eq!(cent(b.price_per_sf), 3.35, "$/SF price");
        assert_eq!(cent(b.cost_per_sf), 2.35, "$/SF cost");
    }

    #[test]
    fn margin_mode_gives_the_exact_margin() {
        // cost 1000, 30% margin → price 1000/0.7, margin exactly 30%.
        let b = price(&base(1000.0, 1000.0, PricingMode::Margin { margin_pct: 30.0 }));
        assert!((b.price - 1000.0 / 0.7).abs() < 1e-9);
        assert!((b.margin_pct - 30.0).abs() < 1e-9);
    }

    #[test]
    fn target_sqft_mode_hits_the_target() {
        // 1000 SF × $4 = $4,000 target; cost 2500 → markup 1500, price 4000.
        let b = price(&base(2500.0, 1000.0, PricingMode::Sqft { price_per_sf: 4.0 }));
        assert!((b.markup - 1500.0).abs() < 1e-9);
        assert!((b.price - 4000.0).abs() < 1e-9);
    }

    #[test]
    fn discount_then_card_fee_apply_after_markup() {
        // legacy cost 1000, no profit/adder → markup 0; 10% discount → −100;
        // subtotal 900; card 3.5% → 31.50; price 931.50.
        let b = price(&CostInputs {
            discount_pct: 10.0,
            cc_pct: Some(3.5),
            ..base(1000.0, 1000.0, PricingMode::Legacy { profit_pct: 0.0, legacy_adder: 0.0 })
        });
        assert!((b.discount + 100.0).abs() < 1e-9, "discount {}", b.discount);
        assert!((b.cc - 31.5).abs() < 1e-9, "cc {}", b.cc);
        assert!((b.price - 931.5).abs() < 1e-9, "price {}", b.price);
    }
}
