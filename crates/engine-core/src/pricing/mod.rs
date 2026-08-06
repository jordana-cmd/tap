//! Pricing catalog types — the SHAPE of a rate card, never its values.
//!
//! # Why no numbers live here
//!
//! `engine-core` is a measurement kernel. Baking material costs into a
//! compiled artifact would mean a `wasm-pack` build and a redeploy every time
//! a supplier raises a price, so the rates live in a JSON data file the HOST
//! loads at runtime (`data/rate-cards/*.json`) and hands in as a parameter.
//! This module defines the types, deserializes them (behind the `serde`
//! feature), and validates the result. It computes no money — the cost engine
//! is a separate, later piece that will take a [`RateCard`] as an argument.
//!
//! # The cost model these types serve
//!
//! ```text
//! material qty  = rate × driver          (driver per RateBasis)
//! material cost = qty × unit_cost
//! labor         = wage × crew × hours, + payroll tax on WAGES only
//! overhead      = crew × hours × overhead_per_man_hour
//! total         = materials + consumables + labor + overhead + job costs
//! price         = total × (1 + margin)
//! ```
//!
//! Labor and overhead are both functions of `crew × hours`, so man-hours is
//! the only cost driver; crew size affects scheduling, not dollars.
//!
//! # Two things nothing downstream may assume
//!
//! 1. **Costs can be negative.** Reclaimed flake is a credit
//!    (`flake_recovered` at `-74.00/box`). Any `> 0.0` assertion on
//!    `unit_cost` is a bug.
//! 2. **A recipe may be wrong.** The four system recipes are INFERRED by
//!    comparing same-square-footage quote sheets, not signed off. They carry
//!    [`SystemRecipe::confirmed`]` == false` so the uncertainty is visible to
//!    code, not just to a reader of a comment.

pub mod calc;

pub use calc::{
    consumable_rate_per_sf, material_rate_per_sf, price_area, price_from_cost, price_job,
    rate_summary, AreaInput, AreaQuote, JobCosts, JobInput, JobQuote, LaborInput, LineSource,
    PricingContext, PricingError, Productivity, QuoteLine, MARGIN_FLOOR, SF_PER_MAN_HOUR_MAX,
    SF_PER_MAN_HOUR_MIN, SF_PER_MAN_HOUR_TYPICAL,
};

#[cfg(feature = "serde")]
pub use calc::{price_job_json, PricingJsonError};

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The physical unit a product is bought in. Display/ordering only — it takes
/// no part in cost arithmetic, which is always `qty × unit_cost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProductUnit {
    Litre,
    Cup,
    Cap,
    Box,
    Gallon,
    Tube,
    Pound,
    Each,
}

/// Which subtotal a product lands in. The cost buildup reports materials and
/// consumables separately, so this is not cosmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProductClass {
    /// Named by a system recipe or added by an add-on.
    Direct,
    /// Applied to every job, scaled per system — see
    /// [`SystemRecipe::consumable_multipliers`].
    Consumable,
}

/// What quantity a product's `rate` multiplies.
///
/// The basis lives on the PRODUCT rather than on the add-on that pulls it in:
/// a rate without its basis is meaningless, and putting it here makes it
/// impossible for two add-ons to interpret the same rate against different
/// drivers. Every product in the current card is [`RateBasis::AreaSf`];
/// the other variants are the structural room for cove base (per LF),
/// discrete fixtures (per EA), and flat charges (per job).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RateBasis {
    #[default]
    AreaSf,
    PerimeterLf,
    LengthLf,
    CountEa,
    /// Flat per job — quantity is `rate` itself, no measurement involved.
    PerJob,
}

/// One catalog line: what it costs and how much of it a job consumes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Product {
    /// Stable machine key. NEVER renamed — recipes, add-ons, and saved quotes
    /// reference it. Display text changes via `name` alone.
    pub id: String,
    pub name: String,
    pub class: ProductClass,
    pub unit: ProductUnit,
    /// MAY BE NEGATIVE — reclaim credits. See the module note.
    pub unit_cost: f64,
    /// Quantity consumed per unit of [`Product::rate_basis`].
    pub rate: f64,
    #[cfg_attr(feature = "serde", serde(default))]
    pub rate_basis: RateBasis,
}

