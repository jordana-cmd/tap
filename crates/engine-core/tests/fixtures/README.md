# Pricing golden fixtures

Each `pricing/*.json` is a self-contained pricing case:

```json
{
  "name": "...",
  "note": "what this case is for",
  "provenance": "...",
  "rate_card": { ...a complete RateCard... },
  "job":       { ...a complete JobInput... },
  "expected":  { per-area subtotals, job cost, price, profit, margin, floor flag }
}
```

## Why each fixture embeds its own rate card

Unit costs drift. Haze Gray Epoxy appears at 44.00, 96.24 and 144.36 across
historical quote sheets, and the standard wage at 25.00, 27.50 and 32.00. A
fixture that referenced `data/rate-cards/` by version would either break every
time a price is corrected, or silently start asserting something different.

Embedding the card makes each case reproducible forever against the rates it
was actually quoted at, and means adding a product to the shipped catalog
cannot invalidate a golden. It is also the shape a real historical sheet has:
inputs, the rates in force that day, and the number that went out.

## Where the expected numbers come from

**Not from this engine.** They were produced by an independent Python
implementation of the model, written from the specification prose rather than
from `calc.rs`, and `epoxy-2000sf` was additionally verified by longhand
arithmetic:

```
materials    2161.0200      (8 gal haze gray x 44.00, 8 x 42.40, 8 x 159.60, ...)
consumables   347.4872
labor         710.4900      27.50 x 24 man-hours x 1.0765
overhead     1271.7600      52.99 x 24
cost         4490.7572
price        6908.8572      cost / (1 - 0.35)   <- margin, NOT markup
profit       2418.1000      markup would have given 6062.52, a $846.34 error
```

Asserting against numbers this engine produced would be circular. Two
implementations agreeing is evidence; a shared bug would have to be written
twice, in two languages, from the same prose.

## Adding a case from a real job

The 222 historical quote sheets are the intended source. For each one, record
the inputs (SF, crew, hours, system, add-ons, suppressed lines, job-level
costs, margin), the unit costs **from that sheet**, and the total that was
quoted. Tolerance is +/-$0.01; if a sheet disagrees by more than that, the
disagreement is the finding — the workbook used markup in places, and this
engine deliberately does not.

## Coverage the curated set is meant to provide

| Fixture | Exercises |
|---|---|
| `epoxy-2000sf` | Baseline buildup, hand-verified |
| `polyurea-double-broadcast-1000sf` | Add-on REPLACE + ADD; negative unit costs (flake reclaim) |
| `seal-5000sf-reduced-consumables` | Per-system consumable multipliers; four consumables at 0.0 cost nothing |
| `polish-suppressed-mender-3000sf` | Per-job line suppression (factor 0.0) |
| `multi-area-job-costs-sub-floor` | Three areas/systems, job-level costs, manual-cost add-on, sub-floor margin flag |
