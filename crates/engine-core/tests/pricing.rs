//! Rate-card integrity. The shipped card is a DATA FILE, not compiled-in
//! source, so these tests are what stand between a hand edit and a wrong
//! price: they load `data/rate-cards/commercial-v1.json` exactly as the host
//! will and assert every structural property the cost engine will rely on.
//!
//! No cost arithmetic is asserted here — that lives in `pricing_calc.rs`.
//! What IS asserted is the arithmetic-adjacent property that catches the
//! realistic failure mode for reclaim credits: net flake cost must stay
//! positive.

#![cfg(feature = "serde")]

use engine_core::pricing::{
    from_json, validate, AddOn, AddOnEffect, GritLevel, LaborRates, Product, ProductClass,
    ProductOp, ProductUnit, RateBasis, RateCard, RateCardError, SystemRecipe, WageSource,
};
use std::collections::BTreeMap;

/// The shipped card, loaded the way the host loads it.
fn card() -> RateCard {
    let src = include_str!("../../../data/rate-cards/commercial-v1.json");
    from_json(src).expect("shipped rate card must be valid")
}

/// Money tolerance: one cent. The 1e-6 helper used for geometry is far too
/// tight for currency, where rounding ORDER differs between Excel and a Rust
/// port. Rounding here happens once, at the end of a calculation, never
/// per-line.
fn cents(a: f64, b: f64) {
    assert!((a - b).abs() < 0.01, "expected {b}, got {a}");
}

// ---------- the shipped card ----------

#[test]
fn shipped_card_loads_and_validates() {
    let c = card();
    assert_eq!(c.version, "commercial-v1");
    assert_eq!(c.products.len(), 35, "24 direct + 11 consumable");
    assert_eq!(c.systems.len(), 4);
    assert_eq!(c.add_ons.len(), 4);
}

#[test]
fn every_recipe_member_resolves_to_a_direct_product() {
    let c = card();
    for s in &c.systems {
        for id in &s.product_ids {
            let p = c.product(id).unwrap_or_else(|| panic!("{} -> {id}", s.key));
            assert_eq!(p.class, ProductClass::Direct, "{} names {id}", s.key);
        }
    }
}

#[test]
fn every_add_on_operand_resolves() {
    let c = card();
    for a in &c.add_ons {
        if let AddOnEffect::Products { ops } = &a.effect {
            for op in ops {
                match op {
                    ProductOp::Add { product_id } | ProductOp::Remove { product_id } => {
                        assert!(c.product(product_id).is_some(), "{} -> {product_id}", a.key);
                    }
                    ProductOp::Replace { product_id, with } => {
                        assert!(c.product(product_id).is_some(), "{} -> {product_id}", a.key);
                        assert!(c.product(with).is_some(), "{} -> {with}", a.key);
                    }
                }
            }
        }
    }
}

#[test]
fn product_ids_are_unique() {
    let c = card();
    let mut seen = BTreeMap::new();
    for p in &c.products {
        assert!(seen.insert(&p.id, ()).is_none(), "duplicate {}", p.id);
    }
}

#[test]
fn no_orphan_direct_products() {
    // Every direct product must be reachable from a recipe or an add-on.
    // H2 Out and Fast Cure Activator are in NO recipe -- they are reachable
    // only as add-ons, which is exactly what this asserts.
    let c = card();
    validate(&c).expect("shipped card has no orphans");
    assert!(c
        .systems
        .iter()
        .all(|s| !s.product_ids.contains(&"h2_out".to_string())));
    assert!(c.add_on("h2_out").is_some());
    assert!(c.add_on("fast_cure").is_some());
}

#[test]
fn system_keys_match_the_harness_catalog() {
    // These must stay in lockstep with FLOOR_SYSTEMS in harness/index.html;
    // a measurement's systemType resolves straight through them.
    let c = card();
    let mut keys: Vec<&str> = c.systems.iter().map(|s| s.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["epoxy", "polish", "polyurea", "seal"]);
}

#[test]
fn all_recipes_are_flagged_unconfirmed() {
    // Membership was inferred from four same-SF sheets. Until Jordan signs
    // off, a consumer must be able to SEE that -- this is the guard that
    // makes a wrong recipe discoverable rather than silent.
    let c = card();
    assert!(
        c.systems.iter().all(|s| !s.confirmed),
        "a recipe was marked confirmed without sign-off"
    );
}