/// A named subset of the catalog. A system is a RECIPE, not a price — the same
/// catalog priced through different membership.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SystemRecipe {
    /// Matches the harness `FLOOR_SYSTEMS` key (`polish`/`seal`/`epoxy`/
    /// `polyurea`) so a measurement's `systemType` resolves directly.
    pub key: String,
    pub name: String,
    /// Direct products only. Consumables are governed by the multipliers.
    pub product_ids: Vec<String>,
    /// Multiplier per consumable — EXHAUSTIVE. Every consumable in the
    /// catalog must have an entry for every system, enforced by [`validate`],
    /// so an incomplete card fails to LOAD rather than silently mispricing.
    ///
    /// There is deliberately no default. A default of `1.0` silently
    /// OVERCHARGES sealer jobs and `0.0` silently UNDERCHARGES them; neither
    /// is safe, and a test only fires in CI while the person hand-editing
    /// this JSON to add a product is exactly the person not running it.
    /// `0.0` means the system does not use that consumable at all.
    pub consumable_multipliers: BTreeMap<String, f64>,
    /// `false` until the recipe is signed off. Membership was inferred by
    /// comparing four same-square-footage sheets; a consumer that quotes from
    /// an unconfirmed recipe should say so rather than imply authority.
    pub confirmed: bool,
    /// The grit level this system's LABOR FIGURES ALREADY REFLECT, keyed into
    /// [`RateCard::grit_levels`].
    ///
    /// `None` means grit does not apply to this system — it involves no
    /// grinding, so an area may not set one (that is
    /// [`PricingError::GritNotApplicable`], not a silently ignored field).
    ///
    /// This is the baseline the escalator measures FROM. Without it, a system
    /// whose standard is already 800 grit would have the 800-grit multiplier
    /// applied on top of hours that were priced at 800 grit to begin with —
    /// double-counting the very passes the figures include.
    #[cfg_attr(feature = "serde", serde(default))]
    pub standard_grit: Option<String>,
}

/// One grinding grit level the catalog knows about.
///
/// Which levels exist is DATA, not an enum: adding 1200 grit is an edit to
/// `data/rate-cards/*.json` and no rebuild. A level is referenced by `key`
/// everywhere, so display text can change freely.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GritLevel {
    /// Stable machine key (`"400"`, `"800"`). Never renamed.
    pub key: String,
    pub name: String,
    /// Man-hours multiplier for this level, on a scale SHARED by every level.
    ///
    /// It is not applied directly — see [`RateCard::grit_labor_multiplier`].
    /// What reaches an area is this level's figure divided by its system's
    /// standard, so the number here is meaningful only relative to the other
    /// levels. Must be > 0: a 0.0 would zero out an area's hours and produce
    /// a materials-only price that reads as legitimate, the same failure
    /// [`PricingError::MissingLabor`] exists to prevent.
    ///
    /// Seeded at 1.0 across the board, which makes the whole mechanism inert
    /// until real timing data replaces the seeds.
    pub labor_multiplier: f64,
}

/// How an add-on changes the product set a base recipe contributed.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "op", rename_all = "snake_case"))]
pub enum ProductOp {
    /// Introduce a product. Idempotent — adding one already present is a
    /// no-op, never a duplicate line.
    Add { product_id: String },
    /// Swap a base product for another, carrying that product's own rate.
    /// Double Broadcast swaps both flake rows this way.
    Replace { product_id: String, with: String },
    /// Drop a product the base recipe contributed.
    Remove { product_id: String },
}

/// What an add-on actually does.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum AddOnEffect {
    /// Add, replace, or remove catalog products. Each product prices at its
    /// own `rate`/`rate_basis`.
    Products { ops: Vec<ProductOp> },
    /// The user types a dollar figure directly: no product, no rate, no
    /// driver. Crack Stitching is the first of these and has never been used
    /// on a job — this is a hook, deliberately minimal.
    ManualCost {
        /// Label shown beside the amount field.
        prompt: String,
    },
}

/// An optional extra attached to an area alongside its base system.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AddOn {
    pub key: String,
    pub name: String,
    /// System keys this may attach to. EMPTY = any system.
    #[cfg_attr(feature = "serde", serde(default))]
    pub applies_to: Vec<String>,
    pub effect: AddOnEffect,
    /// Fold order when several add-ons apply. Lower runs first; ties break on
    /// declaration order. See [`validate`] for the conflict rules.
    #[cfg_attr(feature = "serde", serde(default))]
    pub priority: i32,
}

