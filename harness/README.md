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
both. `#/quote` is currently a **placeholder** ("Coming soon"): it renders the
handed-off quantities read-only and holds no pricing logic yet.

A route rather than a second page, deliberately: same tab, real history, and the
wasm engine plus live measurement state stay in memory instead of re-booting
just to render a quote.

`buildQuotePayload()` is the entire interface between the two, and the thing a
real pricing tool should build on:

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

All harness-level: `engine-core` and `engine-web` are untouched by the quoting
work.

## Run

- Serve locally: `powershell -ExecutionPolicy Bypass -File serve.ps1`, open `http://localhost:8787/`.
- Interaction test suite (headless Chromium): `cd tests && npm ci && node run-interactions.mjs`.
  Process rule: no interaction behavior is claimed to work without a green suite run.
