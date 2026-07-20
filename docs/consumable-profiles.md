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
| Epoxy 2-Coat | system | coating | full-area coat |
| Epoxy + High Wear Urethane | system | coating | full-area coat |
| Polyurea Flake | system | coating | full-area coat |
| Flake Double Broadcast | system | coating | full-area coat |
| Quartz Double Broadcast | system | coating | full-area coat |
| Polished Concrete | system | **grinding** ⚠ | grind |
| Grind & Seal | system | **grinding** ⚠ | grind |
| Moisture Mitigation (H2 Out) | add-on | coating | full-area coat |
| Crack Repair (Mender + Sand) | add-on | repair | localized repair |
| Crack Stitching | add-on | repair | localized repair |
| Joint Fill (Polyurea Caulk) | add-on (Linear) | repair | localized; area-driven consumables ⇒ none on a Linear job |
| Anti-Slip (Shark Grip) | add-on | **none** | additive — broadcast onto a coat already billed |
| Fast Cure Activator | add-on | **none** | additive — mixed into a coat already billed |

## What this changed (validation)

The phase-2 stacking fixture (Epoxy+HW + Crack Repair, 5,000 SF) moves from
**$6,196.27** → **$6,243.35** (+$47.08). The swing is Crack Repair's second
application adding just its own quart cups + brush; the trowel is a durable tool
kept in the coating profile only, and per-area items (gloves/rags/trash) stay
charged once. A single-system job is unchanged (per-app once + per-area once =
the old flat rate).
