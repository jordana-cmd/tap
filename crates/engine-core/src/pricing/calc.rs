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
    AddOn, AddOnEffect, Product, ProductClass, ProductOp, ProductUnit, RateBasis, RateCard,
    WageSource,
};
use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The business floor for job margin. Quotes below this still COMPUTE and
/// price — the result carries [`JobQuote::below_margin_floor`] and the UI
/// warns. Refusing to price a thin job just hides it.
pub const MARGIN_FLOOR: f64 = 0.20;

/// Median SF per man-hour across ~220 historical jobs. Shown as the reference
/// a quoted figure is read against; nothing computes from it.
pub const SF_PER_MAN_HOUR_TYPICAL: f64 = 22.0;

/// Outside `[SF_PER_MAN_HOUR_MIN, SF_PER_MAN_HOUR_MAX]` an area is flagged as
/// implausible — ADVISORY ONLY, never an error and never blocking, because
/// some jobs genuinely run outside it.
///
/// The band exists because hours are the most leveraged input in the model by
/// an order of magnitude: at $27.50/h plus 7.65% payroll tax plus $52.99/h
/// overhead, one man-hour is $82.59 fully loaded, so a doubled hour figure
/// roughly doubles the quote. A real job priced at 10.4 SF per man-hour when
/// the work runs at about 24 — 2.3× the hours it takes — is what these bounds
/// exist to catch, and nothing on screen questioned it at the time.
pub const SF_PER_MAN_HOUR_MIN: f64 = 8.0;
/// Upper edge of the plausibility band. See [`SF_PER_MAN_HOUR_MIN`].
pub const SF_PER_MAN_HOUR_MAX: f64 = 60.0;

/// How an area's SF per man-hour reads against the plausibility band.
///
/// A flag rather than a threshold the UI re-implements: the engine owns the
/// bounds, the screen owns the wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Productivity {
    /// No hours entered, or zero — there is no ratio to judge.
    Unknown,
    /// Fewer SF per man-hour than the band: MORE hours than this much floor
    /// usually takes. The direction that overprices a job.
    Low,
    /// Within the band.
    Typical,
    /// More SF per man-hour than the band: fewer hours than usual. The
    /// direction that underbids one.
    High,
}

impl Productivity {
    fn of(sf_per_man_hour: Option<f64>) -> Self {
        match sf_per_man_hour {
            None => Productivity::Unknown,
            Some(v) if v < SF_PER_MAN_HOUR_MIN => Productivity::Low,
            Some(v) if v > SF_PER_MAN_HOUR_MAX => Productivity::High,
            Some(_) => Productivity::Typical,
        }
    }
}

/// SF per man-hour, or `None` when there are no hours to divide by.
///
/// Returning `None` rather than infinity is the point: hours are routinely
/// empty mid-entry, and a readout showing `Infinity` or `NaN` beside a dollar
/// figure looks like a broken quote rather than an unfinished one.
fn sf_per_man_hour(area_sf: f64, man_hours: f64) -> Option<f64> {
    (man_hours > 0.0 && area_sf.is_finite()).then(|| area_sf / man_hours)
}

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
    /// Grinding grit this area is specified at, keyed into
    /// [`super::RateCard::grit_levels`]. `None` = quote at the system's own
    /// standard, which is what every area does until a spec says otherwise.
    ///
    /// Setting one on a system that does not grind is an ERROR rather than an
    /// ignored field — "I selected 1200 grit and it changed nothing" is
    /// indistinguishable from a bug.
    #[cfg_attr(feature = "serde", serde(default))]
    pub grit_level: Option<String>,
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

/// A customer-facing line of work: one or more areas, presented as one name
/// at one lump sum.
///
/// Areas are TAKEOFF units — a bathroom on page 7 and a corridor on page 22
/// are two measurements. What the customer buys is neither of those; it is
/// "Restrooms — grind & seal, $X". A bid item is that layer, and it is
/// deliberately not page-scoped.
///
/// Membership rules are enforced in [`price_job`], not here: an area belongs
/// to exactly one item, every area belongs to one (unclaimed areas are given
/// their own — see [`resolve_bid_items`]), and every member shares a system.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BidItem {
    pub id: u64,
    /// What appears on the proposal. Free text, independent of area names.
    pub name: String,
    /// Member [`AreaInput::id`]s, in presentation order.
    pub area_ids: Vec<u64>,
    /// `true` flags this OUT of the base bid: priced identically, totalled
    /// separately, never silently summed into the headline.
    #[cfg_attr(feature = "serde", serde(default))]
    pub alternate: bool,
}

