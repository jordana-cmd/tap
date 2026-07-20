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

const waitReady = page => page.waitForFunction(
  () => window.__harness && document.querySelector('#vecInfo').textContent.includes('segs'),
  { timeout: 30_000 },
);

// Each case runs in its OWN browser context → isolated IndexedDB, so
// persisted projects never leak between cases.
const newContext = () =>
  (browser.createBrowserContext?.() ?? browser.createIncognitoBrowserContext());

async function withPage(fn) {
  const context = await newContext();
  const page = await context.newPage();
  const pageErrors = [];
  page.on('pageerror', e => pageErrors.push(String(e)));
  await page.goto(URL);
  await waitReady(page);
  try {
    await fn(page, pageErrors);
    assert.deepEqual(pageErrors, [], 'uncaught page errors');
  } finally {
    await context.close();
  }
}

const state = page => page.evaluate(() => ({
  tool: window.__harness.tool(),
  meas: window.__harness.measurements().map(m => ({
    name: m.name, kind: m.kind, value: m.value, origin: m.origin,
    page: m.page, color: m.color, verts: m.geometry.length / 2,
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
// Detection-tuning controls live behind a collapsed <details id="advanced">;
// open it before driving them with Puppeteer (which needs them visible).
const openAdvanced = page => page.evaluate(() => { document.querySelector('#advanced').open = true; });
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
  await openAdvanced(page); // #doorGap lives under Advanced; open so focus lands
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

await run('loading a different document is a clean slate', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1);
  await page.evaluate(() => window.__harness.flushSave()); // ensure it's persisted
  // A genuinely different document (distinct bytes) → separate project,
  // so the first document's measurements do not carry over.
  await page.evaluate(async () => {
    const bytes = await (await fetch('/test/other.pdf')).arrayBuffer();
    await window.__harness.loadPdf(bytes, 'other.pdf');
  });
  await new Promise(r => setTimeout(r, 400));
  assert.equal((await state(page)).meas.length, 0, 'different document starts clean');
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

await run('measurement rename persists to the state model', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  // Click the name to open the inline editor, replace the text, Enter.
  await page.click('.measRow .measName');
  await page.click('.measRow .renameInput', { clickCount: 3 });
  await page.type('.measRow .renameInput', 'Dining — flooring');
  await page.keyboard.press('Enter');
  const st = await state(page);
  assert.equal(st.meas[0].name, 'Dining — flooring', 'rename persisted to the measurement');
  // And the row shows it (no stray canvas keydown side effects).
  assert.match(st.rows[0], /Dining/, 'renamed row rendered');
  assert.equal(st.meas.length, 1, 'no extra measurement from the rename keystrokes');
});

await run('CSV export: rows with §A2 provenance for area + line + count', async page => {
  // One area, one line, one count on page 1 (deep-linked fpi 7.2 = param).
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'line');
  for (const [x, y] of [[600, 300], [700, 300], [700, 350]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'count');
  for (const [x, y] of [[600, 400], [640, 400], [680, 400]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');

  const csv = await page.evaluate(() => window.__harness.buildCsv());
  const lines = csv.trim().split('\n');
  assert.equal(lines[0],
    'scope,page,name,kind,quantity,unit,origin,page_scale_fpi,scale_source,'
    + 'gross_quantity,deduct_quantity,net_quantity,parent', 'header');
  assert.equal(lines.length, 4, 'header + 3 data rows');
  const cols = lines.slice(1).map(l => l.split(','));
  // Column 0 is scope (all Base Bid); kind is now column 3.
  const area = cols.find(c => c[3] === 'area');
  const line = cols.find(c => c[3] === 'linear');
  const count = cols.find(c => c[3] === 'count');
  assert.ok(area && line && count, 'one row per kind');
  assert.ok(cols.every(c => c[0] === 'Base Bid'), 'every row tagged Base Bid');
  assert.equal(area[5], 'SF'); assert.equal(area[7], '7.2000'); assert.equal(area[8], 'param');
  assert.equal(line[5], 'LF'); assert.equal(line[6], 'manual');
  assert.equal(count[4], '3.00'); assert.equal(count[5], 'EA'); assert.equal(count[7], '7.2000');
});

await run('CSV quotes free-text names containing commas', async page => {
  await setTool(page, 'count');
  await clickBase(page, 620, 420);
  await page.keyboard.press('Enter');
  await page.click('.measRow .measName');
  await page.click('.measRow .renameInput', { clickCount: 3 });
  await page.type('.measRow .renameInput', 'Doors, exterior');
  await page.keyboard.press('Enter');
  const csv = await page.evaluate(() => window.__harness.buildCsv());
  assert.match(csv, /"Doors, exterior"/, 'comma-bearing name is CSV-quoted');
});

await run('min_width override sticks across pages; reset re-derives', async page => {
  await openAdvanced(page); // the min_width slider + reset live under Advanced now
  assert.ok(await page.$eval('#minWidth', el => !el.disabled),
    'min_width slider enabled on a vector page');
  const read = () => page.evaluate(() => ({
    val: document.querySelector('#minWidth').value,
    resetHidden: document.querySelector('#minWidthReset').hidden,
  }));
  // Move the slider → session override + reset affordance shown.
  await page.$eval('#minWidth', el => {
    el.value = '0.77'; el.dispatchEvent(new Event('input', { bubbles: true }));
  });
  let st = await read();
  assert.equal(st.val, '0.77', 'override applied');
  assert.equal(st.resetHidden, false, 'reset control shown');
  // Switch pages → override persists (not the new page's derived default).
  await gotoPage(page, +1);
  st = await read();
  assert.equal(st.val, '0.77', 'override persists across the page switch');
  assert.equal(st.resetHidden, false);
  // Reset → back to this page's derived default, control hidden.
  await page.click('#minWidthReset');
  st = await read();
  assert.equal(st.resetHidden, true, 'reset hides the control');
  assert.notEqual(st.val, '0.77', 'value reverted to the per-page derived default');
});

await run('persistence: measure → reload → restored (page/name/color/scale)', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'count');
  for (const [x, y] of [[600, 200], [640, 200], [680, 200]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  // Rename + recolor the area so the restore checks those fields too.
  await page.click('.measRow .measName');
  await page.click('.measRow .renameInput', { clickCount: 3 });
  await page.type('.measRow .renameInput', 'Dining');
  await page.keyboard.press('Enter');
  await page.$eval('.measRow .measColor', el => {
    el.value = '#ff8800'; el.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.evaluate(() => window.__harness.flushSave());

  await page.reload();
  await waitReady(page);
  const st = await state(page);
  const area = st.meas.find(m => m.kind === 'area');
  const count = st.meas.find(m => m.kind === 'count');
  assert.ok(area && count, 'both measurements restored');
  assert.equal(area.name, 'Dining', 'renamed name restored');
  assert.equal(area.color, '#ff8800', 'color restored');
  assert.equal(area.page, 1, 'page restored');
  assert.ok(near(area.value, 100, 3), `area value re-derived from scale (${area.value})`);
  assert.equal(count.value, 3, 'count quantity restored');
  assert.match(st.status, /restored/, 'load message notes the restore');
});

await run('persistence: same bytes under a different filename → same project', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  const sha = await page.evaluate(() => window.__harness.currentSha());
  // Reload the identical bytes under a new name → content-addressed match.
  await page.evaluate(async () => {
    const bytes = await (await fetch('/test/walls.pdf')).arrayBuffer();
    await window.__harness.loadPdf(bytes, 'renamed-copy.pdf');
  });
  await new Promise(r => setTimeout(r, 400));
  const st = await state(page);
  assert.equal(await page.evaluate(() => window.__harness.currentSha()), sha, 'same content hash');
  assert.equal(st.meas.length, 1, 'measurement restored under the new filename');
});

await run('persistence: a different file is a separate project', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  const sha1 = await page.evaluate(() => window.__harness.currentSha());
  await page.evaluate(async () => {
    const bytes = await (await fetch('/test/other.pdf')).arrayBuffer();
    await window.__harness.loadPdf(bytes, 'other.pdf');
  });
  await new Promise(r => setTimeout(r, 400));
  const st = await state(page);
  assert.notEqual(await page.evaluate(() => window.__harness.currentSha()), sha1, 'distinct hash');
  assert.equal(st.meas.length, 0, 'separate project starts empty');
});

await run('persistence: delete → gone after reload', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  await page.evaluate(async () => {
    await window.__harness.deleteProject(window.__harness.currentSha());
  });
  await page.reload();
  await waitReady(page);
  assert.equal((await state(page)).meas.length, 0, 'deleted project does not restore');
});

await run('project panel: lists, reopens, and deletes via UI', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  // Open the panel → the project is listed.
  await page.click('#projectsBtn');
  await page.waitForSelector('.projRow', { timeout: 5000 });
  let rows = await page.$$eval('.projRow', els => els.length);
  assert.equal(rows, 1, 'one saved project listed');
  // Reopen it (from stored bytes) → measurement restored.
  await page.click('.projRow button'); // first button = "open"
  await waitReady(page);
  assert.equal((await state(page)).meas.length, 1, 'reopened from stored bytes');
  // Delete via the UI (accept the confirm) → gone.
  page.on('dialog', d => d.accept());
  await page.click('#projectsBtn');
  await page.waitForSelector('.projRow', { timeout: 5000 });
  await page.$$eval('.projRow button', bs => bs.find(b => b.textContent === '✕').click());
  await new Promise(r => setTimeout(r, 300));
  rows = await page.$$eval('.projRow', els => els.length);
  assert.equal(rows, 0, 'project removed from the list after delete');
});

await run('JSON round-trip: export → clear → import → identical state', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'count');
  for (const [x, y] of [[600, 200], [640, 200]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.click('.measRow .measName');
  await page.click('.measRow .renameInput', { clickCount: 3 });
  await page.type('.measRow .renameInput', 'Kitchen');
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  const json = await page.evaluate(() => window.__harness.exportJson());
  const before = (await state(page)).meas;

  // Clear: delete the project + reload → empty state.
  await page.evaluate(() => window.__harness.deleteProject(window.__harness.currentSha()));
  await page.reload();
  await waitReady(page);
  assert.equal((await state(page)).meas.length, 0, 'cleared before import');

  // Import the backup → identical measurements restored.
  await page.evaluate(j => window.__harness.importJson(j), json);
  const after = (await state(page)).meas;
  assert.equal(after.length, before.length, 'same measurement count');
  const byName = m => `${m.name}|${m.kind}|${m.page}|${m.color}|${m.value.toFixed(2)}`;
  assert.deepEqual(after.map(byName).sort(), before.map(byName).sort(),
    'names, kinds, pages, colors, values all round-trip');
});

await run('storage write failure blocks edits until acknowledged', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1);
  // Force a write failure → modal shown (covering the toolbar), edits blocked.
  await page.evaluate(() => window.__harness.simulateStorageError());
  assert.equal(await page.evaluate(() => window.__harness.storageBlocked()), true);
  assert.equal(await page.$eval('#storageModal', el => el.hidden), false, 'modal shown');
  // A canvas edit attempt while blocked does nothing (guard + overlay). Use a
  // canvas point that maps onto the modal BACKDROP (a corner), not its centred
  // dismiss/export buttons — otherwise the attempt would close the modal.
  await clickBase(page, 100, 600);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1, 'no new measurement while blocked');
  // Dismiss (the modal button is on top) → unblocked, edits resume.
  await page.click('#storageDismiss');
  assert.equal(await page.evaluate(() => window.__harness.storageBlocked()), false);
  for (const [x, y] of [[600, 300], [700, 300], [700, 400], [600, 400]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 2, 'edits resume after acknowledge');
});

await run('assembly library seeds the engine catalog (synthetic examples + MCFC)', async page => {
  const names = await page.evaluate(async () =>
    (await window.__harness.listAssemblies()).map(a => a.name));
  // 2 synthetic examples + 7 MCFC systems + 6 add-ons (consumables are auto).
  assert.equal(names.length, 15, `seeded catalog size: ${names.length}`);
  assert.ok(!names.includes('Job Consumables'), 'consumables are not a selectable assembly');
  for (const n of ['Commercial Flooring', 'Epoxy Coating', 'Epoxy + High Wear Urethane']) {
    assert.ok(names.includes(n), `library seeded ${n}`);
  }
});

await run('author: invalid formula blocked, valid formula accepted', async page => {
  await page.click('#assembliesBtn');
  await page.click('#newAssembly');
  await page.waitForSelector('#assemblyEditor:not([hidden])', { timeout: 5000 });
  await page.type('#asmName', 'Test Coating');
  // The editor starts with one part row; give it a name and a BAD formula.
  await page.type('.partRow .ptName', 'Bad');
  await page.type('.partRow .formula', 'area_sf / (');
  // Live validation shows an error; Save is refused (nothing persisted).
  let chk = await page.$eval('.partRow .fcheck', el => el.className + '|' + el.textContent);
  assert.match(chk, /bad/, `invalid formula flagged: ${chk}`);
  await page.click('#saveAssembly');
  assert.equal(await page.$eval('#assemblyEditor', el => el.hidden), false, 'editor stays open on invalid');
  assert.match(await page.$eval('#editorError', el => el.textContent), /Fix/, 'save blocked with message');
  let count = await page.evaluate(async () => (await window.__harness.listAssemblies()).length);
  assert.equal(count, 15, 'nothing persisted while invalid (seeded catalog unchanged)');

  // Fix the formula → validates ✓ → Save persists it.
  await page.click('.partRow .formula', { clickCount: 3 });
  await page.type('.partRow .formula', 'area_sf / 250');
  chk = await page.$eval('.partRow .fcheck', el => el.className + '|' + el.textContent);
  assert.match(chk, /ok/, `valid formula accepted: ${chk}`);
  await page.click('#saveAssembly');
  await page.waitForSelector('#assemblyEditor[hidden]', { timeout: 5000 });
  const names = await page.evaluate(async () =>
    (await window.__harness.listAssemblies()).map(a => a.name));
  assert.ok(names.includes('Test Coating'), 'valid assembly persisted to the library');
});

await run('author: unused parameter warns (does not block); reference clears it', async page => {
  await page.click('#assembliesBtn');
  await page.click('#newAssembly');
  await page.waitForSelector('#assemblyEditor:not([hidden])', { timeout: 5000 });
  await page.type('#asmName', 'Warn Test');
  // A valid part formula that does NOT reference the parameter we add.
  await page.type('.partRow .ptName', 'Mat');
  await page.type('.partRow .formula', 'area_sf / 250');
  // Add a parameter `foo` that no formula references → warning.
  await page.click('#addParam');
  await page.type('.paramRow .pName', 'foo');
  await page.type('.paramRow .pDefault', '1');
  let warn = await page.$eval('.paramRow .pWarn', el => el.textContent);
  assert.match(warn, /unused/, `unused parameter flagged: "${warn}"`);
  // Warn, don't block: Save is still allowed and persists.
  await page.click('#saveAssembly');
  await page.waitForSelector('#assemblyEditor[hidden]', { timeout: 5000 });
  assert.ok(
    (await page.evaluate(async () => (await window.__harness.listAssemblies()).map(a => a.name)))
      .includes('Warn Test'),
    'assembly with an unused parameter still saves',
  );
  // Re-open, reference `foo` in the formula → warning clears.
  await page.evaluate(async () => {
    const a = (await window.__harness.listAssemblies()).find(x => x.name === 'Warn Test');
    window.__editId = a.id;
  });
  await page.evaluate(() => document.querySelector('#assemblyPanel').hidden = true);
  await page.click('#assembliesBtn');
  await page.waitForFunction(() => document.querySelectorAll('.asmRow').length > 0);
  // Edit the "Warn Test" row.
  await page.evaluate(() => {
    const rows = [...document.querySelectorAll('.asmRow')];
    const row = rows.find(r => r.querySelector('.asmName').textContent === 'Warn Test');
    row.querySelector('button').click(); // "edit"
  });
  await page.waitForSelector('#assemblyEditor:not([hidden])', { timeout: 5000 });
  await page.click('.partRow .formula', { clickCount: 3 });
  await page.type('.partRow .formula', 'area_sf / foo');
  warn = await page.$eval('.paramRow .pWarn', el => el.textContent);
  assert.equal(warn, '', `warning clears once referenced (was "${warn}")`);
});

// A flooring room whose derived drivers are exactly area_sf=2475, perimeter=210
// (at fpi 7.2, 1 pt = 0.1 ft): W·H = 247500 pt², 2(W+H) = 2100 pt. Injected
// through the real persistence path (importJson → fromStored) so it also
// exercises the new assemblyId/overrides round-trip.
const flooringRoom = (overrides = null) => {
  const W = (1050 + 150 * Math.sqrt(5)) / 2, H = 1050 - W;
  return {
    version: 1, sha: 'x', name: 'flooring-test',
    pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
    measurements: [{
      id: 1, page: 1, kind: 'area', label: 'Room 1', origin: 'manual', color: '#1e3a8a',
      geometry: [0, 0, W, 0, W, H, 0, H],
      assemblyId: 'commercial_flooring', overrides,
    }],
  };
};

await run('apply: attaching commercial flooring derives the engine BOM', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), flooringRoom());
  const bom = await page.evaluate(() => window.__harness.applyBom(1));
  assert.ok(bom.ok, `BOM computed: ${JSON.stringify(bom)}`);
  const q = name => bom.bom.line_items.find(l => l.part_name === name).final_quantity;
  assert.equal(q('Flooring boxes'), 137, 'boxes = ceil(2475/20 · 1.1 waste)');
  assert.equal(q('Adhesive'), 17, 'adhesive = ceil(2475/150)');
  assert.ok(Math.abs(q('Cove base') - 220.5) < 1e-6, `cove = 210 · 1.05, got ${q('Cove base')}`);
  // The expanded DOM shows the same numbers (display wired to the same engine).
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomPanel .bomLine');
  const lines = await page.$$eval('.bomPanel .bomLine', els => els.map(e => ({
    qty: e.querySelector('.bomQty').textContent, name: e.children[1].textContent,
  })));
  assert.equal(lines.find(l => l.name === 'Flooring boxes').qty, '137 BOX', 'boxes line in the DOM');
  // Clicking a line reveals its source formula (the provenance affordance).
  await page.click('.bomPanel .bomLine');
  assert.ok(
    await page.$eval('.bomPanel .bomLine .bomWhy', el => !el.hidden && el.textContent.includes('area_sf')),
    'clicking a BOM line reveals its formula',
  );
});

