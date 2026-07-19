# Real-Plan Evaluation 02 — vector wall mask vs raster, same sheet

**Date:** 2026-07-18 · **Engine:** through commit `4cc0cda` (vector-filtered wall mask, `PageGeometry`, snapped calibration) · **Baseline:** `docs/real-plan-eval-01.md` (raster path, same sheet)

Re-run of eval-01's exact protocol on the same fixture (`fixtures/plans/Floor Plan.pdf`, uncommitted, title block unredacted — see eval-01 §1): same seven rooms, same recorded seeds, same calibration (fpi 17.4536), same ppf 10 working grid (1920×1484). Executed headlessly through the production-intent code path: **`harness/extract.js` (the identical module the browser harness imports) → `PageGeometry` in the real wasm bundle → `detect_room_vector`**. Expected room values and their confidence tags are unchanged from eval-01 §4.

## 1. Extraction and histogram (real numbers from the new engine API)

- 15,065 stroked paths → **73,628 line segments**; 2,170 curve ops skipped (door swings, fixture arcs — skipped by design, with the current point advanced); fills emit nothing (967 fill paths, including gray wall poché); leftover unhandled ops: `eoClip`×15, `paintImageXObject`×1 (title-block stamp image). Extraction 1.6 s, `SegmentIndex` build **77 ms** — well within the addendum's budget at 7× the "10⁴–10⁵" design center's low end.
- Stroke-width histogram via `width_histogram`: **0.12 pt → 4,647 segs · 0.24 pt → 67,836 · 0.36 → 776 · 0.48 → 135 · 0.96 → 234**. `default_min_width` = **0.18 pt** (midpoint of thinnest and modal buckets), which keeps everything ≥ 0.24.
- The histogram is the sheet's confession: this letter-size plot (3.27× reduction, eval-01 §2) collapsed the CAD pen table — **walls, SHX stroked text, furniture, and equipment all share the 0.24 pt bucket**. §A3.3's working assumption that "walls are typically the widest strokes" is **false on this sheet**: only 1,145 of 73,628 segments are wider than the walls' own bucket, and filtering at 0.30 pt (just above it) removes the walls themselves — every room then returns `REGION_NOT_ENCLOSED`.

## 2. Snapping on real plan geometry (first exercise)

Calibration-click snapping (tolerance 10/zoom → 4.13 pts at the working scale):

| Probe | Result |
|---|---|
| Left end of the 90'-6" extension line | `endpoint` snap → x = 114.2025 |
| Right end of the same extension line | `endpoint` snap → x = 487.6950 |
| Guessed mid-wall point in room 118 | no snap (guess was > 4 pts off — tolerance behaving as specified) |

The snapped span is 373.4925 pts → **fpi 17.4455**, vs eval-01's raster-scanline estimate 17.4536 (Δ 0.046%). The snapped value comes from exact CAD endpoints rather than pixel-band centers — this is eval-01 work-queue item A3 (calibration endpoint accuracy) materially addressed: calibration clicks in the harness now land on CAD-exact geometry when within tolerance. Both values remain far inside the §A5.1 1.5% band; eval-01's published fpi is kept for comparability below.

## 3. Detection: vector vs raster, same rooms, same seeds

Vector mask at the derived default `min_width` 0.18, door-gap swept {3.5, 2.0, 1.0} ft; raster columns are eval-01's best result per room across its full (threshold × door-gap) sweep. Expected values (and confidences) from eval-01 §4.

| Room | Expected SF | Raster best (eval-01) | Vector best (min_width 0.18) | Vector verdict |
|---|---|---|---|---|
| OFFICES/MPR 118 | 421.8 | **404.6** (th 50, dg 2.0) | 218.1 (dg 1.0) | fragment — worse than raster's best |
| CONCESSION 113 | 204.1 | 124.8 (th 50, dg 2.0) | 100.1 (dg 1.0) | fragment, comparable |
| LIFE GUARDS 107 | 147.9 | 83.5 (th 50, dg 2.0) | 79.0 (dg 1.0) | fragment, comparable |
| OFFICE 112 | 92.4 | 30.4 (th 200, dg 1.0) | 30.7 (dg 1.0) | fragment, near-identical |
| MECHANICAL 102 | ~97 (low conf) | 32.1 (th 200, dg 1.0) | 36.4 (dg 1.0) | fragment, near-identical |
| MULTI PURPOSE 100 | ~1150.7 | 157.7 fragment (th 80) | `SEED_ON_WALL` at every setting | blocked by seed defect |
| CIRCULATION 103 | (corridor) | 151.4 (th 200, dg 1.0) | 164.0 (dg 1.0) | slightly larger — door-swing arcs no longer block the corridor |

Secondary observations:

