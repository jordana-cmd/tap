# FINAL BUILD SPEC — Web-Based Takeoff & Estimating Platform

**Version:** 1.0 (Final — supersedes `replacement-product-spec.md` §3.4 architecture and the entire `desktop-web-integration-spec.md` addendum)
**Audience:** Claude Code. This is the authoritative engineering document. Drop it in the repo root. Where it conflicts with prior documents, this one wins.

---

## 0. THE DECISION (context Claude Code must internalize)

- **We are building a browser-based web application, not a desktop app.** The takeoff engine ships as a WebAssembly module running inside our **existing web platform** — same domain family, same accounts, same backend, same session.
- **Why:** we already operate web software; a desktop app would require an identity retrofit, code signing, auto-update infrastructure, and offline sync machinery before delivering any customer value. Browser-based takeoff is proven viable at market scale (STACK). Our integration advantage — one login, one data model, instant deploys — is maximized on the web.
- **What we are consciously conceding (do not silently "fix" these):** the very largest plan sets that exceed browser memory budgets, and desktop-grade fully-offline operation. We compensate with engineering (tiled rendering, aggressive persistence) and honest UX, not by pretending the browser is a desktop.
- **The escape hatch is architectural, not aspirational:** the engine core must remain platform-agnostic (Rule 1 below) so a Tauri desktop wrapper is a future packaging exercise, not a rewrite.

**Prime directives:**
1. **The engine kernel (Rust) contains zero browser APIs.** No `web_sys`/`js_sys` calls inside `engine-core`. All platform I/O (rendering surface, storage, network, time) goes through trait interfaces implemented in a thin `engine-web` adapter crate. This single rule preserves the desktop option forever.
2. **The existing web platform owns identity, authorization, billing/entitlements, and canonical business data.** The takeoff module owns geometry, measurement, and rendering. Never duplicate a user table, an auth flow, or a source of truth.
3. **"Lose nothing" is the brand.** Every user action is persisted locally within 250 ms and durably to the server within seconds of connectivity. Reliability UX (autosave indicators, recovery flows) is a launch feature, not polish.

---

## 1. SYSTEM ARCHITECTURE

```
┌────────────────────────── Browser (app.ourdomain.com) ───────────────────────────┐
│                                                                                  │
│  Existing web app shell (auth’d session, nav, projects, billing UI)              │
│      └── /projects/{id}/takeoff  → Takeoff Module (SPA route)                    │
│            ├── UI layer: React/Vue/Svelte (MATCH the existing platform’s stack)  │
│            ├── engine-web (thin JS/TS + wasm-bindgen adapter)                    │
│            ├── engine-core.wasm  ← Rust kernel: geometry, measurement,           │
│            │        assemblies, CRDT doc, tile scheduler (NO browser APIs)       │
│            ├── Render workers: WebGPU (primary) / WebGL2 (fallback) canvas,      │
│            │        raster + PDF decode in Web Workers (wasm threads via         │
│            │        SharedArrayBuffer where COOP/COEP allows; fall back to       │
│            │        message-passing workers otherwise)                           │
│            └── Persistence: OPFS (Origin Private File System) via SQLite-WASM   │
│                     (wa-sqlite/absurd-sql class) + content-addressed tile cache; │
│                     IndexedDB fallback for non-OPFS browsers                     │
│                                                                                  │
└───────────────┬───────────────────────────────┬──────────────────────────────────┘
                │ HTTPS (same-origin/BFF)       │ WSS (ticket-auth’d)
     ┌──────────▼──────────┐          ┌─────────▼─────────┐
     │ Existing backend    │          │ Realtime service   │
     │ /api/v1 (versioned) │          │ (CRDT relay,       │
     │ auth, entitlements, │          │ presence, events)  │
     │ projects, pricing   │          └─────────┬─────────┘
     └──────────┬──────────┘                    │
                │ presigned URLs      ┌─────────▼─────────┐
     ┌──────────▼──────────┐          │ Postgres + Redis   │
     │ Object storage (S3) │          └───────────────────┘
     │ originals + tiles   │
     └──────────┬──────────┘
                │ queue (SQS/equivalent)
     ┌──────────▼──────────┐
     │ Processing workers  │  page-count/split, tile pyramid rasterization,
     │ (server-side)       │  thumbnails, vector text extraction, AV scan
     └─────────────────────┘
```

### 1.1 Language & module layout (monorepo packages)

