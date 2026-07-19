# Harness interaction test suite

Drives the real `harness/index.html` DOM in headless Chromium (Puppeteer)
with synthetic pointer and keyboard events, and asserts on resulting state:
the measurement-list DOM and the read-only `window.__harness` hook. Any
uncaught page exception fails the case.

The fixture is a **generated synthetic vector PDF** (`make-fixture.mjs`) —
known stroked geometry, zero confidentiality concerns — served by a
dependency-free local server (`server.mjs`), so the suite exercises the
real pdf.js extraction → `PageGeometry` → tool pipeline end to end.

Run:

```
cd harness/tests
npm ci                       # first time: downloads Chromium
node run-interactions.mjs
```

**Process rule: no interaction behavior is claimed to work without a green
run of this suite.** (Two pointer-event bugs shipped before it existed;
one was an unimported wasm function that only a real click could reveal.)
