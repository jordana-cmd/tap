# Real-Plan Evaluation 01 — `fixtures/plans/Floor Plan.pdf`

**Date:** 2026-07-18 · **Engine:** `engine-core`/`engine-web` @ working tree (post-calibration feature) · **Renderer:** pdfjs-dist 6.1.200 (pinned, vendored)

First run of the deterministic detection pipeline against a real plan sheet. Executed headlessly through the *same code path the harness uses* — pdf.js render → RGBA → 0.299/0.587/0.114 grayscale → `calibrate_two_point` → `detect_room` from the actual `engine_web` wasm bundle — so every number below is what the browser harness produces at the same (threshold, door_gap, seed) inputs. Seeds are recorded in 6×-render pixels (divide by 6 for PDF points).

## 1. Fixture provenance & confidentiality

- **Sheet:** A-4 "Proposed Floor Plan", pool house renovation, municipal client, Michigan. One page.
- **Rider: PDF stays uncommitted.** It is untracked and must not be staged. It is **not yet golden-set eligible**.
- **Rider: title block check — it DOES carry client-identifying details:** client municipality name (also repeated in the plan notes), engineering firm name, street address, phone/fax, website, project number, plot date, designer/drawer initials, and a licensure stamp. All of these must be redacted before this sheet is formalized into `fixtures/plans/` per `fixtures/README.md`; the scale text and dimension strings must be preserved.

## 2. Sheet characteristics — the first real-world lesson

The PDF page is **792 × 612 pt = 11" × 8.5" (Letter)**. The title block says the drawing is at **3/16" = 1'-0"**, but that statement is true of the *original 36" × 24" D-sheet*, not of this letter-size plot. Anyone trusting the printed scale text would measure everything at ~3.27× undersize.

- Printed-scale fpi (3/16" = 1'-0"): 16/3 ≈ **5.333** — wrong for this file.
- Calibrated fpi (below): **17.454** — matches a 36×24 → letter fit-to-page plot of a 3/16" drawing (theoretical 17.4545) to 0.01%.

**Consequence:** scale-from-text can never be applied without verification against a printed dimension. This is exactly the addendum's scale-verification invariant; the eval confirms it is load-bearing on the very first real sheet.

## 3. Calibration (`calibrate_two_point`)

Reference: the overall horizontal dimension **90'-6"** across the bottom of the plan (high confidence — clean text, cross-checked against segment sums).

