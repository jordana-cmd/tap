// Interaction test suite: drives the REAL harness DOM in headless Chromium
// with synthetic pointer/keyboard events and asserts on resulting state
// (measurement list + window.__harness test hook). Any uncaught page error
// fails the case. Run: npm ci && node run-interactions.mjs
import puppeteer from 'puppeteer';
import assert from 'node:assert/strict';
import { startServer } from './server.mjs';

const server = await startServer();
const browser = await puppeteer.launch();
const URL = `http://127.0.0.1:${server.port}/?url=/test/walls.pdf&fpi=7.2`;
// fpi 7.2 → 10 pts = 1 ft. Fixture: wall run 450 pts = 45 LF at base y 400.

let failures = 0;

async function withPage(fn) {
  const page = await browser.newPage();
  const pageErrors = [];
  page.on('pageerror', e => pageErrors.push(String(e)));
  await page.goto(URL);
  await page.waitForFunction(
    () => window.__harness && document.querySelector('#vecInfo').textContent.includes('segs'),
    { timeout: 30_000 },
  );
  try {
    await fn(page, pageErrors);
    assert.deepEqual(pageErrors, [], 'uncaught page errors');
  } finally {
    await page.close();
  }
}

const state = page => page.evaluate(() => ({
  tool: window.__harness.tool(),
  meas: window.__harness.measurements().map(m => ({
    name: m.name, kind: m.kind, value: m.value, origin: m.origin,
    page: m.page, verts: m.geometry.length / 2,
  })),
  draftVerts: (d => (d ? d.verts.length : null))(window.__harness.draft()),
  chain: !!window.__harness.chainPreview(),
  status: document.querySelector('#status').textContent,
  rows: [...document.querySelectorAll('.measRow')].map(r => r.textContent),
}));

async function clientOf(page, x, y) {
  return page.evaluate((x, y) => {
    const r = document.querySelector('#plan').getBoundingClientRect();
    const s = window.__harness.s();
    return { cx: r.left + x * s, cy: r.top + y * s };
  }, x, y);
}

async function clickBase(page, x, y, opts = {}) {
  const { cx, cy } = await clientOf(page, x, y);
  await page.mouse.move(cx, cy); // hover first, like a real pointer
  await page.mouse.click(cx, cy, opts);
}

const setTool = (page, t) => page.click(`#tools [data-tool="${t}"]`);
const snapOff = page => page.evaluate(() => {
  const c = document.querySelector('#snapChk');
  if (c.checked) c.click();
});
const near = (a, b, tol) => Math.abs(a - b) <= tol;

async function run(name, fn) {
  try {
    await withPage(fn);
    console.log(`PASS  ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL  ${name}\n      ${String(e.message ?? e).split('\n')[0]}`);
  }
}

// Square 100×100 pts (10×10 ft at fpi 7.2) in empty sheet area.
const SQ = [[600, 100], [700, 100], [700, 200], [600, 200]];

await run('area: 4 clicks + click-on-first-vertex closes and commits', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await clickBase(page, ...SQ[0]); // the close gesture
  const st = await state(page);
  assert.equal(st.meas.length, 1, `expected 1 measurement, got ${st.meas.length}`);
  assert.equal(st.meas[0].kind, 'area');
  assert.equal(st.meas[0].origin, 'manual');
  assert.equal(st.meas[0].verts, 4);
  assert.ok(near(st.meas[0].value, 100, 3), `SF ${st.meas[0].value} !~ 100`);
  assert.equal(st.draftVerts, null, 'draft should be cleared');
  // Tool stays armed: next click starts a fresh draft.
  assert.equal(st.tool, 'area');
  await clickBase(page, 620, 120);
  assert.equal((await state(page)).draftVerts, 1, 'tool not armed for next measurement');
});

await run('area: 4 clicks + Enter commits', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const st = await state(page);
  assert.equal(st.meas.length, 1);
  assert.ok(near(st.meas[0].value, 100, 3), `SF ${st.meas[0].value} !~ 100`);
  assert.equal(st.tool, 'area');
});