/// A whole job.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct JobInput {
    pub areas: Vec<AreaInput>,
    /// Grouping of areas into customer-facing lines. EMPTY IS LEGAL and means
    /// "one item per area" — which is exactly what every project saved before
    /// bid items existed deserializes to, so nothing needs migrating.
    #[cfg_attr(feature = "serde", serde(default))]
    pub bid_items: Vec<BidItem>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub job_costs: JobCosts,
    /// Single job-level rate applied uniformly across the whole quote.
    pub margin: f64,
    #[cfg_attr(feature = "serde", serde(default))]
    pub wage_source: WageSource,
}

/// Job-level facts every area must be priced against, bound together so they
/// cannot drift apart.
///
/// `wage_source` used to be a loose argument on [`price_area`]. That is the
/// dangerous kind of API: a live-updating panel calls `price_area` on every
/// keystroke, and passing the wrong source produces no error and no warning —
/// just a quote low by the whole Davis-Bacon delta. Binding the card and the
/// wage together, and RESOLVING the wage once at construction, makes pricing
/// an area against the wrong basis structurally impossible rather than merely
/// discouraged.
///
/// Constructing this is also the single place an unavailable wage schedule is
/// caught: [`PricingContext::new`] fails once, up front, instead of every area
/// failing separately.
#[derive(Debug, Clone, Copy)]
pub struct PricingContext<'a> {
    card: &'a RateCard,
    wage_source: WageSource,
    wage_per_hour: f64,
}

impl<'a> PricingContext<'a> {
    /// Bind a card to a wage basis, resolving the hourly wage now.
    ///
    /// Errors if the basis has no rates loaded — Davis-Bacon before the
    /// prevailing schedule is entered. It does NOT fall back to the standard
    /// wage: a prevailing-wage job priced at 27.50 underbids by roughly half
    /// and looks entirely legitimate.
    pub fn new(card: &'a RateCard, wage_source: WageSource) -> Result<Self, PricingError> {
        let wage_per_hour = card
            .labor
            .wage_for(wage_source)
            .ok_or(PricingError::WageUnavailable(wage_source))?;
        Ok(Self {
            card,
            wage_source,
            wage_per_hour,
        })
    }

    pub fn card(&self) -> &'a RateCard {
        self.card
    }
    pub fn wage_source(&self) -> WageSource {
        self.wage_source
    }
    /// The resolved hourly wage, already chosen by basis.
    pub fn wage_per_hour(&self) -> f64 {
        self.wage_per_hour
    }
}

// ---------- outputs ----------

/// Where a line came from, so the UI can group and explain it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum LineSource {
    /// Named by the system recipe.
    Recipe,
    /// Contributed (or substituted in) by an add-on.
    AddOn { key: String },
    /// A consumable, scaled by the system's multiplier.
    Consumable,
}

/// One itemized line. Carries everything the UI or a PDF needs, so neither
/// recomputes anything.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct QuoteLine {
    pub product_id: String,
    pub product_name: String,
    pub unit: ProductUnit,
    pub source: LineSource,
    /// The system's consumable multiplier already folded into `base_quantity`
    /// (1.0 for everything that is not a consumable). Reported so the UI can
    /// say "seal runs brushes at 0.25×" instead of showing an unexplained
    /// quantity, and so a 0.0 line is legible as "this system does not use it".
    pub multiplier: f64,
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
#[cfg_attr(feature = "serde", derive(Serialize))]
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
    /// Grit this area was quoted at, echoed back. `None` = the system's own
    /// standard.
    pub grit_level: Option<String>,
    /// The escalator actually applied to the entered hours (1.0 when the area
    /// is at its system's standard, or when the ladder is still seeded flat).
    pub grit_labor_multiplier: f64,
    /// `crew × hours` AS ENTERED, before any grit escalation. Reported so a
    /// changed total is attributable: if `man_hours` differs from this, the
    /// grit did it, not a mistyped hour.
    pub base_man_hours: f64,
    /// `base_man_hours × grit_labor_multiplier` — the figure labor and
    /// overhead are actually computed from.
    pub man_hours: f64,
    /// `area_sf / man_hours`, or `None` when no hours are entered.
    pub sf_per_man_hour: Option<f64>,
    /// How that reads against the plausibility band. Advisory only.
    pub productivity: Productivity,
    /// materials + consumables + labor + overhead + manual costs.
    pub cost: f64,
}

