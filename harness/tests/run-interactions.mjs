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
    'page,name,kind,quantity,unit,origin,page_scale_fpi,scale_source', 'header');
  assert.equal(lines.length, 4, 'header + 3 data rows');
  const cols = lines.slice(1).map(l => l.split(','));
  const area = cols.find(c => c[2] === 'area');
  const line = cols.find(c => c[2] === 'linear');
  const count = cols.find(c => c[2] === 'count');
  assert.ok(area && line && count, 'one row per kind');
  assert.equal(area[4], 'SF'); assert.equal(area[6], '7.2000'); assert.equal(area[7], 'param');
  assert.equal(line[4], 'LF'); assert.equal(line[5], 'manual');
  assert.equal(count[3], '3.00'); assert.equal(count[4], 'EA'); assert.equal(count[6], '7.2000');
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
  // A canvas edit attempt while blocked does nothing (guard + overlay).
  await clickBase(page, 600, 300);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1, 'no new measurement while blocked');
  // Dismiss (the modal button is on top) → unblocked, edits resume.
  await page.click('#storageDismiss');
  assert.equal(await page.evaluate(() => window.__harness.storageBlocked()), false);
  for (const [x, y] of [[600, 300], [700, 300], [700, 400], [600, 400]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 2, 'edits resume after acknowledge');
});

await run('assembly library seeds the two engine assemblies', async page => {
  const names = await page.evaluate(async () =>
    (await window.__harness.listAssemblies()).map(a => a.name).sort());
  assert.deepEqual(names, ['Commercial Flooring', 'Epoxy Coating'], 'library seeded from engine seeds');
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
  assert.equal(count, 2, 'nothing persisted while invalid');

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
    { 'waste:material': 20 }, 'override stored on the measurement');
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
  assert.deepEqual(restored.overrides, { 'waste:material': 20 }, 'overrides restored');
  assert.equal(restored.bomStored, false, 'no BOM persisted on the measurement (invariant 5)');
  // The BOM is re-derived from geometry+scale+assembly+overrides, not stored.
  const mat = await page.evaluate(() => {
    const id = (window.__harness.measurements()[0]).id;
    return window.__harness.applyBom(id).bom.line_items.find(l => l.part_name === 'Flooring material').final_quantity;
  });
  assert.ok(Math.abs(mat - 2970) < 1e-6, `re-derived material honors the restored override, got ${mat}`);
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