- **`min_width` 0.18 vs 0.00 differs by only 1–2%** (e.g. 118: 218.1 vs 216.6). The hairline layer the filter removes was never the dominant fragmenter — the 0.24 pt text/furniture ink is, and the filter cannot touch it without deleting walls.
- **The vector mask reproduces raster-at-default-threshold, not raster-at-th-50.** 118 at (0.18, dg 2.0) = 197.7 vs raster (th 200, dg 2.0) = 194.1 — near-identical. Raster's th-50 win on 118 (404.6) came from sub-pixel antialiasing: 0.24 pt lines render at ~8% pixel coverage at ppf 10 and drop below a dark threshold, silently erasing most same-width annotation. The vector mask keeps those segments at a deliberate 1-px minimum — deterministic, but deterministic includes the annotation.
- `measure_polyline` cross-check matched `perimeter_lf` exactly in **all 33 successful detections**; identical inputs reproduced identical contours. The engine math holds on the vector path.
- The scanned-PDF degrade path is exercised for free: the synthetic demo plan (fills only, no strokes) yields zero segments → vector toggle disabled → raster fallback.

## 4. Plain verdict

**Vector extraction did not fix the fragmentation on this sheet.** Stroke-width filtering deterministically removes the hairline annotation layer, door-swing arcs, and fills — real, permanent wins over threshold luck — but on a letter-plotted sheet whose pen table collapsed to effectively one width, *width* carries almost no wall-vs-annotation signal. The measured fragmentation cause is same-width ink: stroked SHX text and furniture at the walls' own 0.24 pt. What vector extraction **did** deliver on day one: CAD-exact snapping (including materially better calibration endpoints), a 77 ms per-page index over 73k segments, a mask whose contents are explainable segment-by-segment (the harness overlay shows exactly which ink became wall), and the infrastructure every future segment-level filter will run on.

### Fixture caveat — this verdict is not settled

Both evals ran against a single fixture that is a **0.31× letter-size plot of a 36"×24" D-sheet** (eval-01 §2). That reduction is a plausible **confound** for the width-filter result, not just background: scaling every pen weight by 0.31 compresses the entire pen table toward the hairline floor (original ~0.39/0.79/1.18/1.57/3.14 pt → 0.12/0.24/0.36/0.48/0.96), and widths that were distinct at full size can collapse into one 0.01-pt bucket after rounding at plot resolution. On a full-size CAD export, walls may well occupy a genuinely distinct width band — in which case the width filter would perform very differently and §4's verdict would not hold. **Do not treat "width carries no wall signal" as settled, and do not build or tune further segment filters (length/collinearity/paired-parallel) against this fixture** — they would risk being tuned to a plot artifact. Re-test on a full-size vector sheet first (the golden-set gap in §5.C.1); this sheet remains valuable precisely as the degraded-plot case, not as the representative one.

## 5. Work queue

### A. Engine defects / next engine steps

1. **Seed-on-dilated-mask defect now blocks a whole room** (Multi Purpose 100 fails at every vector setting; 5 of 7 rooms fail at dg 3.5). Carried from eval-01 A1, still the single highest-leverage fix: validate the seed against the raw mask and nudge to the nearest free dilated-mask pixel.
2. **Width is the wrong (sole) discriminator — add segment-STRUCTURE filters.** The data suggests length/structure separates what width cannot: SHX glyph strokes are short and clustered; walls are long or form long collinear chains; wall pairs run parallel at wall-thickness offsets. Concrete candidates, in order: minimum-segment-length filter (with a length histogram beside the width histogram), collinear-chain merging, paired-parallel "double-line wall" detection. Same architecture — filter before `rasterize_wall_mask` — so the plumbing built this step is reused as-is.
3. **Curves are dropped wholesale.** Correct for door swings; wrong for genuinely curved walls (none on this sheet). When `curveCount` is high relative to path count, the harness should surface it; a flatten-to-segments option is a later, cheap addition.

### B. Tuning issues

1. On this sheet the useful `min_width` range is a knife's edge: [0.13, 0.24] behaves identically, ≥ 0.30 destroys enclosure. The histogram readout next to the slider makes this visible, which is the point of exposing it — but there is no tuning win hiding here.
2. Per-room door-gap adjustment remains required on both paths (unchanged from eval-01).

### C. Inherent to the drawing

1. **The letter-size plot destroyed stroke-width diversity** (all pen weights × 0.3056 collapse toward the hairline floor). A full-size CAD export of the same kind of drawing would plausibly keep walls at distinct widths — the golden set needs a full-size vector sheet to measure that (current golden-set gap; eval-01 §7 collection list stands).
2. **Stroked SHX text is geometrically wall-like ink.** No width threshold separates it; separation needs structure (A2 above) or semantics (the addendum's AI region labeling, which this deterministic layer feeds).
3. **Fill-only wall poché emits no segments** (967 fill paths dropped by design). Sheets whose walls are drawn as fills would lose those walls entirely on the vector path — the raster fallback is load-bearing, and the §9 decision-log row's "fallback for scans" should be read as "fallback for scans *and* fill-drawn walls."
