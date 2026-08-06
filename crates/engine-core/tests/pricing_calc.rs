//! The cost buildup, verified.
//!
//! # Where the expected numbers come from
//!
//! The golden fixtures in `tests/fixtures/pricing/` were produced by an
//! INDEPENDENT Python implementation of the model, written from the spec
//! prose rather than from `calc.rs`, and one of them (`epoxy-2000sf`) was
//! additionally checked by longhand arithmetic. Asserting against numbers
//! this engine produced would be circular and would prove nothing.
//!
//! Each fixture EMBEDS its own rate card. They are therefore immune to
//! catalog drift: raising a material price in `data/rate-cards/` cannot
//! silently invalidate a golden, and a historical job stays reproducible
//! against the rates it was actually quoted at.
//!
//! # Money tolerance
//!
//! ±$0.01 absolute, not the 1e-6 helper the geometry tests use. Nothing is
//! rounded during the buildup — see the rounding-order note on
//! `pricing::calc`.

#![cfg(feature = "serde")]

use engine_core::pricing::{
    consumable_rate_per_sf, from_json, material_rate_per_sf, price_area, price_from_cost,
    price_job, AreaInput, BidItem, JobCosts, JobInput, LaborInput, LineSource, PricingContext,
    PricingError, Productivity, RateCard, WageSource, MARGIN_FLOOR, SF_PER_MAN_HOUR_MAX,
    SF_PER_MAN_HOUR_MIN, SF_PER_MAN_HOUR_TYPICAL,
};
use serde::Deserialize;
use std::collections::BTreeMap;

/// One cent. Money, not geometry.
fn cents(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 0.01, "{what}: expected {b:.4}, got {a:.4}");
}

fn card() -> RateCard {
    from_json(include_str!("../../../data/rate-cards/commercial-v1.json"))
        .expect("shipped rate card must load")
}

// ---------- golden fixtures ----------

#[derive(Debug, Deserialize)]
struct ExpectedArea {
    materials: f64,
    consumables: f64,
    labor: f64,
    overhead: f64,
    manual_costs: f64,
    man_hours: f64,
    cost: f64,
}

#[derive(Debug, Deserialize)]
struct Expected {
    areas: Vec<ExpectedArea>,
    area_cost: f64,
    cost: f64,
    price: f64,
    profit: f64,
    margin: f64,
    below_margin_floor: bool,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    note: String,
    rate_card: RateCard,
    job: JobInput,
    expected: Expected,
}

fn fixtures() -> Vec<Fixture> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pricing");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("fixture dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read fixture");
        out.push(
            serde_json::from_str::<Fixture>(&src)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        );
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[test]
fn golden_fixtures_match_to_the_cent() {
    let fx = fixtures();
    assert!(
        fx.len() >= 5,
        "expected the curated set, found {}",
        fx.len()
    );
    for f in &fx {
        // Each fixture's OWN card, never the shipped one.
        let got = price_job(&f.rate_card, &f.job)
            .unwrap_or_else(|e| panic!("{} ({}): {e}", f.name, f.note));

        assert_eq!(got.areas.len(), f.expected.areas.len(), "{}", f.name);
        for (i, (g, e)) in got.areas.iter().zip(&f.expected.areas).enumerate() {
            let at = |w: &str| format!("{} area[{i}] {w}", f.name);
            cents(g.materials, e.materials, &at("materials"));
            cents(g.consumables, e.consumables, &at("consumables"));
            cents(g.labor, e.labor, &at("labor"));
            cents(g.overhead, e.overhead, &at("overhead"));
            cents(g.manual_costs, e.manual_costs, &at("manual_costs"));
            cents(g.man_hours, e.man_hours, &at("man_hours"));
            cents(g.cost, e.cost, &at("cost"));
        }
        cents(got.area_cost, f.expected.area_cost, &f.name);
        cents(got.cost, f.expected.cost, &f.name);
        cents(got.price, f.expected.price, &f.name);
        cents(got.profit, f.expected.profit, &f.name);
        cents(got.margin, f.expected.margin, &f.name);
        assert_eq!(
            got.below_margin_floor, f.expected.below_margin_floor,
            "{} floor flag",
            f.name
        );
    }
}

#[test]
fn every_fixture_round_trips_its_margin() {
    for f in fixtures() {
        let q = price_job(&f.rate_card, &f.job).expect("prices");
        assert!(
            (q.realized_margin() - q.margin).abs() < 1e-12,
            "{}: margin {} did not round-trip (got {})",
            f.name,
            q.margin,
            q.realized_margin()
        );
    }
}

// ---------- margin, the highest-consequence math in the module ----------

#[test]
fn price_is_margin_not_markup() {
    // The regression this whole module is shaped around. At 50% markup gives
    // 1.5x cost and margin gives 2x cost -- $5,000 apart on a $10,000 job.
    let (price, profit) = price_from_cost(10_000.0, 0.50).unwrap();
    cents(price, 20_000.0, "50% margin doubles cost");
    cents(profit, 10_000.0, "profit dollars");
    assert!(
        (price - 15_000.0).abs() > 1.0,
        "price matched cost x (1 + margin) -- that is MARKUP"
    );
}

#[test]
fn margin_round_trips_at_every_rate() {
    // profit / price must return the input margin. This is the test that
    // catches a markup/margin regression.
    for &m in &[
        0.0, 0.05, 0.12, 0.20, 0.25, 0.35, 0.40, 0.50, 0.65, 0.9, 0.99,
    ] {
        for &cost in &[1.0, 1_234.56, 4490.7572, 250_000.0] {
            let (price, profit) = price_from_cost(cost, m).unwrap();
            let realized = profit / price;
            assert!(
                (realized - m).abs() < 1e-12,
                "margin {m} at cost {cost}: round-tripped to {realized}"
            );
            cents(price - profit, cost, "price - profit == cost");
        }
    }
}

#[test]
fn margin_of_one_or_more_is_rejected() {
    // 1.0 divides by zero; above it prices negative.
    assert!(matches!(
        price_from_cost(1000.0, 1.0),
        Err(PricingError::InvalidMargin(_))
    ));
    assert!(matches!(
        price_from_cost(1000.0, 1.5),
        Err(PricingError::InvalidMargin(_))
    ));
    assert!(matches!(
        price_from_cost(1000.0, f64::NAN),
        Err(PricingError::InvalidMargin(_))
    ));
    assert!(matches!(
        price_from_cost(1000.0, f64::INFINITY),
        Err(PricingError::InvalidMargin(_))
    ));
}

#[test]
fn sub_floor_margin_prices_and_flags_rather_than_blocking() {
    // Jordan takes a thin job deliberately sometimes. The app's job is to make
    // him look at it, not to refuse.
    let c = card();
    let job = job_with(vec![area_of("epoxy", 1000.0, 2.0, 8.0)], 0.10);
    let q = price_job(&c, &job).expect("a sub-floor quote must still price");
    assert!(q.below_margin_floor);
    assert!(q.price > 0.0 && q.profit > 0.0);
    cents(
        q.realized_margin(),
        0.10,
        "sub-floor margin still round-trips",
    );
}

#[test]
fn the_floor_is_a_named_constant_at_twenty_percent() {
    cents(MARGIN_FLOOR, 0.20, "business floor");
    let c = card();
    // Exactly at the floor is NOT below it.
    let at = price_job(
        &c,
        &job_with(vec![area_of("epoxy", 500.0, 2.0, 4.0)], MARGIN_FLOOR),
    )
    .unwrap();
    assert!(!at.below_margin_floor);
    let under = price_job(
        &c,
        &job_with(
            vec![area_of("epoxy", 500.0, 2.0, 4.0)],
            MARGIN_FLOOR - 0.001,
        ),
    )
    .unwrap();
    assert!(under.below_margin_floor);
}

// ---------- helpers ----------

fn area_of(system: &str, sf: f64, crew: f64, hours: f64) -> AreaInput {
    AreaInput {
        id: 1,
        name: format!("{system} area"),
        area_sf: sf,
        perimeter_lf: None,
        system_key: system.to_string(),
        grit_level: None,
        add_on_keys: vec![],
        labor: Some(LaborInput { crew, hours }),
        quantity_overrides: BTreeMap::new(),
        manual_costs: BTreeMap::new(),
    }
}

/// Standard-wage context for the shipped card. Binding the wage to the
/// context is what makes a mismatched basis unrepresentable at a call site.
fn std_ctx(c: &RateCard) -> PricingContext<'_> {
    PricingContext::new(c, WageSource::Standard).expect("standard wage always resolves")
}

fn job_with(areas: Vec<AreaInput>, margin: f64) -> JobInput {
    JobInput {
        areas,
        // No grouping: the engine synthesizes one bid item per area, which is
        // exactly how every quote behaved before this layer existed.
        bid_items: vec![],
        job_costs: JobCosts::default(),
        margin,
        wage_source: WageSource::Standard,
    }
}