await run('override: per-measurement waste changes the BOM and persists', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), flooringRoom());
  const matBefore = await page.evaluate(() =>
    window.__harness.applyBom(1).bom.line_items.find(l => l.part_name === 'Flooring material').final_quantity);
  assert.ok(Math.abs(matBefore - 2722.5) < 1e-6, `material before = 2475·1.10, got ${matBefore}`);
  // Drive the DOM overrides editor: raise the material waste 10% → 20%.
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomOv .ovRow');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.bomOv .ovRow')]
      .find(r => r.querySelector('label').textContent === 'Flooring material waste %');
    const inp = row.querySelector('input');
    inp.value = '20';
    inp.dispatchEvent(new Event('change'));
  });
  const matAfter = await page.evaluate(() =>
    window.__harness.applyBom(1).bom.line_items.find(l => l.part_name === 'Flooring material').final_quantity);
  assert.ok(Math.abs(matAfter - 2970) < 1e-6, `material after = 2475·1.20, got ${matAfter}`);
  assert.deepEqual(
    await page.evaluate(() => window.__harness.measOverrides(1)),
    { commercial_flooring: { 'waste:material': 20 } }, 'override stored per-assembly on the measurement');
});

await run('reload: assembly assignment + overrides restored, BOM re-derived', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)),
    flooringRoom({ 'waste:material': 20 }));
  await page.evaluate(() => window.__harness.flushSave());
  // Reload the SAME context (same IndexedDB) and same PDF → project restores.
  await page.goto(URL);
  await waitReady(page);
  const restored = await page.evaluate(() => {
    const m = window.__harness.measurements().find(x => x.id === 1) ?? window.__harness.measurements()[0];
    return { assemblyId: m.assemblyId, overrides: window.__harness.measOverrides(m.id), bomStored: 'bom' in m };
  });
  assert.equal(restored.assemblyId, 'commercial_flooring', 'assembly id restored');
  // Imported FLAT (phase-1 shape) → migrated to nested-by-assembly on load.
  assert.deepEqual(restored.overrides, { commercial_flooring: { 'waste:material': 20 } }, 'overrides restored (nested)');
  assert.equal(restored.bomStored, false, 'no BOM persisted on the measurement (invariant 5)');
  // The BOM is re-derived from geometry+scale+assembly+overrides, not stored.
  const mat = await page.evaluate(() => {
    const id = (window.__harness.measurements()[0]).id;
    return window.__harness.applyBom(id).bom.line_items.find(l => l.part_name === 'Flooring material').final_quantity;
  });
  assert.ok(Math.abs(mat - 2970) < 1e-6, `re-derived material honors the restored override, got ${mat}`);
});