/// One bid item, priced as a lump sum.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct BidItemQuote {
    pub id: u64,
    pub name: String,
    pub area_ids: Vec<u64>,
    /// Shared by every member — enforced, so the scope narrative this will
    /// carry has exactly one recipe behind it.
    pub system_key: String,
    pub alternate: bool,
    /// Σ member area costs. Job-level costs are NOT included; they belong to
    /// the job, not to any one line of work — see [`JobQuote::cost`].
    pub cost: f64,
    /// `cost / (1 - margin)` at the job's margin. The lump sum.
    pub price: f64,
    pub profit: f64,
    pub area_sf: f64,
    pub man_hours: f64,
    /// Distinct grit levels among the members, sorted; empty when none is set.
    /// More than one is legal and correctly priced — each area carries its own
    /// hours — but it is REPORTED because one lump sum labelled with a single
    /// grit would misdescribe the work in a way the price cannot reveal.
    pub grit_levels: Vec<String>,
    /// `grit_levels.len() > 1`, or one explicit level mixed with unset members.
    pub mixed_grit: bool,
}

/// The whole job, priced.
///
/// # Base bid versus alternates
///
/// `cost`, `price`, and `profit` are the BASE BID — the number quoted. An
/// alternate is priced identically and reported in `bid_items` and the
/// `alternate_*` rollups, but is never summed into the headline: "add double
/// broadcast: +$X" is an offer, not part of what was bid.
///
/// With no alternates (the default, and every project saved before bid items
/// existed) base is everything and these fields mean exactly what they always
/// did.
///
/// Job-level costs sit inside the BASE bid. Mobilization and permits are not
/// contingent on an alternate being accepted, and an alternate that quietly
/// carried a share of them would be priced differently depending on what else
/// happened to be on the quote. One consequence to know: Σ base bid item
/// prices is short of `price` by the marked-up job costs, reported as
/// [`JobQuote::job_cost_price`] so the arithmetic closes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct JobQuote {
    pub areas: Vec<AreaQuote>,
    /// Every bid item, base and alternate, in presentation order.
    pub bid_items: Vec<BidItemQuote>,
    /// Σ BASE-bid area costs. Alternates are in `alternate_cost`.
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
    /// The job costs above, marked up at the job margin. Σ base bid item
    /// prices plus this equals [`JobQuote::price`] — stated so a proposal
    /// that lists lump sums can be shown to add up.
    pub job_cost_price: f64,
    /// Σ ALTERNATE bid item costs. Zero when there are none.
    pub alternate_cost: f64,
    /// Σ alternate lump sums — what accepting every alternate would add.
    /// Never part of `price`.
    pub alternate_price: f64,
    pub alternate_profit: f64,
    /// Σ BASE-bid area square footage — the denominator's partner in the
    /// blended productivity figure below.
    pub area_sf: f64,
    /// Σ escalated man-hours across BASE-bid areas. Alternates are excluded
    /// for the same reason they are excluded from the price: they describe
    /// work that may never happen, and blending them into a rate for work
    /// that will would describe a job nobody is going to run. Per-area
    /// readouts still cover every area, alternate or not.
    pub man_hours: f64,
    /// Blended `area_sf / man_hours` for the whole job. NOT the mean of the
    /// per-area figures: a 200 SF closet and a 20,000 SF warehouse are not
    /// equal votes on how fast this job runs.
    pub sf_per_man_hour: Option<f64>,
    /// How the blended figure reads against the band. Advisory only.
    pub productivity: Productivity,
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
    #[error("area `{area}`: unknown grit level `{grit}`")]
    UnknownGritLevel { area: String, grit: String },
    #[error("duplicate bid item id `{0}`")]
    DuplicateBidItem(u64),
    #[error("bid item `{bid_item}` has no areas in it")]
    EmptyBidItem { bid_item: String },
    #[error("bid item `{bid_item}` names area {area_id}, which is not on this job")]
    BidItemUnknownArea { bid_item: String, area_id: u64 },
    #[error(
        "area `{area}` is in two bid items (`{first}` and `{second}`) — an area \
         belongs to exactly one line of work"
    )]
    AreaInTwoBidItems {
        area: String,
        first: String,
        second: String,
    },
    #[error(
        "bid item `{bid_item}` mixes systems ({systems}) — one lump sum can carry \
         only one scope narrative, and a mixed item would hide one system's work \
         inside another's description"
    )]
    BidItemMixedSystems { bid_item: String, systems: String },
    #[error(
        "area `{area}`: system `{system}` involves no grinding, so a grit level \
         (`{grit}`) has no meaning on it"
    )]
    GritNotApplicable {
        area: String,
        system: String,
        grit: String,
    },
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
                        set.push((
                            product_id.clone(),
                            LineSource::AddOn {
                                key: add_on.key.clone(),
                            },
                        ));
                    }
                }
                ProductOp::Remove { product_id } => set.retain(|(id, _)| id != product_id),
                ProductOp::Replace { product_id, with } => {
                    match set.iter().position(|(id, _)| id == product_id) {
                        Some(i) => {
                            set[i] = (
                                with.clone(),
                                LineSource::AddOn {
                                    key: add_on.key.clone(),
                                },
                            );
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
    multiplier: f64,
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
        multiplier,
        base_quantity,
        factor,
        quantity,
        unit_cost: product.unit_cost,
        extended_cost: quantity * product.unit_cost,
        suppressed: factor == 0.0,
    })
}

