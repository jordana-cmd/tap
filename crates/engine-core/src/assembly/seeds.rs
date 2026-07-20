//! Seed assemblies for real trades — shipped builders that double as
//! worked examples and test fixtures. Defaults are typical, editable
//! values (this is model + math; the authoring UI comes later).

use super::{
    expr, Assembly, AssemblyError, BillOfMaterials, Category, ConsumableProfile, LineItem,
    MeasureKind, MeasurementInput, Parameter, Part, Rounding, Scope, Unit,
};
use std::collections::{BTreeMap, BTreeSet};

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
        unit_cost: 0.0, // synthetic example: unpriced
        manual: false,
        supplier: None,
        sku: None,
        category: Category::Material,
        scope: Scope::PerApplication,
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
        consumable_profile_id: None, // synthetic example — no consumable profile
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
        consumable_profile_id: None, // synthetic example — no consumable profile
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

/// A priced MATERIAL part with no waste and no rounding — the MCFC product
/// convention (material total is fractional quantity × cost, to the cent).
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
        category: Category::Material,
        scope: Scope::PerApplication,
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

/// An Area assembly whose parts are the named catalog products, tied to a
/// consumable profile (None = adds no consumables — an additive mixed into a
/// coat already billed).
fn product_system(id: &str, name: &str, product_ids: &[&str], profile: Option<&str>) -> Assembly {
    Assembly {
        id: id.to_string(),
        name: name.to_string(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![],
        parts: product_ids.iter().map(|pid| product_part(pid)).collect(),
        consumable_profile_id: profile.map(str::to_string),
    }
}

/// The seven MCFC systems. Coating systems (epoxy/flake/quartz) carry the
/// coating profile; grinding systems (polish/grind & seal) the grinding profile.
pub fn mcfc_systems() -> Vec<Assembly> {
    let coat = Some("coating");
    let grind = Some("grinding");
    vec![
        product_system("epoxy2", "Epoxy 2-Coat", &["haze_gray", "clear_epoxy"], coat),
        product_system("epoxy_hw", "Epoxy + High Wear Urethane", &["haze_gray", "clear_epoxy", "hw_urethane"], coat),
        product_system("flake", "Polyurea Flake", &["base_a", "base_b", "flake_thrown", "flake_recovered", "clear_a", "clear_b"], coat),
        product_system("flake_db", "Flake Double Broadcast", &["base_a", "base_b", "flake_thrown_db", "flake_rec_db", "clear_a", "clear_b", "top2_a", "top2_b"], coat),
        product_system("polish", "Polished Concrete", &["simihard"], grind),
        product_system("grindseal", "Grind & Seal", &["cs_first", "cs_second"], grind),
        product_system("quartz_db", "Quartz Double Broadcast", &["quartz", "haze_gray", "clear_epoxy", "hw_urethane"], coat),
    ]
}

/// The MCFC add-ons, each classified by how it physically consumes:
///   - full-area coats (Moisture Mitigation) → coating profile (a second coat
///     really does use a second set of cups/rollers);
///   - localized repairs (crack repair, crack stitching, joint fill) → repair
///     profile (trowel + small cups + brush + PPE; no rollers, no big mixing);
///   - additives (Fast Cure, Anti-Slip) → NO profile — mixed into / broadcast
///     onto a coat whose consumables are already billed, so zero additional.
pub fn mcfc_addons() -> Vec<Assembly> {
    let mut v = vec![
        product_system("crack_repair", "Crack Repair (Mender + Sand)", &["mender_a", "mender_b", "sand"], Some("repair")),
        product_system("moisture", "Moisture Mitigation (H2 Out)", &["h2_out"], Some("coating")),
        product_system("antislip", "Anti-Slip (Shark Grip)", &["shark_grip"], None),
        product_system("fastcure", "Fast Cure Activator", &["fast_cure"], None),
    ];
    // Crack stitching: per-stitch, MANUAL quantity ($4/stitch) — localized repair.
    let mut stitch = priced("crack_stitch", "Crack Stitching", Unit::Stitch, "", 4.0);
    stitch.manual = true;
    v.push(Assembly {
        id: "stitching".to_string(),
        name: "Crack Stitching".to_string(),
        applies_to: vec![MeasureKind::Area],
        parameters: vec![],
        parts: vec![stitch],
        consumable_profile_id: Some("repair".to_string()),
    });
    // Joint fill: LF-driven caulk — one tube per `lf_per_tube` (24) linear feet.
    // Repair-classed, but consumables are area-driven so a Linear-only job yields
    // none (the caulk consumables are minor and handled with the coat's).
    v.push(Assembly {
        id: "joint_caulk".to_string(),
        name: "Joint Fill (Polyurea Caulk)".to_string(),
        applies_to: vec![MeasureKind::Linear],
        parameters: vec![param("lf_per_tube", 24.0, Unit::LF)],
        parts: vec![priced("caulk", "Polyurea Caulk (Joint Fill)", Unit::Tube, "length_lf / lf_per_tube", 55.0)],
        consumable_profile_id: Some("repair".to_string()),
    });
    v
}

/// (id, name, cost, rate, scope) — the consumable catalog. Scope is load-bearing
/// now that stacking ships: PerApplication scales with each profile-bearing
/// assembly; PerArea is charged once per area (deduped across the stack).
const CONSUMABLES: &[(&str, &str, f64, f64, Scope)] = &[
    ("cup10", "10-Quart Cups", 4.95, 0.003, Scope::PerApplication),
    ("cup5", "5-Quart Cups", 4.7, 0.003, Scope::PerApplication),
    ("cup25", "2.5-Quart Cups", 2.27, 0.006, Scope::PerApplication),
    ("cupq", "Quart Cups", 0.5, 0.01, Scope::PerApplication),
    ("brush", "Brushes", 0.69, 0.0064, Scope::PerApplication),
    ("roller", "Roller Covers", 8.4, 0.003, Scope::PerApplication),
    ("trowel", "Trowels", 32.99, 0.002, Scope::PerApplication),
    ("miniroller", "Mini Roller Covers", 4.33, 0.002, Scope::PerApplication),
    ("whips", "Whips", 5.49, 0.00000005, Scope::PerApplication),
    ("rags", "Rags", 13.82, 0.001, Scope::PerArea),
    ("gloves", "Gloves", 0.178, 0.0192, Scope::PerArea),
    ("trash", "Trash Bags", 0.78, 0.006, Scope::PerArea),
];

/// The consumable catalog as Consumable parts (`area_sf × rate`), scoped.
/// Referenced by [`consumable_profiles`]; never a standalone assembly.
pub fn consumable_catalog() -> Vec<Part> {
    CONSUMABLES
        .iter()
        .map(|&(id, name, cost, rate, scope)| {
            let mut p = priced(id, name, Unit::EA, &format!("area_sf * {rate}"), cost);
            p.category = Category::Consumable;
            p.scope = scope;
            p
        })
        .collect()
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Seeded consumable profiles.
///
/// UNVALIDATED DOMAIN PROPOSAL: the quoting tool applied ALL consumables to
/// EVERY job — it did not split them by system. The coating / grinding / repair
/// split below (and the membership, especially GRINDING) is a proposal that
/// needs review against real jobs. See `docs/consumable-profiles.md` for the
/// readable assignment table.
pub fn consumable_profiles() -> Vec<ConsumableProfile> {
    vec![
        // Coating: the full process — mix, apply, broadcast, clean.
        ConsumableProfile {
            id: "coating".into(),
            name: "Coating".into(),
            part_ids: CONSUMABLES.iter().map(|c| c.0.to_string()).collect(),
        },
        // Grinding: no large mixing cups, no trowel, no whips (UNVALIDATED guess).
        ConsumableProfile {
            id: "grinding".into(),
            name: "Grinding".into(),
            part_ids: ids(&["cupq", "brush", "roller", "miniroller", "rags", "gloves", "trash"]),
        },
        // Repair: small cups + trowel + brush + PPE/cleanup — a localized patch,
        // not a coat (no rollers, no large mixing cups).
        ConsumableProfile {
            id: "repair".into(),
            name: "Repair".into(),
            part_ids: ids(&["trowel", "cupq", "brush", "gloves", "rags", "trash"]),
        },
    ]
}

/// Auto-derived consumables for a STACK. Given each stacked assembly's
/// consumable profile id (None = adds none), aggregate the profiles' consumables
/// with the scope rules. PerApplication is counted once per profile-bearing
/// assembly (two stacked applications use two sets of cups/rollers); PerArea is
/// charged ONCE across the whole stack, deduped by part id (a three-assembly
/// stack does not bill trash bags three times). Consumables are area-driven — a
/// measurement with no `area_sf` (linear/count) yields none. Priced like
/// materials; `materials_total` is the consumables sum.
pub fn stack_consumables(
    profile_ids: &[Option<String>],
    input: &MeasurementInput,
) -> Result<BillOfMaterials, AssemblyError> {
    let empty = BillOfMaterials { line_items: vec![], materials_total: 0.0 };
    let Some(area_sf) = input.area_sf else { return Ok(empty) };

    let profiles = consumable_profiles();
    let catalog = consumable_catalog();
    let mut per_app: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_area: BTreeSet<String> = BTreeSet::new();
    for pid in profile_ids.iter().flatten() {
        let Some(profile) = profiles.iter().find(|p| &p.id == pid) else { continue };
        for part_id in &profile.part_ids {
            let part = catalog
                .iter()
                .find(|p| &p.id == part_id)
                .expect("profile references a catalog consumable");
            match part.scope {
                Scope::PerApplication => *per_app.entry(part_id.clone()).or_insert(0) += 1,
                Scope::PerArea => {
                    per_area.insert(part_id.clone());
                }
            }
        }
    }

    let mut vars: BTreeMap<String, f64> = BTreeMap::new();
    vars.insert("area_sf".to_string(), area_sf);

    let mut line_items = Vec::new();
    let mut materials_total = 0.0;
    for part in &catalog {
        let count = if per_area.contains(&part.id) {
            1
        } else {
            *per_app.get(&part.id).unwrap_or(&0)
        };
        if count == 0 {
            continue;
        }
        let ast = expr::parse(&part.formula).map_err(|source| AssemblyError::Formula {
            part: part.name.clone(),
            source,
        })?;
        let base = ast.eval_num(&vars).map_err(|source| AssemblyError::Formula {
            part: part.name.clone(),
            source,
        })?;
        let qty = base * count as f64;
        let extended_cost = qty * part.unit_cost;
        materials_total += extended_cost;
        let formula_text = if count > 1 {
            format!("{} × {count} applications", part.formula)
        } else {
            part.formula.clone()
        };
        line_items.push(LineItem {
            part_name: part.name.clone(),
            unit: part.unit,
            raw_quantity: qty,
            waste_applied: qty,
            final_quantity: qty,
            formula_text,
            unit_cost: part.unit_cost,
            extended_cost,
        });
    }
    Ok(BillOfMaterials {
        line_items,
        materials_total,
    })
}

/// The full seeded MCFC catalog: systems + add-ons. Consumables are NOT here —
/// they are auto-derived from each assembly's profile, never a standalone,
/// selectable assembly.
pub fn mcfc_catalog() -> Vec<Assembly> {
    let mut v = mcfc_systems();
    v.extend(mcfc_addons());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::{apply, MeasurementInput};

    #[test]
    fn single_system_material_total_matches_quoting_tool_to_the_cent() {
        // Epoxy + High Wear Urethane @ 5,000 SF, no add-ons. A single coating
        // application → each consumable counted once (per-app ×1, per-area ×1),
        // so the total is UNCHANGED from the phase-1 flat rate:
        //   system                          = $4,920.00
        //   consumables (coating, 1 app)    = $868.7193725
        //   grand materials                 = $5,788.72 (to the cent)
        let input = MeasurementInput::area(5000.0, 0.0);

        let sys = mcfc_systems().into_iter().find(|a| a.id == "epoxy_hw").unwrap();
        let sys_bom = apply(&sys, &input).unwrap();
        assert!((sys_bom.materials_total - 4920.0).abs() < 1e-9, "system {}", sys_bom.materials_total);

        let cons = stack_consumables(&[sys.consumable_profile_id.clone()], &input).unwrap();
        let expected_cons = 74.25 + 70.5 + 68.1 + 25.0 + 22.08 + 126.0 + 329.9
            + 43.3 + 69.1 + 17.088 + 0.0013725 + 23.4;
        assert!((cons.materials_total - expected_cons).abs() < 1e-9, "consumables {}", cons.materials_total);

        let grand = sys_bom.materials_total + cons.materials_total;
        assert_eq!((grand * 100.0).round() / 100.0, 5788.72, "grand to the cent");
    }

    #[test]
    fn stacked_job_material_total_under_profiles_to_the_cent() {
        // Epoxy + High Wear Urethane (coating) + Crack Repair (repair) @ 5,000 SF.
        // Consumables are AUTO-DERIVED from each assembly's profile:
        //   materials    epoxy_hw $4,920.00 + crack_repair $407.55 = $5,327.55
        //   consumables  coating (all 12) + repair (trowel/quart cups/brush per-
        //                app; PPE per-area) with PerArea deduped once:
        //                cupq/brush/trowel billed twice, everything else once
        //                                                 = $1,245.6993725
        //   grand materials                               = $6,573.25 (to the cent)
        // NEW vs phase-2's $6,196.27: +$376.98. WHY: the profile model bills
        // per-application consumables once per stacked assembly, so Crack
        // Repair's second application adds its own trowel + quart cups + brush;
        // per-area items (gloves/rags/trash) stay charged once. A single-system
        // job is unchanged (per-app once + per-area once = the old flat rate).
        let input = MeasurementInput::area(5000.0, 0.0);
        let systems = mcfc_systems();
        let addons = mcfc_addons();
        let stack = [
            systems.iter().find(|a| a.id == "epoxy_hw").unwrap().clone(),
            addons.iter().find(|a| a.id == "crack_repair").unwrap().clone(),
        ];

        let mut materials = 0.0;
        for asm in &stack {
            materials += apply(asm, &input).unwrap().materials_total;
        }
        assert!((materials - 5327.55).abs() < 1e-9, "materials {materials}");

        let profile_ids: Vec<Option<String>> =
            stack.iter().map(|a| a.consumable_profile_id.clone()).collect();
        let cons = stack_consumables(&profile_ids, &input).unwrap();
        assert!((cons.materials_total - 1245.6993725).abs() < 1e-9, "consumables {}", cons.materials_total);

        let grand = materials + cons.materials_total;
        assert_eq!((grand * 100.0).round() / 100.0, 6573.25, "stacked grand to the cent");
    }

    #[test]
    fn per_area_consumables_dedupe_across_the_stack() {
        // Three coating applications: PerApplication cups billed three times,
        // PerArea trash bags billed ONCE (not three).
        let input = MeasurementInput::area(5000.0, 0.0);
        let three = [Some("coating".to_string()), Some("coating".to_string()), Some("coating".to_string())];
        let bom = stack_consumables(&three, &input).unwrap();
        let ext = |name: &str| bom.line_items.iter().find(|l| l.part_name == name).unwrap().extended_cost;
        assert!((ext("Trash Bags") - 0.006 * 5000.0 * 0.78).abs() < 1e-9, "trash once");
        assert!((ext("Gloves") - 0.0192 * 5000.0 * 0.178).abs() < 1e-9, "gloves once");
        assert!((ext("10-Quart Cups") - 0.003 * 5000.0 * 4.95 * 3.0).abs() < 1e-9, "cups ×3");
    }

    #[test]
    fn linear_measurement_gets_no_area_consumables() {
        // The joint-caulk add-on is repair-classed but Linear — no area_sf, so no
        // consumables (they can't be area-driven).
        let bom = stack_consumables(&[Some("repair".to_string())], &MeasurementInput::linear(100.0)).unwrap();
        assert!(bom.line_items.is_empty() && bom.materials_total == 0.0);
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
    fn catalog_has_no_job_consumables_and_profiles_are_assigned() {
        let cat = mcfc_catalog();
        assert_eq!(cat.len(), 7 + 6, "7 systems + 6 add-ons (no standalone consumables)");
        assert!(!cat.iter().any(|a| a.id == "job_consumables"), "Job Consumables removed");
        let profile = |id: &str| cat.iter().find(|a| a.id == id).unwrap().consumable_profile_id.as_deref();
        assert_eq!(profile("epoxy_hw"), Some("coating"));
        assert_eq!(profile("polish"), Some("grinding"));
        assert_eq!(profile("crack_repair"), Some("repair"));
        assert_eq!(profile("moisture"), Some("coating"));
        assert_eq!(profile("antislip"), None, "additive → no consumables");
        assert_eq!(profile("fastcure"), None, "additive → no consumables");
    }

    #[test]
    fn profile_membership_matches_the_proposal() {
        let by = |id: &str| consumable_profiles().into_iter().find(|p| p.id == id).unwrap().part_ids;
        assert_eq!(by("coating").len(), 12, "coating = all consumables");
        let grinding = by("grinding");
        for excluded in ["cup10", "cup5", "cup25", "trowel", "whips"] {
            assert!(!grinding.contains(&excluded.to_string()), "grinding excludes {excluded}");
        }
        assert!(by("repair").contains(&"trowel".to_string()) && by("repair").contains(&"cupq".to_string()));
    }
}