await run('material list: rolls up line items across measurements by (part, unit)', async page => {
  // Two flooring rooms (same 2475 SF geometry) on the same scaled page.
  const proj = flooringRoom();
  const W = (1050 + 150 * Math.sqrt(5)) / 2, H = 1050 - W;
  proj.measurements.push({
    id: 2, page: 1, kind: 'area', label: 'Room 2', origin: 'manual', color: '#1e3a8a',
    geometry: [0, 0, W, 0, W, H, 0, H], assemblyId: 'commercial_flooring', overrides: null,
  });
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), proj);
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  const lines = out.csv.trim().split('\n');
  assert.equal(lines[0], 'scope,category,condition,part,unit,total_quantity,unit_cost,extended_cost,measurements', 'header');
  // No condition on these rooms → "(unassigned)" condition column.
  // Adhesive: ceil(2475/150)=17 GAL per room, summed across both rooms = 34.
  const adhesive = lines.filter(l => l.includes(',Adhesive,'));
  assert.equal(adhesive.length, 1, 'exactly one Adhesive/GAL row (rolled up, not duplicated)');
  assert.equal(adhesive[0], 'Base Bid,material,(unassigned),Adhesive,GAL,34,0.00,0.00,Room 1; Room 2', `adhesive rollup: ${adhesive[0]}`);
  // Boxes: 137 each → 274. Rows are sorted by condition then part.
  assert.ok(lines.includes('Base Bid,material,(unassigned),Flooring boxes,BOX,274,0.00,0.00,Room 1; Room 2'), 'boxes summed to 274');
  assert.equal(out.skipped.length, 0, 'both rooms quantifiable');
});

await run('material list: skips measurements whose BOM cannot be quantified', async page => {
  // A flooring room on an UNSCALED page (0) can't derive area → skipped, not
  // silently counted as zero, and its name surfaces to the caller.
  await page.evaluate(() => window.__harness.importJson(JSON.stringify({
    version: 1, sha: 'x', name: 'no-scale',
    pageScales: [], // page 0 falls back to synthetic; but we put the room on page 5 (no scale)
    measurements: [{
      id: 1, page: 5, kind: 'area', label: 'Unscaled room', origin: 'manual', color: '#1e3a8a',
      geometry: [0, 0, 100, 0, 100, 80, 0, 80], assemblyId: 'commercial_flooring', overrides: null,
    }],
  })));
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  assert.equal(out.lineCount, 0, 'no material lines without a scale');
  assert.deepEqual(out.skipped, ['Unscaled room'], 'the unquantifiable room is surfaced, not dropped silently');
});

// A project with one or more measurements on the scaled page 1 (fpi 7.2 →
// 1 pt = 0.1 ft, so area_sf = pts² × 0.01). Injected through the real
// persistence path so parentId/includePerimeter round-trip too.
const deductProject = measurements => ({
  version: 1, sha: 'x', name: 'deduct-test',
  pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
  measurements,
});
const rectFlat = (x, y, w, h) => [x, y, x + w, y, x + w, y + h, x, y + h];
const area = (id, x, y, w, h) => ({
  id, page: 1, kind: 'area', label: `Room ${id}`, origin: 'manual', color: '#1e3a8a',
  geometry: rectFlat(x, y, w, h),
});

await run('deduct: net area drops by exactly the deduct area', async page => {
  // 400×300 pt room = 1200 SF at fpi 7.2.
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)),
    deductProject([area(1, 0, 0, 400, 300)]));
  assert.ok(Math.abs((await page.evaluate(() => window.__harness.grossArea(1))) - 1200) < 1e-6, 'gross 1200');
  // 100×100 pt column inside = 100 SF.
  const res = await page.evaluate(() => window.__harness.addDeduct([50, 50, 150, 50, 150, 150, 50, 150]));
  assert.equal(res.status, 'ok', 'attached');
  assert.equal(res.parentId, 1, 'attached to the containing room');
  assert.ok(Math.abs((await page.evaluate(() => window.__harness.deductArea(1))) - 100) < 1e-6, 'deduct 100');
  assert.ok(Math.abs((await page.evaluate(() => window.__harness.netArea(1))) - 1100) < 1e-6,
    'net = gross − deduct = 1100');
});

await run('deduct: two deducts under one parent sum', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)),
    deductProject([area(1, 0, 0, 400, 300)]));
  await page.evaluate(() => window.__harness.addDeduct([50, 50, 150, 50, 150, 150, 50, 150])); // 100 SF
  await page.evaluate(() => window.__harness.addDeduct([200, 50, 260, 50, 260, 150, 200, 150])); // 60 SF
  assert.ok(Math.abs((await page.evaluate(() => window.__harness.deductArea(1))) - 160) < 1e-6, 'deducts sum to 160');
  assert.ok(Math.abs((await page.evaluate(() => window.__harness.netArea(1))) - 1040) < 1e-6, 'net 1040');
  assert.equal((await page.evaluate(() => window.__harness.deductIdsOf(1))).length, 2, 'two children');
});

