//! The cost buildup — pure functions over a [`RateCard`] and a job's inputs.
//!
//! # Rounding order
//!
//! Every intermediate is `f64` and NOTHING is rounded on the way through.
//! Quantities, extended costs, subtotals, job cost, price, and profit are all
//! full-precision; rounding to cents happens ONCE, in whatever presents the
//! number. Rounding per line and re-summing drifts by cents on a large quote
//! and makes the itemization fail to add up to the total — the single most
//! common complaint about spreadsheet estimates. Tests assert to ±$0.01.
//!
//! # Margin, not markup
//!
//! ```text
//! price  = cost / (1 - margin)
//! profit = price - cost
//! margin = profit / price      // round-trips to the input margin
//! ```
//!
//! `cost × (1 + margin)` is MARKUP and is what the source workbook did. At
//! 50% markup gives 1.5× cost and margin gives 2× cost — a $5,000 gap on a
//! $10,000 job. [`price_from_cost`] is the only place price is derived, and
//! [`MARGIN_FLOOR`] is a named constant rather than a literal in a comparison.
//!
//! # Nothing assumes costs are positive
//!
//! Flake reclaim rows carry negative `unit_cost`. A line, a subtotal, or a
//! whole area may legitimately come out negative.

use super::{
    AddOn, AddOnEffect, LaborRates, Product, ProductClass, ProductOp, ProductUnit, RateBasis,
    RateCard, WageSource,
};
use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The business floor for job margin. Quotes below this still COMPUTE and
/// price — the result carries [`JobQuote::below_margin_floor`] and the UI
/// warns. Refusing to price a thin job just hides it.
pub const MARGIN_FLOOR: f64 = 0.20;

// ---------- inputs ----------

/// Crew size and duration for one area. Both are required: labor and overhead
/// are each a function of `crew × hours`, so a missing figure cannot be
/// defaulted to zero without producing a materials-only price that looks
/// entirely legitimate.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LaborInput {
    pub crew: f64,
    pub hours: f64,
}

impl LaborInput {
    /// The only labor cost driver. Crew size affects scheduling, not dollars:
    /// 4 crew × 10 h and 2 crew × 20 h cost identically.
    pub fn man_hours(&self) -> f64 {
        self.crew * self.hours
    }
}

/// One area to be priced.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AreaInput {
    /// Measurement id, echoed into the result so the UI can map lines back.
    pub id: u64,
    pub name: String,
    pub area_sf: f64,
    /// Only needed if a product prices per perimeter; `None` is fine while
    /// every catalog rate is per SF.
    #[cfg_attr(feature = "serde", serde(default))]
    pub perimeter_lf: Option<f64>,
    pub system_key: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub add_on_keys: Vec<String>,
    /// `None` = hours not entered yet. Pricing ERRORS rather than guessing.
    pub labor: Option<LaborInput>,
    /// PER-QUOTE line overrides: `product_id` → factor on the computed
    /// quantity. `0.0` suppresses, `1.0` is the default, `0.5` is half.
    ///
    /// A factor rather than an absolute quantity, because an absolute would
    /// stop tracking area — re-trace the room and a hardcoded quantity would
    /// silently stay put. Deleting a line is the special case `0.0`, so
    /// "half the normal mender" needs no later refactor.
    ///
    /// The rate card is NEVER touched: the next job still defaults to the
    /// full recipe.
    #[cfg_attr(feature = "serde", serde(default))]
    pub quantity_overrides: BTreeMap<String, f64>,
    /// Dollar amounts for [`AddOnEffect::ManualCost`] add-ons, keyed by
    /// add-on key. A manual-cost add-on with no entry is an error, not $0.
    #[cfg_attr(feature = "serde", serde(default))]
    pub manual_costs: BTreeMap<String, f64>,
}

/// Costs that belong to the job rather than to any one area.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
// Struct-level default: a job or a fixture names only the costs it actually
// has, and an omitted one is $0 rather than a parse failure.
#[cfg_attr(feature = "serde", serde(default))]
pub struct JobCosts {
    pub travel: f64,
    pub permits: f64,
    pub equipment: f64,
    pub mobilization: f64,
}

impl JobCosts {
    pub fn total(&self) -> f64 {
        self.travel + self.permits + self.equipment + self.mobilization
    }
}

/// A whole job.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct JobInput {
    pub areas: Vec<AreaInput>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub job_costs: JobCosts,
    /// Single job-level rate applied uniformly across the whole quote.
    pub margin: f64,
    #[cfg_attr(feature = "serde", serde(default))]
    pub wage_source: WageSource,
}

