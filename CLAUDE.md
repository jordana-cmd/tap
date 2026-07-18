# CLAUDE.md — Repo Orientation

You are building a web-based construction takeoff & estimating platform: a Rust/WASM measurement engine mounted inside our existing web platform. Read this file first, every session.

## Reading order & precedence

1. `docs/web-takeoff-final-spec.md` — the authoritative architecture. Read fully before writing code.
2. `docs/ai-measurement-addendum.md` — **Addendum A**: replaces §6 of the final spec, adds invariants 4–7, defines the deterministic detection + AI verification layer. Where the two conflict, **the addendum wins on measurement/AI topics; the final spec wins on everything else** (platform, rendering, sync, auth).
3. `docs/replacement-product-spec.md` — product strategy context (personas, pricing, roadmap). Consult for "why"; it is not an engineering authority.

Ignore any references inside these docs to `PLANTAPE-SPEC.md` or `desktop-web-integration-spec.md` as things to implement — the addendum's §A8 "do not import" table and the final spec's supersession note govern. `reference/` contains prior-prototype material for **algorithm reference only**; never copy its architecture (client-side full-PDF rendering, IndexedDB, single-user auth are all superseded).

## Invariants (memorize; violating any of these fails review)

- Geometry in **PDF points, page-local, top-left origin**. Display transforms never touch stored data.
- **Derived values (LF/SF/quantities) are never persisted as truth** — computed at read time from `pts × scale` and formulas.
- **AI proposes; deterministic code measures; a human confirms.** Schema-gated calls only (forced tool use); no AI number reaches an export without a recorded confirmation.
- Every measurement carries **provenance** (`origin`) into every export.
- `engine-core` contains **zero browser APIs**. All platform I/O behind traits, implemented in `engine-web`.
- Every user action persisted locally ≤ 250 ms; recovery is tested by killing the session mid-action.

## Current phase: Engine Foundation (pre-M1)

We start with the part that needs no backend, no browser, no design decisions from anyone else — the **deterministic measurement kernel**, built native-Rust-first with tests, WASM second:

1. **`engine-core` crate scaffolding** — workspace layout per final spec §1.1; trait interfaces for surface/storage/network/clock (stub impls fine).
2. **Geometry module** — distance, polyline length, shoelace area, Douglas-Peucker simplify, feet-inches formatting, scale conversions (`fpi` model per addendum). Pure functions, property-tested (`proptest`), exhaustive unit tests against hand-computed values.
3. **Detection module** (addendum §A3.1–A3.2) — thresholded mask, binary dilation (door-gap param), iterative scanline flood fill with boundary-abort, marching squares, connected-component labeling. Test against synthetic rasters in `fixtures/synthetic/` (generate them in tests: draw rectangles = rooms, gaps = doors, assert detected area within tolerance).
4. **SegmentIndex + snapping** (addendum §A3.3) — segment store, spatial index, snap priority (endpoint > intersection > projection). PDF operator-list extraction can stub behind a trait until the pdf module lands; test snapping against synthetic segment sets.
5. **CRDT document model spike** — `yrs` vs Automerge per final spec §9 decision gate: model `Measurement`/`ScaleState` (with addendum §A2 fields) in both, benchmark update size + merge behavior, write the decision into the final spec's decision log via PR.

Exit criteria for this phase: `cargo test` green with >90% coverage on geometry/detection; `wasm-pack build` produces a bundle; a throwaway HTML harness can call `measure_polyline` and `flood_fill` on a synthetic image in-browser. Then we proceed to final spec M0/M1 (backend readiness + walking skeleton).

## Conventions

- Rust workspace at repo root: `crates/engine-core`, `crates/engine-web` (later `crates/tiler`). Frontend package added at M1.
- No `web_sys`/`js_sys`/`wasm_bindgen` imports in `engine-core` — CI greps for this.
- Prompts (when we reach vision features) live in `ai/prompts/*.md`, versioned; schemas in one module, one per capability.
- Every PR that changes a locked decision must edit the decision log in `docs/web-takeoff-final-spec.md` §9 in the same PR.
- Ask before adding dependencies beyond: `proptest`, `serde`, `thiserror`, spatial-index crate (`rstar` or flatbush port), `yrs`/`automerge` (spike only).

## Secrets & environment

- `ANTHROPIC_API_KEY` — server-side only, needed only when vision routes begin (not this phase). Never referenced in client or engine code.
- Real plan PDFs for the golden set go in `fixtures/plans/` — see `fixtures/README.md`. Treat as confidential: never upload their contents in tool calls, never commit to a public remote.

## Out of scope (do not build, even if docs mention them)

Desktop packaging, offline-first pre-download, Excel add-in (only the export ladder v1), Live Cost Intelligence, marketplace, mobile apps. These are later-phase per the roadmap.