| Package | Language | Responsibility |
|---|---|---|
| `engine-core` | Rust → wasm32 (and native for tests/benches) | Geometry kernel: linear/area/count/volume/deduction math, scale & unit handling, snapping, assembly evaluation (formula engine), CRDT document model (Yjs-compatible via `yrs`, or Automerge — pick one, see §9), tile visibility scheduler. Pure; deterministic; property-tested. |
| `engine-web` | Rust (`wasm-bindgen`) + TS | Adapter implementing `engine-core` traits against browser APIs: WebGPU/WebGL surface, OPFS storage, fetch/WS transport, `performance.now()` clock. All browser-specific code lives here. |
| `takeoff-ui` | Existing frontend framework | Canvas host, toolbars, assembly editor, measurement panels, review queues. Talks to the engine via a typed command/event bus (no direct wasm memory poking from app code). |
| `api-contract` | OpenAPI YAML | Source of truth for `/api/v1`. CI codegens TS client for `takeoff-ui` and (if backend is typed) server stubs. Breaking-change diff check on every PR. |
| `tiler` | Rust or existing backend language | Server-side worker: PDF/TIF/DWG→ tile pyramids (see §4). Reuses `engine-core` PDF parsing where possible. |

### 1.2 Rendering pipeline (the performance heart)