// ---------- outputs ----------

/// Where a line came from, so the UI can group and explain it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineSource {
    /// Named by the system recipe.
    Recipe,
    /// Contributed (or substituted in) by an add-on.
    AddOn(String),
    /// A consumable, scaled by the system's multiplier.
    Consumable,
}

/// One itemized line. Carries everything the UI or a PDF needs, so neither
/// recomputes anything.
#[derive(Debug, Clone, PartialEq)]
pub struct QuoteLine {
    pub product_id: String,
    pub product_name: String,
    pub unit: ProductUnit,
    pub source: LineSource,
    /// Quantity BEFORE any override factor — what the recipe would have used.
    pub base_quantity: f64,
    /// The override factor actually applied (1.0 when untouched).
    pub factor: f64,
    /// `base_quantity × factor` — what is actually consumed.
    pub quantity: f64,
    pub unit_cost: f64,
    /// `quantity × unit_cost`. May be NEGATIVE (reclaim credits).
    pub extended_cost: f64,
    /// True when `factor == 0`. The line is still REPORTED, at $0.00, so the
    /// UI can show it struck through with an undo — a line that vanishes is
    /// indistinguishable from one that was forgotten.
    pub suppressed: bool,
}

/// One area's full breakdown.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaQuote {
    pub area_id: u64,
    pub name: String,
    pub area_sf: f64,
    pub system_key: String,
    /// Mirrors [`super::SystemRecipe::confirmed`] — false means this priced
    /// against an inferred recipe and the UI should say so.
    pub system_confirmed: bool,
    /// Every line, INCLUDING suppressed ones at $0.00.
    pub lines: Vec<QuoteLine>,
    pub materials: f64,
    pub consumables: f64,
    pub labor: f64,
    pub overhead: f64,
    pub manual_costs: f64,
    pub man_hours: f64,
    /// materials + consumables + labor + overhead + manual costs.
    pub cost: f64,
}

/// The whole job, priced.
#[derive(Debug, Clone, PartialEq)]
pub struct JobQuote {
    pub areas: Vec<AreaQuote>,
    /// Σ area costs.
    pub area_cost: f64,
    pub job_costs: JobCosts,
    /// area_cost + job_costs.total()
    pub cost: f64,
    /// The margin rate this was priced at (the input).
    pub margin: f64,
    /// `cost / (1 - margin)`.
    pub price: f64,
    /// `price - cost`, in DOLLARS — a primary output, not something the UI
    /// derives.
    pub profit: f64,
    /// `margin < MARGIN_FLOOR`. A flag, never an error.
    pub below_margin_floor: bool,
    /// System keys quoted from an UNCONFIRMED recipe, deduplicated.
    pub unconfirmed_systems: Vec<String>,
}

impl JobQuote {
    /// Margin recovered from the output figures: `profit / price`. Must equal
    /// the input [`JobQuote::margin`] — this is the round-trip that catches a
    /// markup/margin regression. Zero when the price is zero.
    pub fn realized_margin(&self) -> f64 {
        if self.price == 0.0 {
            0.0
        } else {
            self.profit / self.price
        }
    }
}

/// Pricing failed. Every variant names what and where, so nothing degrades to
/// a plausible-but-wrong number.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PricingError {
    #[error("unknown system `{0}`")]
    UnknownSystem(String),
    #[error("unknown add-on `{0}`")]
    UnknownAddOn(String),
    #[error("add-on `{add_on}` does not apply to system `{system}`")]
    AddOnNotApplicable { add_on: String, system: String },
    #[error("area `{area}` has a system assigned but no crew/hours entered")]
    MissingLabor { area: String },
    #[error(
        "area `{area}`: crew and hours must be finite and non-negative (got {crew} × {hours})"
    )]
    InvalidLabor { area: String, crew: f64, hours: f64 },
    #[error("area `{area}`: area_sf must be finite and non-negative (got {area_sf})")]
    InvalidArea { area: String, area_sf: f64 },
    #[error("margin {0} is not usable — must be finite and < 1.0 (1.0 divides by zero)")]
    InvalidMargin(f64),
    #[error("area `{area}`: quantity override for `{product}` must be finite and non-negative (got {factor})")]
    InvalidOverride {
        area: String,
        product: String,
        factor: f64,
    },
    #[error("add-on `{add_on}` is a manual-cost add-on but area `{area}` supplied no amount")]
    MissingManualCost { area: String, add_on: String },
    #[error(
        "area `{area}`: add-on `{add_on}` replaces `{product}`, which the recipe does not contain"
    )]
    ReplaceTargetMissing {
        area: String,
        add_on: String,
        product: String,
    },
    #[error("system `{system}` has no consumable multiplier for `{consumable}`")]
    MissingConsumableMultiplier { system: String, consumable: String },
    #[error("wage source {0:?} has no rates loaded — Davis-Bacon prevailing wages are unpopulated, and falling back to the standard wage would underbid by roughly half")]
    WageUnavailable(WageSource),
    #[error(
        "area `{area}`: product `{product}` prices per {basis:?}, which this area does not carry"
    )]
    MissingDriver {
        area: String,
        product: String,
        basis: RateBasis,
    },
}