await run('area: 3 clicks + double-click commits with no stray vertex', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ.slice(0, 3)) await clickBase(page, x, y);
  // Real double-click at the 4th corner: click, then click with count 2.
  const { cx, cy } = await clientOf(page, ...SQ[3]);
  await page.mouse.move(cx, cy);
  await page.mouse.click(cx, cy);
  await page.mouse.click(cx, cy, { clickCount: 2 });
  const st = await state(page);
  assert.equal(st.meas.length, 1);
  assert.equal(st.meas[0].verts, 4, `expected 4 vertices, got ${st.meas[0].verts} (stray vertex?)`);
  assert.ok(near(st.meas[0].value, 100, 3), `SF ${st.meas[0].value} !~ 100`);
});

await run('line: 3 clicks + Enter commits open polyline', async page => {
  await setTool(page, 'line');
  await snapOff(page);
  for (const [x, y] of [[600, 300], [700, 300], [700, 350]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const st = await state(page);
  assert.equal(st.meas.length, 1);
  assert.equal(st.meas[0].kind, 'linear');
  assert.equal(st.meas[0].verts, 3);
  assert.ok(near(st.meas[0].value, 15, 0.5), `LF ${st.meas[0].value} !~ 15 (open, not closed)`);
  assert.equal(st.tool, 'line');
  await clickBase(page, 620, 320);
  assert.equal((await state(page)).draftVerts, 1, 'tool not armed for next measurement');
});

await run('wall: click near run previews chain, Enter commits 45 LF', async page => {
  await setTool(page, 'wall');
  await clickBase(page, 325, 398); // near the 3-segment run at base y 400
  let st = await state(page);
  assert.ok(st.chain, 'no chain preview');
  await page.keyboard.press('Enter');
  st = await state(page);
  assert.equal(st.meas.length, 1);
  assert.ok(near(st.meas[0].value, 45, 0.2), `run LF ${st.meas[0].value} !~ 45`);
  assert.equal(st.meas[0].origin, 'snap');
  assert.equal(st.tool, 'wall');
  assert.equal(st.chain, false, 'preview should clear after commit');
  await clickBase(page, 325, 398);
  assert.ok((await state(page)).chain, 'tool not armed for next wall');
});

await run('Esc cancels draft; Backspace removes exactly one vertex', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ.slice(0, 3)) await clickBase(page, x, y);
  await page.keyboard.press('Backspace');
  assert.equal((await state(page)).draftVerts, 2, 'Backspace should remove one vertex');
  await page.keyboard.press('Escape');
  const st = await state(page);
  assert.equal(st.draftVerts, null, 'Esc should cancel the draft');
  assert.equal(st.meas.length, 0, 'nothing should be committed');
});

await run('detect: hatched room traps without hide-hatch, detects with it', async page => {
  await setTool(page, 'detect');
  const setHide = on => page.evaluate(v => {
    const c = document.querySelector('#hideHatch');
    if (c.checked !== v) c.click();
  }, on);
  await setHide(false);
  await clickBase(page, 675, 225); // center of the hatched second room
  let st = await state(page);
  assert.equal(st.meas.length, 0, 'trapped click must not commit a room');
  assert.match(st.status, /SEED_TRAPPED|SEED_ON_WALL/, st.status);
  await setHide(true);
  await clickBase(page, 675, 225);
  st = await state(page);
  assert.equal(st.meas.length, 1, 'hide-hatch should recover the room');
  assert.equal(st.meas[0].origin, 'floodfill');
  assert.ok(st.meas[0].value > 140 && st.meas[0].value < 235,
    `recovered SF ${st.meas[0].value} outside plausible band`);
});

await run('Enter still finishes after focusing a control (slider)', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.focus('#doorGap'); // leftover focus on a range input
  await page.keyboard.press('Enter');
  const st = await state(page);
  assert.equal(st.meas.length, 1, 'Enter swallowed by control-focus keydown guard');
});

// ---- state-model & rendering-class cases ----

const countPixels = (page, pred) => page.evaluate(predSrc => {
  const c = document.querySelector('#plan');
  const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
  const pred = new Function('r', 'g', 'b', `return ${predSrc};`);
  let n = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (pred(d[i], d[i + 1], d[i + 2])) n++;
  }
  return n;
}, pred);
// Snap-indicator green #15803d ≈ (21,128,61); committed blue ≈ (30,58,138).
const GREEN = 'Math.abs(r-21)<30 && Math.abs(g-128)<40 && Math.abs(b-61)<30';
const BLUE = 'Math.abs(r-30)<30 && Math.abs(g-58)<40 && Math.abs(b-138)<40';