// ---------- negative costs ----------

#[test]
fn reclaim_credits_are_negative() {
    let c = card();
    assert!(c.product("flake_recovered").unwrap().unit_cost < 0.0);
    assert!(
        c.product("flake_recovered_double_broadcasted")
            .unwrap()
            .unit_cost
            < 0.0
    );
}

#[test]
fn net_flake_cost_is_positive() {
    // The realistic failure mode for reclaim credits is a sign error or a
    // swapped rate, either of which makes flake a net CREDIT and silently
    // deflates every polyurea quote. Asserting cost is monotonic in area
    // would NOT catch it -- negative unit costs make that property false by
    // construction. This is the property that actually protects us.
    let c = card();
    let net = |thrown: &str, recovered: &str| {
        let t = c.product(thrown).unwrap();
        let r = c.product(recovered).unwrap();
        t.unit_cost * t.rate + r.unit_cost * r.rate
    };
    cents(net("flake_thrown", "flake_recovered"), 0.2405);
    cents(
        net(
            "flake_thrown_double_broadcasted",
            "flake_recovered_double_broadcasted",
        ),
        0.37,
    );
    assert!(net("flake_thrown", "flake_recovered") > 0.0);
}

// ---------- consumable multipliers ----------

#[test]
fn commercial_systems_use_full_consumable_rates() {
    let c = card();
    for key in ["polish", "epoxy", "polyurea"] {
        for cons in c.consumables() {
            let m = c.consumable_multiplier(key, &cons.id).unwrap();
            cents(m, 1.0);
        }
    }
}

#[test]
fn seal_reduces_or_omits_consumables() {
    // A flat "consumables always apply at full rate" model overcharges every
    // sealer job; this pins the sealer template's divisors.
    let c = card();
    let m = |id: &str| c.consumable_multiplier("seal", id).unwrap();
    cents(m("cups_10_quart"), 0.0);
    cents(m("cups_5_quart"), 1.0 / 3.0);
    cents(m("cups_2_5_quart"), 1.0 / 5.0);
    cents(m("cups_quart"), 0.0);
    cents(m("brushes"), 1.0 / 4.0);
    cents(m("roller_covers"), 1.0 / 3.0);
    cents(m("trowles"), 0.0);
    cents(m("mini_roller_covers"), 0.0);
    cents(m("rags"), 1.0 / 8.0);
    cents(m("gloves"), 1.0 / 3.0);
    cents(m("trash_bags"), 1.0 / 8.0);
}

#[test]
fn every_system_covers_every_consumable() {
    // Belt and braces: validate() already refuses to LOAD a card with any
    // system x consumable pair missing (there is no default to fall back on),
    // but this states the rule where a reader will see it.
    let c = card();
    for s in &c.systems {
        for cons in c.consumables() {
            assert!(
                s.consumable_multipliers.contains_key(&cons.id),
                "system `{}` has no multiplier for `{}`",
                s.key,
                cons.id
            );
        }
    }
    assert_eq!(
        c.systems
            .iter()
            .map(|s| s.consumable_multipliers.len())
            .sum::<usize>(),
        44,
        "4 systems x 11 consumables, all explicit"
    );
}

#[test]
fn rejects_a_card_with_a_missing_consumable_multiplier() {
    // The load-time guard that replaced the default. The person hand-editing
    // the JSON to add a product is exactly the person not running cargo test,
    // so this has to fail the LOAD, not just a test.
    let mut c = minimal();
    c.products.push(product("wipes", ProductClass::Consumable));
    assert!(
        errs(&c).contains(&RateCardError::MissingConsumableMultiplier {
            system: "s".into(),
            consumable: "wipes".into(),
        })
    );
}

// ---------- add-on shapes ----------

#[test]
fn double_broadcast_replaces_both_flake_rows_and_adds_quartz() {
    let c = card();
    let AddOnEffect::Products { ops } = &c.add_on("double_broadcast").unwrap().effect else {
        panic!("double_broadcast must be a products add-on");
    };
    assert_eq!(ops.len(), 3);
    assert!(ops.contains(&ProductOp::Replace {
        product_id: "flake_thrown".into(),
        with: "flake_thrown_double_broadcasted".into(),
    }));
    assert!(ops.contains(&ProductOp::Replace {
        product_id: "flake_recovered".into(),
        with: "flake_recovered_double_broadcasted".into(),
    }));
    assert!(ops.contains(&ProductOp::Add {
        product_id: "quartz_double_broadcast".into(),
    }));
}