// ---------- margin ----------

/// Price and profit from a cost at a margin rate.
///
/// `price = cost / (1 - margin)`. NOT `cost × (1 + margin)` — see the module
/// docs. Rejects margin ≥ 1.0 (division by zero, then negative price) and
/// non-finite input. A NEGATIVE margin is permitted and simply prices below
/// cost; it trips [`MARGIN_FLOOR`] like any other thin quote.
pub fn price_from_cost(cost: f64, margin: f64) -> Result<(f64, f64), PricingError> {
    if !margin.is_finite() || margin >= 1.0 {
        return Err(PricingError::InvalidMargin(margin));
    }
    let price = cost / (1.0 - margin);
    Ok((price, price - cost))
}

// ---------- resolution ----------

/// The product set for an area: the recipe, then each add-on's ops folded in.
///
/// Order is `(priority, declaration order in the card)`, so the result is
/// deterministic. `Add` is idempotent; `Remove` drops; `Replace` swaps in
/// place, preserving the recipe's ordering. A `Replace` whose target is
/// already the replacement is a no-op, which is what makes two add-ons
/// declaring the SAME replacement agree rather than the second one failing.
fn resolve_products<'a>(
    card: &'a RateCard,
    area: &AreaInput,
) -> Result<Vec<(&'a Product, LineSource)>, PricingError> {
    let system = card
        .system(&area.system_key)
        .ok_or_else(|| PricingError::UnknownSystem(area.system_key.clone()))?;

    let mut set: Vec<(String, LineSource)> = system
        .product_ids
        .iter()
        .map(|id| (id.clone(), LineSource::Recipe))
        .collect();

    // Resolve add-ons and sort by (priority, position in the card).
    let mut applied: Vec<(&AddOn, usize)> = Vec::new();
    for key in &area.add_on_keys {
        let pos = card
            .add_ons
            .iter()
            .position(|a| &a.key == key)
            .ok_or_else(|| PricingError::UnknownAddOn(key.clone()))?;
        let add_on = &card.add_ons[pos];
        if !add_on.applies_to.is_empty() && !add_on.applies_to.contains(&area.system_key) {
            return Err(PricingError::AddOnNotApplicable {
                add_on: key.clone(),
                system: area.system_key.clone(),
            });
        }
        applied.push((add_on, pos));
    }
    applied.sort_by_key(|(a, pos)| (a.priority, *pos));

    for (add_on, _) in &applied {
        let AddOnEffect::Products { ops } = &add_on.effect else {
            continue; // manual-cost add-ons contribute dollars, not products
        };
        for op in ops {
            match op {
                ProductOp::Add { product_id } => {
                    if !set.iter().any(|(id, _)| id == product_id) {
                        set.push((product_id.clone(), LineSource::AddOn(add_on.key.clone())));
                    }
                }
                ProductOp::Remove { product_id } => set.retain(|(id, _)| id != product_id),
                ProductOp::Replace { product_id, with } => {
                    match set.iter().position(|(id, _)| id == product_id) {
                        Some(i) => {
                            set[i] = (with.clone(), LineSource::AddOn(add_on.key.clone()));
                        }
                        // Already replaced by an add-on declaring the same
                        // swap — agreement, not a failure.
                        None if set.iter().any(|(id, _)| id == with) => {}
                        None => {
                            return Err(PricingError::ReplaceTargetMissing {
                                area: area.name.clone(),
                                add_on: add_on.key.clone(),
                                product: product_id.clone(),
                            })
                        }
                    }
                }
            }
        }
    }

    set.into_iter()
        .map(|(id, src)| {
            card.product(&id)
                .map(|p| (p, src))
                // validate() proves every id resolves, so this is defensive.
                .ok_or_else(|| PricingError::UnknownSystem(id.clone()))
        })
        .collect()
}