// ---------- the buildup ----------

/// Price one area. The wage basis comes from the context and cannot be
/// mismatched at the call site.
pub fn price_area(ctx: &PricingContext<'_>, area: &AreaInput) -> Result<AreaQuote, PricingError> {
    let card = ctx.card;
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
        lines.push(line(area, product, source, 1.0, base)?);
    }

    // Consumables, scaled by this system's multiplier. A multiplier of 0.0
    // means the system does not use it -- still REPORTED, at $0.00 with the
    // multiplier attached, for the same reason a suppressed line is: an
    // omitted row is indistinguishable from one nobody considered, and
    // "seal doesn't use trowels" was inferred from a template sheet rather
    // than established as a law. The UI hides them behind a toggle.
    for cons in card.consumables() {
        let mult = system
            .consumable_multipliers
            .get(&cons.id)
            .copied()
            .ok_or_else(|| PricingError::MissingConsumableMultiplier {
                system: system.key.clone(),
                consumable: cons.id.clone(),
            })?;
        // No driver lookup at 0.0: a system that does not use a product must
        // not fail the area for want of a measurement it will never multiply.
        let base = if mult == 0.0 {
            0.0
        } else {
            cons.rate * driver(area, cons)? * mult
        };
        lines.push(line(area, cons, LineSource::Consumable, mult, base)?);
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

    // Grit escalates HOURS, never cost directly. Everything downstream is a
    // function of man-hours, so labor, overhead, and the productivity readout
    // all pick the change up with no special-casing anywhere.
    let grit_multiplier = grit_escalator(card, system, area)?;
    let base_man_hours = labor_in.man_hours();
    let man_hours = base_man_hours * grit_multiplier;

    let labor = ctx.wage_per_hour * man_hours * (1.0 + card.labor.payroll_tax_rate)
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
        grit_level: area.grit_level.clone(),
        grit_labor_multiplier: grit_multiplier,
        base_man_hours,
        man_hours,
        sf_per_man_hour: sf_per_man_hour(area.area_sf, man_hours),
        productivity: Productivity::of(sf_per_man_hour(area.area_sf, man_hours)),
        cost: materials + consumables + labor + overhead + manual_costs,
    })
}

/// The man-hours escalator for an area, or 1.0 when it names no grit.
///
/// Exactly 1.0 — not "approximately" — when there is no grit, and `x * 1.0 ==
/// x` in IEEE-754, so an area with no grit prices to the identical bits it did
/// before this field existed.
fn grit_escalator(
    card: &RateCard,
    system: &super::SystemRecipe,
    area: &AreaInput,
) -> Result<f64, PricingError> {
    let Some(grit) = &area.grit_level else {
        return Ok(1.0);
    };
    if system.standard_grit.is_none() {
        return Err(PricingError::GritNotApplicable {
            area: area.name.clone(),
            system: system.key.clone(),
            grit: grit.clone(),
        });
    }
    card.grit_labor_multiplier(&system.key, grit)
        .ok_or_else(|| PricingError::UnknownGritLevel {
            area: area.name.clone(),
            grit: grit.clone(),
        })
}