#[test]
fn crack_stitching_is_a_manual_cost_hook() {
    let c = card();
    let AddOnEffect::ManualCost { prompt } = &c.add_on("crack_stitching").unwrap().effect else {
        panic!("crack_stitching must be a manual-cost add-on");
    };
    assert!(!prompt.is_empty());
}

#[test]
fn universal_add_ons_apply_to_any_system() {
    let c = card();
    for key in ["h2_out", "fast_cure", "crack_stitching"] {
        assert!(
            c.add_on(key).unwrap().applies_to.is_empty(),
            "{key} should be available to any system"
        );
    }
    assert_eq!(
        c.add_on("double_broadcast").unwrap().applies_to,
        ["polyurea"]
    );
}

// ---------- wage source ----------

#[test]
fn davis_bacon_is_an_unpopulated_hook() {
    let c = card();
    assert!(c.labor.davis_bacon.is_none());
    cents(c.labor.wage_for(WageSource::Standard).unwrap(), 27.50);
    // Must NOT fall back to the standard wage: a prevailing-wage job priced
    // at 27.50 underbids by roughly half and looks entirely legitimate.
    assert_eq!(c.labor.wage_for(WageSource::DavisBacon), None);
}

#[test]
fn davis_bacon_wage_is_base_plus_fringe_once_populated() {
    let mut labor = card().labor;
    labor.davis_bacon = Some(engine_core::pricing::DavisBaconRates {
        county: "Wayne".into(),
        classification: "Painter".into(),
        base_wage_per_hour: 40.0,
        fringe_per_hour: 18.5,
    });
    cents(labor.wage_for(WageSource::DavisBacon).unwrap(), 58.5);
}

#[test]
fn labor_constants_match_the_workbook() {
    let c = card();
    cents(c.labor.standard_wage_per_hour, 27.50);
    cents(c.labor.payroll_tax_rate, 0.0765);
    cents(c.labor.insurance_benefits_per_hour, 0.0);
    cents(c.labor.overhead_per_man_hour, 52.99);
}

// ---------- validation rejects broken cards ----------

fn product(id: &str, class: ProductClass) -> Product {
    Product {
        id: id.into(),
        name: id.into(),
        class,
        unit: ProductUnit::Each,
        unit_cost: 1.0,
        rate: 0.001,
        rate_basis: RateBasis::AreaSf,
        scope_line: None,
        scope_order: 0,
    }
}

fn minimal() -> RateCard {
    RateCard {
        version: "test".into(),
        effective_date: "2026-01-01".into(),
        source: "test".into(),
        products: vec![product("a", ProductClass::Direct)],
        systems: vec![SystemRecipe {
            key: "s".into(),
            name: "S".into(),
            product_ids: vec!["a".into()],
            consumable_multipliers: BTreeMap::new(),
            confirmed: false,
            standard_grit: None,
            scope_intro: vec![],
            scope_outro: vec![],
        }],
        add_ons: vec![],
        grit_levels: vec![],
        labor: LaborRates {
            standard_wage_per_hour: 27.5,
            payroll_tax_rate: 0.0765,
            insurance_benefits_per_hour: 0.0,
            overhead_per_man_hour: 52.99,
            davis_bacon: None,
        },
    }
}

fn errs(c: &RateCard) -> Vec<RateCardError> {
    validate(c).unwrap_err()
}

#[test]
fn minimal_card_is_valid() {
    validate(&minimal()).expect("minimal card should validate");
}

#[test]
fn rejects_duplicate_product_id() {
    let mut c = minimal();
    c.products.push(product("a", ProductClass::Direct));
    assert!(errs(&c).contains(&RateCardError::DuplicateProduct("a".into())));
}

#[test]
fn rejects_duplicate_system_and_add_on_keys() {
    let mut c = minimal();
    c.systems.push(c.systems[0].clone());
    assert!(errs(&c).contains(&RateCardError::DuplicateSystem("s".into())));

    let mut c = minimal();
    let a = AddOn {
        key: "x".into(),
        name: "X".into(),
        applies_to: vec![],
        effect: AddOnEffect::ManualCost { prompt: "p".into() },
        priority: 0,
    };
    c.add_ons = vec![a.clone(), a];
    assert!(errs(&c).contains(&RateCardError::DuplicateAddOn("x".into())));
}

