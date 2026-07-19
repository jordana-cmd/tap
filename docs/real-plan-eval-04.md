# Real-Plan Evaluation 04 — hatch-structure filter vs the eval-03 baseline

**Date:** 2026-07-19 · **Engine:** through the lattice-classifier commit (`6e558da`) · **Fixture:** 002, pages 37 (A7.0 floor finish plan, heavy tile hatch) and 18 (A1.0 floor plan, hatch-free control) · **Protocol:** eval-03's exact room set, seeds, fpi 4.0, ppf 10, min_width at the (fixed) derived default 0.18, door-gap sweep {3.5, 2.0, 1.0} — run twice: `exclude_hatch` off (baseline) and on.

## 1. Verdict

**The hatch filter removes the hatch obstacle where hatch was the obstacle — and the honest boundary is now visible: fixtures are the next one.** On page 37 with exclusion ON:

- **STORAGE (the eval-03 flagship `SEED_TRAPPED`-at-every-setting case): unblocked** — 59.9 SF @ dg 2.0, 131.9 SF @ dg 1.0. Still a fragment (shelving/equipment ink bounds it), but a measurable region instead of a dead click.
- **COOLER: trapped → 31.0/34.6/36.5 SF** across the sweep; **OFFICE: seed-on-wall/trapped → 131.9 SF @ dg 1.0**.
- The open FOH grew 491.2 → 498.8 SF (+1.5%), within its own open-plan extent — **the leak check found zero suspected wall absorptions** (no room grew past a printed expectation; p18 areas are bit-identical with exclusion on/off).
- **MEN / WOMEN / FREEZER remain trapped — by plumbing fixtures and shelving at wall weight, not by hatch.** The filter does not claim fixtures; that is the promoted next stage (fixture-cluster suppression), not a hatch-filter failure.
- Page 18 (hatch-free control): 1,350 segments classified (poché/pattern bits), room results unchanged in every cell — the filter neither helps nor harms where hatch isn't the problem. **No regressions anywhere; 100% of `measure_polyline` cross-checks match (both pages, both modes).**

Classification cost: p37 3,214 of 10,175 segments flagged in **43 ms**; p18 in 53 ms. The hatch-removed skeleton render shows the storage/kitchen region essentially cleared of mesh with walls, door openings, and fixture outlines intact.

## 2. What it took — the classifier that shipped vs the one that was planned

The planned sequential "CV-comb" (accrete rails into combs via each comb's last member, pitch-CV gated) **failed on the real sheet and was replaced during the build**, with each step driven by instrumented evidence (segment dumps + join traces, all reproducible via the checked-in `hatch_debug` test):

1. Sequential accretion died on interleave: fixture ink between tile courses captured every course into a different junk comb before any comb reached 5 rails (35/441 courses classified). Neither first-match, member-count-preferring, CV-preferring, nor extent-similarity join orders survived — all traced, all failed.
2. **v2 decouples pitch discovery from chaining**: windowed pairwise Δρ **voting** with harmonic folding finds each family's fundamental pitches straight through arbitrary interleave; rails then chain onto the discovered **pitch lattice** (ρ ≈ last + k·pitch, k ≤ 3, tolerance = the same-line tolerance), where off-lattice junk *cannot* join by construction.
3. Pairwise voting exposed a real false positive the plan had only feared: a row of six equal-width rooms qualified its 162-pt module (same-role wall faces chain on the lattice). The **`min_density` gate** — median rail span ≥ 4 × pitch, the "dense field" half of the hatch signature — rejects it (measured ratios: tile mesh ~145, poché ~20, room-row walls ~1.7). This failure is preserved as the `equal_room_row_rails_not_hatch` test.
4. The extent-outlier wall rescue was re-based from median-span (which wrongly rescued most of the real mesh — course fragments vary wildly in span) to **union-extent overshoot**: a wall pokes beyond the field's aggregate extent; a long course lies within it.

Final derived parameters on the real pages (all data-derived, `hatch_params_json`): p37 `{merge 0.6, dash_gap 18.3, min_rails 5, lattice_tol 0.6, max_pitch 39, overlap 0.5, outlier 1.5, density 4}`; p18 identical shape with `max_pitch 30.75`.

## 3. Per-room table (page 37, vector path, min_width 0.18)

| Room | Baseline best (hatch off) | Hatch ON best | Δ class |
|---|---|---|---|
| STORAGE (hatch seed) | `SEED_TRAPPED` all 3 dg | **131.9 SF** @ dg 1.0 (59.9 @ 2.0) | **trapped → fragment** |
| COOLER | `SEED_TRAPPED` all 3 | **36.5 SF** @ dg 1.0 | **trapped → fragment** |
| OFFICE | `SEED_ON_WALL` all 3 | **131.9 SF** @ dg 1.0 | **blocked → fragment** |
| DINING+CUSTOMER+KIOSKS (open FOH) | 491.2 SF @ dg 1.0 | 498.8 SF @ dg 1.0 | clean-merge, +1.5% (cleared pockets) |
| MEN / WOMEN | 9.9 / 7.5 SF | 9.9 / 7.5 SF | unchanged — fixture-bounded |
| FREEZER | `SEED_TRAPPED` | `SEED_TRAPPED` | unchanged — shelving/fixture ink |

Page 18: all 9 rooms identical with exclusion on/off (control holds).

## 4. Work queue update

### A. Engine
1. **Fixture-cluster suppression is now the top structural item** (was second stage): toilets, sinks, shelving, and equipment at wall weight bound MEN/WOMEN/FREEZER and clip STORAGE/COOLER to fragments. Signature: small closed loops / dense compact clusters that are not room boundaries.
2. Hatch v2 residual: ~40% of tile-course fragments in transition zones (multi-zone tile offsets, FOH borders) evade chains; harmless for the measured rooms here but worth a pass once fixture suppression lands (candidate: per-zone phase re-anchoring).
3. Carried: `default_min_width` two-thinnest rule and the form-XObject matrix fix landed this build (eval-03 queue A.2/A.3 closed).

### B. Tuning
1. dg 1.0 remains the productive door-gap on this fixture at 1/4" scale (carried from eval-03).
2. `min_density` default 4.0 is the newest constant with the least mileage — revisit when a sparse-hatch sheet (wide-pitch tile, e.g. 12"+) appears.

### C. Inherent
1. Open-plan FOH merging is correct geometry (carried).
2. Real hatch is not one clean grid: multiple zones at offset phases, occlusion-fragmented courses, scribble fills interleaved in the same angle family — any future pattern work must assume this (this eval's traces are the reference).

## 5. Riders

Fixture uncommitted (gitignore-protected); no client identifiers in this doc; every number from the scripted rig (`eval4.mjs`, seeds identical to eval-03 §3); skeleton evidence retained in the scratchpad; interaction suite + extraction probe green after every engine change.