/// Every area's bid item, explicit ones first, then one synthesized per area
/// nobody claimed.
///
/// The synthesis is what makes this layer free: a job that names no bid items
/// at all resolves to one item per area, which is exactly how the quote behaved
/// before bid items existed. No migration, no legacy branch, no flag.
///
/// Synthesized ids are allocated above every explicit id so they cannot
/// collide, and assignment is deterministic — the same input always resolves
/// to the same ids.
pub fn resolve_bid_items(job: &JobInput) -> Result<Vec<BidItem>, PricingError> {
    let name_of = |id: u64| {
        job.areas
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| format!("area {id}"))
    };

    let mut seen_ids: BTreeMap<u64, ()> = BTreeMap::new();
    // area id -> the item that claimed it, so a second claim can name both.
    let mut claimed: BTreeMap<u64, String> = BTreeMap::new();
    let mut out: Vec<BidItem> = Vec::new();

    for item in &job.bid_items {
        if seen_ids.insert(item.id, ()).is_some() {
            return Err(PricingError::DuplicateBidItem(item.id));
        }
        if item.area_ids.is_empty() {
            return Err(PricingError::EmptyBidItem {
                bid_item: item.name.clone(),
            });
        }
        for &area_id in &item.area_ids {
            if !job.areas.iter().any(|a| a.id == area_id) {
                return Err(PricingError::BidItemUnknownArea {
                    bid_item: item.name.clone(),
                    area_id,
                });
            }
            if let Some(first) = claimed.get(&area_id) {
                return Err(PricingError::AreaInTwoBidItems {
                    area: name_of(area_id),
                    first: first.clone(),
                    second: item.name.clone(),
                });
            }
            claimed.insert(area_id, item.name.clone());
        }
        out.push(item.clone());
    }

    let mut next_id = seen_ids.keys().copied().max().map_or(1, |m| m + 1);
    for area in &job.areas {
        if claimed.contains_key(&area.id) {
            continue;
        }
        out.push(BidItem {
            id: next_id,
            // Named from the area, because that is the only name anyone has
            // given this work yet.
            name: area.name.clone(),
            area_ids: vec![area.id],
            alternate: false,
        });
        next_id += 1;
    }
    Ok(out)
}