await run('deduct: concave L-room — attaches in the solid leg, rejects in the notch', async page => {
  // 400×400 square minus the top-right 200×200 quadrant.
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), deductProject([{
    id: 1, page: 1, kind: 'area', label: 'L Room', origin: 'manual', color: '#1e3a8a',
    geometry: [0, 0, 400, 0, 400, 200, 200, 200, 200, 400, 0, 400],
  }]));
  const solid = await page.evaluate(() => window.__harness.addDeduct([50, 50, 150, 50, 150, 150, 50, 150]));
  assert.equal(solid.status, 'ok', 'column in the solid leg attaches');
  assert.equal(solid.parentId, 1);
  const notch = await page.evaluate(() => window.__harness.addDeduct([250, 250, 350, 250, 350, 350, 250, 350]));
  assert.equal(notch.status, 'rejected', 'column in the removed notch is rejected');
});

await run('deduct: outside every area is rejected, nothing committed', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)),
    deductProject([area(1, 0, 0, 400, 300)]));
  const before = await page.evaluate(() => window.__harness.measurements().length);
  const res = await page.evaluate(() => window.__harness.addDeduct([500, 500, 560, 500, 560, 560, 500, 560]));
  assert.equal(res.status, 'rejected', 'not inside any area');
  assert.equal(await page.evaluate(() => window.__harness.measurements().length), before, 'no measurement added');
});

await run('deduct: deleting the parent cascades to its deducts (confirmed)', async page => {
  page.on('dialog', d => d.accept()); // accept the cascade confirmation
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), deductProject([
    area(1, 0, 0, 400, 300),
    { id: 2, page: 1, kind: 'deduct', label: 'Deduct 1', origin: 'manual', color: '#1e3a8a',
      geometry: rectFlat(50, 50, 100, 100), parentId: 1 },
  ]));
  assert.equal(await page.evaluate(() => window.__harness.measurements().length), 2, 'room + deduct');
  // Click the parent row's ✕ (parent = the non-deduct row).
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.measRow')].find(r => !r.classList.contains('deductRow'));
    [...row.querySelectorAll('button')].find(b => b.textContent === '✕').click();
  });
  assert.equal(await page.evaluate(() => window.__harness.measurements().length), 0,
    'deleting the parent removed it and its deduct');
});

await run('deduct: reload restores the parent/child link and re-derives net', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), deductProject([
    area(1, 0, 0, 400, 300),
    { id: 2, page: 1, kind: 'deduct', label: 'Deduct 1', origin: 'manual', color: '#1e3a8a',
      geometry: rectFlat(50, 50, 100, 100), parentId: 1, includePerimeter: true },
  ]));
  await page.evaluate(() => window.__harness.flushSave());
  await page.goto(URL);
  await waitReady(page);
  const restored = await page.evaluate(() => {
    const d = window.__harness.measurements().find(m => m.kind === 'deduct');
    return { parentId: d?.parentId, includePerimeter: d?.includePerimeter,
             net: window.__harness.netArea(1), bomStored: d ? 'bom' in d : false };
  });
  assert.equal(restored.parentId, 1, 'parentId restored');
  assert.equal(restored.includePerimeter, true, 'includePerimeter restored');
  assert.ok(Math.abs(restored.net - 1100) < 1e-6, 'net re-derived to 1100 (nothing derived was stored)');
});

await run('deduct tool: draw a column inside a room → attaches and nets (click-driven)', async page => {
  await snapOff(page);
  // Draw the room with the area tool.
  await setTool(page, 'area');
  for (const [x, y] of [[100, 100], [300, 100], [300, 250], [100, 250]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  // Draw a column inside with the deduct tool.
  await setTool(page, 'deduct');
  for (const [x, y] of [[150, 150], [200, 150], [200, 200], [150, 200]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const st = await page.evaluate(() => {
    const ms = window.__harness.measurements();
    const areaM = ms.find(m => m.kind === 'area');
    const ded = ms.find(m => m.kind === 'deduct');
    return {
      kinds: ms.map(m => m.kind),
      parentMatches: ded && areaM && ded.parentId === areaM.id,
      net: window.__harness.netArea(areaM.id), gross: window.__harness.grossArea(areaM.id),
    };
  });
  assert.deepEqual(st.kinds, ['area', 'deduct'], 'an area and a deduct were committed');
  assert.ok(st.parentMatches, 'the deduct attached to the room it was drawn inside');
  assert.ok(st.net < st.gross, `net (${st.net}) is below gross (${st.gross})`);
});

// Room (400×300 pt = 1200 SF gross, 140 LF perimeter) with commercial flooring
// and a 100×100 pt deduct (100 SF, 40 LF perimeter) → net 1100 SF.
const flooringRoomWithDeduct = () => deductProject([
  { ...area(1, 0, 0, 400, 300), assemblyId: 'commercial_flooring' },
  { id: 2, page: 1, kind: 'deduct', label: 'Deduct 1', origin: 'manual', color: '#1e3a8a',
    geometry: rectFlat(50, 50, 100, 100), parentId: 1 },
]);

await run('deduct: a BOM on a room with deducts uses NET area, not gross', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), flooringRoomWithDeduct());
  const mat = await page.evaluate(() =>
    window.__harness.applyBom(1).bom.line_items.find(l => l.part_name === 'Flooring material').final_quantity);
  // material = area_sf × 1.10 waste. Net 1100 → 1210, NOT gross 1200 → 1320.
  assert.ok(Math.abs(mat - 1210) < 1e-6, `material uses net (1100·1.10=1210), got ${mat}`);
});

await run('deduct: "include perimeter" flag changes the cove-base LF by the deduct perimeter', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), flooringRoomWithDeduct());
  // Flag OFF (default): cove = parent perimeter 140 × 1.05 = 147.
  const coveOff = await page.evaluate(() => window.__harness.applyBom(1).bom.line_items
    .find(l => l.part_name === 'Cove base').final_quantity);
  assert.ok(Math.abs(coveOff - 147) < 1e-6, `cove off = 140·1.05 = 147, got ${coveOff}`);
  // Flag ON: cove = (140 + deduct 40) × 1.05 = 189.
  await page.evaluate(() => window.__harness.setIncludePerimeter(2, true));
  const coveOn = await page.evaluate(() => window.__harness.applyBom(1).bom.line_items
    .find(l => l.part_name === 'Cove base').final_quantity);
  assert.ok(Math.abs(coveOn - 189) < 1e-6, `cove on = (140+40)·1.05 = 189, got ${coveOn}`);
  assert.ok(Math.abs((coveOn - coveOff) - 42) < 1e-6, 'delta = deduct perimeter 40 × 1.05 = 42');
});

await run('deduct: the material list inherits net (rolls up from the net BOM)', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), flooringRoomWithDeduct());
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  const line = out.csv.trim().split('\n').find(l => l.includes(',Flooring material,'));
  // Net-based material quantity (1210), same as the BOM — no separate rollup path.
  assert.equal(line, 'Base Bid,material,(unassigned),Flooring material,SF,1210,0.00,0.00,Room 1', `material list uses net: ${line}`);
});

await run('deduct: takeoff.csv breaks out gross/deduct/net and names the deduct’s parent', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), deductProject([
    area(1, 0, 0, 400, 300), // 1200 SF gross
    { id: 2, page: 1, kind: 'deduct', label: 'Column A', origin: 'manual', color: '#1e3a8a',
      geometry: rectFlat(50, 50, 100, 100), parentId: 1 }, // 100 SF
  ]));
  const lines = (await page.evaluate(() => window.__harness.buildCsv())).trim().split('\n');
  const cols = lines.slice(1).map(l => l.split(','));
  // Columns shift +1 for the leading scope column.
  const idx = { gross: 9, deduct: 10, net: 11, parent: 12 };
  const room = cols.find(c => c[3] === 'area');
  const ded = cols.find(c => c[3] === 'deduct');
  // Room: quantity column carries net; gross/deduct/net broken out; no parent.
  assert.equal(room[4], '1100.00', 'quantity column = net');
  assert.equal(room[idx.gross], '1200.00', 'gross_quantity');
  assert.equal(room[idx.deduct], '100.00', 'deduct_quantity');
  assert.equal(room[idx.net], '1100.00', 'net_quantity');
  assert.equal(room[idx.parent], '', 'a room has no parent');
  // gross − deduct = net, internally consistent.
  assert.ok(Math.abs(Number(room[idx.gross]) - Number(room[idx.deduct]) - Number(room[idx.net])) < 1e-9);
  // Deduct row: names its parent, quantity = its own area, breakout blank.
  assert.equal(ded[2], 'Column A', 'the deduct row is Column A');
  assert.equal(ded[idx.parent], 'Room 1', 'deduct names its parent room');
  assert.equal(ded[4], '100.00', 'deduct quantity = its own SF');
  assert.equal(ded[idx.gross] + ded[idx.deduct] + ded[idx.net], '', 'no gross/deduct/net on a deduct row');
});