#[test]
fn rejects_unknown_product_in_system() {
    let mut c = minimal();
    c.systems[0].product_ids.push("ghost".into());
    assert!(errs(&c).contains(&RateCardError::UnknownProductInSystem {
        system: "s".into(),
        product: "ghost".into(),
    }));
}

#[test]
fn rejects_consumable_named_as_a_recipe_member() {
    // Consumables are governed by multipliers, never by membership.
    let mut c = minimal();
    c.products.push(product("wipes", ProductClass::Consumable));
    c.systems[0].product_ids.push("wipes".into());
    assert!(errs(&c).contains(&RateCardError::ConsumableInSystem {
        system: "s".into(),
        product: "wipes".into(),
    }));
}

#[test]
fn rejects_multiplier_for_a_non_consumable() {
    let mut c = minimal();
    c.systems[0].consumable_multipliers.insert("a".into(), 0.5);
    assert!(errs(&c).contains(&RateCardError::MultiplierNotConsumable {
        system: "s".into(),
        product: "a".into(),
    }));
}

#[test]
fn rejects_unknown_operands_in_add_ons() {
    let mut c = minimal();
    c.add_ons.push(AddOn {
        key: "bad".into(),
        name: "Bad".into(),
        applies_to: vec!["nosuch".into()],
        effect: AddOnEffect::Products {
            ops: vec![ProductOp::Replace {
                product_id: "a".into(),
                with: "ghost".into(),
            }],
        },
        priority: 0,
    });
    let e = errs(&c);
    assert!(e.contains(&RateCardError::UnknownProductInAddOn {
        add_on: "bad".into(),
        product: "ghost".into(),
    }));
    assert!(e.contains(&RateCardError::UnknownSystemInAddOn {
        add_on: "bad".into(),
        system: "nosuch".into(),
    }));
}

#[test]
fn rejects_two_add_ons_replacing_the_same_product_differently() {
    // Silent last-wins would produce a wrong price that looks legitimate.
    let mut c = minimal();
    c.products.push(product("b", ProductClass::Direct));
    c.products.push(product("c", ProductClass::Direct));
    c.systems[0].product_ids.push("b".into());
    c.systems[0].product_ids.push("c".into());
    let mk = |key: &str, with: &str| AddOn {
        key: key.into(),
        name: key.into(),
        applies_to: vec![],
        effect: AddOnEffect::Products {
            ops: vec![ProductOp::Replace {
                product_id: "a".into(),
                with: with.into(),
            }],
        },
        priority: 0,
    };
    c.add_ons = vec![mk("one", "b"), mk("two", "c")];
    assert!(errs(&c).contains(&RateCardError::ConflictingReplace {
        a: "one".into(),
        b: "two".into(),
        product: "a".into(),
    }));
}

#[test]
fn allows_two_add_ons_replacing_the_same_product_identically() {
    // They agree, so there is nothing to disambiguate.
    let mut c = minimal();
    c.products.push(product("b", ProductClass::Direct));
    c.systems[0].product_ids.push("b".into());
    let mk = |key: &str| AddOn {
        key: key.into(),
        name: key.into(),
        applies_to: vec![],
        effect: AddOnEffect::Products {
            ops: vec![ProductOp::Replace {
                product_id: "a".into(),
                with: "b".into(),
            }],
        },
        priority: 0,
    };
    c.add_ons = vec![mk("one"), mk("two")];
    validate(&c).expect("identical replacements agree");
}

