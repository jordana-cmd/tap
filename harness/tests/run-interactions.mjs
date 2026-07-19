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
    verts: m.geometry.length / 2,
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

await browser.close();
server.close();
if (failures) {
  console.log(`\n${failures} case(s) FAILED`);
  process.exit(1);
}
console.log('\nall interaction cases passed');