// ---------- suppression ----------

#[test]
fn suppressed_line_stays_visible_at_zero_with_its_factor() {
    // A line that vanishes is indistinguishable from a line that was never
    // there. "Did I delete mender or forget it?" is a real failure an hour
    // into building a quote, so the line is REPORTED at $0.00.
    let c = card();
    let mut a = area_of("polish", 3000.0, 3.0, 9.0);
    a.quantity_overrides.insert("mender_part_a".into(), 0.0);
    let q = price_area(&std_ctx(&c), &a).unwrap();

    let line = q
        .lines
        .iter()
        .find(|l| l.product_id == "mender_part_a")
        .expect("suppressed line must still be present");
    assert!(line.suppressed);
    cents(line.factor, 0.0, "factor");
    cents(line.quantity, 0.0, "quantity");
    cents(line.extended_cost, 0.0, "extended cost");
    // base_quantity survives so the UI can offer an undo showing what it was.
    cents(line.base_quantity, 0.004 * 3000.0, "base quantity retained");
}

#[test]
fn partial_factor_is_the_general_case_delete_is_just_zero() {
    // "Half the normal mender" on a slab needing light crack repair.
    let c = card();
    let full = price_area(&std_ctx(&c), &area_of("polish", 3000.0, 3.0, 9.0)).unwrap();
    let mut half_in = area_of("polish", 3000.0, 3.0, 9.0);
    half_in
        .quantity_overrides
        .insert("mender_part_a".into(), 0.5);
    let half = price_area(&std_ctx(&c), &half_in).unwrap();

    let line = half
        .lines
        .iter()
        .find(|l| l.product_id == "mender_part_a")
        .unwrap();
    assert!(!line.suppressed, "0.5 is reduced, not suppressed");
    cents(line.factor, 0.5, "factor");
    // Exactly half of one line's cost comes off the total.
    let full_line = full
        .lines
        .iter()
        .find(|l| l.product_id == "mender_part_a")
        .unwrap();
    cents(
        full.materials - half.materials,
        full_line.extended_cost / 2.0,
        "half of one line",
    );
}

#[test]
fn suppression_does_not_touch_the_rate_card() {
    // Per-JOB override. The next job still defaults to the full recipe.
    let c = card();
    let mut a = area_of("polish", 1000.0, 2.0, 4.0);
    a.quantity_overrides.insert("mender_part_a".into(), 0.0);
    let _ = price_area(&std_ctx(&c), &a).unwrap();

    let clean = price_area(&std_ctx(&c), &area_of("polish", 1000.0, 2.0, 4.0)).unwrap();
    let line = clean
        .lines
        .iter()
        .find(|l| l.product_id == "mender_part_a")
        .unwrap();
    assert!(
        !line.suppressed,
        "the card was mutated by a per-job override"
    );
    assert!(line.extended_cost > 0.0);
    assert!(c
        .system("polish")
        .unwrap()
        .product_ids
        .contains(&"mender_part_a".to_string()));
}

#[test]
fn suppressing_an_add_on_contributed_product_zeroes_only_that_line() {
    // Defined behaviour: the override applies to the resolved line whatever
    // its source, so quartz can be dropped while Double Broadcast's flake
    // REPLACEMENTS stay in force.
    let c = card();
    let mut a = area_of("polyurea", 1000.0, 4.0, 10.0);
    a.add_on_keys = vec!["double_broadcast".into()];
    a.quantity_overrides
        .insert("quartz_double_broadcast".into(), 0.0);
    let q = price_area(&std_ctx(&c), &a).unwrap();

    let quartz = q
        .lines
        .iter()
        .find(|l| l.product_id == "quartz_double_broadcast")
        .expect("still reported");
    assert!(quartz.suppressed);
    cents(quartz.extended_cost, 0.0, "quartz suppressed");
    assert_eq!(
        quartz.source,
        LineSource::AddOn {
            key: "double_broadcast".into()
        }
    );

    // The replacements are untouched.
    assert!(q
        .lines
        .iter()
        .any(|l| l.product_id == "flake_thrown_double_broadcasted" && !l.suppressed));
    assert!(!q.lines.iter().any(|l| l.product_id == "flake_thrown"));
}

#[test]
fn a_negative_override_factor_is_rejected() {
    let c = card();
    let mut a = area_of("epoxy", 1000.0, 2.0, 8.0);
    a.quantity_overrides.insert("clear_epoxy".into(), -1.0);
    assert!(matches!(
        price_area(&std_ctx(&c), &a),
        Err(PricingError::InvalidOverride { .. })
    ));
}

// ---------- add-ons ----------

#[test]
fn double_broadcast_replaces_both_flake_rows_and_adds_quartz() {
    let c = card();
    let mut a = area_of("polyurea", 1000.0, 4.0, 10.0);
    a.add_on_keys = vec!["double_broadcast".into()];
    let q = price_area(&std_ctx(&c), &a).unwrap();
    let ids: Vec<&str> = q.lines.iter().map(|l| l.product_id.as_str()).collect();

    assert!(ids.contains(&"flake_thrown_double_broadcasted"));
    assert!(ids.contains(&"flake_recovered_double_broadcasted"));
    assert!(ids.contains(&"quartz_double_broadcast"));
    assert!(!ids.contains(&"flake_thrown"), "base row must be replaced");
    assert!(!ids.contains(&"flake_recovered"));
}

#[test]
fn reclaim_credit_lines_are_negative_but_net_flake_is_positive() {
    // Negative unit costs are legitimate. The property that actually protects
    // us is that thrown + recovered nets POSITIVE -- a sign error or swapped
    // rate flips it and silently deflates every polyurea quote. Monotonicity
    // in area would be false by construction here.
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("polyurea", 1000.0, 4.0, 10.0)).unwrap();
    let thrown = q
        .lines
        .iter()
        .find(|l| l.product_id == "flake_thrown")
        .unwrap();
    let recovered = q
        .lines
        .iter()
        .find(|l| l.product_id == "flake_recovered")
        .unwrap();

    assert!(recovered.extended_cost < 0.0, "reclaim is a credit");
    assert!(thrown.extended_cost > 0.0);
    let net = thrown.extended_cost + recovered.extended_cost;
    assert!(net > 0.0, "net flake cost went negative: {net}");
    cents(net, 240.50, "net flake at 1000 SF"); // 0.2405/SF
}

#[test]
fn a_manual_cost_add_on_requires_an_amount() {
    let c = card();
    let mut a = area_of("epoxy", 1000.0, 2.0, 8.0);
    a.add_on_keys = vec!["crack_stitching".into()];
    // No amount supplied -> error, never a silent $0.
    assert!(matches!(
        price_area(&std_ctx(&c), &a),
        Err(PricingError::MissingManualCost { .. })
    ));

    a.manual_costs.insert("crack_stitching".into(), 675.0);
    let q = price_area(&std_ctx(&c), &a).unwrap();
    cents(q.manual_costs, 675.0, "manual cost");
    assert!(q.cost > 675.0);
}

#[test]
fn an_add_on_scoped_to_another_system_is_rejected() {
    let c = card();
    let mut a = area_of("epoxy", 1000.0, 2.0, 8.0);
    a.add_on_keys = vec!["double_broadcast".into()]; // polyurea only
    assert!(matches!(
        price_area(&std_ctx(&c), &a),
        Err(PricingError::AddOnNotApplicable { .. })
    ));
}

#[test]
fn universal_add_ons_attach_to_any_system() {
    let c = card();
    for system in ["seal", "polish", "epoxy", "polyurea"] {
        let mut a = area_of(system, 1000.0, 2.0, 8.0);
        a.add_on_keys = vec!["h2_out".into(), "fast_cure".into()];
        let q = price_area(&std_ctx(&c), &a).unwrap();
        assert!(q.lines.iter().any(|l| l.product_id == "h2_out"), "{system}");
        assert!(q
            .lines
            .iter()
            .any(|l| l.product_id == "fast_cure_activator"));
    }
}

#[test]
fn unknown_system_and_add_on_are_named_in_the_error() {
    let c = card();
    assert!(matches!(
        price_area(&std_ctx(&c), &area_of("marble", 100.0, 1.0, 1.0)),
        Err(PricingError::UnknownSystem(s)) if s == "marble"
    ));
    let mut a = area_of("epoxy", 100.0, 1.0, 1.0);
    a.add_on_keys = vec!["glitter".into()];
    assert!(matches!(
        price_area(&std_ctx(&c), &a),
        Err(PricingError::UnknownAddOn(s)) if s == "glitter"
    ));
}

// ---------- labor ----------