await run('layout: export controls sit in a header above the list — no overlap; list scrolls', async page => {
  // Give the list content so it renders alongside the header controls.
  await setTool(page, 'count');
  await clickBase(page, 600, 400);
  await page.keyboard.press('Enter');
  const geo = await page.evaluate(() => {
    const listTop = document.querySelector('#measList').getBoundingClientRect().top;
    const ids = ['allPages', 'exportCsv', 'exportMaterialList', 'exportJson', 'importJsonLabel'];
    const maxBottom = Math.max(...ids.map(id => document.getElementById(id).getBoundingClientRect().bottom));
    return { listTop, maxBottom, overflowY: getComputedStyle(document.querySelector('#measList')).overflowY };
  });
  // Every export control ends at or above the list's top edge (header block,
  // not floated over the rows).
  assert.ok(geo.maxBottom <= geo.listTop + 1,
    `controls end (${geo.maxBottom.toFixed(1)}) at/above list top (${geo.listTop.toFixed(1)})`);
  assert.equal(geo.overflowY, 'auto', 'the list scrolls independently');
});

await run('layout: the per-row colour swatch is a visible, bordered affordance', async page => {
  await setTool(page, 'count');
  await clickBase(page, 600, 400);
  await page.keyboard.press('Enter');
  const sw = await page.evaluate(() => {
    const el = document.querySelector('.measRow .measColor');
    if (!el) return null;
    const cs = getComputedStyle(el);
    return { w: el.getBoundingClientRect().width, borderStyle: cs.borderTopStyle };
  });
  assert.ok(sw, 'the row has a colour swatch');
  assert.ok(sw.w >= 18, `swatch is visibly sized (${sw?.w}px ≥ 18)`);
  assert.notEqual(sw.borderStyle, 'none', 'swatch has a visible border');
});