- **Never load whole documents into memory.** Server pre-generates a **tile pyramid** per page (like web maps: zoom levels × 512px tiles, WebP/lossless where linework demands). The client renders visible tiles + one ring of prefetch, evicting by LRU against an explicit **memory budget** (default 700 MB tile cache; measure, don't guess — see §8 telemetry).
- **Vector overlay on raster base:** takeoff geometry (traces, fills, counts) renders as GPU vector layers on top of raster plan tiles. "If it's colored, it's counted" — the painted-measurement metaphor from the product spec is sacred; every measurement visibly paints, every paint is quantified.
- **WebGPU primary, WebGL2 fallback,** feature-detected at startup; the render abstraction in `engine-web` hides which is active. CPU-canvas final fallback renders static tiles only (view-only mode) with a visible capability notice.
- **Interaction budget:** trace cursor and snapping run on the main-thread engine at ≤ 4 ms/frame; decode/raster/network never on the main thread. 60 fps pan/zoom on a 200-page set is the acceptance bar (§10).
- **High-DPI plans / civil sets:** the tile approach makes page size irrelevant to client memory; the concession in §0 is really about *simultaneous working-set breadth* (dozens of pages open with heavy overlays), and the budgeter + eviction handles that gracefully rather than crashing.

### 1.3 Local persistence & crash-proofing (the "lose nothing" implementation)

- **Every action → local write ≤ 250 ms:** engine emits CRDT updates; `engine-web` appends them to a WAL-style log in OPFS-backed SQLite. Compaction to snapshots every N updates / on idle.
- **Session recovery:** on load, if a local log is newer than the last server-acked state, replay it automatically and show "Recovered N unsynced changes from your last session." Test this by killing the tab mid-trace in E2E (§10).
- **Tab lifecycle:** handle `visibilitychange`/`freeze`/`pagehide` — flush the log synchronously-enough (OPFS sync access handle in a worker) before the browser suspends the tab. Assume mobile Safari and Chrome tab-discard will kill us without warning; design for it.
- **Storage quota:** request `navigator.storage.persist()`; monitor quota; when local cache pressure hits 80%, evict tiles first, never the un-synced WAL. If the WAL itself can't persist (quota denial), block further edits with an explicit modal rather than silently losing work — this is the one acceptable hard-stop in the product.
- **Multi-tab:** same project in two tabs must not corrupt state — use `Web Locks API` for single-writer on the WAL; second tab becomes live-view (CRDT-subscribed) with a "This project is being edited in another tab" affordance.

---

## 2. IDENTITY & SESSION (dramatically simplified vs. the desktop plan)

- **Same-origin session reuse.** The takeoff module is a route inside the existing authenticated app (or a subdomain behind a BFF sharing first-party cookies). **No OAuth device flows, no PKCE loopback listeners, no OS keychains, no token storage in the client.** The module inherits whatever session the platform already has.
- Requirements on the existing platform:
  - [ ] Expose `GET /api/v1/me` → `{user_id, org_id, name, entitlements[]}` for the module's bootstrap.
  - [ ] CSRF: the module's mutating calls use the platform's existing CSRF mechanism (it's a same-origin browser client — standard rules apply, unlike the desktop case).
  - [ ] If the platform spans multiple apps/domains today, put the takeoff module on the primary app domain; solve cross-domain SSO later only if forced.
- **WebSocket auth:** `POST /api/v1/realtime/ticket` (session-auth'd) → one-time short-lived ticket presented in the WS handshake. No long-lived credentials in query strings.
- **Session expiry mid-work:** on a 401, the module pauses sync, keeps the local WAL accumulating, and shows a non-blocking "Session expired — sign in to keep syncing" banner that opens the platform's login in a new tab; on success (poll `/me`), sync resumes and the WAL drains. **Editing never hard-stops on auth expiry.**
- **Entitlements:** seats/tiers live in existing billing. Module checks entitlements at bootstrap and subscribes to entitlement-change events on the WS. Feature-gate in UI, **enforce on the server** — the browser client is untrusted, same as always.

---

## 3. BACKEND API REQUIREMENTS (work items on the existing platform)

Versioned surface at `/api/v1`. These remain necessary even without a desktop app — they serve the takeoff module, future mobile, and third parties:

| Endpoint group | Spec |
|---|---|
| **Projects/business objects** | Standard REST on existing resources; cursor pagination; `updated_since` delta param on lists (cheap now, enables offline-tolerant sync + future clients); soft-delete **tombstones** so deltas propagate deletions. |
| **CRDT document store** | `POST /v1/projects/{id}/doc/updates` (append opaque binary updates, server assigns seq), `GET /v1/projects/{id}/doc/updates?since_seq=`, `GET /v1/projects/{id}/doc/snapshot` (latest compacted snapshot + seq). Server compacts snapshots async. Backend never interprets geometry. |
| **Derived quantities** | Client posts materialized takeoff results (`items[]` with quantities, assembly refs, page refs) on change-debounce → powers reports, dashboards, and the rest of the platform without running the engine server-side. Marked as derived; CRDT doc remains geometric truth. |
| **Files** | `POST /v1/files` → presigned **multipart** upload (direct to object storage, resumable, per-part checksums); `POST /v1/files/{id}/complete` → enqueue processing; status webhook/WS event when tiles are ready. Never proxy plan bytes through app servers. AV-scan on complete. |
| **Tiles** | `GET` via short-lived signed CDN URLs (`/tiles/{file}/{page}/{z}/{x}/{y}`); immutable + content-hashed → aggressive CDN + browser caching. |
| **Idempotency** | Accept `Idempotency-Key` on all mutating endpoints; dedupe ≥ 24 h. Client generates UUIDv7 ids for created objects. (Protects against retry storms and flaky-connection replays even in a "mostly online" product.) |
| **Errors & limits** | Uniform JSON error envelope `{code, message, retryable}`; per-user rate limits with `429` + `Retry-After`; client implements exponential backoff + jitter. |
| **Client config** | `GET /v1/client-config` → `{min_supported_engine_version, feature_flags, tile_cdn_base, limits}`. Web deploys are instant, but the wasm bundle is cached — this endpoint lets us force-refresh stale engine bundles and kill-switch features. |

**Realtime service:** WS endpoint relaying CRDT updates per project room, presence (who's viewing/editing which page), and platform events (file processed, entitlement changed, plan revision uploaded). Heartbeat 25 s; client auto-reconnect with backoff and `since_seq` resume. SSE fallback for proxy environments that kill WS.

---

## 4. FILE INGEST & PROCESSING PIPELINE

1. Upload (presigned multipart, resumable; drag-drop + plan-room import per product spec).
2. Worker: identify type (PDF/TIF/DWG/DXF/DWF/JPG/PNG), split pages, extract vector text layer (for search & future AI), rasterize **tile pyramid** per page at fixed zoom levels, generate thumbnails, compute per-page suggested scale (detect scale bars/known title-block patterns where feasible — feeds Auto Scale later).
3. Emit `file.ready` event → module shows pages progressively **as each page's tiles land** (never block on whole-set completion; a 400-page set should show page 1 in seconds).
4. Revisions: uploading a new version of a sheet links it to its predecessor (same sheet number heuristic + manual override) — foundation for Revision Radar in the product spec, not built at launch but the data model anticipates it.

---

## 5. OFFLINE POSTURE (honest scope)

- **Target: offline-tolerant, not offline-first.** Connectivity blips (jobsite Wi-Fi, tethering, elevators) must never lose work or interrupt tracing; a fully-offline day is out of scope for v1.
- Mechanism: the local WAL (§1.3) already queues everything; sync layer drains it on reconnect with idempotent replay; tile cache serves already-visited pages offline; service worker caches the app shell + wasm bundle so a mid-session network drop doesn't break navigation within the module.
- UI: single connectivity pill — `Synced ✓ / Syncing… / Offline — changes saved locally (N pending)`. Timestamps on hover. This *is* the trust surface from the product spec's UX philosophy.
- Explicitly deferred: pre-downloading full projects for planned offline work ("take this project to the jobsite" button) — designed-for but Phase 2; note it in the UI roadmap, don't fake it.

---

## 6. AI TAKEOFF INTEGRATION (server-side, verifiable)

- Inference runs **server-side** on uploaded originals/tiles (client stays lean); results return as **proposed measurements** into a review queue: each carries geometry, confidence score, and a click-to-zoom provenance view. Accept/reject writes real measurements into the CRDT doc attributed to "AI, accepted by {user}".
- Contract: `POST /v1/projects/{id}/ai/detect {pages[], modes[]}` → job id → results streamed over WS as ready per page.
- **Training-data promise from the product spec is binding:** customer plan content is used for model training **only with explicit org-level opt-in** (flag in entitlements); enforce in the data pipeline, not just the ToS.
- Ship order: Auto Count (symbol matching) → Auto Scale → area/linear detection. Never market unverified accuracy percentages; log accept/edit/reject rates as the accuracy metric from day one.

---

## 7. EXCEL INTEGRATION (web-era version of "Live Link")

Desktop COM-style live links don't exist in a browser. The web-honest ladder, in build order:
1. **v1 — Structured export:** one-click `.xlsx` (SheetJS or server-side) with **stable named ranges & sheet schema** so customers' legacy pricing workbooks can `VLOOKUP`/reference it unchanged; plus CSV/JSON.
2. **v1.5 — Pull-refresh:** documented, entitlement-guarded read endpoint (`/v1/projects/{id}/quantities.xlsx|json`) usable from Excel's Power Query / "Get Data from Web" — quantities refresh inside the customer's own workbook with one click. This is 80% of "live" for near-zero build cost; write the customer-facing how-to as part of the feature.
3. **v2 — Office.js add-in:** true in-Excel task pane syncing bidirectionally, and Google Sheets add-on equivalent. Scoped later; the API from step 2 is its foundation.

---

## 8. OBSERVABILITY, PERFORMANCE BUDGETS & BROWSER SUPPORT

- **Error/crash reporting:** Sentry (JS + wasm with source maps/DWARF) from the first commit; scrub plan content and customer-identifying paths from payloads; user-consent toggle per platform policy.
- **Perf telemetry (the numbers behind the reliability brand):** wasm bundle size (budget ≤ 8 MB compressed), time-to-first-page-rendered (budget ≤ 3 s on the reference machine), frame-time p95 during pan/zoom (≤ 16 ms), tile-cache hit rate, memory high-water mark, WAL flush latency p99, sync drain time after reconnect, recovery events count. Dashboards + alerts; these feed the product spec's "public reliability reports."
- **Browser support matrix (enforced in CI via Playwright):** Chrome/Edge latest-2 (primary — most Windows construction offices), Safari latest-2 (macOS/iPad — kills the Parallels persona's pain), Firefox latest-2 (best-effort). WebGPU where available; WebGL2 floor. **iPad Safari is a real target** (field viewing per product spec), so test touch pan/zoom and its stricter memory limits explicitly.
- **COOP/COEP headers** required on the module route for SharedArrayBuffer threading — coordinate with the existing platform's embeds/analytics, which cross-origin isolation can break; if isolation is infeasible on the main domain, fall back to non-shared workers (slower decode, identical correctness) and record the decision.

---

## 9. DECISION LOG (defaults locked; override in a PR that edits this file)

| Decision | Choice | Rationale / rejected alternative |
|---|---|---|
| Platform | Browser web app inside existing platform | Desktop deferred; engine-core purity (§0 Rule 1) keeps the option open |
| Engine language | Rust → WASM | Performance + native-ready; TS engine rejected (perf ceiling, no desktop path) |
| CRDT library | `yrs` (Yjs Rust port) | Mature ecosystem, y-websocket-compatible relays, JS interop for UI; Automerge acceptable if richer history wins in a spike — decide in Milestone 1, then locked |
| Local store | SQLite-WASM on OPFS, IndexedDB fallback | Raw IndexedDB rejected (no transactional WAL semantics) |
| Rendering | Server tile pyramid + client GPU vector overlay | Client-side full-PDF rendering rejected (memory ceiling = the PlanSwift failure mode reborn) |
| PDF originals on client | Only for on-demand vector snapping regions, streamed | Whole-file download rejected |
| Auth | Existing platform session, same-origin | Separate IdP/token flow rejected as needless complexity for a same-domain module |
| Realtime | WS + ticket auth, SSE fallback | Polling-only rejected (collab is a differentiator) |
| Excel | Export → Power Query pull → Office.js add-in ladder | Claiming "live link" at v1 rejected as dishonest |
| AI inference | Server-side | On-device wasm inference rejected for v1 (bundle size, capability variance) |
| Frontend framework | Whatever the existing platform uses | New framework rejected — the module must feel native to the product |

---

## 10. MILESTONES (build order for Claude Code)

**M0 — Platform readiness** *(existing backend repo)*
`/api/v1` skeleton + OpenAPI + CI contract diff; `/me`; presigned multipart uploads + processing queue + tiler producing pyramids for PDF (other formats next); tile CDN routes; CRDT update/snapshot endpoints; realtime ticket + WS relay; client-config; idempotency middleware; error envelope.
*Exit test:* upload a 200-page PDF via API; tiles appear on CDN; WS emits `file.ready` per page.

**M1 — Engine walking skeleton** *(new packages)*
`engine-core` with scale/units + linear & area measurement + CRDT doc (spike `yrs` vs Automerge here, then lock §9); `engine-web` with WebGPU/WebGL tile canvas, pan/zoom, OPFS WAL; `takeoff-ui` route mounted in the existing app behind a feature flag; draw a line on a real plan, see its length, reload the tab, it's still there, open a second browser, watch it appear live.
*Exit test:* the sentence above, automated in Playwright, plus kill-tab-mid-trace recovery.

**M2 — Takeoff completeness**
Count, volume, cutout/deduction modes; snapping; per-page scale UI with calibration; multi-page navigation with prefetch; measurement list panel; derived-quantities posting; connectivity pill + sync drain; multi-tab locks; quota handling.
*Exit test:* full manual takeoff of a real 50-page trade set by a human tester without touching the console; perf budgets green.

**M3 — Assemblies (the crown jewel)**
Formula engine in `engine-core` (typed expressions: quantities, waste %, labor rates, pitch/thickness corrections, conditionals); assembly authoring UI with progressive disclosure + starter templates per trade; apply-assembly-to-measurement flow; versioned assemblies stored via platform API; xlsx export with stable named ranges (§7.1).
*Exit test:* the drywall scenario from the product spec — trace one wall, get sheets/studs/insulation/fasteners/hours.

**M4 — Collaboration & trust surfaces**
Presence, comments/@mentions on measurements, entitlement gating, Power Query pull endpoint + customer doc (§7.2), reliability dashboard internal, browser-support CI matrix complete.

**M5 — AI review queue** (§6, Auto Count first) and revision-linked uploads (§4.4). Product-spec Phase-2 features (Live Cost Intelligence, marketplace, field app) follow per the original roadmap, unchanged.

---

## 11. RISKS SPECIFIC TO THE WEB CHOICE (watchlist)

| Risk | Signal | Response |
|---|---|---|
| Browser memory ceiling hits real customers | Telemetry: eviction thrash / OOM reloads on large sets | Tune budgets; page-group working sets; if a paying segment persistently exceeds the browser, trigger the Tauri wrapper (engine-core is ready by design) |
| Safari/OPFS quirks | CI matrix failures, iPad field complaints | IndexedDB fallback path stays maintained, not rotting |
| COOP/COEP breaks a platform integration | Third-party embed failures on the module route | Non-shared-worker fallback (§8) |
| "Web app" perception vs desktop incumbents | Sales objections | Sell reliability *behavior* (autosave, recovery, uptime stats) not architecture; publish the perf numbers |
| Wasm bundle bloat | Bundle budget CI check fails | Feature-split wasm (assemblies module lazy-loaded), `wasm-opt`, no kitchen-sink crates |

---

**README one-liner:** *A Rust/WASM takeoff engine mounted inside our existing web platform: server-tiled plans rendered on the GPU, every action journaled locally within 250 ms and synced via CRDTs over an authenticated WebSocket, assemblies as the crown jewel, the platform as the single source of identity and truth — and an engine core clean enough that a desktop wrapper stays a packaging decision, never a rewrite.*