#[test]
fn an_area_with_no_hours_errors_rather_than_pricing_materials_only() {
    // The exact failure this must not have: a materials-only price reads as
    // entirely legitimate.
    let c = card();
    let mut a = area_of("epoxy", 2000.0, 0.0, 0.0);
    a.labor = None;
    match price_area(&std_ctx(&c), &a) {
        Err(PricingError::MissingLabor { area }) => assert_eq!(area, a.name),
        other => panic!("expected MissingLabor, got {other:?}"),
    }
    // And it propagates: one bad area fails the whole job.
    let job = job_with(vec![a], 0.35);
    assert!(matches!(
        price_job(&c, &job),
        Err(PricingError::MissingLabor { .. })
    ));
}

#[test]
fn crew_times_hours_is_the_only_labor_driver() {
    // 4 crew x 10 h and 2 crew x 20 h cost identically; crew affects
    // scheduling, not dollars.
    let c = card();
    let wide = price_area(&std_ctx(&c), &area_of("epoxy", 2000.0, 4.0, 10.0)).unwrap();
    let deep = price_area(&std_ctx(&c), &area_of("epoxy", 2000.0, 2.0, 20.0)).unwrap();
    cents(wide.labor, deep.labor, "labor");
    cents(wide.overhead, deep.overhead, "overhead");
    cents(wide.cost, deep.cost, "area cost");
    cents(wide.man_hours, 40.0, "man hours");
}

#[test]
fn payroll_tax_applies_to_wages_and_overhead_is_per_man_hour() {
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("epoxy", 100.0, 3.0, 8.0)).unwrap();
    let mh = 24.0;
    cents(q.labor, 27.50 * mh * 1.0765, "wage + 7.65% employer FICA");
    cents(q.overhead, 52.99 * mh, "overhead per man-hour");
}

#[test]
fn davis_bacon_without_rates_errors_instead_of_using_the_standard_wage() {
    // Underbidding a prevailing-wage job by roughly half looks entirely
    // legitimate, which is why this must not fall back. The CONTEXT now
    // catches it once, up front, instead of every area failing separately —
    // and there is no way to build an area-pricing call that dodges it.
    let c = card();
    assert!(matches!(
        PricingContext::new(&c, WageSource::DavisBacon),
        Err(PricingError::WageUnavailable(WageSource::DavisBacon))
    ));
    // And it propagates through a whole job.
    let mut job = job_with(vec![area_of("epoxy", 1000.0, 2.0, 8.0)], 0.35);
    job.wage_source = WageSource::DavisBacon;
    assert!(matches!(
        price_job(&c, &job),
        Err(PricingError::WageUnavailable(_))
    ));
}

#[test]
fn a_context_binds_the_wage_so_it_cannot_be_mismatched() {
    // The whole point of the struct: the resolved wage travels with the card,
    // so a live-updating panel cannot price one area on the wrong basis.
    let c = card();
    let ctx = std_ctx(&c);
    assert_eq!(ctx.wage_source(), WageSource::Standard);
    cents(ctx.wage_per_hour(), 27.50, "resolved once at construction");

    let mut db = c.clone();
    db.labor.davis_bacon = Some(engine_core::pricing::DavisBaconRates {
        county: "Wayne".into(),
        classification: "Painter".into(),
        base_wage_per_hour: 40.0,
        fringe_per_hour: 18.5,
    });
    let prevailing = PricingContext::new(&db, WageSource::DavisBacon).unwrap();
    cents(prevailing.wage_per_hour(), 58.5, "base + fringe");

    // Same area, same card, different basis -> different labor, and the only
    // way to express that is to say so when building the context.
    let a = area_of("epoxy", 1000.0, 2.0, 8.0);
    let standard = price_area(&std_ctx(&db), &a).unwrap();
    let db_priced = price_area(&prevailing, &a).unwrap();
    assert!(
        db_priced.labor > standard.labor * 2.0,
        "prevailing wage must move labor materially"
    );
    cents(
        standard.materials,
        db_priced.materials,
        "materials unaffected",
    );
}

#[test]
fn negative_or_non_finite_labor_is_rejected() {
    let c = card();
    for (crew, hours) in [(-1.0, 8.0), (2.0, -8.0), (f64::NAN, 8.0)] {
        assert!(matches!(
            price_area(&std_ctx(&c), &area_of("epoxy", 100.0, crew, hours)),
            Err(PricingError::InvalidLabor { .. })
        ));
    }
}

// ---------- consumables ----------

#[test]
fn seal_applies_reduced_multipliers_and_reports_the_zeroed_ones_at_nothing() {
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("seal", 5000.0, 2.0, 6.0)).unwrap();
    // All 11 are reported. The four seal does not use come back at zero
    // rather than missing: an omitted row is indistinguishable from one
    // nobody considered, and they cost nothing either way.
    assert_eq!(
        q.lines
            .iter()
            .filter(|l| l.source == LineSource::Consumable)
            .count(),
        11,
        "every consumable is reported"
    );
    for unused in ["cups_10_quart", "cups_quart", "trowles", "mini_roller_covers"] {
        let l = q.lines.iter().find(|l| l.product_id == unused).unwrap();
        cents(l.multiplier, 0.0, unused);
        cents(l.quantity, 0.0, unused);
        cents(l.extended_cost, 0.0, unused);
        assert!(
            !l.suppressed,
            "{unused} is unused by the system, not suppressed by the estimator"
        );
    }
    // Brushes run at 1/4, and say so.
    let brushes = q.lines.iter().find(|l| l.product_id == "brushes").unwrap();
    cents(brushes.multiplier, 0.25, "brushes multiplier");
    cents(brushes.quantity, 0.0064 * 5000.0 * 0.25, "brushes at 1/4");
    // A recipe line carries no multiplier of its own.
    let recipe = q
        .lines
        .iter()
        .find(|l| l.source == LineSource::Recipe)
        .unwrap();
    cents(recipe.multiplier, 1.0, "recipe lines are unscaled");
}

#[test]
fn commercial_systems_run_consumables_at_full_rate() {
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("epoxy", 5000.0, 2.0, 6.0)).unwrap();
    let brushes = q.lines.iter().find(|l| l.product_id == "brushes").unwrap();
    cents(brushes.quantity, 0.0064 * 5000.0, "full rate");
    assert_eq!(
        q.lines
            .iter()
            .filter(|l| l.source == LineSource::Consumable)
            .count(),
        11
    );
}

// ---------- job roll-up ----------

#[test]
fn job_cost_is_areas_plus_job_level_costs() {
    let c = card();
    let areas = vec![
        area_of("epoxy", 1000.0, 2.0, 8.0),
        area_of("seal", 2000.0, 2.0, 4.0),
    ];
    let bare = price_job(&c, &job_with(areas.clone(), 0.35)).unwrap();
    let mut job = job_with(areas, 0.35);
    job.job_costs = JobCosts {
        travel: 450.0,
        permits: 125.0,
        equipment: 800.0,
        mobilization: 300.0,
    };
    let with_costs = price_job(&c, &job).unwrap();

    cents(with_costs.area_cost, bare.area_cost, "areas unchanged");
    cents(with_costs.cost - bare.cost, 1675.0, "job costs added once");
    cents(
        with_costs.realized_margin(),
        0.35,
        "margin still round-trips",
    );
}

#[test]
fn every_change_flows_through_to_price_and_profit() {
    // No cached state, no recalculate button: each call recomputes from
    // scratch, so hours, suppression, add-ons and job costs all move price.
    let c = card();
    let base = price_job(
        &c,
        &job_with(vec![area_of("epoxy", 2000.0, 3.0, 8.0)], 0.35),
    )
    .unwrap();

    let mut more_hours = job_with(vec![area_of("epoxy", 2000.0, 3.0, 12.0)], 0.35);
    more_hours.areas[0].labor = Some(LaborInput {
        crew: 3.0,
        hours: 12.0,
    });
    assert!(price_job(&c, &more_hours).unwrap().price > base.price);

    let mut suppressed = job_with(vec![area_of("epoxy", 2000.0, 3.0, 8.0)], 0.35);
    suppressed.areas[0]
        .quantity_overrides
        .insert("high_wear_urethane".into(), 0.0);
    assert!(price_job(&c, &suppressed).unwrap().price < base.price);

    let richer = price_job(
        &c,
        &job_with(vec![area_of("epoxy", 2000.0, 3.0, 8.0)], 0.45),
    )
    .unwrap();
    assert!(richer.price > base.price);
    cents(richer.cost, base.cost, "margin does not change COST");
}

#[test]
fn unconfirmed_recipes_are_surfaced_on_the_result() {
    // Every recipe is still inferred; a consumer must be able to say so.
    let c = card();
    let q = price_job(
        &c,
        &job_with(
            vec![
                area_of("epoxy", 100.0, 1.0, 1.0),
                area_of("seal", 100.0, 1.0, 1.0),
            ],
            0.35,
        ),
    )
    .unwrap();
    assert_eq!(q.unconfirmed_systems, vec!["epoxy", "seal"]);
    assert!(q.areas.iter().all(|a| !a.system_confirmed));
}