const gotoPage = async (page, dir) => {
  await page.click(dir > 0 ? '#next' : '#prev');
  await page.waitForFunction(
    n => document.querySelector('#pageLabel').textContent.startsWith(`${n} `),
    {}, dir > 0 ? 2 : 1,
  );
  await new Promise(r => setTimeout(r, 400)); // extraction + geometry swap
};

await run('snap indicators never accumulate (mousemove sweep + zoom churn)', async page => {
  await setTool(page, 'line'); // snap stays on: indicators active
  // Sweep along the wall run (snappable) — many hover positions.
  for (let i = 0; i < 25; i++) {
    const { cx, cy } = await clientOf(page, 110 + i * 17, 399);
    await page.mouse.move(cx, cy);
  }
  // Zoom churn mid-sweep with NO settling waits — overlapping renderPage
  // calls are exactly the unclean-compositing window; interleave moves.
  await page.click('#zoomIn');
  await page.click('#zoomIn');
  for (let i = 0; i < 10; i++) {
    const { cx, cy } = await clientOf(page, 110 + i * 17, 399);
    await page.mouse.move(cx, cy);
  }
  await page.click('#zoomOut');
  await new Promise(r => setTimeout(r, 700));
  for (let i = 0; i < 25; i++) {
    const { cx, cy } = await clientOf(page, 110 + i * 17, 399);
    await page.mouse.move(cx, cy);
  }
  // Reset the view so the park point is inside the viewport (a park move
  // that misses the canvas would leave the LIVE indicator painted and
  // read as a false accumulation), then park far from geometry.
  await page.click('#zoomFit');
  await new Promise(r => setTimeout(r, 700));
  const { cx, cy } = await clientOf(page, 700, 550);
  await page.mouse.move(cx, cy);
  const green = await countPixels(page, GREEN);
  assert.ok(green < 40, `${green} green indicator pixels remain after sweep`);
});

await run('measurements are page-scoped (render + list + restore)', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  let st = await state(page);
  assert.equal(st.meas.length, 1);
  assert.equal(st.meas[0].page, 1, 'measurement must record its page');
  await gotoPage(page, +1);
  // Canvas must not render page 1's measurement on page 2.
  const blue = await countPixels(page, BLUE);
  assert.ok(blue < 40, `${blue} measurement pixels rendered on page 2`);
  // List defaults to current page: zero rows on p2.
  st = await state(page);
  assert.equal(st.rows.length, 0, `page-2 list should be empty, got ${st.rows.length} rows`);
  // All-pages view shows it with its badge.
  await page.evaluate(() => { const c = document.querySelector('#allPages'); if (!c.checked) c.click(); });
  st = await state(page);
  assert.equal(st.rows.length, 1, 'all-pages view shows the row');
  assert.match(st.rows[0], /p1/, 'row carries its page badge');
  await page.evaluate(() => { const c = document.querySelector('#allPages'); if (c.checked) c.click(); });
  // Back to page 1: exact restoration.
  await gotoPage(page, -1);
  st = await state(page);
  assert.equal(st.rows.length, 1, 'page-1 row restored');
  const blueBack = await countPixels(page, BLUE);
  assert.ok(blueBack >= 40, 'measurement renders again on its own page');
});

await run('page switch cancels an in-progress draft', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ.slice(0, 3)) await clickBase(page, x, y);
  await gotoPage(page, +1);
  const st = await state(page);
  assert.equal(st.draftVerts, null, 'draft must cancel on page switch');
  assert.equal(st.meas.length, 0, 'nothing committed by the switch');
});