/// The measurement a product's rate multiplies.
fn driver(area: &AreaInput, product: &Product) -> Result<f64, PricingError> {
    match product.rate_basis {
        RateBasis::AreaSf => Ok(area.area_sf),
        RateBasis::PerimeterLf => area
            .perimeter_lf
            .ok_or_else(|| PricingError::MissingDriver {
                area: area.name.clone(),
                product: product.id.clone(),
                basis: product.rate_basis,
            }),
        RateBasis::PerJob => Ok(1.0),
        basis => Err(PricingError::MissingDriver {
            area: area.name.clone(),
            product: product.id.clone(),
            basis,
        }),
    }
}

/// Build one line, applying any per-quote override factor.
fn line(
    area: &AreaInput,
    product: &Product,
    source: LineSource,
    base_quantity: f64,
) -> Result<QuoteLine, PricingError> {
    let factor = match area.quantity_overrides.get(&product.id) {
        Some(f) if !f.is_finite() || *f < 0.0 => {
            return Err(PricingError::InvalidOverride {
                area: area.name.clone(),
                product: product.id.clone(),
                factor: *f,
            })
        }
        Some(f) => *f,
        None => 1.0,
    };
    let quantity = base_quantity * factor;
    Ok(QuoteLine {
        product_id: product.id.clone(),
        product_name: product.name.clone(),
        unit: product.unit,
        source,
        base_quantity,
        factor,
        quantity,
        unit_cost: product.unit_cost,
        extended_cost: quantity * product.unit_cost,
        suppressed: factor == 0.0,
    })
}

// ---------- the buildup ----------

/// Price one area against a card.
pub fn price_area(
    card: &RateCard,
    area: &AreaInput,
    wage_source: WageSource,
) -> Result<AreaQuote, PricingError> {
    if !area.area_sf.is_finite() || area.area_sf < 0.0 {
        return Err(PricingError::InvalidArea {
            area: area.name.clone(),
            area_sf: area.area_sf,
        });
    }
    let system = card
        .system(&area.system_key)
        .ok_or_else(|| PricingError::UnknownSystem(area.system_key.clone()))?;

    // Hours are required. A silent zero yields a materials-only price that
    // reads as legitimate -- the exact failure this must not have.
    let labor_in = area.labor.ok_or_else(|| PricingError::MissingLabor {
        area: area.name.clone(),
    })?;
    if !labor_in.crew.is_finite()
        || !labor_in.hours.is_finite()
        || labor_in.crew < 0.0
        || labor_in.hours < 0.0
    {
        return Err(PricingError::InvalidLabor {
            area: area.name.clone(),
            crew: labor_in.crew,
            hours: labor_in.hours,
        });
    }

    let mut lines = Vec::new();

    // Direct products (recipe + add-ons).
    for (product, source) in resolve_products(card, area)? {
        let base = product.rate * driver(area, product)?;
        lines.push(line(area, product, source, base)?);
    }

    // Consumables, scaled by this system's multiplier. A multiplier of 0.0
    // means the system does not use it -- omitted entirely rather than shown
    // as a suppressed line, because that is a catalog fact, not a user
    // decision about THIS job.
    for cons in card.consumables() {
        let mult = system
            .consumable_multipliers
            .get(&cons.id)
            .copied()
            .ok_or_else(|| PricingError::MissingConsumableMultiplier {
                system: system.key.clone(),
                consumable: cons.id.clone(),
            })?;
        if mult == 0.0 && !area.quantity_overrides.contains_key(&cons.id) {
            continue;
        }
        let base = cons.rate * driver(area, cons)? * mult;
        lines.push(line(area, cons, LineSource::Consumable, base)?);
    }

    let materials: f64 = lines
        .iter()
        .filter(|l| l.source != LineSource::Consumable)
        .map(|l| l.extended_cost)
        .sum();
    let consumables: f64 = lines
        .iter()
        .filter(|l| l.source == LineSource::Consumable)
        .map(|l| l.extended_cost)
        .sum();

    let man_hours = labor_in.man_hours();
    let wage = wage_for(&card.labor, wage_source)?;
    let labor = wage * man_hours * (1.0 + card.labor.payroll_tax_rate)
        + card.labor.insurance_benefits_per_hour * man_hours;
    let overhead = card.labor.overhead_per_man_hour * man_hours;

    // Manual-cost add-ons: an amount is REQUIRED, never defaulted to $0.
    let mut manual_costs = 0.0;
    for key in &area.add_on_keys {
        let add_on = card
            .add_on(key)
            .ok_or_else(|| PricingError::UnknownAddOn(key.clone()))?;
        if matches!(add_on.effect, AddOnEffect::ManualCost { .. }) {
            manual_costs += area.manual_costs.get(key).copied().ok_or_else(|| {
                PricingError::MissingManualCost {
                    area: area.name.clone(),
                    add_on: key.clone(),
                }
            })?;
        }
    }

    Ok(AreaQuote {
        area_id: area.id,
        name: area.name.clone(),
        area_sf: area.area_sf,
        system_key: area.system_key.clone(),
        system_confirmed: system.confirmed,
        lines,
        materials,
        consumables,
        labor,
        overhead,
        manual_costs,
        man_hours,
        cost: materials + consumables + labor + overhead + manual_costs,
    })
}