#[test]
fn an_empty_job_prices_to_zero_without_dividing_by_zero() {
    let c = card();
    let q = price_job(&c, &job_with(vec![], 0.35)).unwrap();
    cents(q.cost, 0.0, "cost");
    cents(q.price, 0.0, "price");
    cents(q.profit, 0.0, "profit");
    cents(q.realized_margin(), 0.0, "margin guard, not NaN");
}

// ---------- queryable rates ----------

#[test]
fn per_sf_rates_are_queryable_without_building_a_quote() {
    // Lets a recipe be sanity-checked against a known job.
    let c = card();
    cents(
        material_rate_per_sf(&c, "polyurea").unwrap(),
        1.81071,
        "polyurea material/SF",
    );
    cents(
        consumable_rate_per_sf(&c, "epoxy").unwrap(),
        0.1741,
        "epoxy consumable/SF",
    );
    // Seal's reduced multipliers make its consumable rate much lower.
    assert!(consumable_rate_per_sf(&c, "seal").unwrap() < 0.1741);
    assert!(matches!(
        material_rate_per_sf(&c, "nope"),
        Err(PricingError::UnknownSystem(_))
    ));
}

#[test]
fn material_rate_times_area_equals_the_material_subtotal() {
    // The queryable rate and the full buildup must agree, or the sanity check
    // is worthless.
    let c = card();
    for system in ["seal", "polish", "epoxy", "polyurea"] {
        let q = price_area(&std_ctx(&c), &area_of(system, 1234.0, 2.0, 6.0)).unwrap();
        cents(
            q.materials,
            material_rate_per_sf(&c, system).unwrap() * 1234.0,
            system,
        );
        cents(
            q.consumables,
            consumable_rate_per_sf(&c, system).unwrap() * 1234.0,
            system,
        );
    }
}

// ---------- itemization completeness (what the UI will read) ----------

#[test]
fn subtotals_equal_the_sum_of_their_lines() {
    // The itemization must add up to the totals, or the screen contradicts
    // itself. Nothing is rounded during the buildup, which is what makes this
    // exact rather than approximate.
    let c = card();
    let mut a = area_of("polyurea", 3210.0, 4.0, 9.0);
    a.add_on_keys = vec!["double_broadcast".into(), "h2_out".into()];
    a.quantity_overrides.insert("sand".into(), 0.0);
    let q = price_area(&std_ctx(&c), &a).unwrap();

    let mats: f64 = q
        .lines
        .iter()
        .filter(|l| l.source != LineSource::Consumable)
        .map(|l| l.extended_cost)
        .sum();
    let cons: f64 = q
        .lines
        .iter()
        .filter(|l| l.source == LineSource::Consumable)
        .map(|l| l.extended_cost)
        .sum();
    cents(mats, q.materials, "material lines sum to subtotal");
    cents(cons, q.consumables, "consumable lines sum to subtotal");
    cents(
        q.cost,
        q.materials + q.consumables + q.labor + q.overhead + q.manual_costs,
        "area cost is the sum of its parts",
    );
}

#[test]
fn every_line_carries_what_the_ui_needs_to_render_it() {
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("epoxy", 1000.0, 2.0, 8.0)).unwrap();
    assert!(!q.lines.is_empty());
    for l in &q.lines {
        assert!(!l.product_id.is_empty());
        assert!(!l.product_name.is_empty(), "UI shows the name, not the id");
        cents(l.extended_cost, l.quantity * l.unit_cost, &l.product_id);
        cents(l.quantity, l.base_quantity * l.factor, &l.product_id);
    }
}

// ---------- grit escalator ----------
//
// The whole mechanism ships INERT: every multiplier on the shipped card is
// seeded at 1.0 because the timing data to populate it does not exist yet.
// The tests below are therefore in two halves — the inertness of the seeded
// card, which is what protects today's quotes, and the arithmetic on a card
// with real multipliers, which is what will be true once it is populated.

/// A card whose grit ladder is NOT seeded flat, for testing the escalator
/// itself. Standard is 400; 800 costs a quarter more time, 200 a fifth less.
fn gritted_card() -> RateCard {
    let mut c = card();
    for g in c.grit_levels.iter_mut() {
        g.labor_multiplier = match g.key.as_str() {
            "200" => 0.8,
            "400" => 1.0,
            "800" => 1.25,
            "1200" => 1.5,
            _ => 1.0,
        };
    }
    c
}

fn at_grit(system: &str, sf: f64, crew: f64, hours: f64, grit: &str) -> AreaInput {
    let mut a = area_of(system, sf, crew, hours);
    a.grit_level = Some(grit.into());
    a
}

#[test]
fn the_shipped_grit_ladder_is_seeded_entirely_flat() {
    // If this fails someone has populated a multiplier, and every assertion
    // about prices being unchanged below is no longer about the shipped card.
    let c = card();
    assert!(!c.grit_levels.is_empty(), "the ladder must exist to be used");
    for g in &c.grit_levels {
        assert_eq!(
            g.labor_multiplier, 1.0,
            "grit `{}` is populated at {} — seed values only until real timing \
             data lands, and the inertness tests below assume it",
            g.key, g.labor_multiplier
        );
    }
}

#[test]
fn seeded_grit_leaves_every_number_bit_identical() {
    // THE test this session exists to satisfy: adding the grit field may not
    // move a quote by a cent. Exact f64 equality, not a cent tolerance --
    // x * 1.0 == x in IEEE-754, so "unchanged" is provable rather than
    // approximate, and a tolerance would hide a multiplier that had drifted.
    let c = card();
    let base = price_job(&c, &job_with(vec![area_of("polish", 3000.0, 3.0, 9.0)], 0.35)).unwrap();

    for grit in ["100", "200", "400", "800", "1200", "1500", "3000"] {
        let with = price_job(
            &c,
            &job_with(vec![at_grit("polish", 3000.0, 3.0, 9.0, grit)], 0.35),
        )
        .unwrap();
        assert_eq!(with.cost, base.cost, "{grit}: cost moved");
        assert_eq!(with.price, base.price, "{grit}: price moved");
        assert_eq!(with.profit, base.profit, "{grit}: profit moved");
        assert_eq!(with.margin, base.margin, "{grit}: margin moved");
        assert_eq!(with.man_hours, base.man_hours, "{grit}: hours moved");
        assert_eq!(with.areas[0].labor, base.areas[0].labor, "{grit}: labor");
        assert_eq!(
            with.areas[0].overhead, base.areas[0].overhead,
            "{grit}: overhead"
        );
        assert_eq!(with.areas[0].grit_labor_multiplier, 1.0);
        // Only the echoed-back field differs, which is the point of a hook.
        assert_eq!(with.areas[0].grit_level.as_deref(), Some(grit));
    }
}

#[test]
fn an_area_at_no_grit_reports_the_identity_multiplier() {
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("polish", 1000.0, 2.0, 8.0)).unwrap();
    assert_eq!(q.grit_level, None);
    assert_eq!(q.grit_labor_multiplier, 1.0);
    assert_eq!(q.base_man_hours, q.man_hours, "nothing escalated");
}

#[test]
fn a_populated_escalator_scales_hours_relative_to_the_systems_standard() {
    let c = gritted_card();
    // Polish's standard is 400, so 400 is exactly 1.0 no matter what figure
    // the level carries -- it is the baseline the hours were entered against.
    let at_standard = price_area(&std_ctx(&c), &at_grit("polish", 3000.0, 3.0, 10.0, "400")).unwrap();
    cents(at_standard.grit_labor_multiplier, 1.0, "standard is unity");
    cents(at_standard.man_hours, 30.0, "hours as entered");

    // 800 is 1.25 / 1.0 = 1.25x the time.
    let up = price_area(&std_ctx(&c), &at_grit("polish", 3000.0, 3.0, 10.0, "800")).unwrap();
    cents(up.grit_labor_multiplier, 1.25, "800 against a 400 standard");
    cents(up.base_man_hours, 30.0, "entered hours are preserved");
    cents(up.man_hours, 37.5, "escalated hours");

    // A finer spec than standard can also mean LESS time, and nothing forbids it.
    let down = price_area(&std_ctx(&c), &at_grit("polish", 3000.0, 3.0, 10.0, "200")).unwrap();
    cents(down.grit_labor_multiplier, 0.8, "200 against a 400 standard");
    cents(down.man_hours, 24.0, "de-escalated hours");
}