/// Roll priced areas up into one bid item, enforcing the shared-system rule.
fn price_bid_item(
    item: &BidItem,
    by_id: &BTreeMap<u64, &AreaQuote>,
    margin: f64,
) -> Result<BidItemQuote, PricingError> {
    let members: Vec<&AreaQuote> = item
        .area_ids
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect();

    // One scope narrative needs one recipe. This is the rule that stops an
    // epoxy area being hidden inside a grind-and-seal line.
    let mut systems: Vec<&str> = members.iter().map(|a| a.system_key.as_str()).collect();
    systems.sort_unstable();
    systems.dedup();
    if systems.len() > 1 {
        return Err(PricingError::BidItemMixedSystems {
            bid_item: item.name.clone(),
            systems: systems.join(", "),
        });
    }

    let cost: f64 = members.iter().map(|a| a.cost).sum();
    let (price, profit) = price_from_cost(cost, margin)?;

    let mut grits: Vec<String> = members.iter().filter_map(|a| a.grit_level.clone()).collect();
    grits.sort();
    grits.dedup();
    // Two areas at different grits is legal and priced correctly -- each
    // carries its own hours. It is flagged because ONE lump sum under ONE
    // grit-bearing name would describe work that is not what was priced, and
    // no number on the page reveals that.
    let mixed_grit = grits.len() > 1 || (grits.len() == 1 && members.len() > grits.len() && members.iter().any(|a| a.grit_level.is_none()));

    Ok(BidItemQuote {
        id: item.id,
        name: item.name.clone(),
        area_ids: item.area_ids.clone(),
        system_key: systems.first().map(|s| s.to_string()).unwrap_or_default(),
        alternate: item.alternate,
        cost,
        price,
        profit,
        area_sf: members.iter().map(|a| a.area_sf).sum(),
        man_hours: members.iter().map(|a| a.man_hours).sum(),
        grit_levels: grits,
        mixed_grit,
    })
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
    // One context for the whole job: every area is priced against the same
    // wage basis by construction, and an unavailable schedule fails here once.
    let ctx = PricingContext::new(card, job.wage_source)?;
    let areas = job
        .areas
        .iter()
        .map(|a| price_area(&ctx, a))
        .collect::<Result<Vec<_>, _>>()?;

    // Group into customer-facing lines. Every area lands in exactly one,
    // whether or not the caller said anything about grouping.
    let by_id: BTreeMap<u64, &AreaQuote> = areas.iter().map(|a| (a.area_id, a)).collect();
    let bid_items = resolve_bid_items(job)?
        .iter()
        .map(|it| price_bid_item(it, &by_id, job.margin))
        .collect::<Result<Vec<_>, _>>()?;

    // The split the whole layer exists for. An alternate is priced exactly
    // like anything else and then kept OUT of the number being quoted.
    let (alt, base): (Vec<&BidItemQuote>, Vec<&BidItemQuote>) =
        bid_items.iter().partition(|b| b.alternate);

    let area_cost: f64 = base.iter().map(|b| b.cost).sum();
    let cost = area_cost + job.job_costs.total();
    let (price, profit) = price_from_cost(cost, job.margin)?;
    let (job_cost_price, _) = price_from_cost(job.job_costs.total(), job.margin)?;

    let alternate_cost: f64 = alt.iter().map(|b| b.cost).sum();
    let alternate_price: f64 = alt.iter().map(|b| b.price).sum();
    let alternate_profit: f64 = alt.iter().map(|b| b.profit).sum();

    // Unconfirmed recipes are reported across EVERY area, alternate included:
    // an alternate the customer accepts is quoted from the same inferred
    // recipe, so hiding the caveat until acceptance would be backwards.
    let mut unconfirmed: Vec<String> = areas
        .iter()
        .filter(|a| !a.system_confirmed)
        .map(|a| a.system_key.clone())
        .collect();
    unconfirmed.sort();
    unconfirmed.dedup();

    // Blended from the BASE bid's totals, not averaged across areas — see the
    // notes on JobQuote::man_hours and JobQuote::sf_per_man_hour.
    let total_sf: f64 = base.iter().map(|b| b.area_sf).sum();
    let total_hours: f64 = base.iter().map(|b| b.man_hours).sum();
    let blended = sf_per_man_hour(total_sf, total_hours);

    Ok(JobQuote {
        areas,
        bid_items,
        area_cost,
        job_costs: job.job_costs,
        cost,
        margin: job.margin,
        price,
        profit,
        job_cost_price,
        alternate_cost,
        alternate_price,
        alternate_profit,
        area_sf: total_sf,
        man_hours: total_hours,
        sf_per_man_hour: blended,
        productivity: Productivity::of(blended),
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

// ---------- JSON boundary (the host prices through this) ----------

/// Everything that can go wrong pricing from JSON, kept separate so the host
/// can tell a bad rate card from a bad job from a pricing failure.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PricingJsonError {
    #[error("rate card: {}", .0.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))]
    Card(Vec<super::RateCardError>),
    #[error("job input is not valid JSON: {0}")]
    Job(String),
    #[error("{0}")]
    Pricing(#[from] PricingError),
}

/// Price a job from JSON and return the quote as JSON.
///
/// The single entry point the harness uses. Stateless on purpose: the card is
/// re-parsed per call so there is no cached rate card to go stale against the
/// data file, and "recompute on every change" needs no invalidation logic.
///
/// The result serializes but deliberately does NOT deserialize — a quote may
/// be emitted for display and never read back as truth (invariant 5).
#[cfg(feature = "serde")]
pub fn price_job_json(card_json: &str, job_json: &str) -> Result<String, PricingJsonError> {
    let card = super::from_json(card_json).map_err(PricingJsonError::Card)?;
    let job: JobInput =
        serde_json::from_str(job_json).map_err(|e| PricingJsonError::Job(e.to_string()))?;
    let quote = price_job(&card, &job)?;
    serde_json::to_string(&quote).map_err(|e| PricingJsonError::Job(e.to_string()))
}
