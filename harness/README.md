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

## Run

- Serve locally: `powershell -ExecutionPolicy Bypass -File serve.ps1`, open `http://localhost:8787/`.
- Interaction test suite (headless Chromium): `cd tests && npm ci && node run-interactions.mjs`.
  Process rule: no interaction behavior is claimed to work without a green suite run.