#[test]
fn the_escalator_is_a_ratio_so_a_systems_own_standard_never_double_counts() {
    // The failure this design exists to prevent: seal's standard is 100 and
    // polish's is 400. If the multiplier were applied ABSOLUTELY, quoting
    // seal at its own standard would multiply hours that already describe
    // 100-grit work. Both systems must come out at exactly 1.0 at their own
    // standard despite carrying different figures.
    let mut c = gritted_card();
    // Give the two systems different standards AND different figures at them.
    c.grit_levels
        .iter_mut()
        .find(|g| g.key == "100")
        .expect("100 is on the shipped ladder")
        .labor_multiplier = 0.5;
    let seal = price_area(&std_ctx(&c), &at_grit("seal", 2000.0, 2.0, 6.0, "100")).unwrap();
    let polish = price_area(&std_ctx(&c), &at_grit("polish", 2000.0, 2.0, 6.0, "400")).unwrap();
    cents(seal.grit_labor_multiplier, 1.0, "seal at its own standard");
    cents(polish.grit_labor_multiplier, 1.0, "polish at its own standard");
    cents(seal.man_hours, 12.0, "seal hours unescalated");
    cents(polish.man_hours, 12.0, "polish hours unescalated");

    // And the SAME level means different things to the two systems, because
    // they start from different baselines: 800 is 1.25/0.5 = 2.5x for seal
    // but 1.25/1.0 = 1.25x for polish.
    let seal_800 = price_area(&std_ctx(&c), &at_grit("seal", 2000.0, 2.0, 6.0, "800")).unwrap();
    let polish_800 = price_area(&std_ctx(&c), &at_grit("polish", 2000.0, 2.0, 6.0, "800")).unwrap();
    cents(seal_800.grit_labor_multiplier, 2.5, "800 from a 100 baseline");
    cents(polish_800.grit_labor_multiplier, 1.25, "800 from a 400 baseline");
}

#[test]
fn escalated_hours_flow_into_labor_overhead_and_productivity_with_no_special_casing() {
    // Routing grit through HOURS rather than cost is what buys this. Nothing
    // downstream knows grit exists.
    let c = gritted_card();
    let base = price_area(&std_ctx(&c), &at_grit("polish", 3000.0, 3.0, 10.0, "400")).unwrap();
    let up = price_area(&std_ctx(&c), &at_grit("polish", 3000.0, 3.0, 10.0, "800")).unwrap();

    cents(up.labor, base.labor * 1.25, "labor scaled with hours");
    cents(up.overhead, base.overhead * 1.25, "overhead scaled with hours");
    cents(
        up.sf_per_man_hour.unwrap(),
        3000.0 / 37.5,
        "productivity reflects escalated hours, not entered ones",
    );
    // Materials do NOT move: more passes is more time, not more product.
    cents(up.materials, base.materials, "materials untouched by grit");
    cents(up.consumables, base.consumables, "consumables untouched");
}

#[test]
fn a_grit_on_a_system_that_does_not_grind_is_an_error_not_a_silent_no_op() {
    // Epoxy has no standard_grit. Accepting and ignoring the field would make
    // "I selected 1200 grit and the price did not move" indistinguishable
    // from the seeded-flat case, which is exactly the confusion to avoid.
    let c = card();
    let err = price_area(&std_ctx(&c), &at_grit("epoxy", 1000.0, 2.0, 8.0, "800")).unwrap_err();
    match err {
        PricingError::GritNotApplicable {
            ref system,
            ref grit,
            ..
        } => {
            assert_eq!(system, "epoxy");
            assert_eq!(grit, "800");
        }
        other => panic!("expected GritNotApplicable, got {other}"),
    }
    assert!(
        err.to_string().contains("no grinding"),
        "the message must say why: {err}"
    );
}

#[test]
fn an_unknown_grit_level_names_itself_rather_than_pricing_at_par() {
    let c = card();
    let err = price_area(&std_ctx(&c), &at_grit("polish", 1000.0, 2.0, 8.0, "9999")).unwrap_err();
    assert!(
        matches!(err, PricingError::UnknownGritLevel { ref grit, .. } if grit == "9999"),
        "got {err}"
    );
}

// ---------- SF per man-hour ----------

#[test]
fn productivity_is_reported_per_area_and_blended_for_the_job() {
    let c = card();
    // 3000 SF at 3 crew x 10 h = 30 man-hours = 100 SF/mh (implausibly fast).
    // 1000 SF at 2 crew x 40 h = 80 man-hours = 12.5 SF/mh (typical-ish).
    let job = job_with(
        vec![
            area_of("polish", 3000.0, 3.0, 10.0),
            {
                let mut a = area_of("epoxy", 1000.0, 2.0, 40.0);
                a.id = 2;
                a
            },
        ],
        0.35,
    );
    let q = price_job(&c, &job).unwrap();
    cents(q.areas[0].sf_per_man_hour.unwrap(), 100.0, "area 1");
    cents(q.areas[1].sf_per_man_hour.unwrap(), 12.5, "area 2");

    // Blended is total/total, NOT the mean of 100 and 12.5 (=56.25). The big
    // slow area dominates, which is the honest reading of the job.
    cents(q.man_hours, 110.0, "job man-hours");
    cents(q.area_sf, 4000.0, "job square footage");
    cents(q.sf_per_man_hour.unwrap(), 4000.0 / 110.0, "blended");
    assert!(
        (q.sf_per_man_hour.unwrap() - 56.25).abs() > 1.0,
        "blended must not be the mean of the per-area figures"
    );
}

#[test]
fn the_productivity_band_flags_both_directions_and_blocks_neither() {
    let c = card();
    // 1554 SF at 150 man-hours = 10.4 SF/mh: the real quote that came out
    // 2.3x high. It is inside the 8-60 band, so the band alone would not have
    // caught it -- the READOUT is what does, by putting 10.4 next to a median
    // of 22. The band only catches the wilder cases.
    let real = price_area(&std_ctx(&c), &area_of("polish", 1554.0, 3.0, 50.0)).unwrap();
    cents(real.sf_per_man_hour.unwrap(), 10.36, "the real job");
    assert_eq!(real.productivity, Productivity::Typical);

    // 500 SF at 2 x 40 = 80 mh = 6.25 SF/mh: below the band.
    let slow = price_area(&std_ctx(&c), &area_of("polish", 500.0, 2.0, 40.0)).unwrap();
    assert_eq!(slow.productivity, Productivity::Low);
    // 10000 SF at 1 x 8 = 8 mh = 1250 SF/mh: above it.
    let fast = price_area(&std_ctx(&c), &area_of("polish", 10000.0, 1.0, 8.0)).unwrap();
    assert_eq!(fast.productivity, Productivity::High);

    // Advisory ONLY: both still priced, and neither is an error.
    assert!(slow.cost > 0.0 && fast.cost > 0.0);
}

#[test]
fn the_band_edges_are_inclusive_and_named() {
    cents(SF_PER_MAN_HOUR_MIN, 8.0, "band floor");
    cents(SF_PER_MAN_HOUR_MAX, 60.0, "band ceiling");
    cents(SF_PER_MAN_HOUR_TYPICAL, 22.0, "historical median");
    let c = card();
    // Exactly on an edge is INSIDE the band -- "roughly 8-60" should not fire
    // on the boundary itself.
    let at_min = price_area(&std_ctx(&c), &area_of("polish", 80.0, 1.0, 10.0)).unwrap();
    cents(at_min.sf_per_man_hour.unwrap(), 8.0, "on the floor");
    assert_eq!(at_min.productivity, Productivity::Typical);
    let at_max = price_area(&std_ctx(&c), &area_of("polish", 600.0, 1.0, 10.0)).unwrap();
    cents(at_max.sf_per_man_hour.unwrap(), 60.0, "on the ceiling");
    assert_eq!(at_max.productivity, Productivity::Typical);
}

#[test]
fn zero_hours_yields_no_ratio_rather_than_infinity() {
    // Hours are routinely empty mid-entry. Infinity or NaN beside a dollar
    // figure reads as a broken quote rather than an unfinished one.
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("polish", 1000.0, 0.0, 0.0)).unwrap();
    assert_eq!(q.man_hours, 0.0);
    assert_eq!(q.sf_per_man_hour, None);
    assert_eq!(q.productivity, Productivity::Unknown);

    // And at the job level, where every area has no hours.
    let job = price_job(&c, &job_with(vec![area_of("polish", 1000.0, 0.0, 0.0)], 0.35)).unwrap();
    assert_eq!(job.sf_per_man_hour, None);
    assert_eq!(job.productivity, Productivity::Unknown);
}

#[test]
fn a_zero_square_foot_area_is_a_ratio_of_zero_not_a_missing_one() {
    // Distinct from the case above: there ARE hours, so the ratio is real and
    // is legitimately 0.0. Flagged Low, which is correct -- hours booked
    // against no floor is exactly the kind of thing worth noticing.
    let c = card();
    let q = price_area(&std_ctx(&c), &area_of("polish", 0.0, 2.0, 4.0)).unwrap();
    cents(q.sf_per_man_hour.unwrap(), 0.0, "zero SF over real hours");
    assert_eq!(q.productivity, Productivity::Low);
}

// ---------- bid items ----------
//
// The layer between takeoff units and what a customer buys. An area is a
// measurement; a bid item is a line on the proposal, and the two are not the
// same shape: a bathroom on page 7 and a corridor on page 22 may be one
// "Restrooms" line at one price.

