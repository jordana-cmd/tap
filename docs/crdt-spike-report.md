# CRDT Spike Report — `yrs` vs `automerge` (final spec §9 decision gate)

**Date:** 2026-07-18 · **Versions:** yrs **0.27.3** (Jul 13 2026), automerge **0.10.0** (Jun 5 2026) — both current at spike time.
**Instrument:** `spikes/crdt/` (own workspace, excluded from the production build; pinned lockfile). Deterministic: seeded LCG (`0xC4D7_5EED_0000_0001`), fixed actor ids, fixed timestamps; two consecutive runs produce byte-identical output (timings excepted). Machine: Windows 11, native release build.

**Document shape** (identical in both): `pages` map → per-page `ScaleState { fpi, source, verification? {method, medianDeviationPct, verdict, at, samples[3]} }`; `measurements` map → `{ id, page, kind, name, color, origin, confirmedBy?, parentId?, pts }` with `pts` a **CRDT list of flat f64 pairs** (base-unit points, invariant 4 — the representation that makes per-vertex incremental updates possible) and `name` a **plain string register** (short names, atomic renames from a text input; a Text CRDT was not benchmarked).

**Wire bytes** = each library's actual sync unit: yrs `observe_update_v1` payload per transaction; automerge `save_incremental()` per committed change.

---

## W1 — Update size (bytes on the wire) — weight 30%

| case | yrs | automerge | ratio |
|---|---|---|---|
| (a) first vertex append | 15 | 118 | 7.9× |
| (a) **steady-state per-vertex append** (median, appends 10–29) | **15** | **119** | **7.9×** |
| (a) 29 appends total | 435 | 3,443 | 7.9× |
| (b) completed 30-vertex measurement, one transaction | 686 | 691 | 1.0× |
| (c) rename | 35 | 129 | 3.7× |
| (d) confirmedBy stamp | 66 | 143 | 2.2× |

The number that dominates jobsite bandwidth is (a): tracing emits one small update per vertex, continuously, and yrs ships it in 15 bytes to automerge's 119. Automerge's per-change envelope (change hash, actor, deps) is fixed overhead that can't amortize on tiny changes; on the one-transaction batch (b) the two are equal. **yrs wins this dimension decisively.**

## W2 — Document growth (create 2,000 → edit 500 → delete 500) — weight 10%

| metric | yrs | automerge |
|---|---|---|
| cumulative update-log bytes | 956,468 | 1,206,084 |
| snapshot after session | 705,742 | 585,579 |
| snapshot after reload + re-save (compaction) | 705,742 | 585,579 |

