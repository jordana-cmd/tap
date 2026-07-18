# fixtures/ — Golden Set for Measurement & AI Evals

This folder is the ground truth that keeps the engine and the AI honest (addendum §A6). Claude Code cannot fabricate real plan sheets — **a human must supply the PDFs and the expected values.**

## Layout

```
fixtures/
├── synthetic/          # generated in tests — do not hand-edit
├── plans/              # real, sanitized plan sheets (PDF, one sheet per file)
│   ├── 001-cad-arch-floor/
│   │   ├── sheet.pdf
│   │   └── expected.json
│   ├── 002-scan-arch-floor/
│   └── …
└── README.md
```

## What to collect (8–12 sheets, aim for spread, not volume)

| # | Type | Why it's in the set |
|---|---|---|
| 2–3 | Clean CAD-exported architectural floor plans | Happy path: vector extraction, snapping, flood fill, room detection |
| 1–2 | Scanned/rasterized plans (photocopied quality) | No vectors → snapping off, detection under noise |
| 1 | Sheet with **multiple scales** (plan + details) | Scale-read must set `multiple_scales_on_sheet` and not guess |
| 1 | Open floor plan (unenclosed regions) | Flood fill must abort cleanly, not leak |
| 1 | Industrial sheet with oversized door/equipment openings | Door-gap slider case |
| 1 | Civil/site sheet (engineer scale, e.g. 1"=20') | Non-architectural scale parsing |
| 1 | Very dense sheet (heavy linework/hatching) | Detection filter + performance case |

**Sanitize before committing:** strip or redact client names, addresses, project numbers from title blocks if the repo will ever leave your machines. Keep the scale text and dimension strings intact — they're the test.

## expected.json schema (per sheet)

```json
{
  "sheet": "A1.1 First Floor Plan",
  "source_type": "cad | scan",
  "scale": {
    "text": "1/4\" = 1'-0\"",
    "feet_per_paper_inch": 4.0,
    "multiple_scales_on_sheet": false
  },
  "printed_dimensions": [
    {
      "text": "24'-6\"",
      "feet": 24.5,
      "endpoints_base_units": { "a": {"x": 0, "y": 0}, "b": {"x": 0, "y": 0} },
      "note": "north exterior wall, left segment"
    }
  ],
  "rooms": [
    {
      "name": "OFFICE 101",
      "area_sf": 142.0,
      "tolerance_pct": 3.0,
      "click_point_base_units": {"x": 0, "y": 0},
      "enclosed": true
    }
  ],
  "symbol_counts": [
    { "symbol": "duplex receptacle", "count": 14 }
  ]
}
```

Filling in `endpoints_base_units` and `click_point_base_units` requires the app itself (click and read coordinates) — leave them `0,0` initially; the first working build includes a fixture-annotation mode to capture them. `printed_dimensions.feet` and `rooms.area_sf` should be computed by hand from the printed drawing (dimension strings + room dims), independently of any software.

## How the set is used

- **Tier 1 (every CI run, free):** native Rust tests run geometry/detection against `synthetic/` and against `plans/` where expected values don't need a model.
- **Tier 2 (`eval` task, behind env flag, costs API money):** live vision calls for scale read, dimension location, room labeling; asserts tolerance bands (scale exact; dimension deviation ≤1.5% median; room labels judged by include/exclude agreement). Run before merging any prompt or model change.

A sheet with wrong expected values is worse than no sheet — double-check the hand math.
