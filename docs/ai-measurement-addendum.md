# ADDENDUM A — AI Measurement & Verification Layer

**Status:** Amends `web-takeoff-final-spec.md`. Replaces its §6 (AI Takeoff Integration) and extends §1.1 (`engine-core` scope), §3 (API), and the `Measurement` data model. Everything else in the final spec stands.
**Source:** Selectively ported from `PLANTAPE-SPEC.md` (prior single-user prototype). PlanTape's *architecture* (Next.js client-side rendering, IndexedDB, Zustand, single-user auth) is **superseded and must not be imported** — it conflicts with the tile-pyramid/CRDT/OPFS architecture already locked in the final spec. PlanTape's *measurement IP* — invariants, detection algorithms, verification loop, AI contract discipline — is adopted below, because it is architecture-independent and more rigorous than the final spec's original §6.

---

## A0. Adopted invariants (add to final spec §0 as Rules 4–7)

4. **All geometry is stored in base units = PDF points (1/72 paper inch), page-local, top-left origin.** Zoom, DPR, tile resolution, and export resolution are display transforms only. This is what makes measurements survive zoom, re-render, revision overlay, and export.
5. **Derived values are never persisted as truth.** LF/SF/quantities are computed at read time from `pts × scale` (and assembly formulas). Recalibrating a page's scale silently corrects every measurement on it. (The final spec's "derived quantities" endpoint still receives materialized numbers for dashboards — those are explicitly marked derived and are regenerable, never authoritative.)
6. **Every AI output passes through a schema gate and a human gate.** AI proposes; deterministic code measures; the user confirms. No AI-derived number reaches an export or a posted quantity without a confirmation event recorded in the CRDT doc.
7. **Only crops and downscaled page images go to vision models — never original plan files from the client path, and only the minimum region needed.** Cost, latency, and confidentiality all point the same way. (Server-side inference on stored originals remains allowed per final spec §6, but must obey the same minimal-region discipline and the org-level training opt-in.)

---

## A1. The layered pattern (governs every AI capability)

```
deterministic geometry layer  →  semantic AI layer  →  human confirmation gate
        (engine-core)             (schema-forced)         (review queue UI)
                └──────────── verifiable feedback loop ────────────┘
```

Each new AI capability is a tuple: **(deterministic extractor, response schema, versioned prompt, confirm-UI)**. Nothing else. This keeps AI features cheap to add and impossible to sneak past review.

---

## A2. Data model amendment

Add to `Measurement` in the CRDT document:

```
origin: 'manual' | 'snap' | 'floodfill' | 'ai'     // provenance, immutable after creation
confirmedBy?: { userId, at }                        // required before origin:'ai'|'floodfill'
                                                    // items count toward posted quantities
parentId?: measurementId                            // deductions reference their parent area
```

Add to per-page `ScaleState`:

```
source: 'ai' | 'calibrated' | 'manual'
verification?: { method: 'two-point' | 'auto', samples: [{printedText, printedFt,
                 measuredFt, deviationPct}], medianDeviationPct, verdict, at }
```

**Provenance flows to every export** (CSV/XLSX/PDF deliverable): an `origin` column per line item and per-page scale source + verification status on the summary sheet. Provenance in the deliverable is what makes AI-assisted numbers defensible to a client or a GC.

---

## A3. Deterministic detection layer (goes in `engine-core`, not TypeScript)

Port PlanTape's algorithms into Rust (they are language-agnostic math); they run in the wasm workers against tile/region rasters supplied by `engine-web`. No model calls involved — this is the pre-AI workhorse and ships **before** any vision feature.