/// Michigan prevailing-wage rates. STRUCTURAL HOOK — deliberately unpopulated;
/// actual rates vary by county and trade classification.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DavisBaconRates {
    pub county: String,
    pub classification: String,
    pub base_wage_per_hour: f64,
    pub fringe_per_hour: f64,
}

/// Which wage schedule a job is priced against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum WageSource {
    #[default]
    Standard,
    DavisBacon,
}

/// Labor and overhead constants.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LaborRates {
    pub standard_wage_per_hour: f64,
    /// Employer FICA (6.2% SS + 1.45% Medicare). Applies to WAGES ONLY, not
    /// to benefits — do not widen the base without checking that.
    pub payroll_tax_rate: f64,
    pub insurance_benefits_per_hour: f64,
    /// Burden per man-hour: `crew × hours × this`.
    pub overhead_per_man_hour: f64,
    /// `None` until prevailing rates are entered. Selecting
    /// [`WageSource::DavisBacon`] against `None` must ERROR — never silently
    /// fall back to the standard wage.
    #[cfg_attr(feature = "serde", serde(default))]
    pub davis_bacon: Option<DavisBaconRates>,
}

impl LaborRates {
    /// Wage for a source, or `None` if Davis-Bacon was asked for and no
    /// prevailing rates are loaded. Returning `Option` rather than falling
    /// back is the point: a prevailing-wage job priced at the standard wage
    /// underbids by roughly half and looks entirely legitimate.
    pub fn wage_for(&self, source: WageSource) -> Option<f64> {
        match source {
            WageSource::Standard => Some(self.standard_wage_per_hour),
            WageSource::DavisBacon => self
                .davis_bacon
                .as_ref()
                .map(|d| d.base_wage_per_hour + d.fringe_per_hour),
        }
    }
}

/// A complete, versioned set of rates. Immutable once referenced by a quote —
/// corrections create a NEW version rather than editing one in place, so a
/// historical quote reprices to the same number forever.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RateCard {
    /// Version identifier a saved quote pins. Never reused.
    pub version: String,
    /// ISO-8601 date these rates took effect.
    pub effective_date: String,
    /// Where the numbers came from, for audit.
    pub source: String,
    pub products: Vec<Product>,
    pub systems: Vec<SystemRecipe>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub add_ons: Vec<AddOn>,
    /// The grit ladder. Empty is legal and means no area may specify a grit —
    /// a card written before this existed keeps pricing exactly as it did.
    #[cfg_attr(feature = "serde", serde(default))]
    pub grit_levels: Vec<GritLevel>,
    pub labor: LaborRates,
}

impl RateCard {
    pub fn product(&self, id: &str) -> Option<&Product> {
        self.products.iter().find(|p| p.id == id)
    }
    pub fn system(&self, key: &str) -> Option<&SystemRecipe> {
        self.systems.iter().find(|s| s.key == key)
    }
    pub fn add_on(&self, key: &str) -> Option<&AddOn> {
        self.add_ons.iter().find(|a| a.key == key)
    }
    /// Every consumable, in declaration order.
    pub fn consumables(&self) -> impl Iterator<Item = &Product> {
        self.products
            .iter()
            .filter(|p| p.class == ProductClass::Consumable)
    }
    /// Multiplier this system applies to a consumable. `None` only if the
    /// system or the entry is missing — [`validate`] guarantees the entry
    /// exists for every consumable on a card that loaded.
    pub fn consumable_multiplier(&self, system_key: &str, consumable_id: &str) -> Option<f64> {
        self.system(system_key)?
            .consumable_multipliers
            .get(consumable_id)
            .copied()
    }
    pub fn grit_level(&self, key: &str) -> Option<&GritLevel> {
        self.grit_levels.iter().find(|g| g.key == key)
    }
    /// Grit levels a system may be quoted at: the whole ladder if it grinds,
    /// nothing if it does not. What a UI populates a picker from.
    pub fn grits_for_system(&self, system_key: &str) -> &[GritLevel] {
        match self.system(system_key).and_then(|s| s.standard_grit.as_ref()) {
            Some(_) => &self.grit_levels,
            None => &[],
        }
    }
    /// The man-hours escalator for quoting `system_key` at `grit_key`:
    /// **that level's multiplier divided by the system's standard**.
    ///
    /// The division is the whole design. A system's entered hours already
    /// describe work at its standard grit, so only the DIFFERENCE from that
    /// baseline may be charged; quoting a system at its own standard yields
    /// exactly 1.0 and changes nothing. Returns `None` if either level is
    /// unknown — the caller decides which error that is.
    pub fn grit_labor_multiplier(&self, system_key: &str, grit_key: &str) -> Option<f64> {
        let standard = self.system(system_key)?.standard_grit.as_ref()?;
        let at = self.grit_level(grit_key)?.labor_multiplier;
        let base = self.grit_level(standard)?.labor_multiplier;
        (base > 0.0).then_some(at / base)
    }
}