fn area_id(id: u64, system: &str, sf: f64, crew: f64, hours: f64) -> AreaInput {
    let mut a = area_of(system, sf, crew, hours);
    a.id = id;
    a.name = format!("{system} {id}");
    a
}

fn item(id: u64, name: &str, area_ids: &[u64], alternate: bool) -> BidItem {
    BidItem {
        id,
        name: name.into(),
        area_ids: area_ids.to_vec(),
        alternate,
    }
}

fn job_grouped(areas: Vec<AreaInput>, items: Vec<BidItem>, margin: f64) -> JobInput {
    let mut j = job_with(areas, margin);
    j.bid_items = items;
    j
}

#[test]
fn a_job_that_names_no_bid_items_gets_one_per_area() {
    // The compatibility guarantee: every project saved before this layer
    // existed deserializes with an empty bid_items and must price identically.
    let c = card();
    let q = price_job(
        &c,
        &job_with(
            vec![
                area_id(1, "epoxy", 1000.0, 2.0, 8.0),
                area_id(2, "polish", 2000.0, 3.0, 10.0),
            ],
            0.35,
        ),
    )
    .unwrap();

    assert_eq!(q.bid_items.len(), 2, "one item per area, synthesized");
    for (b, a) in q.bid_items.iter().zip(&q.areas) {
        assert_eq!(b.area_ids, vec![a.area_id]);
        assert_eq!(b.name, a.name, "named from the area it came from");
        assert!(!b.alternate, "nothing is an alternate by default");
        cents(b.cost, a.cost, "a lone member's cost IS the item's cost");
    }
    // And the job figures are untouched by the grouping layer.
    cents(
        q.area_cost,
        q.areas.iter().map(|a| a.cost).sum::<f64>(),
        "area cost",
    );
}

#[test]
fn a_single_member_item_prices_identically_to_the_area_ungrouped() {
    let c = card();
    let areas = vec![area_id(1, "epoxy", 1000.0, 2.0, 8.0)];
    let loose = price_job(&c, &job_with(areas.clone(), 0.35)).unwrap();
    let named = price_job(
        &c,
        &job_grouped(areas, vec![item(9, "Front lobby", &[1], false)], 0.35),
    )
    .unwrap();

    // Naming a line of work is a labelling act, not a pricing one. Exact
    // equality: renaming must not move a cent.
    assert_eq!(named.cost, loose.cost);
    assert_eq!(named.price, loose.price);
    assert_eq!(named.profit, loose.profit);
    assert_eq!(named.bid_items[0].price, loose.bid_items[0].price);
    assert_eq!(named.bid_items[0].name, "Front lobby");
    assert_eq!(loose.bid_items[0].name, "epoxy 1", "defaulted from the area");
}

#[test]
fn a_bid_items_total_is_the_sum_of_its_members_across_pages() {
    // Members are not page-scoped -- that is the entire point. Nothing in the
    // pricing layer knows what page an area came from.
    let c = card();
    let areas = vec![
        area_id(1, "polish", 400.0, 2.0, 6.0),
        area_id(2, "polish", 250.0, 1.0, 4.0),
        area_id(3, "epoxy", 1000.0, 2.0, 8.0),
    ];
    let q = price_job(
        &c,
        &job_grouped(
            areas,
            vec![
                item(10, "Restrooms", &[1, 2], false),
                item(11, "Warehouse", &[3], false),
            ],
            0.35,
        ),
    )
    .unwrap();

    let restrooms = &q.bid_items[0];
    let a1 = q.areas.iter().find(|a| a.area_id == 1).unwrap();
    let a2 = q.areas.iter().find(|a| a.area_id == 2).unwrap();
    cents(restrooms.cost, a1.cost + a2.cost, "lump sum cost");
    cents(restrooms.area_sf, 650.0, "square footage rolls up");
    cents(restrooms.man_hours, 12.0 + 4.0, "so do man-hours");
    assert_eq!(restrooms.system_key, "polish");
    // The lump sum is the members' cost at the job margin.
    cents(
        restrooms.price,
        restrooms.cost / (1.0 - 0.35),
        "priced at the job margin like anything else",
    );
    cents(
        restrooms.price - restrooms.cost,
        restrooms.profit,
        "profit closes",
    );
}

#[test]
fn mixing_systems_in_one_bid_item_is_rejected_by_name() {
    // One lump sum carries one scope narrative. A mixed item would hide an
    // epoxy area inside a grind-and-seal description, and no number on the
    // page would reveal it.
    let c = card();
    let err = price_job(
        &c,
        &job_grouped(
            vec![
                area_id(1, "polish", 400.0, 2.0, 6.0),
                area_id(2, "epoxy", 400.0, 2.0, 6.0),
            ],
            vec![item(10, "Ground floor", &[1, 2], false)],
            0.35,
        ),
    )
    .unwrap_err();

    match err {
        PricingError::BidItemMixedSystems {
            ref bid_item,
            ref systems,
        } => {
            assert_eq!(bid_item, "Ground floor");
            assert_eq!(systems, "epoxy, polish", "both named, sorted");
        }
        other => panic!("expected BidItemMixedSystems, got {other}"),
    }
    assert!(
        err.to_string().contains("scope narrative"),
        "the message must say WHY, not just refuse: {err}"
    );
}

#[test]
fn an_area_may_belong_to_only_one_bid_item() {
    let c = card();
    let err = price_job(
        &c,
        &job_grouped(
            vec![area_id(1, "polish", 400.0, 2.0, 6.0)],
            vec![
                item(10, "Restrooms", &[1], false),
                item(11, "Corridors", &[1], false),
            ],
            0.35,
        ),
    )
    .unwrap_err();
    match err {
        PricingError::AreaInTwoBidItems {
            ref first,
            ref second,
            ..
        } => {
            assert_eq!(first, "Restrooms");
            assert_eq!(second, "Corridors", "both claimants named");
        }
        other => panic!("expected AreaInTwoBidItems, got {other}"),
    }
}

#[test]
fn a_bid_item_that_is_empty_or_names_a_missing_area_is_rejected() {
    let c = card();
    let areas = vec![area_id(1, "polish", 400.0, 2.0, 6.0)];

    let empty = price_job(
        &c,
        &job_grouped(areas.clone(), vec![item(10, "Nothing", &[], false)], 0.35),
    )
    .unwrap_err();
    assert!(matches!(empty, PricingError::EmptyBidItem { .. }), "{empty}");

    // The dangling-reference case: an area deleted out from under an item.
    let dangling = price_job(
        &c,
        &job_grouped(areas, vec![item(10, "Ghost", &[1, 77], false)], 0.35),
    )
    .unwrap_err();
    assert!(
        matches!(dangling, PricingError::BidItemUnknownArea { area_id: 77, .. }),
        "{dangling}"
    );
}

#[test]
fn duplicate_bid_item_ids_are_rejected() {
    let c = card();
    let err = price_job(
        &c,
        &job_grouped(
            vec![
                area_id(1, "polish", 400.0, 2.0, 6.0),
                area_id(2, "polish", 400.0, 2.0, 6.0),
            ],
            vec![item(10, "A", &[1], false), item(10, "B", &[2], false)],
            0.35,
        ),
    )
    .unwrap_err();
    assert!(matches!(err, PricingError::DuplicateBidItem(10)), "{err}");
}

#[test]
fn partially_grouped_jobs_synthesize_items_only_for_the_leftovers() {
    let c = card();
    let q = price_job(
        &c,
        &job_grouped(
            vec![
                area_id(1, "polish", 400.0, 2.0, 6.0),
                area_id(2, "polish", 250.0, 1.0, 4.0),
                area_id(3, "epoxy", 1000.0, 2.0, 8.0),
            ],
            vec![item(10, "Restrooms", &[1, 2], false)],
            0.35,
        ),
    )
    .unwrap();
    assert_eq!(q.bid_items.len(), 2);
    assert_eq!(q.bid_items[0].name, "Restrooms", "explicit items come first");
    assert_eq!(q.bid_items[1].area_ids, vec![3], "the leftover got its own");
    assert!(
        q.bid_items[1].id > 10,
        "synthesized ids are allocated above every explicit one so they cannot collide"
    );
    // Every area is in exactly one item, always.
    let mut covered: Vec<u64> = q.bid_items.iter().flat_map(|b| b.area_ids.clone()).collect();
    covered.sort_unstable();
    assert_eq!(covered, vec![1, 2, 3], "no orphans, no duplicates");
}

// ---------- alternates ----------