/// Wage for a source, erroring rather than falling back to the standard rate.
fn wage_for(labor: &LaborRates, source: WageSource) -> Result<f64, PricingError> {
    labor
        .wage_for(source)
        .ok_or(PricingError::WageUnavailable(source))
}

/// Price a whole job: every area, plus job-level costs, then margin.
///
/// Recomputes from scratch every call — there is no cached state, so a changed
/// hour, a suppressed line, a new area, or a changed job cost all flow through
/// by construction.
pub fn price_job(card: &RateCard, job: &JobInput) -> Result<JobQuote, PricingError> {
    if !job.margin.is_finite() || job.margin >= 1.0 {
        return Err(PricingError::InvalidMargin(job.margin));
    }
    let areas = job
        .areas
        .iter()
        .map(|a| price_area(card, a, job.wage_source))
        .collect::<Result<Vec<_>, _>>()?;

    let area_cost: f64 = areas.iter().map(|a| a.cost).sum();
    let cost = area_cost + job.job_costs.total();
    let (price, profit) = price_from_cost(cost, job.margin)?;

    let mut unconfirmed: Vec<String> = areas
        .iter()
        .filter(|a| !a.system_confirmed)
        .map(|a| a.system_key.clone())
        .collect();
    unconfirmed.sort();
    unconfirmed.dedup();

    Ok(JobQuote {
        areas,
        area_cost,
        job_costs: job.job_costs,
        cost,
        margin: job.margin,
        price,
        profit,
        below_margin_floor: job.margin < MARGIN_FLOOR,
        unconfirmed_systems: unconfirmed,
    })
}

// ---------- queryable rates (sanity-check a recipe without a full quote) ----------

/// Direct-material cost per square foot for a system: `Σ unit_cost × rate`
/// over the recipe. Polyurea is $1.81071/SF — a figure that can be checked
/// against a known job without building a quote.
pub fn material_rate_per_sf(card: &RateCard, system_key: &str) -> Result<f64, PricingError> {
    let system = card
        .system(system_key)
        .ok_or_else(|| PricingError::UnknownSystem(system_key.to_string()))?;
    Ok(system
        .product_ids
        .iter()
        .filter_map(|id| card.product(id))
        .filter(|p| p.rate_basis == RateBasis::AreaSf)
        .map(|p| p.unit_cost * p.rate)
        .sum())
}

/// Consumable cost per square foot for a system, with its multipliers applied.
pub fn consumable_rate_per_sf(card: &RateCard, system_key: &str) -> Result<f64, PricingError> {
    let system = card
        .system(system_key)
        .ok_or_else(|| PricingError::UnknownSystem(system_key.to_string()))?;
    let mut total = 0.0;
    for cons in card.consumables() {
        let mult = system.consumable_multipliers.get(&cons.id).copied().ok_or(
            PricingError::MissingConsumableMultiplier {
                system: system_key.to_string(),
                consumable: cons.id.clone(),
            },
        )?;
        if cons.rate_basis == RateBasis::AreaSf {
            total += cons.unit_cost * cons.rate * mult;
        }
    }
    Ok(total)
}

/// Both rates, for a quick per-SF sanity read on a recipe.
pub fn rate_summary(card: &RateCard, system_key: &str) -> Result<(f64, f64), PricingError> {
    Ok((
        material_rate_per_sf(card, system_key)?,
        consumable_rate_per_sf(card, system_key)?,
    ))
}

/// True when a product is a consumable — small helper for callers grouping
/// lines without reaching for [`ProductClass`].
pub fn is_consumable(p: &Product) -> bool {
    p.class == ProductClass::Consumable
}
