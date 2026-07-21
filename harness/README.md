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

## Two views: TAKEOFF and QUOTE

Measuring and quoting are different jobs, so they are different views, switched
from the tabs above the toolbar.

- **TAKEOFF** — the drawing: canvas, tools, scale, the measurement list grouped
  by scope and condition, and a single `exports…` menu.
- **QUOTE** — a full-width page with no canvas: project/customer fields, the
  scope summary (each measured line, editable), the cost stack, pricing
  controls, live per-scope and combined totals, and the export actions.

Which view you are in is session state and is deliberately **not** persisted —
reopening a project puts you back on the drawing.

**Invariant 5 on the quote page.** A scope line's quantity is DERIVED from
geometry × page scale, so it is displayed read-only with a jump back to the
drawing. Everything else on the line (assembly, stacked add-ons, per-part
overrides, scope assignment) is an input and is editable there. What persists
is inputs only — `projectInfo`, `costInputs`, `laborByScope`; never a cost,
price, margin or BOM.

## The assembly library is versioned

The library seeds from `engine-core`'s catalog and is UPGRADED when the engine's
`SEED_VERSION` rises (`migrateAssemblyLibrary` in `index.html`). It holds three
properties at once: a seed you deleted never comes back, an old library reaches
the current catalog, and anything you authored or hand-edited is never
clobbered. Retiring a seed requires an entry in `RETIRED_ASSEMBLIES` so existing
projects that reference it keep pricing identically.

Rates and modelling assumptions that have not been checked against a real job
are listed in [`../docs/pricing-review.md`](../docs/pricing-review.md).

## Run

- Serve locally: `powershell -ExecutionPolicy Bypass -File serve.ps1`, open `http://localhost:8787/`.
- Interaction test suite (headless Chromium): `cd tests && npm ci && node run-interactions.mjs`.
  Process rule: no interaction behavior is claimed to work without a green suite run.