#[test]
fn allows_conflicting_replaces_on_disjoint_systems() {
    let mut c = minimal();
    c.products.push(product("b", ProductClass::Direct));
    c.products.push(product("c", ProductClass::Direct));
    c.systems[0].product_ids.push("b".into());
    c.systems[0].product_ids.push("c".into());
    c.systems.push(SystemRecipe {
        key: "t".into(),
        name: "T".into(),
        product_ids: vec![],
        consumable_multipliers: BTreeMap::new(),
        confirmed: false,
        standard_grit: None,
        scope_intro: vec![],
        scope_outro: vec![],
    });
    let mk = |key: &str, with: &str, sys: &str| AddOn {
        key: key.into(),
        name: key.into(),
        applies_to: vec![sys.into()],
        effect: AddOnEffect::Products {
            ops: vec![ProductOp::Replace {
                product_id: "a".into(),
                with: with.into(),
            }],
        },
        priority: 0,
    };
    c.add_ons = vec![mk("one", "b", "s"), mk("two", "c", "t")];
    validate(&c).expect("add-ons on disjoint systems cannot collide");
}

#[test]
fn rejects_orphan_direct_product() {
    let mut c = minimal();
    c.products.push(product("lonely", ProductClass::Direct));
    assert!(errs(&c).contains(&RateCardError::OrphanProduct("lonely".into())));
}

#[test]
fn consumables_are_never_orphans() {
    // They are reached via multipliers, not membership. The multiplier is
    // supplied here so this isolates the orphan rule rather than tripping the
    // exhaustiveness rule.
    let mut c = minimal();
    c.products.push(product("wipes", ProductClass::Consumable));
    c.systems[0]
        .consumable_multipliers
        .insert("wipes".into(), 1.0);
    validate(&c).expect("an unreferenced consumable is not an orphan");
}

#[test]
fn rejects_non_finite_and_negative_values() {
    let mut c = minimal();
    c.products[0].unit_cost = f64::NAN;
    assert!(errs(&c)
        .iter()
        .any(|e| matches!(e, RateCardError::NotFinite { .. })));

    let mut c = minimal();
    c.products[0].rate = -1.0; // a negative COST is fine; a negative RATE is not
    assert!(errs(&c)
        .iter()
        .any(|e| matches!(e, RateCardError::Negative { .. })));

    let mut c = minimal();
    c.labor.overhead_per_man_hour = -1.0;
    assert!(errs(&c)
        .iter()
        .any(|e| matches!(e, RateCardError::Negative { .. })));

    let mut c = minimal();
    c.systems[0].consumable_multipliers.insert("a".into(), -1.0);
    assert!(errs(&c)
        .iter()
        .any(|e| matches!(e, RateCardError::Negative { .. })));
}

#[test]
fn negative_unit_cost_is_accepted() {
    let mut c = minimal();
    c.products[0].unit_cost = -74.0;
    validate(&c).expect("reclaim credits are legitimate");
}

#[test]
fn validate_reports_every_defect_not_just_the_first() {
    // Fixing a hand-edited data file one error per run is miserable.
    let mut c = minimal();
    c.systems[0].product_ids.push("ghost1".into());
    c.systems[0].product_ids.push("ghost2".into());
    c.products.push(product("lonely", ProductClass::Direct));
    assert!(errs(&c).len() >= 3);
}

#[test]
fn rejects_malformed_json() {
    let e = from_json("{ not json").unwrap_err();
    assert!(matches!(e[0], RateCardError::Json(_)));
}

#[test]
fn rejects_structurally_invalid_json() {
    // Parses cleanly, but names a product that does not exist.
    let src = r#"{
      "version": "v", "effective_date": "2026-01-01", "source": "t",
      "products": [],
      "systems": [{ "key": "s", "name": "S", "product_ids": ["ghost"],
                    "consumable_multipliers": {}, "confirmed": false }],
      "labor": { "standard_wage_per_hour": 27.5, "payroll_tax_rate": 0.0765,
                 "insurance_benefits_per_hour": 0.0, "overhead_per_man_hour": 52.99 }
    }"#;
    assert!(from_json(src)
        .unwrap_err()
        .contains(&RateCardError::UnknownProductInSystem {
            system: "s".into(),
            product: "ghost".into(),
        }));
}

#[test]
fn round_trips_through_json() {
    let c = card();
    let json = serde_json::to_string(&c).expect("serialize");
    let back = from_json(&json).expect("reparse");
    assert_eq!(c, back);
}

// ---------- documented sanity, not a cost engine ----------

