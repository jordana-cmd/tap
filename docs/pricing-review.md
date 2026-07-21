# Pricing review — every number not yet confirmed against a real job

One page listing every rate, cost and modelling assumption in the estimating
stack that has **not** been checked against a completed job. Everything here
prices real bids today; nothing here has been validated.

The live values are in `crates/engine-core/src/assembly/seeds.rs`
(`PRODUCTS`, `CONSUMABLES`, `consumable_profiles`) and
`crates/engine-core/src/pricing.rs` (`COST_DEFAULTS` mirror in the harness).
`docs/consumable-profiles.md` holds the readable profile tables; this file is
the *review queue*.

**How to clear an item:** check it against a real job, then either confirm the
number in place (move the row to "Confirmed" with the job reference) or change
it — and update the affected to-the-cent test in the same commit.

---

## 1. Rates that look wrong

### ⚠⚠ Quartz at 1.0 lb/SF — the largest unverified number in the system

| | |
|---|---|
| where | `PRODUCTS` → `("quartz", "Quartz (Double Broadcast)", Unit::Lb, 1.0, 1.0)` |
| means | 1.0 lb of quartz per square foot, at $1.00/lb |
| effect | **$1.00/SF — $5,000 of quartz on a 5,000 SF floor** |
| context | It is ~3× the entire 2-Coat Epoxy system ($0.3456/SF) and ~2.9× the flake system's flake lines. A Quartz Broadcast job prices at $1.984/SF of product against $0.346/SF for plain epoxy. |
| provenance | Carried over verbatim from `reference/mcfc-quoting-tool.jsx`, whose own UI text for this system reads **"Quartz system — check line items"**. The source author flagged it and never resolved it. |
| suspicion | The unit and the rate are both suspect. 1.0 lb/SF is a plausible *broadcast* rate for a full-refusal quartz floor, but $1.00/lb is a round placeholder, and the entry is the only one in `PRODUCTS` with a rate of exactly 1.0 — the shape of an unfilled default. |

**To check:** pull a completed quartz job. How many pounds of quartz were
actually broadcast, over how many SF, at what delivered cost per pound?

### Whips at 0.00000005/SF

`("whips", "Whips", 5.49, 0.00000005)` — 5×10⁻⁸ per SF is $0.00137 on a 5,000 SF
job. Either a typo in the source (a dropped exponent) or a deliberate
near-zero. It is small enough not to matter and wrong-looking enough to fix.

---

## 2. Modelling assumptions that move money

### ⚠⚠ Trowels billed per application — now the dominant consumable

| | |
|---|---|
| where | `CONSUMABLES` → `("trowel", "Trowels", 32.99, 0.002, Scope::PerApplication)` |
| effect | **$329.90 per coating application at 5,000 SF** — the single largest consumable line, larger than roller covers ($126.00) and all cup sizes combined ($212.85). |
| tension | The code already calls this out: a trowel is a **durable tool**, not something discarded per job. The 0.002/SF rate reads as *amortization across a full coat*. It was kept in the coating profile only (removed from repair) as a partial fix. |
| newly urgent | Before the v3 catalog restructure, "Epoxy + High Wear Urethane" was one system = one application = one trowel charge. It is now 2-Coat Epoxy + a High Wear add-on = **two** applications = **two** trowel charges. That single change is the bulk of a +$759.13 consumables swing at 5,000 SF (see §4). |

**To check:** on a real two-coat job, how many trowels were consumed or worn
out? If the answer is "none, we own them," trowels should be a fixed overhead
line, not a per-application consumable — which would also make the v3
consumables delta almost entirely disappear.

### ⚠ The grinding profile membership

Grinding is modelled as Quart Cups + Gloves + Rags + Trash Bags — no rollers, no
brushes, no large cups, no durable tools. This is a **guess**, never checked. A
grind & seal job applies two sealer coats; those plausibly need rollers back.
Affects Concrete Polish and Grind & Seal.

**To check:** a completed polish job and a completed grind & seal job. What did
the crew actually burn?

### ⚠ The coating/grinding/repair split itself

The source tool applied **all** consumables to **every** job — it had no split
at all. The three-profile model is entirely ours. The split is more defensible
than the flat rate, but "more defensible" is not "validated."

### ⚠ Per-application vs per-area scope

Which consumables scale with each coat (`PerApplication`) and which are charged
once per area (`PerArea`) is a judgement call made per item. PPE and cleanup
went per-area; everything applicator-shaped went per-application. Untested.

---

## 3. Cost-stack defaults

Defaults in the harness (`COST_DEFAULTS`), applied to every new project:

| input | default | status |
|---|---|---|
| wage $/hr | 27.50 | ⚠ inherited from the source tool; check against current payroll |
| payroll tax % | 7.65 | ✅ this is the employer FICA rate — correct by law |
| overhead $/labor-hr | 52.99 | ⚠⚠ **unvalidated and large** — at 3 crew × 24 hr it contributes $3,815.28, more than labor itself. Derivation unknown. |
| legacy adder % | 12.85 | ⚠ "Rob's formula" — origin undocumented |
| card fee % | 3.50 | ⚠ check against the current merchant agreement |
| profit % | 30 | business input, not a measurement — no validation needed |
| insurance / equipment / permits / travel / mobilization / misc | 0 | entered per job |

**Overhead is the one to check first.** $52.99 per labor-hour applied to crew ×
hours is the second-largest line in a typical bid after materials, and nothing
records where the figure came from.

---

## 4. Numbers that changed in the v3 restructure

Materials did **not** change. Consumables did, because the same job now has
more profile-bearing applications.

| fixture (5,000 SF) | before | after | delta |
|---|---|---|---|
| Epoxy + HW — materials | $4,920.00 | $4,920.00 | **$0.00** |
| Epoxy + HW — consumables | $868.72 | $1,627.85 | +$759.13 |
| Epoxy + HW — grand | $5,788.72 | $6,547.85 | +$759.13 |
| Epoxy + HW + Crack Repair — materials | $5,327.55 | $5,327.55 | **$0.00** |
| Epoxy + HW + Crack Repair — grand | $6,243.35 | $7,002.48 | +$759.13 |

The delta is one extra coating application. **$329.90 of it is the second
trowel charge** — see §2. If trowels move to overhead, the restructure becomes
nearly cost-neutral.

---

## Confirmed against a real job

*(nothing yet)*
