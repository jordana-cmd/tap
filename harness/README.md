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

### The hours readout

One man-hour is **$82.59 fully loaded** ($27.50 wage + 7.65% payroll tax +
$52.99 overhead), which makes hours the most leveraged input in the model by
an order of magnitude — and a real quote came out roughly 2.3× high on an hour
figure nothing on screen questioned. So beside every area's crew and hours,
live: **man-hours** and **SF per man-hour**, with the historical median
(~22 SF/man-hour across ~220 jobs) shown next to it.

The median is shown *always*, not only when something looks wrong. The quote
that prompted this ran at 10.4 SF/man-hour, comfortably inside the advisory
band; what catches that is seeing 10.4 beside 22, not a warning.

Outside roughly 8–60 SF/man-hour the readout picks up a quiet advisory. It is
**informational and never blocks** — some jobs genuinely run outside it — and
it is deliberately styled quieter than the sub-margin-floor warning, which
reports a decision about the job rather than a number that looks unusual. The
band and the median are `engine-core` constants read through
`productivity_band_json()`; the flag itself is computed per area by the engine
(`Productivity::{Low,Typical,High,Unknown}`), so the screen renders a verdict
it does not make. With hours empty or zero the readout is **blank** rather than
`Infinity` — the engine returns `None`, because a half-entered quote is
unfinished, not broken.

The job-level block carries the same pair, **blended** (total SF ÷ total
man-hours) rather than averaged: a 200 SF closet and a 20,000 SF warehouse are
not equal votes on how fast a job runs.

### Grit

A grit level per area, offered only on systems that grind (`standard_grit` in
the rate card; polish and seal today, not epoxy or polyurea). Levels come from
the card's `grit_levels` ladder, never a hardcoded list — adding 1200 grit is a
data edit.

**The escalator multiplies man-hours, not cost.** More passes is more time, and
routing it through hours means labor, overhead, and the readout above all pick
it up with no special-casing. What is applied is a *ratio*: the chosen level's
multiplier divided by the system's own standard, so a system quoted at its
standard is always exactly 1.0 and hours that already describe 400-grit work
are never charged twice for it.

**Every multiplier ships seeded at 1.0**, so the whole mechanism is inert until
Jordan has timing data — a test asserts that selecting any grit leaves cost,
price, profit and man-hours bit-identical. A grit stranded on a measurement by
a system change is remembered but never sent, since the engine rejects a grit
it cannot apply rather than ignoring it.

### Bid items

An area is a takeoff unit; a **bid item** is a line on the proposal. They are
not the same shape — a bathroom on page 7 and a corridor on page 22 may be one
"Restrooms" line at one price — so grouping sits *above* pricing and never
changes how an area is priced.

- **Every area is in exactly one item**, always. Anything ungrouped gets its
  own, named from the area, so the default is identical to what existed before.
  A project saved before bid items had them loads to exactly the quote it had:
  there is no migration branch because there is nothing to migrate.
- **Members may span pages.** A bid item is not page-scoped; nothing in the
  pricing layer knows what page an area came from.
- **All members must share a system.** Rejected in the UI with a reason rather
  than allowed: one lump sum carries one scope narrative, and a mixed item
  would hide an epoxy area inside a grind-and-seal description. The engine
  rejects it too, but the UI refusal explains one bad grouping instead of
  blanking the whole quote.
- **Deleting an area** drops it from its item; an item left with no members is
  removed. An item is a name for some work — with no work under it, it is not
  an empty item, it is not an item.
- Membership, names and alternate flags are **inputs**, persisted with the
  project alongside crew/hours. Prices stay derived.

**Alternates** are bid items flagged out of the base bid — the mechanism for
"add double broadcast: +$X". They are priced identically (same engine, same
margin) but never summed into the headline: the headline is the **base bid**,
labelled as such whenever alternates exist, with alternates listed below at
`+$X` each plus an "if all accepted" total.

Job-level costs sit inside the base bid — mobilization is not contingent on an
alternate being accepted, and an alternate quietly carrying a share of it would
be priced differently depending on what else happened to be on the quote. So Σ
base lump sums + marked-up job costs = the quoted price; the engine reports
that markup as `job_cost_price` so a proposal can be shown to add up.

Job-level man-hours and SF/man-hour are **base-bid only**, for the same reason:
blending in hours for work that may never happen describes a job nobody is
going to run. Per-area readouts still cover every area.

**Mixed grit within one item** is allowed and priced correctly — each area
keeps its own hours — but flagged, because one lump sum under one grit-bearing
name would describe work that is not what was priced, and no number on the page
reveals that.

## The proposal route (`#/proposal`)

The customer-facing **content**: who it is for, what the work is, what it
excludes. No rendering here — this is what the PDF will consume.

### The scope of work generates itself

Each product in the rate card carries one customer-facing sentence
(`scope_line`) and a `scope_order` giving its place in the **work sequence** —
which is neither catalog order nor recipe order, since the polish recipe lists
the sealer before the mender. Systems carry `scope_intro` / `scope_outro` for
mobilisation, surface prep and cleanup, which belong to no product; hanging
them off one would make them vanish the day that product is suppressed.

The narrative is built from **the same resolved lines the price is**. Suppress
the mender on a new slab and the joint-repair sentence goes with it, because it
was never a separate list to keep in sync. Consumables never appear — brushes
and rags are not work a customer buys.

Sentences dedupe **by text**, which is what collapses a two-part product
(Mender A and B, flake thrown and recovered) into one step: two catalog lines,
one thing a crew does. Give two products the same sentence and they merge —
that is the mechanism, not a coincidence. A step survives while *any* of its
products does.

Across a multi-area bid item the steps are the **union**, not the intersection:
one lump sum has to describe everything under it, including an add-on only one
member carries.

### Generated draft, sticky edit

The textarea is pre-filled with the engine's draft and stays live with the
quote until someone types in it. From then on the edit is stored verbatim and
**stops tracking the recipe** — which is the honest behaviour for hand-written
language, but must not be silent, so the block says "Edited — no longer follows
the recipe", a top-level alert lists them, and *Reset to generated* returns to
the **current** draft rather than the one that was replaced.

### The three open items, decided

- **Proposal numbers** are `PREFIX-YYYYMMDD-XXXX` with a random suffix, not a
  sequence. A counter in local IndexedDB is exactly the thing that issues the
  same number twice when a job is quoted from a laptop and a tablet. Issued
  once on first visit and never reissued — a number identifies that document
  forever.
- **A bid item spanning sheets names every sheet**, not the first. It came from
  all of them, and picking one would quietly drop the rest. Sheet titles are a
  harness concept (`engine-core` has no notion of a page), so this is resolved
  here, and the title cache is warmed on entering the route so an unvisited
  sheet does not read "Page 22" beside "A-201".
- The **"INTERNAL — NOT FOR CUSTOMER"** treatment belongs to the cost-sheet
  generator in the next session; nothing here renders a document yet.

### Settings are global, the proposal is per project

Company details, the exclusions library, proposal-number prefix, default
validity and terms live in a `settings` object store (IndexedDB v2), shared
across every project: the company address does not change per job, and retyping
it per proposal is how one goes out with last year's phone number on it.
Editing an exclusion's text does not rewrite proposals already written.

Per project: client fields, number, date, validity, selected exclusions, notes,
and any scope overrides — all inputs, persisted beside crew/hours. A project
saved before proposals existed loads with an empty one; the number and date are
assigned on first visit rather than retroactively invented for a job already
quoted.

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