- Dimension-line *tick* endpoints measured at 6× render: x = 677 → 2943 (span 377.67 pt) → fpi 17.253.
- Same procedure on the vertical overall **50'-7"**: span 213.17 pt → fpi 17.085. The two axes disagreed by ~1%.
- Cause: architectural tick style — the dimension line overshoots the extension lines. Solving both axes for a shared symmetric overshoot gives ~14 px @6× (≈1/32" paper) and a consistent fpi ≈ 17.47.
- Re-measured at the **extension lines** (the true extent): x = 687.5 → 2927.5 → span 373.33 pt → **fpi = 17.4536**. Agrees with the 36×24 plot theory to 0.01%.

`calibrate_two_point((114.583, 391), (487.917, 391), 90.5)` → 17.4536, error-free. **Calibration workflow lesson:** users will click ticks, not extension lines; a tick-click introduces ~±0.7% here. Worth a UI affordance later (zoomed magnifier, snap to extension line) — logged in the work queue.

## 4. Vision-read printed dimensions (with confidence tags)

Read by AI vision from 6× (≈432 dpi-equivalent) crops. *These are eval inputs, not measurements; per invariant, no AI-read number is exportable without human confirmation.*

| Dimension | Where | Value | Confidence |
|---|---|---|---|
| 90'-6" | overall width, bottom row | 90.500 ft | high |
| 62'-1", 13'-4", 6'-0", 5'-8", 3'-4" | bottom segment row | — | high |
| 50'-7" | overall height, left column | 50.583 ft | high |
| 13'-1", 11'-4", 13'-1", 6'-7", 6'-7" | left segment row | sum 50.667 ft | high |
| OFFICE 112: 8'-8" wide | interior | 8.667 ft | high |
| OFFICE 112: 6'-4" + 3'-4" + 1'-0" depth | interior right | 10.667 ft | high (sum) |
| OFFICE 112: rotated "10'-8"" wall label | interior left | 10.667 ft | medium (rotated text) |
| CONCESSION 113: 5'-5" + 3'-0" + 8'-10" | interior width | 17.250 ft | high |
| CONCESSION 113: 11'-10" | interior depth | 11.833 ft | high |
| LIFE GUARDS 107: 12'-7" × 11'-9" | interior | 12.583 × 11.750 ft | high |
| MULTI PURPOSE 100: 25'-8" × 44'-10" | interior | 25.667 × 44.833 ft | medium (long extension chains; room may not be a plain rectangle) |
| OFFICES / MULTI PURPOSE ROOM 118: 17'-4" × 24'-4" | interior | 17.333 × 24.333 ft | high |
| MECHANICAL 102: 15'-0" (= 4'-9" + 3'-0" + 7'-3") | interior width | 15.000 ft | high |
| MECHANICAL 102 depth | *not printed in crop*; ~6.5 ft measured off the calibrated raster | ~6.5 ft | low (derived, not printed) |
| CIRCULATION 103: 5'-0" corridor width | interior | 5.000 ft | high |
| Stated plan area "4835 SF REVISED PLAN" | under plan | 4835 SF | high (text), unknown basis |

Internal consistency note: the drawing's own left-column segments sum to 50'-8" against a printed overall of 50'-7" — a 1" rounding discrepancy *in the source drawing*.

## 5. Detection results

Expected areas assume rectangular rooms from the dims above. Seeds were chosen in visually open floor space (first-attempt seeds on/near room-label text all returned `SEED_ON_WALL`; see queue).

Best result per room across the sweep (threshold ∈ {200, 128, 80, 50}, door_gap_ft ∈ {3.5, 2.0, 1.0}, ppf 10):

| Room | Expected SF | Best detected | At (th, dg) | Δ | Failure mode elsewhere |
|---|---|---|---|---|---|
| OFFICES/MPR 118 | 421.8 | **404.6** | 50, 2.0 | **−4.1%** | fragments to ~172–243 SF at th ≥ 80 (interior dim lines) |
| CONCESSION 113 | 204.1 | 124.8 | 50, 2.0 | −39% | counter/fixture linework partitions fill |
| LIFE GUARDS 107 | 147.9 | 83.5 | 50, 2.0 | −44% | lounge-chair linework partitions fill |
| OFFICE 112 | 92.4 | 30.4 | 200, 1.0 | −67% | at (50, 2.0) leaks through door → 233.9 SF; at dg 3.5 seed area fully dilated away |
| MECHANICAL 102 | ~97 (low conf) | 32.1 | 200, 1.0 | −67% | equipment linework partitions fill |
| MULTI PURPOSE 100 | ~1150.7 | 157.7 (fragment) | 80, 2.0 | −86% | at (50, 2.0) thin existing-wall lines drop out → **19,608 SF leak** across the sheet |
| CIRCULATION 103 | (corridor) | 151.4 | 200, 1.0 | — | at (50, 2.0) merges into room 118 through open door (identical 404.6 SF contour) |

Supplementary observations:

- **ppf 20 changes nothing** (118 → 197.3 SF at th 128, same failure signature). Vector hairlines rasterize solid-black at any resolution; stroke *darkness* never separates annotation from wall. Separation must come from stroke **width** or from the vector operator list.
- **Engine math is self-consistent:** in every successful run (24 total), `measure_polyline(closed contour)` equaled the reported `perimeter_lf` exactly. No engine-math discrepancies observed anywhere in the sweep.
- **Determinism confirmed:** identical inputs reproduced identical contours across runs (e.g. two different seeds in the same fragment returned byte-identical 157.7 SF / 64.0 LF results).
- Reproducibility: fpi 17.4536; working raster 1920×1484 @ s = 2.4241 (ppf 10). Seeds (6× px): office 800:1500 · concession 825:2150 · lifeguards 910:1145 · multi 2550:1500 · center118 2310:1700 · mechanical 2350:1070 · circulation 2180:1220.

**Bottom line:** on a real, annotation-dense CAD sheet, threshold+dilate+flood-fill recovers a clean open room to within ~4%, but *no single (threshold, door_gap) pair works across rooms*, and rooms with interior annotation (dims, grid lines, furniture, equipment) fragment or leak at every setting. The raster path as-is is a fallback, not the primary mechanism — consistent with the addendum's plan for vector-first wall extraction (§A3.3 SegmentIndex + pdf operator-list module).

## 6. Work queue

### A. Engine defects (our code, fixable now)

1. **`SEED_ON_WALL` fires on visually-open floor.** Seed validity is checked against the *dilated* mask, so a click ≥ r px from ink still fails when r is large (dg 3.5 → r 18 px @ ppf 10 dilates label text into a blob that swallows small rooms entirely). Fix: validate against the raw mask and nudge the seed to the nearest free dilated-mask pixel within a small radius; error only if none exists. (This alone made 5 of 7 first-attempt clicks fail.)
2. **No stroke-width discrimination.** Add a morphological *opening* (erode r₁ → dilate r₁) on the wall mask before door-gap dilation: hairline dims/grid lines (~1 px @ ppf 10) vanish at r₁ = 1–2 while 8" CMU walls (~7 px) survive. Cheap, deterministic, and directly targets the dominant failure mode. Needs synthetic + real fixtures.
3. **Calibration endpoint accuracy.** Tick-mark clicks vs extension-line extent cost ~0.7% here. Harness/UI: magnifier + (later) snap calibration clicks to the SegmentIndex.
4. **Vector wall extraction is the real fix** (already planned, addendum §A3): pdf.js operator-list → SegmentIndex → stroke-width/layer-aware wall mask. This eval is the evidence for its priority.

### B. Tuning issues (parameters, no code change)

1. Defaults (th 200, dg 3.5) are tuned to the synthetic demo and fail wholesale on this sheet. Best all-around here: **th 50, dg 2.0** — but it opens thin *existing* walls (multi-purpose leak) and open doors (corridor merge). Per-click parameter adjustment is currently *required*, and the harness sliders support exactly that workflow.
2. door_gap radius `ceil(dg/2 × ppf)` globally thickens all ink; in a 5' corridor, dg 3.5 consumes 70% of the width. A local/adaptive closing (seal only detected gap spans) would decouple "close doors" from "shrink rooms" — candidate algorithm change, file under A if pursued.
3. ppf has no useful tuning headroom (10 vs 20 identical failure signature); don't spend more time there.

### C. Inherent to the drawing (no engine change can fix; workflow/AI layer must absorb)

1. **Plotted-to-letter sheet invalidates printed scale text.** Only calibration or verified scale-read works. Product must treat scale text as a *proposal* requiring verification — already the addendum's position.
2. **Annotation shares ink with walls in a flattened raster.** Dimension strings/lines, structural grid lines, door swings, lounge chairs, mechanical equipment are indistinguishable from walls by luminance. Threshold alone can never separate them on CAD-plotted PDFs.
3. **Wall-type poché varies within one sheet** (hatched CMU, gray-shaded framed walls, thin double-line existing walls): any single threshold includes some wall types and drops others.
4. **The drawing's own dimensions carry rounding slop** (50'-7" overall vs 50'-8" segment sum). Expected-value tolerances below ~0.2% are not meaningful against this source.

## 7. Golden-set disposition

Usable as golden-set sheet **after title-block redaction** (§1). Suggested slot: "clean CAD-exported architectural floor plan" — with the bonus characteristics: letter-size plot (scale-text trap), multiple wall types, annotation-dense interiors. `expected.json` values for rooms 107/112/113/118 and the two overall dims can be seeded from §4 (human re-verification of the hand math required per `fixtures/README.md`).
