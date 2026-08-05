# Throwaway measurement harness

A local, single-file instrument for exercising the `engine-core` / `engine-web`
wasm measurement kernel against real PDFs: pdf.js renders and extracts geometry,
the wasm bundle measures, and the harness draws overlays and keeps a page-scoped
measurement list. It is **not** the product — the product renders server-side
tile pyramids and lives in a React app (final spec). See the §A8 guard comment in
`index.html`.

## Persistence — a bounded, harness-only deviation from §A8

The addendum's §A8 rejects **IndexedDB** for the product in favor of
OPFS + SQLite-WASM + `yrs`. That stands for the product. The **harness** uses
IndexedDB (`store.js`) as an explicit, bounded deviation: losing a full sheet of
measurements on reload blocks real-world testing today, and gating persistence
behind the unbuilt product store would stall that for weeks. The stored shapes
mirror §A2 field names (`Measurement` label/origin/geometry/…; `ScaleState`
`feet_per_paper_inch`/`source`) and never persist derived values (invariant 5 —
LF/SF/quantities recompute on load), so migrating to the product store is a
serialization mapping, not a rewrite. This is a harness note, **not** a spec
change.

Content-addressed by PDF SHA-256: the same file restores under any path or
filename. JSON project export/import is the backup and escape hatch if the
browser clears site data.

## Routes and the takeoff → quoting handoff

The harness is two views behind a hash route, swapped by a `data-route`
attribute on `#appShell`: `#/takeoff` (the canvas + measurement tree, default)
and `#/quote`. The **Quote this job** button at the foot of the takeoff panel
navigates; so do the browser's back/forward buttons, because navigation always
goes through `location.hash` rather than a direct view swap — one code path for
both. `#/quote` is the **priced quote screen**: profit and margin at the top,
one expandable block per area (system, crew, hours, add-ons, every line item),
and job-level costs with the margin that sets the price.

A route rather than a second page, deliberately: same tab, real history, and the
wasm engine plus live measurement state stay in memory instead of re-booting
just to render a quote.

### Every number on it comes from `engine-core`

The screen calls `price_job_json(cardJson, jobJson)` and renders what comes
back. It does not compute a cost, a price, a margin, a profit, or a subtotal —
a second implementation of the cost buildup in JavaScript is exactly how the
two drift apart, and the one in Rust is the one with the tests and the golden
fixtures behind it. Consequences worth knowing:

- **Re-priced on every change**, from scratch. No cached quote, no recalculate
  button, so nothing can go stale; there is no invalidation logic to get wrong.
- **The rate card is host-loaded** from `/data/rate-cards/` at runtime (never
  compiled into the wasm), validated by the engine rather than by JS — a card
  that merely parses can still name a product that does not exist. If it fails
  to load, the screen says so and shows no prices at all: a blank price is
  recoverable, a confidently wrong one is not.
- **The engine's refusals are surfaced verbatim**, naming the area with no
  crew/hours or the add-on that does not apply. An area with no system is
  listed and called out, never silently dropped from the total.
- **Lines are never hidden by disappearing.** Removing a line stores a per-job
  factor of 0 and it stays on screen struck through at $0.00 with an undo;
  consumables a system runs at 0× come back from the engine as $0.00 lines and
  sit behind a "show N items not used by this system" toggle, read-only,
  because that is a catalog fact rather than a decision about this job.
- `serve.ps1` and the test server both map `/data/` to the repo's `data/`
  directory; without it the screen loads but prices nothing.

### The handoff payload

`buildQuotePayload()` remains the takeoff → quoting handoff (and the shape an
external pricing tool would consume; the priced screen above reads the live
measurements directly):

- `quantity` mirrors `engine-core` `assembly::MeasurementInput` field names
  (`area_sf` / `perimeter_lf` / `length_lf` / `count_ea`), so an item maps onto
  an assembly input with no translation layer. Fields a kind does not carry are
  `null`, never `0` — "no perimeter" and "a perimeter that happens to be zero"
  must not collide inside a formula. Area perimeter is a real field here, not
  the parenthetical inside `valueText`; a flooring assembly prices cove base and
  transition strip off it.
- **Derived, never persisted** (invariant 5 — the rule Rust's `BillOfMaterials`
  follows for the same reason). Rebuilt on every navigation, so it cannot drift
  from the measurements it describes, and nothing new is written to IndexedDB.
- `projectId` is the PDF SHA — the same content-addressed key the stored project
  record uses, so navigating back and forth stays bound to the right job.

Transport is deliberately not baked in: today the quote view reads the returned
object directly. Moving to `postMessage` or a server fetch later changes the
caller, not the shape. Bump the `schema` string (`takeoff-quote-payload@1`) if
the shape changes.

The payload itself is harness-level. The pricing behind the screen is not:
`engine-core::pricing` owns the buildup and `engine-web` exposes the single
`price_job_json` entry point it is reached through.

## Run

- Serve locally: `powershell -ExecutionPolicy Bypass -File serve.ps1`, open `http://localhost:8787/`.
- Interaction test suite (headless Chromium): `cd tests && npm ci && node run-interactions.mjs`.
  Process rule: no interaction behavior is claimed to work without a green suite run.
