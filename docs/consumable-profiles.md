# Consumable profiles — assignment & membership

Consumables are **not selectable**. They're auto-derived from the assemblies on
a measurement, via each assembly's consumable profile. This file is the readable
mirror of the seeded data in `crates/engine-core/src/assembly/seeds.rs`
(`consumable_profiles()`, `mcfc_systems()`, `mcfc_addons()`).

> ⚠ **UNVALIDATED DOMAIN PROPOSAL.** The MCFC quoting tool applied *all*
> consumables to *every* job — it did not split them by system. The
> coating/grinding/repair split below (membership **and** scope) is a proposal
> that needs review against real jobs. The **grinding** membership is the least
> certain.
>
> **See [`pricing-review.md`](pricing-review.md)** — the single page listing
> every unvalidated number in the estimating stack, including the quartz rate,
> the trowel amortization, and the overhead default.

## Scope of each consumable

`PerApplication` scales with each profile-bearing assembly stacked (two coats
use two sets); `PerArea` is charged once per area, deduped across the whole
effective stack.

| consumable | scope | note |
|---|---|---|
| 10-Quart Cups | PerApplication | |
| 5-Quart Cups | PerApplication | |
| 2.5-Quart Cups | PerApplication | |
| Quart Cups | PerApplication | |
| Brushes | PerApplication | |
| Roller Covers | PerApplication | |
| Mini Roller Covers | PerApplication | |
| **Trowels** | PerApplication | ⚠ **AMORTIZED TOOL, not a true consumable** — a trowel is durable, not discarded per job. The 0.002/SF rate reads as amortization across a full coat, so it lives in the **coating profile only** (removed from repair). Flagged for review. |
| **Whips** | PerApplication | ⚠ **AMORTIZED TOOL** (mixing paddle, durable) — same treatment: coating profile only, cost ≈ $0. Flagged for review. |
| Rags | PerArea | |
| Gloves | PerArea | |
| Trash Bags | PerArea | |

## Profile membership

- **Coating** — the full process (mix / apply / broadcast / clean): all 12
  (trowels & whips included here as amortized tools — see the scope table).
- **Grinding** ⚠ — Quart Cups, Gloves, Rags, Trash Bags. Trimmed to **mostly PPE
  + small cups**: a polish/grind crew doesn't burn rollers/brushes like a coating
  crew. ⚠ **needs checking against a real polish job** — a grind & seal sealer
  coat may warrant a roller back.
- **Repair** — Quart Cups, Brushes, Gloves, Rags, Trash Bags. A localized patch:
  no rollers, no large mixing cups, **no trowel** (durable tool).

## Assembly → profile

| assembly | kind | profile | behavior |
|---|---|---|---|
| 2-Coat Epoxy | system | coating | full-area coat |
| Polyurea w/ Flake | system | coating | full-area coat |
| Concrete Polish | system | **grinding** ⚠ | grind |
| Grind & Seal | system | **grinding** ⚠ | grind |
| High Wear Urethane Top Coat | add-on | coating | an extra coat — its own application ⚠ |
| Double Broadcast | add-on | coating | extra broadcast + 2nd top coat ⚠ |
| Quartz Broadcast | add-on | coating | extra broadcast ⚠ |
| Moisture Mitigation (H2 Out) | add-on | coating | full-area coat |
| Crack Repair (Mender + Sand) | add-on | repair | localized repair |
| Crack Stitching | add-on | repair | localized repair |
| Joint Fill (Polyurea Caulk) | add-on (Linear) | repair | localized; area-driven consumables ⇒ none on a Linear job |
| Anti-Slip (Shark Grip) | add-on | **none** | additive — broadcast onto a coat already billed |
| Fast Cure Activator | add-on | **none** | additive — mixed into a coat already billed |

## What this changed (validation)

**Phase 3 (profiles).** The stacking fixture (Epoxy+HW + Crack Repair, 5,000 SF)
moved **$6,196.27** → **$6,243.35** (+$47.08): Crack Repair's second application
adds just its own quart cups + brush; the trowel stays in coating only; per-area
items stay charged once.

**Phase 4b (catalog restructure).** Turning the three bundled systems into
incremental add-ons means a job that was one application is now two. Materials
are unchanged to the cent; consumables rise by one full coating set:

| fixture (5,000 SF) | materials | consumables | grand |
|---|---|---|---|
| Epoxy + HW — before | $4,920.00 | $868.72 | $5,788.72 |
| Epoxy + HW — after | $4,920.00 (=) | $1,627.85 | **$6,547.85** |
| + Crack Repair — before | $5,327.55 | $915.80 | $6,243.35 |
| + Crack Repair — after | $5,327.55 (=) | $1,674.93 | **$7,002.48** |

⚠ **$329.90 of the $759.13 swing is a second trowel charge** — and trowels are
flagged above as durable tools, not consumables. If that flag resolves the way
it reads, most of this delta disappears. Tracked in
[`pricing-review.md`](pricing-review.md).