#[test]
fn an_alternate_is_priced_but_excluded_from_the_base_bid() {
    let c = card();
    let areas = vec![
        area_id(1, "epoxy", 1000.0, 2.0, 8.0),
        area_id(2, "polish", 500.0, 2.0, 5.0),
    ];
    let base_only = price_job(
        &c,
        &job_grouped(
            areas.clone(),
            vec![item(10, "Warehouse", &[1], false)],
            0.35,
        ),
    )
    .unwrap();
    // ^ area 2 is synthesized as a BASE item here.

    let with_alt = price_job(
        &c,
        &job_grouped(
            areas,
            vec![
                item(10, "Warehouse", &[1], false),
                item(11, "Add: polished office", &[2], true),
            ],
            0.35,
        ),
    )
    .unwrap();

    let alt = with_alt.bid_items.iter().find(|b| b.alternate).unwrap();
    // The base total drops by EXACTLY the alternate's price.
    cents(
        with_alt.price,
        base_only.price - alt.price,
        "flagging an item as an alternate removes exactly its lump sum",
    );
    cents(with_alt.alternate_price, alt.price, "and reports it apart");
    cents(with_alt.alternate_cost, alt.cost, "cost too");
    cents(with_alt.alternate_profit, alt.profit, "and profit");
    // It is still fully priced -- an alternate is an offer, not a stub.
    assert!(alt.price > 0.0 && alt.cost > 0.0);
    assert!(
        !with_alt.areas.iter().any(|a| a.area_id == 2 && a.cost == 0.0),
        "the alternate's area is priced in full"
    );
}

#[test]
fn margin_applies_uniformly_to_base_and_alternate_items() {
    let c = card();
    let q = price_job(
        &c,
        &job_grouped(
            vec![
                area_id(1, "epoxy", 1000.0, 2.0, 8.0),
                area_id(2, "polish", 500.0, 2.0, 5.0),
                area_id(3, "seal", 800.0, 2.0, 4.0),
            ],
            vec![
                item(10, "Base", &[1], false),
                item(11, "Alt A", &[2], true),
                item(12, "Alt B", &[3], true),
            ],
            0.35,
        ),
    )
    .unwrap();

    // Every item, base or alternate, round-trips the SAME margin.
    for b in &q.bid_items {
        assert!(
            (b.profit / b.price - 0.35).abs() < 1e-12,
            "{} realized {} not 0.35",
            b.name,
            b.profit / b.price
        );
    }
    // And so does the base bid as a whole.
    cents(q.realized_margin(), 0.35, "base bid margin");
    cents(
        q.alternate_profit / q.alternate_price,
        0.35,
        "alternates blended margin",
    );
}

#[test]
fn job_costs_sit_in_the_base_bid_and_the_lump_sums_add_up() {
    // An alternate must not carry a share of mobilization: whether it is
    // accepted cannot change what the base bid costs to mobilize.
    let c = card();
    let mut job = job_grouped(
        vec![
            area_id(1, "epoxy", 1000.0, 2.0, 8.0),
            area_id(2, "polish", 500.0, 2.0, 5.0),
        ],
        vec![
            item(10, "Warehouse", &[1], false),
            item(11, "Add: office", &[2], true),
        ],
        0.35,
    );
    job.job_costs = JobCosts {
        travel: 250.0,
        permits: 100.0,
        equipment: 0.0,
        mobilization: 400.0,
    };
    let q = price_job(&c, &job).unwrap();

    let base_sum: f64 = q
        .bid_items
        .iter()
        .filter(|b| !b.alternate)
        .map(|b| b.price)
        .sum();
    // THE PROPOSAL ARITHMETIC: lump sums plus marked-up job costs is the price.
    cents(
        base_sum + q.job_cost_price,
        q.price,
        "base lump sums + marked-up job costs = the quoted price",
    );
    cents(q.job_cost_price, 750.0 / (1.0 - 0.35), "job costs marked up");
    // The alternate's lump sum carries none of it.
    let alt = q.bid_items.iter().find(|b| b.alternate).unwrap();
    let a2 = q.areas.iter().find(|a| a.area_id == 2).unwrap();
    cents(alt.cost, a2.cost, "alternate cost is its areas, nothing else");
}

#[test]
fn base_bid_productivity_excludes_alternate_hours() {
    // The headline describes the base bid, so the rate beside it must too --
    // blending in hours for work that may never happen describes a job nobody
    // is going to run.
    let c = card();
    let q = price_job(
        &c,
        &job_grouped(
            vec![
                area_id(1, "epoxy", 1000.0, 2.0, 8.0),  // 16 mh
                area_id(2, "polish", 500.0, 10.0, 10.0), // 100 mh, absurdly slow
            ],
            vec![
                item(10, "Base", &[1], false),
                item(11, "Alt", &[2], true),
            ],
            0.35,
        ),
        )
    .unwrap();
    cents(q.man_hours, 16.0, "base hours only");
    cents(q.area_sf, 1000.0, "base square footage only");
    cents(q.sf_per_man_hour.unwrap(), 62.5, "base bid rate");
    // The alternate's own area still carries its (implausible) readout.
    let a2 = q.areas.iter().find(|a| a.area_id == 2).unwrap();
    cents(a2.sf_per_man_hour.unwrap(), 5.0, "per-area readout unaffected");
    assert_eq!(a2.productivity, Productivity::Low);
}

#[test]
fn a_job_that_is_entirely_alternates_has_a_base_bid_of_just_its_job_costs() {
    // Degenerate but reachable: everything optional. The base must not become
    // NaN or silently absorb the alternates.
    let c = card();
    let mut job = job_grouped(
        vec![area_id(1, "epoxy", 1000.0, 2.0, 8.0)],
        vec![item(10, "Everything optional", &[1], true)],
        0.35,
    );
    job.job_costs = JobCosts {
        travel: 100.0,
        ..JobCosts::default()
    };
    let q = price_job(&c, &job).unwrap();
    cents(q.area_cost, 0.0, "no base areas");
    cents(q.cost, 100.0, "just the job costs");
    cents(q.price, 100.0 / 0.65, "priced normally");
    assert!(q.alternate_price > 0.0, "the alternate is still quoted");
    assert_eq!(q.sf_per_man_hour, None, "no base work to rate");
    assert_eq!(q.productivity, Productivity::Unknown);
}

// ---------- mixed grit ----------

#[test]
fn a_bid_item_mixing_grit_levels_prices_correctly_and_says_so() {
    // Grouping is ALLOWED -- each area carries its own hours, so the sum is
    // right. It is flagged because one lump sum under one grit-bearing name
    // would describe work that is not what was priced, and no number on the
    // page reveals that.
    let c = card();
    let mut a1 = area_id(1, "polish", 400.0, 2.0, 6.0);
    a1.grit_level = Some("400".into());
    let mut a2 = area_id(2, "polish", 250.0, 1.0, 4.0);
    a2.grit_level = Some("800".into());

    let q = price_job(
        &c,
        &job_grouped(
            vec![a1, a2],
            vec![item(10, "Showroom", &[1, 2], false)],
            0.35,
        ),
    )
    .unwrap();
    let b = &q.bid_items[0];
    assert!(b.mixed_grit, "flagged");
    assert_eq!(b.grit_levels, vec!["400", "800"], "both levels named");
    let sum: f64 = q.areas.iter().map(|a| a.cost).sum();
    cents(b.cost, sum, "still exactly the sum of its members");
}

#[test]
fn one_grit_across_every_member_is_not_mixed() {
    let c = card();
    let mut a1 = area_id(1, "polish", 400.0, 2.0, 6.0);
    a1.grit_level = Some("800".into());
    let mut a2 = area_id(2, "polish", 250.0, 1.0, 4.0);
    a2.grit_level = Some("800".into());
    let q = price_job(
        &c,
        &job_grouped(vec![a1, a2], vec![item(10, "Showroom", &[1, 2], false)], 0.35),
    )
    .unwrap();
    assert!(!q.bid_items[0].mixed_grit);
    assert_eq!(q.bid_items[0].grit_levels, vec!["800"]);

    // Neither is no grit at all, which is the overwhelmingly common case.
    let plain = price_job(
        &c,
        &job_grouped(
            vec![area_id(1, "polish", 400.0, 2.0, 6.0), area_id(2, "polish", 250.0, 1.0, 4.0)],
            vec![item(10, "Showroom", &[1, 2], false)],
            0.35,
        ),
    )
    .unwrap();
    assert!(!plain.bid_items[0].mixed_grit);
    assert!(plain.bid_items[0].grit_levels.is_empty());
}

#[test]
fn a_grit_specified_on_only_some_members_is_mixed_too() {
    // The subtle one: "800 grit" on one area and nothing on the other is
    // still two different specifications sharing a name.
    let c = card();
    let mut a1 = area_id(1, "polish", 400.0, 2.0, 6.0);
    a1.grit_level = Some("800".into());
    let a2 = area_id(2, "polish", 250.0, 1.0, 4.0);
    let q = price_job(
        &c,
        &job_grouped(vec![a1, a2], vec![item(10, "Showroom", &[1, 2], false)], 0.35),
    )
    .unwrap();
    assert!(
        q.bid_items[0].mixed_grit,
        "one member specified and one not is a mixed specification"
    );
}