await run('scale is per page (guard + independent values)', async page => {
  // Deep-link fpi seeds page 1 only. Page 2 must guard.
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter'); // p1 area at fpi 7.2 → 100 SF
  await gotoPage(page, +1);
  await setTool(page, 'detect');
  await clickBase(page, 480, 300); // inside page-2 room
  let st = await state(page);
  assert.match(st.status, /[Ss]et a scale/, `page 2 should demand a scale: ${st.status}`);
  // Give page 2 its own scale (engineer 1"=20' → fpi 20) via preset.
  await page.select('#preset', '20');
  await setTool(page, 'area');
  for (const [x, y] of [[600, 100], [700, 100], [700, 200], [600, 200]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  st = await state(page);
  const p2 = st.meas.find(m => m.page === 2);
  assert.ok(p2, 'page-2 measurement committed');
  // ±3%: at fpi 20 each client-px of pointer rounding is ~0.76 base pts.
  assert.ok(near(p2.value, 771.6, 25), `p2 SF ${p2.value} should use fpi 20 (≈771.6)`);
  const p1 = st.meas.find(m => m.page === 1);
  assert.ok(near(p1.value, 100, 3), `p1 SF ${p1.value} must keep page-1 scale`);
});

await run('file load clears document state', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1);
  // Reload the same fixture through the deep-link (fresh document).
  await page.evaluate(async () => {
    const resp = await fetch('/test/walls.pdf');
    // eslint-disable-next-line no-undef
    await window.__harness.loadPdf(await resp.arrayBuffer(), 'reload.pdf');
  });
  await new Promise(r => setTimeout(r, 600));
  const st = await state(page);
  assert.equal(st.meas.length, 0, 'measurements must not survive a new document');
});

await run('count tool: place N markers, Enter commits N EA, armed after', async page => {
  await setTool(page, 'count');
  // Count is un-gated by scale and does not snap — place 4 markers.
  const marks = [[600, 100], [640, 100], [680, 100], [720, 100]];
  for (const [x, y] of marks) await clickBase(page, x, y);
  assert.equal((await page.evaluate(() => window.__harness.countDraft().length)), 4,
    'four markers in the in-progress group');
  await page.keyboard.press('Enter');
  let st = await state(page);
  const c = st.meas.find(m => m.kind === 'count');
  assert.ok(c, 'a count measurement committed');
  assert.equal(c.value, 4, 'quantity equals marker count');
  assert.equal(c.verts, 4, 'geometry has one point per marker');
  assert.equal(c.origin, 'manual');
  assert.equal(st.tool, 'count', 'tool stays armed');
  // Fresh group after commit.
  await clickBase(page, 600, 150);
  assert.equal((await page.evaluate(() => window.__harness.countDraft().length)), 1,
    'a new group starts armed');
  // Backspace drops one; Esc clears the rest with nothing new committed.
  await page.keyboard.press('Backspace');
  assert.equal((await page.evaluate(() => window.__harness.countDraft().length)), 0);
  await clickBase(page, 600, 150);
  await clickBase(page, 640, 150);
  await page.keyboard.press('Escape');
  st = await state(page);
  assert.equal((await page.evaluate(() => window.__harness.countDraft().length)), 0, 'Esc clears the group');
  assert.equal(st.meas.filter(m => m.kind === 'count').length, 1, 'no extra count committed');
});

await run('every wasm import in index.html exists in the module (class 4)', async page => {
  const result = await page.evaluate(async () => {
    const html = await (await fetch('/index.html')).text();
    const m = html.match(/import init,\s*\{([^}]+)\}\s*from '\.\/pkg\/engine_web\.js'/);
    const imports = m[1].split(',').map(s => s.trim()).filter(Boolean);
    const mod = await import('/pkg/engine_web.js');
    const missing = imports.filter(name => !(name in mod));
    return { imports, missing };
  });
  assert.ok(result.imports.length >= 5, 'import list parsed');
  assert.deepEqual(result.missing, [], `imports missing from wasm module: ${result.missing}`);
});

await run('all-tools smoke: zero page errors across every tool', async page => {
  await setTool(page, 'line');
  await clickBase(page, 620, 320);
  await clickBase(page, 680, 320);
  await page.keyboard.press('Enter');
  await setTool(page, 'area');
  for (const [x, y] of [[600, 250], [660, 250], [660, 290]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'wall');
  await clickBase(page, 325, 398);
  await page.keyboard.press('Enter');
  await setTool(page, 'detect');
  await clickBase(page, 200, 350); // inside room 1
  await page.click('#calibBtn');
  await clickBase(page, 100, 400);
  await clickBase(page, 250, 400);
  await page.keyboard.press('Escape');
  // pageErrors asserted by withPage automatically.
});

await browser.close();
server.close();
if (failures) {
  console.log(`\n${failures} case(s) FAILED`);
  process.exit(1);
}
console.log('\nall interaction cases passed');
