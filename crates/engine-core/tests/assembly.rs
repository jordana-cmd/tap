//! Integration tests for the assembly formula engine: hand-computed bills
//! of materials for the two seed assemblies, plus a linear-scaling
//! property. Hand math lives in the assertions so the seeds double as
//! executable documentation of what each formula produces.

use engine_core::assembly::seeds::{commercial_flooring, epoxy_coating};
use engine_core::{apply, BillOfMaterials, MeasurementInput, Unit};
use proptest::prelude::*;

fn line<'a>(bom: &'a BillOfMaterials, name: &str) -> &'a engine_core::LineItem {
    bom.line_items
        .iter()
        .find(|l| l.part_name == name)
        .unwrap_or_else(|| panic!("no line item `{name}`"))
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "expected {b}, got {a}");
}

#[test]
fn commercial_flooring_bom_2475_sf() {
    // A 2,475 SF room with a 210 LF perimeter.
    let bom = apply(&commercial_flooring(), &MeasurementInput::area(2475.0, 210.0)).unwrap();

    // Flooring material: 2475 SF + 10% waste = 2722.5 SF, no packaging.
    let m = line(&bom, "Flooring material");
    assert_eq!(m.unit, Unit::SF);
    close(m.raw_quantity, 2475.0);
    close(m.waste_applied, 2722.5);
    close(m.final_quantity, 2722.5);
    assert_eq!(m.formula_text, "area_sf");

    // Boxes: 2475/20 = 123.75 → +10% = 136.125 → ceil = 137 boxes.
    let b = line(&bom, "Flooring boxes");
    assert_eq!(b.unit, Unit::BOX);
    close(b.raw_quantity, 123.75);
    close(b.waste_applied, 136.125);
    close(b.final_quantity, 137.0);

    // Adhesive: 2475/150 = 16.5 (no waste) → ceil = 17 gal.
    let a = line(&bom, "Adhesive");
    assert_eq!(a.unit, Unit::GAL);
    close(a.raw_quantity, 16.5);
    close(a.final_quantity, 17.0);

    // Cove base: 210 LF + 5% = 220.5 LF.
    let c = line(&bom, "Cove base");
    assert_eq!(c.unit, Unit::LF);
    close(c.raw_quantity, 210.0);
    close(c.final_quantity, 220.5);

    // Transitions: default 0 LF (set per job).
    let t = line(&bom, "Transition strips");
    close(t.final_quantity, 0.0);

    // Labor: 2475/200 = 12.375 HR.
    let l = line(&bom, "Labor");
    assert_eq!(l.unit, Unit::HR);
    close(l.raw_quantity, 12.375);
    close(l.final_quantity, 12.375);

    // Every line carries its formula text (provenance).
    assert!(bom.line_items.iter().all(|l| !l.formula_text.is_empty()));
}

#[test]
fn epoxy_coating_bom_2475_sf() {
    let bom = apply(&epoxy_coating(), &MeasurementInput::area(2475.0, 0.0)).unwrap();

    // Primer: 2475/300 = 8.25 → +5% = 8.6625 → ceil = 9 gal.
    let p = line(&bom, "Primer");
    assert_eq!(p.unit, Unit::GAL);
    close(p.raw_quantity, 8.25);
    close(p.waste_applied, 8.6625);
    close(p.final_quantity, 9.0);

    // Epoxy: 2475 × 2 / 250 = 19.8 → +5% = 20.79 → ceil = 21 gal.
    let e = line(&bom, "Epoxy");
    assert_eq!(e.unit, Unit::GAL);
    close(e.raw_quantity, 19.8);
    close(e.waste_applied, 20.79);
    close(e.final_quantity, 21.0);

    // Labor: 2475 × 2 / 150 = 33.0 HR.
    let l = line(&bom, "Labor");
    assert_eq!(l.unit, Unit::HR);
    close(l.raw_quantity, 33.0);
    close(l.final_quantity, 33.0);
}

proptest! {
    /// A linear part (`area_sf` with a constant waste factor, no rounding)
    /// scales linearly: doubling the driver doubles both the raw and the
    /// final quantity. Rounding-free so linearity is exact to f64.
    #[test]
    fn material_scales_linearly_with_area(area in 1.0..100_000.0f64) {
        let asm = commercial_flooring();
        let single = apply(&asm, &MeasurementInput::area(area, 100.0)).unwrap();
        let double = apply(&asm, &MeasurementInput::area(2.0 * area, 100.0)).unwrap();
        let m1 = line(&single, "Flooring material");
        let m2 = line(&double, "Flooring material");
        prop_assert!((m2.raw_quantity - 2.0 * m1.raw_quantity).abs() <= 1e-6 * m2.raw_quantity.max(1.0));
        prop_assert!((m2.final_quantity - 2.0 * m1.final_quantity).abs() <= 1e-6 * m2.final_quantity.max(1.0));
    }
}