await run('condition: a new trace inherits the active condition’s colour + assembly', async page => {
  const condId = await page.evaluate(() =>
    window.__harness.addCondition({ name: 'Epoxy', color: '#15803d', kind: 'area', assemblyId: 'epoxy_coating' }));
  await page.evaluate(id => window.__harness.setActiveCondition(id), condId);
  // Trace an area under the active condition.
  await snapOff(page);
  await setTool(page, 'area');
  for (const [x, y] of [[100, 100], [300, 100], [300, 250], [100, 250]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const st = await page.evaluate(() => {
    const m = window.__harness.measurements().find(x => x.kind === 'area');
    return { conditionId: m.conditionId, color: window.__harness.effColor(m.id),
             asm: window.__harness.effAssemblyId(m.id) };
  });
  assert.equal(st.conditionId, condId, 'measurement joined the active condition');
  assert.equal(st.color, '#15803d', 'colour derives from the condition');
  assert.equal(st.asm, 'epoxy_coating', 'assembly derives from the condition');
});

await run('condition: per-measurement colour/assembly override beats the condition', async page => {
  const condId = await page.evaluate(() =>
    window.__harness.addCondition({ name: 'Epoxy', color: '#15803d', kind: 'area', assemblyId: 'epoxy_coating' }));
  await page.evaluate(id => window.__harness.setActiveCondition(id), condId);
  await snapOff(page);
  await setTool(page, 'area');
  for (const [x, y] of [[100, 100], [300, 100], [300, 250], [100, 250]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const id = await page.evaluate(() => window.__harness.measurements().find(x => x.kind === 'area').id);
  await page.evaluate(i => window.__harness.setMeasColor(i, '#dc2626'), id);
  await page.evaluate(i => window.__harness.setMeasAssembly(i, 'commercial_flooring'), id);
  const eff = await page.evaluate(i => ({ c: window.__harness.effColor(i), a: window.__harness.effAssemblyId(i) }), id);
  assert.equal(eff.c, '#dc2626', 'per-measurement colour overrides the condition');
  assert.equal(eff.a, 'commercial_flooring', 'per-measurement assembly overrides the condition');
});

await run('condition: the BOM uses the condition’s assembly when no per-measurement override', async page => {
  // 400×300 pt room = 1200 SF; condition supplies commercial_flooring.
  const condId = await page.evaluate(() =>
    window.__harness.addCondition({ name: 'Floor', color: '#1d4ed8', kind: 'area', assemblyId: 'commercial_flooring' }));
  await page.evaluate(id => window.__harness.setActiveCondition(id), condId);
  await snapOff(page);
  await setTool(page, 'area');
  for (const [x, y] of [[100, 100], [300, 100], [300, 250], [100, 250]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const id = await page.evaluate(() => window.__harness.measurements().find(x => x.kind === 'area').id);
  const bom = await page.evaluate(i => window.__harness.applyBom(i), id);
  // No per-measurement assemblyId, yet the BOM computes from the condition's.
  assert.ok(bom.ok, `BOM derives from the condition assembly: ${JSON.stringify(bom).slice(0, 120)}`);
  assert.ok(bom.bom.line_items.some(l => l.part_name === 'Flooring boxes'), 'flooring parts present');
});

await run('condition: kind guard — a line under an area condition stays unassigned', async page => {
  const condId = await page.evaluate(() =>
    window.__harness.addCondition({ name: 'Epoxy', color: '#15803d', kind: 'area' }));
  await page.evaluate(id => window.__harness.setActiveCondition(id), condId);
  await snapOff(page);
  await setTool(page, 'line');
  await clickBase(page, 120, 300); await clickBase(page, 280, 300);
  await page.keyboard.press('Enter');
  const cid = await page.evaluate(() => {
    const m = window.__harness.measurements().find(x => x.kind === 'linear');
    return window.__harness.conditionIdOf(m.id);
  });
  assert.equal(cid, null, 'a linear trace does not join an area condition');
});

await run('condition: migration — a pre-conditions project loads and renders', async page => {
  // No `conditions` key, measurement with no conditionId (the old shape).
  await page.evaluate(() => window.__harness.importJson(JSON.stringify({
    version: 1, sha: 'x', name: 'legacy',
    pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
    measurements: [{ id: 1, page: 1, kind: 'area', label: 'Old Room', origin: 'manual',
      color: '#1e3a8a', geometry: [0, 0, 400, 0, 400, 300, 0, 300] }],
  })));
  const st = await page.evaluate(() => ({
    n: window.__harness.measurements().length,
    conds: window.__harness.conditions().length,
    cid: window.__harness.conditionIdOf(1),
    color: window.__harness.effColor(1),
  }));
  assert.equal(st.n, 1, 'legacy measurement loaded');
  assert.equal(st.conds, 0, 'no conditions');
  assert.equal(st.cid, null, 'legacy measurement is unassigned');
  assert.equal(st.color, '#1e3a8a', 'legacy colour preserved');
});

await run('condition: reload restores conditions, active id, and conditionId', async page => {
  const condId = await page.evaluate(() =>
    window.__harness.addCondition({ name: 'Polish', color: '#7c3aed', kind: 'area', assemblyId: 'epoxy_coating' }));
  await page.evaluate(id => window.__harness.setActiveCondition(id), condId);
  await snapOff(page);
  await setTool(page, 'area');
  for (const [x, y] of [[100, 100], [300, 100], [300, 250], [100, 250]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await page.evaluate(() => window.__harness.flushSave());
  await page.goto(URL);
  await waitReady(page);
  const st = await page.evaluate(() => {
    const conds = window.__harness.conditions();
    const m = window.__harness.measurements().find(x => x.kind === 'area');
    return { conds, active: window.__harness.activeCondition(),
             cid: m?.conditionId, color: m ? window.__harness.effColor(m.id) : null };
  });
  assert.equal(st.conds.length, 1, 'condition restored');
  assert.equal(st.conds[0].name, 'Polish', 'condition fields restored');
  assert.equal(st.active, st.conds[0].id, 'active condition restored');
  assert.equal(st.cid, st.conds[0].id, 'measurement conditionId restored');
  assert.equal(st.color, '#7c3aed', 'colour still derives after reload');
});

await run('condition: the list groups by condition with per-condition subtotals', async page => {
  // Two area conditions + one unassigned area, imported directly.
  await page.evaluate(() => window.__harness.importJson(JSON.stringify({
    version: 1, sha: 'x', name: 'grp',
    pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
    conditions: [
      { id: 1, name: 'Epoxy', color: '#15803d', kind: 'area', assemblyId: null },
      { id: 2, name: 'Polish', color: '#7c3aed', kind: 'area', assemblyId: null },
    ],
    activeConditionId: 1,
    measurements: [
      // 400×300 = 1200 SF and 200×300 = 600 SF under Epoxy → 1800 SF, 2 areas.
      { id: 1, page: 1, kind: 'area', label: 'E1', origin: 'manual', geometry: [0, 0, 400, 0, 400, 300, 0, 300], conditionId: 1 },
      { id: 2, page: 1, kind: 'area', label: 'E2', origin: 'manual', geometry: [0, 0, 200, 0, 200, 300, 0, 300], conditionId: 1 },
      // 100×300 = 300 SF under Polish.
      { id: 3, page: 1, kind: 'area', label: 'P1', origin: 'manual', geometry: [0, 0, 100, 0, 100, 300, 0, 300], conditionId: 2 },
      // Unassigned area.
      { id: 4, page: 1, kind: 'area', label: 'U1', origin: 'manual', geometry: [0, 0, 100, 0, 100, 100, 0, 100] },
    ],
  })));
  // Group headers render, in condition order then Unassigned.
  const headers = await page.evaluate(() =>
    [...document.querySelectorAll('.measGroupHdr')].map(h => h.textContent));
  assert.equal(headers.length, 3, 'Epoxy, Polish, Unassigned group headers');
  assert.match(headers[0], /Epoxy.*2 areas, 1,800 SF/, `epoxy subtotal: ${headers[0]}`);
  assert.match(headers[1], /Polish.*1 area, 300 SF/, `polish subtotal: ${headers[1]}`);
  assert.match(headers[2], /Unassigned/, `unassigned header: ${headers[2]}`);
  // The hook subtotals agree.
  const subs = await page.evaluate(() => window.__harness.conditionSubtotals());
  assert.deepEqual(subs.map(s => [s.name, s.count, Math.round(s.total)]),
    [['Epoxy', 2, 1800], ['Polish', 1, 300], ['Unassigned', 1, 100]]);
});

await run('condition: the material list separates products (per-condition rollup)', async page => {
  // Epoxy → epoxy_coating; Polish → commercial_flooring; each a 1200 SF room.
  await page.evaluate(() => window.__harness.importJson(JSON.stringify({
    version: 1, sha: 'x', name: 'ml',
    pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
    conditions: [
      { id: 1, name: 'Epoxy', color: '#15803d', kind: 'area', assemblyId: 'epoxy_coating' },
      { id: 2, name: 'Polish', color: '#7c3aed', kind: 'area', assemblyId: 'commercial_flooring' },
    ],
    activeConditionId: 1,
    measurements: [
      { id: 1, page: 1, kind: 'area', label: 'E1', origin: 'manual', geometry: [0, 0, 400, 0, 400, 300, 0, 300], conditionId: 1 },
      { id: 2, page: 1, kind: 'area', label: 'P1', origin: 'manual', geometry: [0, 0, 400, 0, 400, 300, 0, 300], conditionId: 2 },
    ],
  })));
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  const lines = out.csv.trim().split('\n');
  assert.equal(lines[0], 'scope,category,condition,part,unit,total_quantity,unit_cost,extended_cost,measurements');
  // Data rows (excluding the header and the TOTAL footers) each carry their
  // condition; Epoxy and Polish are separate. (Synthetic assemblies → no
  // consumables, so every data row is a material row.)
  const dataRows = lines.slice(1).filter(l => !l.endsWith('TOTAL MATERIALS') && !l.endsWith('TOTAL CONSUMABLES'));
  // All Base Bid; each row carries its condition (col 2). Epoxy and Polish separate.
  assert.ok(dataRows.every(l => l.startsWith('Base Bid,material,Epoxy,') || l.startsWith('Base Bid,material,Polish,')),
    'every row carries its condition');
  assert.ok(lines.some(l => l.startsWith('Base Bid,material,Epoxy,Epoxy,GAL,')), `epoxy line present: ${lines.join(' | ')}`);
  assert.ok(lines.some(l => l.startsWith('Base Bid,material,Polish,Flooring boxes,BOX,')), 'polish flooring line present');
  // No row mixes the two products.
  assert.ok(!lines.some(l => l.startsWith('Base Bid,material,Epoxy,Flooring boxes')), 'epoxy has no flooring parts');
});

// ---- pricing (phase 1): cost on the BOM ----
// A 400×300 pt room = 1,200 SF at fpi 7.2, attached to a seeded priced system.
const pricedRoom = assemblyId => ({
  version: 1, sha: 'x', name: 'price',
  pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
  measurements: [{
    id: 1, page: 1, kind: 'area', label: 'Room 1', origin: 'manual', color: '#1e3a8a',
    geometry: [0, 0, 400, 0, 400, 300, 0, 300], assemblyId,
  }],
});

await run('pricing: BOM shows unit + extended cost and a materials total (Epoxy+HW @ 1200 SF)', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  const res = await page.evaluate(() => window.__harness.applyBom(1));
  assert.ok(res.ok, `BOM computed: ${JSON.stringify(res).slice(0, 140)}`);
  assert.ok(Math.abs(res.bom.materials_total - 1180.8) < 1e-6, `materials 1180.80, got ${res.bom.materials_total}`);
  const hw = res.bom.line_items.find(l => l.part_name === 'High Wear Urethane');
  assert.equal(hw.unit_cost, 159.6);
  assert.ok(Math.abs(hw.extended_cost - 766.08) < 1e-6, `HW extended 766.08, got ${hw.extended_cost}`);
  // The expanded DOM shows the extended cost per line and a materials total.
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomPanel .bomTotal');
  const total = await page.$eval('.bomPanel .bomTotal', el => el.textContent);
  assert.match(total, /Materials.*\$1,?180\.80/, `materials total shown: ${total}`);
  const exts = await page.$$eval('.bomPanel .bomLine .bomExt', els => els.map(e => e.textContent));
  assert.ok(exts.some(t => t.includes('766.08')), `HW extended-cost cell shown: ${exts.join(' ')}`);
});

await run('pricing: a recovered-flake credit renders as a negative extended cost', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('flake'));
  const res = await page.evaluate(() => window.__harness.applyBom(1));
  const credit = res.bom.line_items.find(l => l.part_name.includes('Recovered'));
  assert.ok(credit.extended_cost < 0, `recovered-flake extended cost is negative: ${credit.extended_cost}`);
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomPanel .bomExt.credit'); // green credit cell rendered
  assert.ok(await page.$eval('.bomPanel .bomExt.credit', el => el.textContent.includes('$')), 'credit shows a $ amount');
});

await run('pricing: a manual-quantity part errors until a quantity is entered in the BOM', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('stitching'));
  let res = await page.evaluate(() => window.__harness.applyBom(1));
  assert.equal(res.ok, false, 'missing manual quantity → apply fails (not a silent zero)');
  assert.match(res.reason, /MISSING_QUANTITY|quantity/, `clear error surfaced: ${res.reason}`);
  // The panel still renders the qty input so it can be fixed.
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomPanel .bomOv .ovRow');
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.bomPanel .bomOv .ovRow')]
      .find(r => r.querySelector('label').textContent === 'Crack Stitching qty');
    const inp = row.querySelector('input');
    inp.value = '10';
    inp.dispatchEvent(new Event('change'));
  });
  res = await page.evaluate(() => window.__harness.applyBom(1));
  assert.ok(res.ok, 'supplying the quantity computes the BOM');
  assert.ok(Math.abs(res.bom.materials_total - 40) < 1e-9, '10 stitches × $4 = $40');
});

await run('pricing: material list export carries unit + extended cost and a grand total', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  const lines = out.csv.trim().split('\n');
  assert.equal(lines[0], 'scope,category,condition,part,unit,total_quantity,unit_cost,extended_cost,measurements');
  const hw = lines.find(l => l.includes(',High Wear Urethane,'));
  assert.ok(hw.startsWith('Base Bid,material,') && hw.includes(',159.60,766.08,'), `HW priced row: ${hw}`);
  // epoxy_hw has the coating profile → auto consumables split into their own
  // rows + a separate TOTAL CONSUMABLES footer.
  assert.ok(lines.some(l => l.startsWith('Base Bid,consumable,')), 'consumable rows present (auto)');
  assert.ok(lines.some(l => l.endsWith('1180.80,TOTAL MATERIALS')), 'materials total footer');
  assert.ok(lines.some(l => l.endsWith('TOTAL CONSUMABLES')), 'consumables total footer');
  assert.ok(Math.abs(out.materialsTotal - 1180.8) < 1e-6, `materialsTotal ${out.materialsTotal}`);
  assert.ok(out.consumablesTotal > 0, `consumablesTotal ${out.consumablesTotal}`);
});