Both reclaim deleted-measurement *content*: 500 deletions shrink the doc rather than growing it (yrs GC of deleted blocks is on by default; automerge's `save()` is already columnar-compressed). Neither shrinks further on reload — each is already at its steady state. Automerge's snapshot is ~17% smaller (columnar compression); yrs's cumulative log is ~21% smaller. Near-wash, slight edge automerge on at-rest size, yrs on wire.

## W3 — Merge semantics — weight 20% (with the 3(c) gate)

Two replicas fork from the same base (m0000 "Room 0" 5 vertices, m0001 "Room 1" 5 vertices), diverge, then merge. Every scenario was applied in **both orders**; all outcomes below were order-independent for both libraries.

### (a) Different measurements — both correct

```
yrs:        m0000: name="Kitchen A" vertices=6    automerge:  m0000: name="Kitchen A" vertices=6
            m0001: name="Bath B"    vertices=6                m0001: name="Bath B"    vertices=6
```

### (b) Both rename m0000 concurrently ("Kitchen A" vs "Kitchen B")

```
yrs:        m0000: name="Kitchen B"   — deterministic LWW; the losing value is GONE
automerge:  m0000: name="Kitchen B"   — deterministic winner; loser still queryable:
            get_all(name) = ["Kitchen A", "Kitchen B"]
```

Both resolve deterministically. Automerge additionally keeps the losing register value recoverable via `get_all` — a real (if minor) edge for building "someone else renamed this" affordances.

### (c) A deletes m0000 while B is mid-trace on it (3 vertex appends + rename) — **the gate**

```
yrs (order-independent: true):        m0000 ABSENT — B's three vertices and rename silently discarded
automerge (order-independent: true):  m0000 ABSENT — B's three vertices and rename silently discarded
```

**Stated plainly, per the review rider: BOTH libraries silently discard the estimator's in-progress work when a concurrent delete lands.** In both map semantics, deleting the key wins over concurrent edits *inside* the mapped object; the edits merge into an unreachable object and vanish from the visible doc. The outcome is deterministic in both — no resurrection, no partial ghost — but "your last 30 seconds of tracing evaporated because a teammate hit delete" is unacceptable UX either way. **Neither library is disqualified by 3(c), because neither is better**: this is a property of CRDT map-delete semantics, not of one implementation.

**Required application-level mitigation (recommended for the final spec's data model):** measurements must be **soft-deleted** — `deleted: {by, at}` is *set as a field* rather than removing the map entry. Concurrent edits then merge normally into a tombstoned measurement; the UI filters `deleted` items, offers restore, and a compaction policy hard-deletes tombstones after a retention window (e.g. on snapshot compaction after N days). This costs a tombstone record per deletion (bounded, and W2 shows both libraries handle eventual hard-deletes well). This mitigation is identical for both libraries, so it does not differentiate the choice — but it belongs in the spec before the document model is built.

## W4 — Snapshot + cold load at 5,000 measurements — weight 3%

| metric | yrs | automerge |
|---|---|---|
| snapshot bytes | 2,275,276 | 1,440,395 |
| cold load, median of 10 (ms, native release) | 114.7 | 466.7 |

Automerge's snapshot is 37% smaller; yrs loads 4× faster. At ~2.3 MB / ~115 ms for a 5,000-measurement project, neither number threatens the product; wasm will be slower for both.

## W5 — Wasm footprint — weight 10%

Method: three `cdylib` crates + a no-op baseline, identical manifests and profile (`opt-level="z"`, fat LTO, 1 CGU, `panic="abort"`), `wasm32-unknown-unknown`, no wasm-bindgen anywhere; each export genuinely exercises create → encode → apply → save → load so DCE can't cheat. Sizes are net of the 377 B / 334 B baseline. (`wasm-opt` was not available via winget; raw + `gzip -9` reported, method identical for both. The automerge crate carries a ~40-byte deterministic getrandom shim its uuid dependency requires on this target — a stand-in for the JS randomness glue a production build would carry.)

| | yrs 0.27.3 | automerge 0.10.0 |
|---|---|---|
| raw .wasm (net) | 401,482 B | 1,292,884 B |
| gzip -9 (net) | 127,331 B | 373,150 B |
| % of the 8 MB compressed budget | 1.6% | 4.6% |

Both fit the budget easily; yrs is ~2.9× smaller compressed. (Note: automerge's footprint nearly doubled between 0.5.12 and 0.10.0 — 228 KB → 373 KB gz — measured during the version bump.)

## W6 — Undo/redo — weight 10%

- **yrs: native `UndoManager`** — scoped to chosen shared types, origin-filterable (undo only *local* changes — exactly what a collaborative estimator needs), with capture-timeout batching of adjacent transactions (configurable; the demo's rename+append merged into one undo step). Demonstrated working: rename + vertex append undone and redone correctly.
- **automerge 0.10: no undo/redo API exists** (verified against the 0.10.0 source — no public `undo` symbol). Undo would be application-built: per-local-change inverse patches or head-diffing, carefully excluding remote changes. That is exactly the "expensive to bolt on later" cost the evaluation was told to weigh.

## Qualitative — ecosystem/relay + JS interop (15%), maintenance (2%)

- **yrs**: the y-crdt README commits to "behavior and binary protocol compatibility with Yjs, therefore projects using Yjs/Yrs should be able to interoperate with each other." That means the final spec's WS relay can be any y-websocket/y-protocols-compatible server (several production options exist), and the `takeoff-ui` layer can consume updates with mainline Yjs tooling. `ywasm` is the first-party wasm/JS wrapper. This is precisely the relay path final spec §9 already assumed.
- **automerge**: first-party `@automerge/automerge` JS package (same core via wasm) and a sync protocol "for efficiently transmitting changes over the network"; networked sync in practice means adopting automerge-repo or implementing their sync protocol on our relay — a heavier lift for our §3 realtime service than a y-protocol relay.
- **Maintenance (both healthy):** yrs 0.27.3 released 2026-07-13, 80 releases, ~2.0M downloads. automerge 0.10.0 released 2026-06-05 with three minor releases in 2026, ~426K downloads. No maintenance red flag on either side; the 0.5→0.10 automerge jump compiled here with a one-line change, and the yrs 0.21→0.27 jump needed only the UndoManager constructor updated.

---

## Recommendation

**Adopt `yrs`. The final spec's default is confirmed by data, not by default.**

| dimension | weight | yrs | automerge | weighted yrs | weighted am |
|---|---|---|---|---|---|
| update size (W1) | 30% | 10 | 3 | 3.00 | 0.90 |
| merge semantics (W3) | 20% | 6 | 7 | 1.20 | 1.40 |
| ecosystem/relay + JS | 15% | 9 | 6 | 1.35 | 0.90 |
| wasm footprint (W5) | 10% | 9 | 6 | 0.90 | 0.60 |
| growth/compaction (W2) | 10% | 7 | 8 | 0.70 | 0.80 |
| undo (W6) | 10% | 9 | 2 | 0.90 | 0.20 |
| snapshot/load (W4) | 3% | 7 | 5 | 0.21 | 0.15 |
| maintenance | 2% | 8 | 8 | 0.16 | 0.16 |
| **total** | 100% | | | **8.42** | **5.11** |

The 3(c) gate does not overturn this: both libraries fail it identically, so it mandates the **soft-delete mitigation** (above) rather than discriminating between them. Automerge's genuine advantages — recoverable conflict losers, smaller snapshots — do not outweigh a 7.9× penalty on the single highest-volume message the product emits, the missing undo manager, a 2.9× wasm footprint, and a relay protocol we'd have to build rather than adopt.

**Revisit condition for automerge:** reopen this decision if (1) the document model comes to depend on register-conflict *recovery* (multi-writer attribute merging beyond LWW) as a product feature, or (2) automerge ships a native undo manager AND its per-change wire overhead drops to within ~2× of yrs on workload W1(a) — re-run this spike's pinned workloads to check; the instrument is committed for exactly that purpose.

**Follow-ups for the document-model step (not this spike):** soft-delete tombstones in the `Measurement` model (gate mitigation, above); `origin`/`confirmedBy` immutability enforced at the application layer (CRDTs don't do field immutability); undo scoped to local origin via `UndoManager::include_origin`.