// ---------- scope of work ----------
//
// The narrative is built from the SAME resolved lines the price is. That is
// the whole design: a scope kept in a separate list drifts from the quote it
// describes the first time someone suppresses a line.

fn scope_text(q: &engine_core::pricing::JobQuote, item: usize) -> Vec<String> {
    q.bid_items[item]
        .scope
        .iter()
        .map(|s| s.text.clone())
        .collect()
}

#[test]
fn the_scope_bookends_product_steps_with_the_systems_own_text() {
    let c = card();
    let q = price_job(&c, &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    let steps = &q.bid_items[0].scope;
    assert!(steps.len() > 4, "a real narrative, got {}", steps.len());

    // Mobilisation and cleanup belong to no product -- they bookend, and they
    // carry no product_ids, which is how a UI can tell them apart.
    assert!(steps[0].text.contains("Mobilise"), "{:?}", steps[0].text);
    assert!(steps[0].product_ids.is_empty(), "intro belongs to no product");
    let last = steps.last().unwrap();
    assert!(last.text.contains("demobilise"), "{:?}", last.text);
    assert!(last.product_ids.is_empty(), "outro belongs to no product");

    // Everything in between came from a product.
    for s in &steps[1..steps.len() - 1] {
        if s.text.contains("Diamond grind") {
            continue; // the other intro line
        }
        assert!(!s.product_ids.is_empty(), "orphan step: {}", s.text);
    }
}

#[test]
fn suppressing_the_mender_drops_the_joint_repair_step() {
    // THE requirement: delete mender on a new slab and the joint-repair
    // sentence goes with it, with no second list to keep in sync.
    let c = card();
    let with = price_job(&c, &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    assert!(
        scope_text(&with, 0).iter().any(|t| t.contains("Repair spalls")),
        "the mender step should be there to begin with: {:?}",
        scope_text(&with, 0)
    );

    let mut a = area_of("polish", 2000.0, 3.0, 9.0);
    a.quantity_overrides.insert("mender_part_a".into(), 0.0);
    a.quantity_overrides.insert("mender_part_b".into(), 0.0);
    let without = price_job(&c, &job_with(vec![a], 0.35)).unwrap();
    assert!(
        !scope_text(&without, 0).iter().any(|t| t.contains("Repair spalls")),
        "suppressed lines must not narrate work nobody is doing: {:?}",
        scope_text(&without, 0)
    );
    // And nothing else moved.
    assert_eq!(
        scope_text(&with, 0).len() - 1,
        scope_text(&without, 0).len(),
        "exactly one step should have gone"
    );
}

#[test]
fn a_two_part_product_collapses_into_one_step() {
    // Mender Part A and Part B are two catalog lines and one thing a crew
    // does. They share a sentence, so they share a step.
    let c = card();
    let q = price_job(&c, &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    let repair: Vec<_> = q.bid_items[0]
        .scope
        .iter()
        .filter(|s| s.text.contains("Repair spalls"))
        .collect();
    assert_eq!(repair.len(), 1, "one step, not two");
    assert_eq!(
        repair[0].product_ids,
        vec!["mender_part_a", "mender_part_b"],
        "and it names both lines it came from"
    );
}

#[test]
fn steps_follow_work_sequence_not_catalog_order() {
    // The polish recipe lists the sealer BEFORE the mender. The narrative must
    // not: joints are repaired before the floor is sealed.
    let c = card();
    let recipe = &c.system("polish").unwrap().product_ids;
    let seal_first = recipe.iter().position(|p| p == "cure_and_seal_first").unwrap();
    let mender_first = recipe.iter().position(|p| p == "mender_part_a").unwrap();
    assert!(seal_first < mender_first, "premise: the recipe is in catalog order");

    let q = price_job(&c, &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    let texts = scope_text(&q, 0);
    let at = |needle: &str| texts.iter().position(|t| t.contains(needle)).unwrap();
    assert!(at("Repair spalls") < at("penetrating cure and seal"), "{texts:?}");
    assert!(at("control joints") < at("lithium densifier"), "{texts:?}");
    assert!(at("lithium densifier") < at("first coat"), "{texts:?}");
    assert!(at("first coat") < at("second coat"), "{texts:?}");
}

#[test]
fn consumables_never_appear_in_the_scope() {
    // Brushes and rags are not work a customer is buying.
    let c = card();
    let q = price_job(&c, &job_with(vec![area_of("epoxy", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    let ids: Vec<&str> = q.bid_items[0]
        .scope
        .iter()
        .flat_map(|s| s.product_ids.iter().map(|s| s.as_str()))
        .collect();
    for consumable in ["brushes", "rags", "gloves", "roller_covers", "trash_bags"] {
        assert!(!ids.contains(&consumable), "{consumable} is not scope");
    }
}

#[test]
fn an_add_on_contributes_its_own_step() {
    let c = card();
    let mut a = area_of("polyurea", 1000.0, 2.0, 8.0);
    a.add_on_keys.push("double_broadcast".into());
    let q = price_job(&c, &job_with(vec![a], 0.35)).unwrap();
    assert!(
        scope_text(&q, 0).iter().any(|t| t.contains("second full coat of flake")),
        "the add-on's work must be described: {:?}",
        scope_text(&q, 0)
    );
}

#[test]
fn a_bid_item_narrates_the_union_of_its_members_not_the_intersection() {
    // One lump sum has to describe everything under it, including an add-on
    // that only one member carries.
    let c = card();
    let plain = area_id(1, "polyurea", 800.0, 2.0, 6.0);
    let mut fancy = area_id(2, "polyurea", 600.0, 2.0, 6.0);
    fancy.add_on_keys.push("double_broadcast".into());

    let q = price_job(
        &c,
        &job_grouped(
            vec![plain, fancy],
            vec![item(10, "Warehouse", &[1, 2], false)],
            0.35,
        ),
    )
    .unwrap();
    let texts = scope_text(&q, 0);
    assert!(texts.iter().any(|t| t.contains("polyurea base coat")), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("second full coat of flake")),
        "the one member's add-on is still work under this lump sum: {texts:?}"
    );
    // Deduplicated across members: the base coat is described once, not twice.
    assert_eq!(
        texts.iter().filter(|t| t.contains("polyurea base coat")).count(),
        1,
        "{texts:?}"
    );
}

#[test]
fn every_system_produces_a_narrative_and_an_alternate_gets_one_too() {
    let c = card();
    for system in ["polish", "seal", "epoxy", "polyurea"] {
        let q = price_job(&c, &job_with(vec![area_of(system, 1000.0, 2.0, 8.0)], 0.35)).unwrap();
        let steps = &q.bid_items[0].scope;
        assert!(steps.len() >= 4, "{system}: only {} steps", steps.len());
        assert!(
            steps.iter().all(|s| !s.text.trim().is_empty()),
            "{system}: an empty sentence would print as a blank bullet"
        );
    }
    // An alternate is quoted work like any other; it needs its scope.
    let q = price_job(
        &c,
        &job_grouped(
            vec![area_id(1, "epoxy", 1000.0, 2.0, 8.0)],
            vec![item(10, "Add: epoxy the office", &[1], true)],
            0.35,
        ),
    )
    .unwrap();
    assert!(!q.bid_items[0].scope.is_empty(), "an alternate needs a scope too");
}

#[test]
fn a_card_with_no_scope_text_yields_no_scope_rather_than_failing() {
    // Every card written before scope existed. Pricing is unaffected; the
    // narrative is simply empty and the UI has nothing to pre-fill.
    let mut c = card();
    for p in c.products.iter_mut() {
        p.scope_line = None;
    }
    for s in c.systems.iter_mut() {
        s.scope_intro.clear();
        s.scope_outro.clear();
    }
    let bare = price_job(&c, &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    let full = price_job(&card(), &job_with(vec![area_of("polish", 2000.0, 3.0, 9.0)], 0.35)).unwrap();
    assert!(bare.bid_items[0].scope.is_empty());
    assert_eq!(bare.price, full.price, "scope text is not priced");
}

#[test]
fn a_step_survives_while_any_of_its_products_does() {
    // Both mender parts share a sentence, so the step is present if EITHER is
    // being applied. That is the right reading: the step describes work, and
    // the work is happening while any product behind it is.
    let c = card();
    let mut a = area_of("polish", 2000.0, 3.0, 9.0);
    a.quantity_overrides.insert("mender_part_a".into(), 0.0);
    let half = price_job(&c, &job_with(vec![a], 0.35)).unwrap();
    let step = half.bid_items[0]
        .scope
        .iter()
        .find(|s| s.text.contains("Repair spalls"))
        .expect("part B is still being applied, so the repair is still described");
    assert_eq!(
        step.product_ids,
        vec!["mender_part_b"],
        "and the step names only what actually survived"
    );
}
