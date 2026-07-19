# Real-Plan Evaluation 03 — width-filter re-test on fixture 002 (full-size)

**Date:** 2026-07-19 · **Fixture:** 002 — full-size 22"×34" commercial set, 74 pages, ~42 MB (restaurant new-build; client/brand identifiers present in title blocks — flagged for redaction, not transcribed here) · **Pages evaluated:** 18 (A1.0 Floor Plan, dimensioned, hatch-free) and 37 (A7.0 Floor Finish Plan, heavy tile hatch — Jordan's working sheet) · **Pipeline:** identical to eval-01/02 (`harness/extract.js` + the shipped wasm bundle, headless).

## 1. Verdict on eval-02's open question

**No — width filtering does not separate walls from annotation even with a genuinely diverse pen table.** The eval-02 confound (0.31× plot collapse) is real but was not the deciding factor. On this full-size, professionally produced set, the pen table is diverse (0 / 0.36 / 0.72 / 0.84 / 1.32 / 1.74 pt) yet **walls, tile hatch, wall poché, fixtures, and annotation symbols all share the modal 0.72 pt bucket**. Skeleton renders prove it visually: at min_width 0.5 the tile-hatch mesh, toilet/sink outlines, and keynote diamonds survive alongside the walls at identical weight; at 0.78 the walls themselves vanish (every room `REGION_NOT_ENCLOSED` except the freezer/cooler boxes, whose insulated-box outlines are the one place "walls are thickest" actually holds, at 0.84/1.74).

The partial wins are real and worth keeping: the 0-pt and 0.36-pt layers (leader/grid/dimension linework — 4,548 segs on p18, 5,784 on p37) drop cleanly; door-swing arcs never enter the mask; and **text on this modern set is real PDF text (`showText` ops), which extraction skips entirely** — eval-02's dominant fragmenter (stroked SHX text) simply does not exist here. What remains — hatch, fixtures, dashed equipment lines at wall weight — is a **structure** problem, not a width problem.

**Hatch is the dominant obstacle, explicitly:** on the finish plan, the hatch-seeded STORAGE probe was `SEED_TRAPPED` at *every* width threshold and door gap on *both* paths — the 6" tile mesh at wall width turns every cell into its own sub-room and dilation closes them all. **This promotes the hatch-structure filter (periodic-mesh detection / short-segment cell clusters) to the next build item.**

## 2. Calibration & the full-size check

- Every one of the 74 pages measures **2448×1584 pt = exactly 34"×22"** — the sheet's "IF THIS SHEET IS NOT 22"x34" IT IS A REDUCED PRINT" caution is satisfied; this is a true full-size set.
- Calibration off the printed **74'-0" TO FACE OF STUD** overall (p18): both extension-line endpoints snapped as `endpoint` (CAD-exact), span 1332.00 pt → **fpi = 4.000000 — 0.000% deviation** from the title block's 1/4"=1'-0". On a full-size sheet, printed scale and calibration agree perfectly; §A5's REQUIRED verification would pass instantly here (and eval-01 showed exactly why it must still run).
- Cross-sheet consistency: chaining p37's top exterior wall returns a 16-segment run of 1335.78 pt ≈ 74.2 ft — page 37 shares page 18's scale to within the wall-face offset.

## 3. Width sweep

Histograms (segments per bucket):

| width_pts | p18 (A1.0) | p37 (A7.0) | visually contains (from skeleton renders) |
|---|---|---|---|
| 0 (zero-width) | 9,489 | 4,125 | leaders, dims, grid ticks, revision tables, misc fine linework |
| 0.36 | 423 | 1,659 | detail hatching, stipple, minor annotation |
| **0.72** | **7,905** | **3,826** | **walls + wall poché + tile hatch + fixtures + keynote symbols** |
| 0.84 | 302 | 68 | freezer/cooler box faces, select heavy edges |
| 1.32 / 1.74 | 28 / 474 | — / 497 | sheet border, title block, heaviest outlines |

`default_min_width` returns **0 on both pages** — the thinnest bucket is also the modal one, so the derived default degrades to a no-op by design (safe, but useless here; tuning item).

Sweep outcomes (9 rooms × door_gap {3.5, 2.0, 1.0}): min_width 0 → 0.1 → 0.5 change results only marginally (the dropped thin layers rarely bounded rooms); **0.78 destroys enclosure everywhere except freezer/cooler**. There is no width threshold on either page that removes hatch or fixtures while keeping walls.

## 4. Per-room results (best configuration found, either path)

Printed-dimension expected values, vision-read at ≈300 dpi (confidence tagged): COOLER 12'-4" × 7'-8½" = **95.1 SF** (high — both dims printed); FREEZER 5'-2" × 7'-8½" ≈ **39.8 SF** (medium — width inferred from the shared cooler wall); MEN 6'-0½" / WOMEN 6'-6½" widths (high) but depths not legible in the evaluated crops → no area expectation (low); front-of-house has no single printed dim (open plan).

| Room (page) | Best result | Class | Observed cause |
|---|---|---|---|
| FREEZER (18) | **37.2 SF** @ vector mw 0.78, dg 1.0 (vs 39.8 expected, −6.5%) | **near-clean** | insulated-box outline at 0.84/1.74 — the one true "walls are thickest" case; winner: **vector** |
| COOLER (18) | 74.5 SF @ vector mw 0.78, dg 1.0 (vs 95.1, −21.7%) | fragmented | dashed shelving unit + door-side geometry subtract the north band (visible in mask crop) |
| DINING (18) | 102.3 SF raster th50 / 101.1 SF vector mw 0.5 (dg 1.0) | fragmented | dashed equipment/counter rectangles and leader lines at 0 & 0.72 pt partition the FOH (visible on A1.0) |
| CUSTOMER (18) | 43.7 SF | fragmented | counter band between customer and dining, same ink classes |
| KIOSKS (18) | 55.2 SF @ mw 0.5 dg 1.0 | fragmented | kiosk millwork linework |
| MEN (18) | all configs `SEED_ON_WALL` | seed error | seed landed on fixture/tile ink — placement, not engine; MEN on p37 fragments at 35.5 SF instead |
| WOMEN (18) | 13.7 SF @ mw 0.5 dg 1.0 | fragmented | toilet/sink/grab-bar outlines at 0.72 partition the small room |
| OFFICE (18) | 12.1 SF @ mw 0.5 dg 1.0 | fragmented | desk/equipment ink at 0.72 |
| STORAGE (18) | 142.0 SF @ mw 0.5 dg 1.0 | plausible, unverified | no legible printed dims for this room in evaluated crops |
| DINING+CUSTOMER+KIOSKS (37) | **509.8 SF merged** @ vector mw 0.5, dg 1.0 (449.8 @ dg 2.0) | **clean-merge** | the FOH is genuinely open-plan — the merge is the drawing's truth, not a leak; unhatched on A7.0 so interior ink is minimal |
| MEN / WOMEN (37) | 35.5 / 41.2 SF @ mw 0.5 dg 1.0 | fragmented | fixtures + shower tile squares at 0.72 |
| STORAGE (37, hatch seed) | `SEED_TRAPPED` at every setting, both paths | **trapped** | tile hatch mesh at wall width: each 6" cell is its own enclosure; dilation seals them all — the flagship hatch result |
| FREEZER / COOLER (37) | 31.0 / 75.6 SF @ mw 0.78, dg 3.5 | fragmented | as p18, plus hatched surround |

Vector-vs-raster: **effectively tied on accuracy** (both see the same ink; e.g. p37 FOH 502.2 raster vs 509.8 vector), with three systematic vector advantages — deterministic results (no antialiasing threshold sensitivity: raster th50/dg1 on p37 leaked MEN into the FOH at 544.1 SF; vector never did), door-swing arcs absent by construction, and CAD-exact snapping/chaining. `measure_polyline` cross-check matched on **all 86** successful detections.

## 5. Performance (first real product signal — 42 MB, 74 pages)

| Metric | Value |
|---|---|
| Cold open (read + pdf.js parse) | 86 ms + 144 ms |
| Per-page render (s = 0.5) | min 83 / median 308 / max 2,791 ms (p74, image-heavy) |
| Per-page extraction | min 46 / median 134 / max 864 ms |
| Segments per page | min 779 / median 12,181 / **max 183,128** (p69) |
| `SegmentIndex` build, heaviest page | **75 ms** at 183k segments |
| Node RSS after full 74-page survey | 457 MB |
| Detection grid at fpi 4, ppf 10 | 1360×880 px — 7% of `MAX_RASTER_PIXELS` |

Nothing is near a limit: time-to-first-render on a cold 42 MB load is ~0.5 s (open + median render), worst page ~3 s; extraction is lazy per-page and caches. Notable: p69's 183k segments exceed the addendum's 10⁴–10⁵ design range by 1.8× and the rstar build budget absorbs it without complaint. The one soft spot is render time variance (83 ms–2.8 s) — image-heavy sheets, not vector complexity, drive the worst case.

## 6. Work queue

### A. Engine defects / next engine steps
1. **Hatch-structure filter is now the top build item** (promoted per §1): detect periodic short-segment mesh (uniform cell pitch, axis-aligned, high count) and exclude it from the wall mask. On this set it would clear the finish plan's dominant obstacle; fixture-cluster suppression (small closed loops at wall weight) is the natural second stage.
2. **`extract.js` ignores the `paintFormXObjectBegin` transform matrix** (2 form XObjects per evaluated page). No misplaced geometry was visible in the skeletons (matrices likely identity here), but a non-identity form matrix would silently misplace segments — fix the CTM handling.
3. `default_min_width` mode-is-thinnest degeneracy: on both real pages the modal bucket IS the thinnest, so the derived default filters nothing. Consider "drop only buckets strictly below the widest bucket that still encloses" — needs the enclosure notion, so park until the structure filter lands.

### B. Tuning
1. door_gap at 1/4" scale: small BOH rooms (6–8 ft) are fully dilation-closed at dg 3.5 (r=18 px); dg 1.0 was best almost everywhere on this set. Consider scale-aware default or per-click adaptive closing (carried from eval-01 B2).
2. Seed placement remains operator skill: one room (MEN p18) failed solely from a seed on fixture ink. The harness's snap indicator helps; a seed-quality hint ("clicked on ink — nudged/nudge failed") would close the loop.

### C. Inherent to the drawing
1. **CAD pen tables map geometry class ≠ line weight.** This brand-standard set draws walls, hatch, fixtures, and symbols at one weight; weight separates *drafting layers* (annotation vs geometry), not *semantic classes*. Structure/semantics must do what width cannot — consistent with eval-02 §5.C.2, now confirmed on a full-size fixture.
2. Open-plan FOH means room separation there is a labeling problem (addendum AI region labeling), not a geometry problem — the 510 SF merge is correct geometry.
3. Real-text sheets (modern sets) remove the stroked-text fragmenter entirely; older SHX sets (fixture 001) keep it. Both cases now measured.

## 7. Status of the eval-02 verdict

Eval-02 §4 said "width carries almost no wall-vs-annotation signal" and blamed the collapsed pen table as a possible confound. Eval-03 removes the confound and **upholds the verdict with a corrected cause**: the signal is absent not because plots collapse pen tables, but because pen tables encode drafting convention rather than semantics. The width filter stays (it cleanly removes the thin annotation layers and is free), but wall isolation needs the structure filter above.