#[test]
fn polyurea_material_rate_is_plausible() {
    // NOT a cost engine -- a single documented buildup that would catch a
    // decimal-place error in the catalog. Sums unit_cost x rate over the
    // polyurea recipe; the flake credit is included and must not flip it.
    let c = card();
    let sys = c.system("polyurea").unwrap();
    let per_sf: f64 = sys
        .product_ids
        .iter()
        .map(|id| {
            let p = c.product(id).unwrap();
            p.unit_cost * p.rate
        })
        .sum();
    // 12 lines summing to $1.81071/SF, flake credit included. Verified by
    // hand against the catalog; a decimal slip in any row moves this.
    cents(per_sf, 1.81071);
    assert!(per_sf > 0.0);
}

#[test]
fn consumable_rate_at_full_multiplier_is_plausible() {
    let c = card();
    let per_sf: f64 = c.consumables().map(|p| p.unit_cost * p.rate).sum();
    cents(per_sf, 0.1741);
}

// ---------- grit ladder ----------

fn grit(key: &str, mult: f64) -> GritLevel {
    GritLevel {
        key: key.into(),
        name: format!("{key} grit"),
        labor_multiplier: mult,
    }
}

#[test]
fn a_card_with_no_grit_ladder_is_valid() {
    // Every card written before grit existed, and every fixture, is this one.
    let c = minimal();
    assert!(c.grit_levels.is_empty());
    validate(&c).expect("grit is optional");
}

#[test]
fn rejects_duplicate_grit_level_key() {
    let mut c = minimal();
    c.grit_levels.push(grit("400", 1.0));
    c.grit_levels.push(grit("400", 1.2));
    assert!(errs(&c).contains(&RateCardError::DuplicateGritLevel("400".into())));
}

#[test]
fn rejects_a_standard_grit_that_names_no_level() {
    // Caught at LOAD. Otherwise every grit-bearing area on that system fails
    // individually at quote time, far from the typo that caused it.
    let mut c = minimal();
    c.grit_levels.push(grit("400", 1.0));
    c.systems[0].standard_grit = Some("450".into());
    assert!(errs(&c).contains(&RateCardError::UnknownGritInSystem {
        system: "s".into(),
        grit: "450".into(),
    }));
}

#[test]
fn rejects_a_non_positive_grit_multiplier() {
    // 0.0 would zero out an area's hours and price it materials-only -- the
    // same silent-and-plausible failure MissingLabor exists to prevent -- and
    // a 0.0 at a system's STANDARD would divide by zero.
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut c = minimal();
        c.grit_levels.push(grit("400", bad));
        assert!(
            errs(&c).iter().any(|e| matches!(
                e,
                RateCardError::GritMultiplierNotPositive { grit, .. } if grit == "400"
            )),
            "multiplier {bad} should be rejected"
        );
    }
}

#[test]
fn the_shipped_card_grinds_on_exactly_the_systems_with_a_standard_grit() {
    let c = card();
    // Grinding systems offer the whole ladder; the others offer nothing, and
    // that is what a UI populates a picker from.
    for sys in ["polish", "seal"] {
        assert_eq!(
            c.grits_for_system(sys).len(),
            c.grit_levels.len(),
            "{sys} grinds and should offer every level"
        );
    }
    for sys in ["epoxy", "polyurea"] {
        assert!(
            c.grits_for_system(sys).is_empty(),
            "{sys} has no standard grit, so no level may be chosen on it"
        );
    }
    assert!(c.grits_for_system("nonexistent").is_empty());
}

#[test]
fn the_escalator_lookup_is_relative_to_the_system_not_absolute() {
    let mut c = card();
    for g in c.grit_levels.iter_mut() {
        g.labor_multiplier = match g.key.as_str() {
            "100" => 0.5,
            "400" => 1.0,
            "800" => 1.25,
            _ => 1.0,
        };
    }
    // Seal's standard is 100 (0.5), polish's is 400 (1.0). The same level
    // therefore escalates them differently, and each is unity at its own.
    cents(c.grit_labor_multiplier("seal", "100").unwrap(), 1.0);
    cents(c.grit_labor_multiplier("polish", "400").unwrap(), 1.0);
    cents(c.grit_labor_multiplier("seal", "800").unwrap(), 2.5);
    cents(c.grit_labor_multiplier("polish", "800").unwrap(), 1.25);
    // No standard means grit does not apply, which is not the same as 1.0.
    assert_eq!(c.grit_labor_multiplier("epoxy", "800"), None);
    assert_eq!(c.grit_labor_multiplier("polish", "9999"), None);
}