await run('stacking: a system + add-on shows two BOM sections and a combined total', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  // Stack Crack Repair on top of the Epoxy+HW system.
  await page.evaluate(() => window.__harness.setStack(1, ['crack_repair']));
  const res = await page.evaluate(() => window.__harness.stackedBom(1));
  assert.ok(res.ok, `stacked BOM ok: ${JSON.stringify(res).slice(0, 160)}`);
  assert.deepEqual(res.groups.map(g => g.assembly.name),
    ['Epoxy + High Wear Urethane', 'Crack Repair (Mender + Sand)'], 'two groups, base first');
  // Combined total = system 1180.80 + crack_repair (mender_a 4.8·10.17 + mender_b 4.8·10.17 + sand 3.6·0.05).
  const cr = 4.8 * 10.17 + 4.8 * 10.17 + 3.6 * 0.05; // 97.812
  assert.ok(Math.abs(res.groups[1].bom.materials_total - cr) < 1e-6, `crack_repair subtotal ${res.groups[1].bom.materials_total}`);
  assert.ok(Math.abs(res.materialsTotal - (1180.8 + cr)) < 1e-6, `combined total ${res.materialsTotal}`);
  // DOM: two MATERIAL group headers (consumables get their own header) + totals.
  await page.click('.measRow .bomToggle');
  await page.waitForSelector('.bomPanel .bomGroupHdr');
  const hdrs = await page.$$eval('.bomPanel .bomGroupHdr:not(.bomConsHdr) span:first-child', els => els.map(e => e.textContent));
  assert.deepEqual(hdrs, ['Epoxy + High Wear Urethane', 'Crack Repair (Mender + Sand)'], 'grouped by assembly in the DOM');
  const totals = await page.$$eval('.bomPanel .bomTotal', els => els.map(e => e.textContent));
  assert.ok(totals.some(t => t.startsWith('Materials')), `a Materials total: ${totals.join(' | ')}`);
  assert.ok(totals.some(t => t.startsWith('Total')), 'a grand Total row');
});

await run('stacking: a per-assembly override touches only its own section', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('flake'));
  await page.evaluate(() => window.__harness.setStack(1, ['crack_repair']));
  const before = await page.evaluate(() => window.__harness.stackedBom(1).groups.map(g => g.bom.materials_total));
  // Override the base flake system's material waste; the add-on is untouched.
  await page.evaluate(() => window.__harness.setOverride(1, 'waste:flake_thrown', 20, 'flake'));
  const after = await page.evaluate(() => window.__harness.stackedBom(1).groups.map(g => g.bom.materials_total));
  assert.notEqual(after[0], before[0], 'the flake system subtotal changed');
  assert.equal(after[1], before[1], 'the crack-repair subtotal is unchanged');
  // The override is stored under its assembly id only.
  assert.deepEqual(await page.evaluate(() => window.__harness.measOverrides(1)),
    { flake: { 'waste:flake_thrown': 20 } }, 'override keyed by assembly');
});

await run('stacking: reload restores the stack and nested overrides; BOM re-derived', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  await page.evaluate(() => window.__harness.setStack(1, ['crack_repair', 'moisture']));
  await page.evaluate(() => window.__harness.setOverride(1, 'waste:material', 15, 'crack_repair'));
  await page.evaluate(() => window.__harness.flushSave());
  await page.goto(URL);
  await waitReady(page);
  const st = await page.evaluate(() => ({
    stack: window.__harness.stackOf(1),
    eff: window.__harness.effectiveStack(1),
    overrides: window.__harness.measOverrides(1),
    total: window.__harness.stackedBom(1).materialsTotal,
  }));
  assert.deepEqual(st.stack, ['crack_repair', 'moisture'], 'stack restored');
  assert.deepEqual(st.eff, ['epoxy_hw', 'crack_repair', 'moisture'], 'effective stack = base + stack');
  assert.deepEqual(st.overrides, { crack_repair: { 'waste:material': 15 } }, 'nested override restored');
  assert.ok(st.total > 0, 're-derived combined total');
});

await run('stacking UI: the add-on picker grows the BOM + material list; the chip removes it', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  const grand = () => page.evaluate(() => window.__harness.buildMaterialList().materialsTotal);
  const before = await grand();
  assert.ok(Math.abs(before - 1180.8) < 1e-6, `system-only materials ${before}`);
  // Add Crack Repair via the row picker.
  await page.evaluate(() => {
    const sel = document.querySelector('.measRow .stackAdd');
    sel.value = [...sel.options].find(o => o.textContent === 'Crack Repair (Mender + Sand)').value;
    sel.dispatchEvent(new Event('change'));
  });
  assert.deepEqual(await page.evaluate(() => window.__harness.stackOf(1)), ['crack_repair'], 'add-on stacked');
  const withAddon = await grand();
  assert.ok(withAddon > before, `material list grew: ${before} → ${withAddon}`);
  assert.ok(Math.abs(withAddon - (1180.8 + 97.812)) < 1e-6, `grand total ${withAddon}`);
  // The material list now carries a Crack-Repair part row.
  const lines = await page.evaluate(() => window.__harness.buildMaterialList().csv.trim().split('\n'));
  assert.ok(lines.some(l => l.includes(',Mender – Part A,')), 'add-on parts appear in the material list');
  // Remove it via the chip ✕ → back to system-only.
  await page.click('.measRow .stackChip button');
  assert.deepEqual(await page.evaluate(() => window.__harness.stackOf(1)), [], 'add-on removed');
  assert.ok(Math.abs((await grand()) - before) < 1e-6, 'material list shrank back');
});

await run('stacking UI: a Linear add-on is not offered on an area row', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), pricedRoom('epoxy_hw'));
  const opts = await page.evaluate(() =>
    [...document.querySelectorAll('.measRow .stackAdd option')].map(o => o.textContent));
  // An area add-on is offered; Joint Fill (Linear caulk) is not.
  assert.ok(opts.includes('Moisture Mitigation (H2 Out)'), 'a same-kind add-on is offered');
  assert.ok(!opts.some(o => o.includes('Joint Fill')), 'the Linear caulk add-on is not offered on an area');
});

// Two priced rooms for scope tests: one Epoxy+HW, one Polished Concrete.
const twoRooms = () => ({
  version: 1, sha: 'x', name: 'scope',
  pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
  measurements: [
    { id: 1, page: 1, kind: 'area', label: 'Main', origin: 'manual', color: '#1e3a8a',
      geometry: [0, 0, 400, 0, 400, 300, 0, 300], assemblyId: 'epoxy_hw' },
    { id: 2, page: 1, kind: 'area', label: 'Bathroom', origin: 'manual', color: '#1e3a8a',
      geometry: [0, 0, 200, 0, 200, 150, 0, 150], assemblyId: 'polish' },
  ],
});

await run('scope: a measurement defaults to Base Bid (migration) and can move to an alternate', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  // Migration: pre-scope import → both base.
  assert.deepEqual(await page.evaluate(() => window.__harness.scopeOf(1)), { scope: 'base', alternateGroupId: null });
  const t0 = await page.evaluate(() => window.__harness.scopeTotals());
  assert.equal(t0.alternates.length, 0, 'no alternates yet');
  assert.ok(t0.base > 0, 'base bid has both rooms');
  // Move the bathroom into a new alternate.
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Add bathroom'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  const t1 = await page.evaluate(() => window.__harness.scopeTotals());
  assert.equal(t1.alternates.length, 1, 'one alternate');
  assert.equal(t1.alternates[0].name, 'Add bathroom');
  // The alternate's cost left the base bid and stands alone.
  assert.ok(Math.abs(t1.base - (t0.base - t1.alternates[0].total)) < 1e-6, 'alternate no longer folded into base');
  assert.ok(t1.alternates[0].total > 0, 'alternate priced standalone');
});

await run('scope: include/exclude changes the combined number, not the measurement scope', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Alt 1'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  const excluded = await page.evaluate(() => window.__harness.scopeTotals());
  assert.ok(Math.abs(excluded.combined - excluded.base) < 1e-6, 'combined = base while alternate excluded');
  await page.evaluate(g => window.__harness.setIncluded([g]), gid);
  const included = await page.evaluate(() => window.__harness.scopeTotals());
  assert.ok(Math.abs(included.combined - (included.base + included.alternates[0].total)) < 1e-6,
    'combined = base + alternate when included');
  // Toggling inclusion did NOT change the measurement's scope.
  assert.deepEqual(await page.evaluate(g => window.__harness.scopeOf(2), gid), { scope: 'alternate', alternateGroupId: gid });
});