### A3.1 Click-to-room (flood fill) — "Level 1"
1. **Mask:** rasterize a generous region around the click at a resolution chosen from the page scale so `px_per_foot ≈ 8–12` (resolution scales with drawing scale, not fixed DPI). Grayscale → threshold (luminance < ~200 → wall pixel). Hard cap on raster dimensions per the final spec's memory budget.
2. **Door-gap closing:** binary dilation of the wall mask, radius `≈ (door_gap_ft / 2) × px_per_foot`, default `door_gap_ft = 3.5`. **Expose as a user slider** — industrial plans have 12' equipment doors; re-run is cheap.
3. **Flood fill** (scanline, iterative — never recursive) from the click across non-wall pixels. If the fill reaches the raster boundary → "region not enclosed" error, fall back to manual trace. Never guess.
4. **Contour:** marching squares → polygon → Douglas-Peucker simplify (ε ≈ 1.5 base units). Convert back to base units.
5. Emit an ordinary `Measurement { kind:'area', origin:'floodfill' }`. Area via shoelace on the **simplified polygon** (never pixel count — dilation inflates it); the polygon's perimeter is the cove-base/wall-base LF for free. The result is editable exactly like a hand-traced shape — invariant A0.4 is what makes that true.

> **Implementation refinement (Engine Foundation step 3):** step 2's dilation closes door gaps but also insets the fillable interior by the dilation radius `r` on every side (≈1.75 ft at defaults — a ~37% area error on a 20'×15' room). The engine therefore completes a **morphological closing**: dilate walls → flood fill → dilate the filled region back by the same `r` → subtract original wall pixels. Rectangular rooms recover their wall-face dimensions exactly (square structuring element); door openings gain only a small bulge at the door plane (bounded by gap × r), smoothed by step 4's simplification. Step 5's "dilation inflates it" caution refers to the pixel count of this closed region.

### A3.2 Auto-detect candidates — "Level 2"
Full-page mask → invert → connected-component labeling (two-pass union-find). Filter: area between ~40 SF and ~80% of sheet, discard border-touching components. Output per candidate: centroid, SF, bbox, simplified contour. These become the **region proposals** the AI labeling call annotates (A5.3) and the review queue displays.

### A3.3 Vector extraction → SegmentIndex → snapping
For CAD-exported PDFs, extract true geometry server-side during ingest (extend the final spec §4 pipeline) or on-demand in the engine:
- Walk the PDF operator list maintaining a CTM stack (save/restore/transform); decode path construction ops; transform into top-left base-unit space; keep stroked segments with their `lineWidth` (walls are typically the widest strokes on a sheet).
- Store `Segment { x1,y1,x2,y2,width }` per page in an R-tree/flatbush-class spatial index (10⁴–10⁵ segments builds in milliseconds).
- **Snapping:** on pointer-down in any measure tool, query within tolerance `10 / zoom` base units. Priority: endpoints > intersections > perpendicular projection. Measurements created via snap get `origin:'snap'`.
- Scanned PDFs yield no vectors → empty index → snapping silently off. Degrade, don't warn-spam.

This upgrades precision from "how steady is your mouse" to CAD-exact, and it is the prerequisite for trusting AI-proposed geometry later.

---

## A4. AI contract mechanics (replaces "respond with only JSON" everywhere)

- **Forced tool use:** every vision call uses `tool_choice: {type:'tool', name:…}` with a JSON Schema generated from the same source-of-truth schema the server validates against (Zod on a Node backend / equivalent). The model's output *is* the schema — no fences, no preamble, no parse-and-pray. A schema-invalid response is a `502 schema_violation`, never a silent best-effort.
- **One schema per capability**, versioned alongside its prompt:
  - `ScaleRead { found, scale_text, feet_per_paper_inch, confidence, multiple_scales_on_sheet, note }`
  - `DimVerify { found, dimension_text, feet, confidence, note }`
  - `RoomLabels { rooms: [{region_id, label, include_in_takeoff, reason}] }` — `region_id` echoes the candidate ids **burned into the image as numbered markers**, so the model binds labels to pixels, not to prose.
  - `SymbolCount` (Auto Count, per final spec) follows the same pattern: detector proposes candidate points deterministically where possible; model classifies/labels; user confirms.
- **Versioned prompts:** prompts live as `.md` files in the repo (`ai/prompts/*.md`), imported as strings. Prompt changes are git diffs, never runtime drift.
- **Model routing:** stronger model (Sonnet-class) for scale reads and dimension verification (small-text OCR); eval a cheap model (Haiku-class) for region labeling — it's classification over large obvious regions. Route per capability via config, not hardcode.
- **Call logging (server-side):** `{route, model, tokens_in/out, latency_ms, schema_ok, org_id, ts}` to a table. Feeds the accept/edit/reject accuracy metric already required by the final spec.