/// A structural defect in a rate card. Every variant names the offending id so
/// a bad card is diagnosable from the message alone.
// No `Eq`: NotFinite carries the offending f64 so the message can name it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RateCardError {
    #[error("duplicate product id `{0}`")]
    DuplicateProduct(String),
    #[error("duplicate system key `{0}`")]
    DuplicateSystem(String),
    #[error("duplicate add-on key `{0}`")]
    DuplicateAddOn(String),
    #[error("system `{system}` names unknown product `{product}`")]
    UnknownProductInSystem { system: String, product: String },
    #[error("system `{system}` names `{product}`, which is a consumable, not a direct product")]
    ConsumableInSystem { system: String, product: String },
    #[error("system `{system}` sets a multiplier for `{product}`, which is not a consumable")]
    MultiplierNotConsumable { system: String, product: String },
    #[error(
        "system `{system}` has no consumable multiplier for `{consumable}` — every \
         consumable needs an explicit entry on every system, there is no default"
    )]
    MissingConsumableMultiplier { system: String, consumable: String },
    #[error("add-on `{add_on}` references unknown product `{product}`")]
    UnknownProductInAddOn { add_on: String, product: String },
    #[error("add-on `{add_on}` applies to unknown system `{system}`")]
    UnknownSystemInAddOn { add_on: String, system: String },
    #[error(
        "add-ons `{a}` and `{b}` both replace `{product}` with different products \
         and can apply to the same system"
    )]
    ConflictingReplace {
        a: String,
        b: String,
        product: String,
    },
    #[error("product `{0}` belongs to no system recipe and no add-on")]
    OrphanProduct(String),
    #[error("duplicate grit level key `{0}`")]
    DuplicateGritLevel(String),
    #[error("system `{system}` names unknown standard grit `{grit}`")]
    UnknownGritInSystem { system: String, grit: String },
    #[error(
        "grit level `{grit}` has labor_multiplier {value} — it must be greater than zero, \
         because a multiplier of 0 zeroes out an area's hours and prices it materials-only"
    )]
    GritMultiplierNotPositive { grit: String, value: f64 },
    #[error("`{field}` is not a finite number (got {value})")]
    NotFinite { field: String, value: f64 },
    #[error("`{field}` must not be negative (got {value})")]
    Negative { field: String, value: f64 },
    #[cfg(feature = "serde")]
    #[error("rate card is not valid JSON: {0}")]
    Json(String),
}

fn finite(field: &str, value: f64, out: &mut Vec<RateCardError>) {
    if !value.is_finite() {
        out.push(RateCardError::NotFinite {
            field: field.to_string(),
            value,
        });
    }
}

fn non_negative(field: &str, value: f64, out: &mut Vec<RateCardError>) {
    finite(field, value, out);
    if value.is_finite() && value < 0.0 {
        out.push(RateCardError::Negative {
            field: field.to_string(),
            value,
        });
    }
}