await run('scope: the list groups by scope (Base Bid then alternate), condition-nested', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Add bathroom'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  const headers = await page.evaluate(() =>
    [...document.querySelectorAll('#measList .scopeHdr .scopeName')].map(e => e.textContent));
  assert.deepEqual(headers, ['Base Bid', 'Alternate — Add bathroom'], 'Base Bid first, then the alternate');
  // The bid-totals panel shows both plus a combined row.
  const st = await page.evaluate(() => document.querySelector('#scopeTotals').textContent);
  assert.match(st, /Base Bid/);
  assert.match(st, /Add bathroom/);
  assert.match(st, /Combined/);
});

await run('scope: deleting an alternate returns its members to the base bid', async page => {
  page.on('dialog', d => d.accept());
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Alt 1'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  await page.evaluate(g => window.__harness.deleteAlternateGroup(g), gid);
  assert.deepEqual(await page.evaluate(() => window.__harness.scopeOf(2)), { scope: 'base', alternateGroupId: null }, 'member back to base');
  assert.equal((await page.evaluate(() => window.__harness.scopeTotals())).alternates.length, 0, 'alternate gone');
});

await run('scope: reload restores scope + alternate groups', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Add bathroom'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  await page.evaluate(() => window.__harness.flushSave());
  await page.goto(URL);
  await waitReady(page);
  const st = await page.evaluate(() => ({
    groups: window.__harness.alternateGroups(),
    scope2: window.__harness.scopeOf(2),
    totals: window.__harness.scopeTotals(),
  }));
  assert.equal(st.groups.length, 1, 'alternate group restored');
  assert.equal(st.groups[0].name, 'Add bathroom');
  assert.equal(st.scope2.scope, 'alternate', 'measurement scope restored');
  assert.equal(st.scope2.alternateGroupId, st.groups[0].id, 'still in its group');
  assert.equal(st.totals.alternates.length, 1, 're-derived alternate total');
});

await run('scope export: material list + CSV break out by scope; an alternate never folds into base', async page => {
  await page.evaluate(p => window.__harness.importJson(JSON.stringify(p)), twoRooms());
  const gid = await page.evaluate(() => window.__harness.addAlternateGroup('Add bathroom'));
  await page.evaluate(g => window.__harness.setMeasScope(2, 'alternate', g), gid);
  const out = await page.evaluate(() => window.__harness.buildMaterialList());
  const lines = out.csv.trim().split('\n');
  assert.equal(lines[0], 'scope,category,condition,part,unit,total_quantity,unit_cost,extended_cost,measurements');
  const dataRows = lines.slice(1).filter(l => !l.endsWith('TOTAL MATERIALS') && !l.endsWith('TOTAL CONSUMABLES'));
  assert.ok(dataRows.some(l => l.startsWith('Base Bid,')), 'base rows present');
  assert.ok(dataRows.some(l => l.startsWith('Add bathroom,')), 'alternate rows tagged by scope');
  // Per-scope TOTAL footers — the alternate's materials are their own block.
  assert.ok(lines.some(l => l.startsWith('Base Bid,') && l.endsWith('TOTAL MATERIALS')), 'base materials total');
  assert.ok(lines.some(l => l.startsWith('Add bathroom,') && l.endsWith('TOTAL MATERIALS')), 'alternate materials total');
  assert.ok(out.byScope['Base Bid'].materials > 0 && out.byScope['Add bathroom'].materials > 0,
    'each scope has its own materials total');
  // out.materialsTotal is the BASE bid only — the alternate never folds in.
  assert.ok(Math.abs(out.materialsTotal - out.byScope['Base Bid'].materials) < 1e-9, 'materialsTotal = base bid');
  // takeoff.csv: the bathroom row carries the alternate scope.
  const csv = (await page.evaluate(() => window.__harness.buildCsv())).trim().split('\n');
  const bath = csv.slice(1).map(l => l.split(',')).find(c => c[2] === 'Bathroom');
  assert.equal(bath[0], 'Add bathroom', 'CSV row scoped to the alternate');
});

await run('advanced: detection tuning is collapsed by default and holds the knobs + stats', async page => {
  const st = await page.evaluate(() => {
    const adv = document.querySelector('#advanced');
    const ids = ['thresh', 'doorGap', 'maskRaster', 'maskVector', 'minWidth', 'snapChk', 'hideHatch', 'vecInfo'];
    return { tag: adv.tagName, open: adv.open,
             contains: ids.every(id => adv.contains(document.getElementById(id))) };
  });
  assert.equal(st.tag, 'DETAILS', 'Advanced is a disclosure');
  assert.equal(st.open, false, 'collapsed by default — tuning knobs are not daily controls');
  assert.ok(st.contains, 'the tuning knobs + segment stats live inside Advanced');
});

await run('status: a detect shows ONE message; the diagnostic moves to Advanced', async page => {
  await setTool(page, 'detect');
  await clickBase(page, 675, 225); // hatched room; hide-hatch is on by default → recovers
  const st = await page.evaluate(() => ({
    status: document.querySelector('#status').textContent,
    detail: document.querySelector('#advDetail').textContent,
    n: window.__harness.measurements().length,
  }));
  assert.equal(st.n, 1, 'a room was detected');
  assert.ok(!st.status.includes('\n'), `status is one line, not a wall of stats: ${JSON.stringify(st.status)}`);
  assert.match(st.status, /detected —.*SF.*LF/, 'a concise headline');
  assert.match(st.detail, /cross-check/, 'the verbose diagnostic landed in the Advanced pane');
});

await run('toolbar: grouped by function; active tool is uniquely indicated', async page => {
  const groups = await page.evaluate(() =>
    [...document.querySelectorAll('.toolbar .tgroup')].map(g => g.dataset.group));
  assert.deepEqual(groups, ['file', 'zoom', 'scale', 'tools', 'data'],
    'five functional groups in order');
  // The tool buttons live inside the tools group.
  assert.ok(await page.evaluate(() =>
    document.querySelector('.tgroup[data-group="tools"]').contains(document.querySelector('#tools'))),
    'tool buttons are in the tools group');
  // Selecting a tool marks exactly that button active.
  await setTool(page, 'area');
  const st = await page.evaluate(() => ({
    active: [...document.querySelectorAll('#tools .tool.active')].map(b => b.dataset.tool),
    ring: getComputedStyle(document.querySelector('#tools .tool.active')).boxShadow,
  }));
  assert.deepEqual(st.active, ['area'], 'exactly one active tool, and it is the selected one');
  assert.notEqual(st.ring, 'none', 'the active tool has a visible ring');
});

await run('legend: a colour key per condition + Deduct/Unassigned; hidden when empty', async page => {
  // Empty sheet → no legend.
  assert.equal(await page.$eval('#legend', el => el.hidden), true, 'legend hidden on an empty sheet');
  // Two conditions with measurements, one unassigned area, and a deduct.
  await page.evaluate(() => window.__harness.importJson(JSON.stringify({
    version: 1, sha: 'x', name: 'leg',
    pageScales: [{ page: 1, feet_per_paper_inch: 7.2, source: 'test' }],
    conditions: [
      { id: 1, name: 'Epoxy', color: '#15803d', kind: 'area', assemblyId: null },
      { id: 2, name: 'Polish', color: '#7c3aed', kind: 'area', assemblyId: null },
    ],
    activeConditionId: 1,
    measurements: [
      { id: 1, page: 1, kind: 'area', label: 'E1', origin: 'manual', geometry: [0, 0, 400, 0, 400, 300, 0, 300], conditionId: 1 },
      { id: 2, page: 1, kind: 'area', label: 'P1', origin: 'manual', geometry: [500, 0, 700, 0, 700, 300, 500, 300], conditionId: 2 },
      { id: 3, page: 1, kind: 'area', label: 'U1', origin: 'manual', geometry: [0, 400, 100, 400, 100, 500, 0, 500] },
      { id: 4, page: 1, kind: 'deduct', label: 'D1', origin: 'manual', geometry: [50, 50, 150, 50, 150, 150, 50, 150], parentId: 1 },
    ],
  })));
  const leg = await page.evaluate(() => ({
    hidden: document.querySelector('#legend').hidden,
    chips: [...document.querySelectorAll('#legend .legChip')].map(c => c.textContent),
  }));
  assert.equal(leg.hidden, false, 'legend shown once the sheet has measurements');
  assert.deepEqual(leg.chips, ['Epoxy', 'Polish', 'Unassigned', 'Deduct'],
    'a chip per used condition, then Unassigned, then Deduct');
  // Swatch colours match the conditions.
  const colors = await page.evaluate(() =>
    [...document.querySelectorAll('#legend .legChip .condSwatch')].map(s => getComputedStyle(s).backgroundColor));
  assert.equal(colors[0], 'rgb(21, 128, 61)', 'Epoxy swatch is its colour');
  assert.equal(colors[1], 'rgb(124, 58, 237)', 'Polish swatch is its colour');
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