---

## A5. Verification — trust as a first-class feature (the crown jewel; generalize it)

### A5.1 The primitive
`verifyDimension(pointA, pointB) → VerificationRecord`: crop tightly around the two points at high resolution, **burn red fiducial markers** into the crop at the exact points, ask the model to read the printed dimension string for the marked span (`DimVerify` schema), then compare printed vs deterministically measured. Verdict thresholds: **≤1.5% verified · ≤4% warn · >4% reject** with one-click recalibrate.

### A5.2 Auto-verify (runs after any scale is set, any source)
One call asks the model to locate 3–5 printed dimension strings on the sheet with their endpoint coordinates; the engine measures each span deterministically; compute the deviation distribution. **Median < 1.5% → auto-verified badge** on the page's scale chip; outliers listed for inspection. This converts "AI read the title block" into "AI's reading was checked against N independent printed dimensions" — the sentence that lets a number go into a real bid.

### A5.3 Region labeling review (Level 2 UI)
Candidates from A3.2 rendered as a checklist panel: hover highlights the region on canvas; model's `include_in_takeoff` is a pre-check recommendation only. Accepting writes `origin:'ai'` + `confirmedBy`. **Nothing enters posted quantities without a checkbox.** Encode the honest constraint in UI copy: excellent on clean CAD sets, needs babysitting on scans/double-line walls/open plans — the review checklist *is* the product, not an apology for it.

---

## A6. Evals & fixtures (extends final spec §8 observability)

- `fixtures/` golden set: 8–12 real, sanitized plan sheets + `expected.json` each: true scale, ≥3 known printed dimensions with endpoints, known room areas, known symbol counts.
- Two test tiers: (1) headless unit/property tests of all geometry + detection in native Rust (fast, every CI run); (2) live-model eval suite behind an env flag (costs money) asserting tolerance bands per capability. `npm run eval` (or `cargo xtask eval`) answers "did this prompt/model change move accuracy?" before it ships.
- Production accuracy metric remains accept/edit/reject rates per capability per the final spec; fixtures catch regressions pre-deploy, telemetry catches drift post-deploy.

---

## A7. Revised AI ship order (amends final spec §10 M5)

1. **A3.3 snapping** — lands in M2 (no model, huge daily precision/QoL win).
2. **A3.1 click-to-room flood fill** — lands in M2/M3 boundary (no model; kills most area-tracing time).
3. **Scale read + A5 verification loop** — first vision feature; it ships *with* its own checking mechanism on day one.
4. **A3.2 + A5.3 auto-detect rooms with labeling checklist.**
5. **Auto Count** (symbol detection + classification), then Auto Scale refinements — per original spec order.

Rationale: the deterministic layer delivers most of the "AI-era" time savings with zero model risk, and the verification loop must exist before any AI-read number is trusted — the prototype proved both.

---

## A8. Explicitly NOT adopted from PLANTAPE-SPEC.md (do not import)

| PlanTape element | Why rejected |
|---|---|
| Next.js client-side full-PDF rendering (pdfjs in-browser as primary display path) | Conflicts with server tile-pyramid architecture; reintroduces the browser memory ceiling the final spec designed out. PDF.js may still be used **server-side in the tiler** and for on-demand vector extraction. |
| IndexedDB + name/SHA project persistence, Zustand store | Superseded by OPFS SQLite WAL + CRDT doc. |
| Single-user password middleware, Vercel deployment shape | Superseded by platform session auth (final spec §2). |
| "Last-write-wins, don't build sync" stance | Correct for a solo tool; wrong for the multi-user product. CRDT stands. |
| Browser-only vision calls as the sole inference path | Final spec's server-side inference stands; A0.7's minimal-region discipline applies to both paths. |

---

**Summary for the repo README:** *AI never measures. Deterministic code measures; vector snapping and flood-fill do the grunt work with no model at all; vision models only read text and label regions through forced-schema calls; every scale gets checked against printed dimensions on the sheet; and every AI-touched item carries provenance and a human confirmation all the way into the export.*