/// Check a rate card for structural defects, returning EVERY problem found
/// rather than the first — fixing a hand-edited data file one error per run is
/// miserable.
///
/// # Add-on conflict rules
///
/// Ops fold in `(priority, declaration order)`, which makes the result
/// deterministic. Within that fold:
///
/// - [`ProductOp::Add`] is idempotent — set semantics, never a duplicate line.
/// - [`ProductOp::Remove`] after an `Add` nets to removed.
/// - Two add-ons that can apply to the SAME system and
///   [`ProductOp::Replace`] the same product with DIFFERENT targets are a
///   hard error, reported here at load time. Silent last-wins would produce a
///   wrong price that looks legitimate; this module errors instead, matching
///   `AssemblyError`'s rule that a bad recipe never yields a
///   plausible-but-wrong quantity.
///
/// Two add-ons replacing the same product with the SAME target agree, so they
/// are permitted.
///
/// # What is NOT checked
///
/// Whether a recipe's membership is CORRECT — that is a domain question no
/// validator can answer, which is why [`SystemRecipe::confirmed`] exists.
pub fn validate(card: &RateCard) -> Result<(), Vec<RateCardError>> {
    let mut errs = Vec::new();

    // ---- uniqueness ----
    let mut seen = BTreeMap::new();
    for p in &card.products {
        if seen.insert(p.id.clone(), ()).is_some() {
            errs.push(RateCardError::DuplicateProduct(p.id.clone()));
        }
        finite(
            &format!("product `{}`.unit_cost", p.id),
            p.unit_cost,
            &mut errs,
        );
        // A negative RATE is nonsense even though a negative COST is not.
        non_negative(&format!("product `{}`.rate", p.id), p.rate, &mut errs);
    }
    let mut seen_sys = BTreeMap::new();
    for s in &card.systems {
        if seen_sys.insert(s.key.clone(), ()).is_some() {
            errs.push(RateCardError::DuplicateSystem(s.key.clone()));
        }
    }
    let mut seen_add = BTreeMap::new();
    for a in &card.add_ons {
        if seen_add.insert(a.key.clone(), ()).is_some() {
            errs.push(RateCardError::DuplicateAddOn(a.key.clone()));
        }
    }
    let mut seen_grit = BTreeMap::new();
    for g in &card.grit_levels {
        if seen_grit.insert(g.key.clone(), ()).is_some() {
            errs.push(RateCardError::DuplicateGritLevel(g.key.clone()));
        }
        // Strictly positive, not merely non-negative: the escalator divides by
        // the standard level's figure, and a 0.0 anywhere in the ladder is
        // either a division by zero or a free area.
        if !g.labor_multiplier.is_finite() || g.labor_multiplier <= 0.0 {
            errs.push(RateCardError::GritMultiplierNotPositive {
                grit: g.key.clone(),
                value: g.labor_multiplier,
            });
        }
    }

    // ---- system membership resolves, and to the right class ----
    for s in &card.systems {
        // A standard grit naming a level that does not exist would make every
        // grit-bearing area on that system unpriceable at quote time instead
        // of at load time.
        if let Some(g) = &s.standard_grit {
            if card.grit_level(g).is_none() {
                errs.push(RateCardError::UnknownGritInSystem {
                    system: s.key.clone(),
                    grit: g.clone(),
                });
            }
        }
        // Exhaustive: a consumable with no entry is a load-time failure, not
        // a silently-defaulted line on someone's invoice.
        for cons in card.consumables() {
            if !s.consumable_multipliers.contains_key(&cons.id) {
                errs.push(RateCardError::MissingConsumableMultiplier {
                    system: s.key.clone(),
                    consumable: cons.id.clone(),
                });
            }
        }
        for pid in &s.product_ids {
            match card.product(pid) {
                None => errs.push(RateCardError::UnknownProductInSystem {
                    system: s.key.clone(),
                    product: pid.clone(),
                }),
                Some(p) if p.class == ProductClass::Consumable => {
                    errs.push(RateCardError::ConsumableInSystem {
                        system: s.key.clone(),
                        product: pid.clone(),
                    });
                }
                Some(_) => {}
            }
        }
        for (cid, mult) in &s.consumable_multipliers {
            non_negative(
                &format!("system `{}`.consumable_multipliers[{cid}]", s.key),
                *mult,
                &mut errs,
            );
            match card.product(cid) {
                Some(p) if p.class == ProductClass::Consumable => {}
                _ => errs.push(RateCardError::MultiplierNotConsumable {
                    system: s.key.clone(),
                    product: cid.clone(),
                }),
            }
        }
    }

    // ---- add-on operands resolve ----
    for a in &card.add_ons {
        for sys in &a.applies_to {
            if card.system(sys).is_none() {
                errs.push(RateCardError::UnknownSystemInAddOn {
                    add_on: a.key.clone(),
                    system: sys.clone(),
                });
            }
        }
        for pid in add_on_product_ids(a) {
            if card.product(pid).is_none() {
                errs.push(RateCardError::UnknownProductInAddOn {
                    add_on: a.key.clone(),
                    product: pid.clone(),
                });
            }
        }
    }

    // ---- conflicting replaces on an overlapping system ----
    for (i, a) in card.add_ons.iter().enumerate() {
        for b in card.add_ons.iter().skip(i + 1) {
            if !systems_overlap(a, b) {
                continue;
            }
            for (pid, target_a) in replacements(a) {
                if let Some(target_b) = replacements(b)
                    .into_iter()
                    .find(|(p, _)| *p == pid)
                    .map(|(_, t)| t)
                {
                    if target_a != target_b {
                        errs.push(RateCardError::ConflictingReplace {
                            a: a.key.clone(),
                            b: b.key.clone(),
                            product: pid.to_string(),
                        });
                    }
                }
            }
        }
    }

    // ---- orphans: reachable from no recipe and no add-on ----
    for p in &card.products {
        if p.class == ProductClass::Consumable {
            continue; // consumables are reached via multipliers, not membership
        }
        let in_system = card
            .systems
            .iter()
            .any(|s| s.product_ids.iter().any(|id| id == &p.id));
        let in_add_on = card
            .add_ons
            .iter()
            .any(|a| add_on_product_ids(a).into_iter().any(|id| id == &p.id));
        if !in_system && !in_add_on {
            errs.push(RateCardError::OrphanProduct(p.id.clone()));
        }
    }

    // ---- labor ----
    non_negative(
        "labor.standard_wage_per_hour",
        card.labor.standard_wage_per_hour,
        &mut errs,
    );
    non_negative(
        "labor.payroll_tax_rate",
        card.labor.payroll_tax_rate,
        &mut errs,
    );
    non_negative(
        "labor.insurance_benefits_per_hour",
        card.labor.insurance_benefits_per_hour,
        &mut errs,
    );
    non_negative(
        "labor.overhead_per_man_hour",
        card.labor.overhead_per_man_hour,
        &mut errs,
    );

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Every product id an add-on names, in any position.
fn add_on_product_ids(a: &AddOn) -> Vec<&String> {
    match &a.effect {
        AddOnEffect::ManualCost { .. } => Vec::new(),
        AddOnEffect::Products { ops } => ops
            .iter()
            .flat_map(|op| match op {
                ProductOp::Add { product_id } | ProductOp::Remove { product_id } => {
                    vec![product_id]
                }
                ProductOp::Replace { product_id, with } => vec![product_id, with],
            })
            .collect(),
    }
}

/// `(replaced, replacement)` pairs an add-on declares.
fn replacements(a: &AddOn) -> Vec<(&str, &str)> {
    match &a.effect {
        AddOnEffect::ManualCost { .. } => Vec::new(),
        AddOnEffect::Products { ops } => ops
            .iter()
            .filter_map(|op| match op {
                ProductOp::Replace { product_id, with } => {
                    Some((product_id.as_str(), with.as_str()))
                }
                _ => None,
            })
            .collect(),
    }
}

/// Could these two add-ons ever attach to the same system? An empty
/// `applies_to` means "any", so it overlaps with everything.
fn systems_overlap(a: &AddOn, b: &AddOn) -> bool {
    if a.applies_to.is_empty() || b.applies_to.is_empty() {
        return true;
    }
    a.applies_to.iter().any(|s| b.applies_to.contains(s))
}

/// Parse and validate a rate card from JSON — the host-supplied path.
///
/// Parsing alone is not enough: a card that deserializes but names a
/// nonexistent product would fail later, deep inside a cost calculation, with
/// no useful context. Validation happens HERE, at the boundary.
#[cfg(feature = "serde")]
pub fn from_json(src: &str) -> Result<RateCard, Vec<RateCardError>> {
    let card: RateCard =
        serde_json::from_str(src).map_err(|e| vec![RateCardError::Json(e.to_string())])?;
    validate(&card)?;
    Ok(card)
}
