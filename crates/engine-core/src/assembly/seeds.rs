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
        unit_cost: 0.0, // priced catalog lands in the next commit
        manual: false,
        supplier: None,
        sku: None,
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
        // Parameters drive FORMULAS; per-part waste is a fixed field on the
        // part (overridable per application via `waste:<part_id>` at the
        // apply boundary), so it is not repeated as a parameter here.
        parameters: vec![
            param("box_coverage_sf", 20.0, Unit::SF),
            param("adhesive_coverage_sf_per_gal", 150.0, Unit::SF),
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
        // Per-part waste (5%) is a fixed field on each part, overridable per
        // application via `waste:<part_id>`; not repeated as a parameter.
        parameters: vec![
            param("coats", 2.0, Unit::EA),
            param("coverage_sf_per_gal", 250.0, Unit::SF),
            param("primer_coverage_sf_per_gal", 300.0, Unit::SF),
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

// -------------------------------------------------------------------------
// MCFC catalog (Pricing Model 6.0, ported from reference/mcfc-quoting-tool.jsx)
//
// Product = Part: its `rate` is the per-SF formula (quantity = area_sf × rate);
// `unit_cost` is the product cost, which MAY BE NEGATIVE (the recovered-flake
// credit). No waste and no rounding — the material total is fractional
// quantity × cost, to the cent, matching the quoting tool. SYSTEMS and ADDONS
// become seeded assemblies; DEFAULT_CONSUMABLES become one standing "Job
// Consumables" assembly (per-SF parts that apply to every job).
// -------------------------------------------------------------------------

/// A priced part with no waste and no rounding — the MCFC product convention.
fn priced(id: &str, name: &str, unit: Unit, formula: &str, cost: f64) -> Part {
    Part {
        id: id.to_string(),
        name: name.to_string(),
        unit,
        formula: formula.to_string(),
        waste_pct: None,
        rounding: Rounding::None,
        unit_cost: cost,
        manual: false,
        supplier: None,
        sku: None,
    }
}

/// (id, name, unit, cost, rate) — the area-driven MCFC products. The manual
/// crack-stitch and the LF-driven caulk are built inside their add-ons.
const PRODUCTS: &[(&str, &str, Unit, f64, f64)] = &[
    ("mender_a", "Mender – Part A", Unit::L, 10.17, 0.004),
    ("mender_b", "Mender – Part B", Unit::L, 10.17, 0.004),
    ("sand", "Sand", Unit::Cup, 0.05, 0.003),
    ("shark_grip", "Shark Grip (Anti-Slip)", Unit::Cap, 0.05, 0.08),
    ("base_a", "Polyurea Base Paint A", Unit::L, 10.22, 0.01),
    ("base_b", "Polyurea Base Paint B", Unit::L, 10.22, 0.005),
    ("flake_thrown", "Flake Thrown", Unit::BOX, 74.0, 0.0075),
    ("flake_recovered", "Flake Recovered (credit)", Unit::BOX, -74.0, 0.00425),
    ("clear_a", "Polyaspartic Clear A", Unit::L, 15.85, 0.02),
    ("clear_b", "Polyaspartic Clear B", Unit::L, 15.85, 0.02),
    ("top2_a", "2nd Top Coat A", Unit::L, 15.85, 0.022),
    ("top2_b", "2nd Top Coat B", Unit::L, 15.85, 0.022),
    ("h2_out", "H2 Out (Moisture)", Unit::L, 19.82, 0.003),
    ("flake_thrown_db", "Flake Thrown – Double BC", Unit::BOX, 74.0, 0.01),
    ("flake_rec_db", "Flake Recovered – Double BC", Unit::BOX, -74.0, 0.005),
    ("haze_gray", "Haze Gray Epoxy", Unit::GAL, 44.0, 0.004),
    ("clear_epoxy", "Clear Epoxy", Unit::GAL, 42.4, 0.004),
    ("fast_cure", "Fast Cure Activator", Unit::GAL, 30.0, 0.004),
    ("hw_urethane", "High Wear Urethane", Unit::GAL, 159.6, 0.004),
    ("simihard", "Simihard (Polish Densifier)", Unit::GAL, 29.88, 0.001),
    ("cs_first", "Cure & Seal – First Coat", Unit::GAL, 27.02, 0.001),
    ("cs_second", "Cure & Seal – Second Coat", Unit::GAL, 27.02, 0.0008),
    ("quartz", "Quartz (Double Broadcast)", Unit::Lb, 1.0, 1.0),
];

fn product_part(id: &str) -> Part {
    let &(_, name, unit, cost, rate) = PRODUCTS
        .iter()
        .find(|p| p.0 == id)
        .unwrap_or_else(|| panic!("unknown MCFC product `{id}`"));
    priced(id, name, unit, &format!("area_sf * {rate}"), cost)
}

/// An Area assembly whose parts are the named catalog products.
fn product_system(id: &str, name: &str, product_ids: &[&str]) -> Assembly {
    Assembly {
        id: id.to_string(),
        name: name.to_string(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![],
        parts: product_ids.iter().map(|pid| product_part(pid)).collect(),
    }
}

/// The seven MCFC systems (SYSTEMS in the quoting tool).
pub fn mcfc_systems() -> Vec<Assembly> {
    vec![
        product_system("epoxy2", "Epoxy 2-Coat", &["haze_gray", "clear_epoxy"]),
        product_system("epoxy_hw", "Epoxy + High Wear Urethane", &["haze_gray", "clear_epoxy", "hw_urethane"]),
        product_system("flake", "Polyurea Flake", &["base_a", "base_b", "flake_thrown", "flake_recovered", "clear_a", "clear_b"]),
        product_system("flake_db", "Flake Double Broadcast", &["base_a", "base_b", "flake_thrown_db", "flake_rec_db", "clear_a", "clear_b", "top2_a", "top2_b"]),
        product_system("polish", "Polished Concrete", &["simihard"]),
        product_system("grindseal", "Grind & Seal", &["cs_first", "cs_second"]),
        product_system("quartz_db", "Quartz Double Broadcast", &["quartz", "haze_gray", "clear_epoxy", "hw_urethane"]),
    ]
}

/// The MCFC add-ons (ADDONS). Stackable in phase 2; seeded here as ordinary
/// assemblies. Crack stitching is manual-quantity; joint fill is LF-driven.
pub fn mcfc_addons() -> Vec<Assembly> {
    let mut v = vec![
        product_system("crack_repair", "Crack Repair (Mender + Sand)", &["mender_a", "mender_b", "sand"]),
        product_system("moisture", "Moisture Mitigation (H2 Out)", &["h2_out"]),
        product_system("antislip", "Anti-Slip (Shark Grip)", &["shark_grip"]),
        product_system("fastcure", "Fast Cure Activator", &["fast_cure"]),
    ];
    // Crack stitching: per-stitch, MANUAL quantity ($4/stitch).
    let mut stitch = priced("crack_stitch", "Crack Stitching", Unit::Stitch, "", 4.0);
    stitch.manual = true;
    v.push(Assembly {
        id: "stitching".to_string(),
        name: "Crack Stitching".to_string(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![],
        parts: vec![stitch],
    });
    // Joint fill: LF-driven caulk — one tube per `lf_per_tube` (24) linear feet.
    v.push(Assembly {
        id: "joint_caulk".to_string(),
        name: "Joint Fill (Polyurea Caulk)".to_string(),
        applies_to: vec![MeasureKind::Linear],
        parameters: vec![param("lf_per_tube", 24.0, Unit::LF)],
        parts: vec![priced("caulk", "Polyurea Caulk (Joint Fill)", Unit::Tube, "length_lf / lf_per_tube", 55.0)],
    });
    v
}

/// (id, name, cost, rate) — DEFAULT_CONSUMABLES, per-SF costs on every job.
const CONSUMABLES: &[(&str, &str, f64, f64)] = &[
    ("cup10", "10-Quart Cups", 4.95, 0.003),
    ("cup5", "5-Quart Cups", 4.7, 0.003),
    ("cup25", "2.5-Quart Cups", 2.27, 0.006),
    ("cupq", "Quart Cups", 0.5, 0.01),
    ("brush", "Brushes", 0.69, 0.0064),
    ("roller", "Roller Covers", 8.4, 0.003),
    ("trowel", "Trowels", 32.99, 0.002),
    ("miniroller", "Mini Roller Covers", 4.33, 0.002),
    ("rags", "Rags", 13.82, 0.001),
    ("gloves", "Gloves", 0.178, 0.0192),
    ("whips", "Whips", 5.49, 0.00000005),
    ("trash", "Trash Bags", 0.78, 0.006),
];

/// Standing "Job Consumables" assembly — per-SF consumable costs that apply to
/// every job. Recommended phase-1 model: it reuses the Part/BOM/cost machinery,
/// per-SF costs sum correctly across measurements, and it is line-auditable.
/// (May graduate to a project-level line with the phase-4 cost stack.)
pub fn job_consumables() -> Assembly {
    Assembly {
        id: "job_consumables".to_string(),
        name: "Job Consumables".to_string(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![],
        parts: CONSUMABLES
            .iter()
            .map(|&(id, name, cost, rate)| priced(id, name, Unit::EA, &format!("area_sf * {rate}"), cost))
            .collect(),
    }
}

/// The full seeded MCFC catalog: systems + add-ons + job consumables.
pub fn mcfc_catalog() -> Vec<Assembly> {
    let mut v = mcfc_systems();
    v.extend(mcfc_addons());
    v.push(job_consumables());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::{apply, MeasurementInput};

    #[test]
    fn epoxy_hw_material_total_matches_quoting_tool_to_the_cent() {
        // Reproduces the MCFC "Epoxy + High Wear Urethane" quote @ 5,000 SF with
        // no add-ons. Hand math from the quoting-tool constants:
        //   haze_gray   0.004·5000·$44.00  = $880.00
        //   clear_epoxy 0.004·5000·$42.40  = $848.00
        //   hw_urethane 0.004·5000·$159.60 = $3,192.00   → system $4,920.00
        //   consumables Σ(rate·5000·cost)  = $868.7193725
        //   grand materials                = $5,788.72 (to the cent)
        // This is the permanent guard against silent pricing drift.
        let input = MeasurementInput::area(5000.0, 0.0);

        let sys = mcfc_systems().into_iter().find(|a| a.id == "epoxy_hw").unwrap();
        let sys_bom = apply(&sys, &input).unwrap();
        assert!(
            (sys_bom.materials_total - 4920.0).abs() < 1e-9,
            "system materials {} != 4920.00",
            sys_bom.materials_total
        );

        let cons_bom = apply(&job_consumables(), &input).unwrap();
        let expected_cons = 74.25 + 70.5 + 68.1 + 25.0 + 22.08 + 126.0 + 329.9
            + 43.3 + 69.1 + 17.088 + 0.0013725 + 23.4;
        assert!(
            (cons_bom.materials_total - expected_cons).abs() < 1e-9,
            "consumables materials {} != {}",
            cons_bom.materials_total,
            expected_cons
        );

        let grand = sys_bom.materials_total + cons_bom.materials_total;
        assert_eq!((grand * 100.0).round() / 100.0, 5788.72, "grand materials to the cent");
    }

    #[test]
    fn stacked_job_material_total_matches_quoting_tool_to_the_cent() {
        // Phase 2 stacking: a job is a base system PLUS add-ons PLUS consumables,
        // combined into one materials total. Reproduces Epoxy + High Wear
        // Urethane + Crack Repair + Job Consumables @ 5,000 SF:
        //   system epoxy_hw                              = $4,920.00 (see above)
        //   crack_repair: mender_a 20·$10.17 = $203.40
        //                 mender_b 20·$10.17 = $203.40
        //                 sand     15·$0.05  = $0.75     → $407.55
        //   consumables                                  = $868.7193725
        //   grand materials                              = $6,196.27 (to the cent)
        // The permanent drift guard for STACKED jobs.
        let input = MeasurementInput::area(5000.0, 0.0);
        let systems = mcfc_systems();
        let addons = mcfc_addons();
        let stack = [
            systems.iter().find(|a| a.id == "epoxy_hw").unwrap().clone(),
            addons.iter().find(|a| a.id == "crack_repair").unwrap().clone(),
            job_consumables(),
        ];

        // Combine like the harness does: sum each assembly's materials_total.
        let mut grand = 0.0;
        for asm in &stack {
            grand += apply(asm, &input).unwrap().materials_total;
        }

        // Sanity on the crack-repair leg on its own.
        let cr = apply(&stack[1], &input).unwrap().materials_total;
        assert!((cr - 407.55).abs() < 1e-9, "crack_repair {cr} != 407.55");

        assert_eq!((grand * 100.0).round() / 100.0, 6196.27, "stacked grand materials to the cent");
    }

    #[test]
    fn flake_system_carries_the_negative_recovered_credit() {
        let bom = apply(
            &mcfc_systems().into_iter().find(|a| a.id == "flake").unwrap(),
            &MeasurementInput::area(1000.0, 0.0),
        )
        .unwrap();
        let credit = bom
            .line_items
            .iter()
            .find(|l| l.part_name.contains("Recovered"))
            .expect("flake system has a recovered-flake credit line");
        assert!(credit.extended_cost < 0.0, "recovered flake is a credit (negative)");
    }

    #[test]
    fn catalog_covers_every_system_and_addon() {
        let cat = mcfc_catalog();
        assert_eq!(cat.len(), 7 + 6 + 1, "7 systems + 6 add-ons + job consumables");
        assert!(cat.iter().any(|a| a.id == "job_consumables"));
        assert!(cat.iter().any(|a| a.id == "joint_caulk" && a.applies_to == vec![MeasureKind::Linear]));
    }
}
