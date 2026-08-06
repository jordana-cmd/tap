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
    assert.deepEqual(pageErrors, [], `uncaught page errors: ${JSON.stringify(pageErrors)}`);
  } finally {
    await context.close();
  }
}

const state = page => page.evaluate(() => ({
  tool: window.__harness.tool(),
  meas: window.__harness.measurements().map(m => ({
    name: m.name, kind: m.kind, value: m.value, origin: m.origin,
    page: m.page, color: m.color, verts: m.geometry.length / 2,
    systemType: m.systemType,
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
// The redesigned ribbon tab-gates the scale/preset controls and drawer-gates
// the raw engine knobs; real Puppeteer clicks (unlike in-page .click()/$eval)
// require the target to actually be visible, so switch context first.
const openScaleTab = page => page.click('.ribbonTab[data-tab="scale"]');
const openEngineDrawer = page => page.click('#engineSettingsBtn');
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
  await openEngineDrawer(page);
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

// The sheet name in #pageLabel is unbounded (outline titles have no length
// cap), and an auto-width label used to re-flow the row on every page flip —
// the ▶ arrow and the whole zoom group slid ~350px between a short and a long
// name, so rapid page-flipping meant re-aiming after every click.
const LONG_SHEET = '2 / 22 — 32_LS1_LIFE SAFETY PLAN & DETAILS(Version=2)(Version=1)';
const navGeometry = page => page.evaluate(() => {
  const left = id => Math.round(document.querySelector(id).getBoundingClientRect().left);
  const lb = document.querySelector('#pageLabel');
  return {
    prev: left('#prev'), next: left('#next'),
    zoomOut: left('#zoomOut'), zoomFit: left('#zoomFit'), zoomIn: left('#zoomIn'),
    labelH: Math.round(lb.getBoundingClientRect().height),
    labelW: Math.round(lb.getBoundingClientRect().width),
    text: lb.textContent, title: lb.title,
    truncated: lb.scrollWidth > lb.clientWidth,
  };
});
const setLabel = (page, t) => page.evaluate(v => {
  const el = document.querySelector('#pageLabel');
  el.textContent = v; el.title = v;
}, t);

await run('page nav: arrows hold position regardless of sheet-name length', async page => {
  await setLabel(page, '2 / 22');
  const short = await navGeometry(page);
  await setLabel(page, LONG_SHEET);
  const long = await navGeometry(page);

  assert.equal(long.prev, short.prev, 'prev arrow moved');
  assert.equal(long.next, short.next, `next arrow moved ${long.next - short.next}px`);
  assert.equal(long.zoomOut, short.zoomOut, `zoom controls moved ${long.zoomOut - short.zoomOut}px`);
  assert.equal(long.zoomFit, short.zoomFit);
  assert.equal(long.zoomIn, short.zoomIn);
  assert.equal(long.labelW, short.labelW, 'label box width is content-independent');
  assert.equal(long.labelH, short.labelH, 'long name must not wrap to a second line');
  assert.ok(long.truncated, 'the long name is actually being ellipsized');
  assert.ok(!short.truncated, 'a short name is not');
});

await run('page nav: full sheet name survives truncation (PDF header + tooltip)', async page => {
  // Truncation is presentational ONLY: exportPdf()/buildTakeoffPdfBytes()
  // lift pageLabel.textContent into PDF headers, and gotoPage() matches on it.
  await setLabel(page, LONG_SHEET);
  const g = await navGeometry(page);
  assert.equal(g.text, LONG_SHEET, 'textContent must keep the full string');
  assert.equal(g.title, LONG_SHEET, 'title carries the full name for hover');

  // And a REAL page flip must set both, not just textContent.
  await page.reload();
  await waitReady(page);
  await gotoPage(page, +1);
  const real = await navGeometry(page);
  assert.match(real.text, /^2 \/ /, 'label reflects the new page');
  assert.equal(real.title, real.text, 'title mirrors the label after a real flip');
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
  await openScaleTab(page);
  await page.select('#preset', '20');
  // The tool buttons live under the Takeoff Tools ribbon tab (hidden while
  // Scale & Pages is active) — a real user switches back to reach them.
  await page.click('.ribbonTab[data-tab="tools"]');
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
  // Leading job_name/job_address rows come before the column header —
  // top-level and immediately visible, not buried in a nested field.
  assert.match(lines[0], /^job_name,/, 'job_name row leads the file');
  assert.match(lines[1], /^job_address,/, 'job_address row follows');
  assert.equal(lines[2],
    'page,name,kind,quantity,unit,origin,page_scale_fpi,scale_source,system_type', 'header');
  assert.equal(lines.length, 6, '2 job rows + header + 3 data rows');
  const cols = lines.slice(3).map(l => l.split(','));
  const area = cols.find(c => c[2] === 'area');
  const line = cols.find(c => c[2] === 'linear');
  const count = cols.find(c => c[2] === 'count');
  assert.ok(area && line && count, 'one row per kind');
  assert.equal(area[4], 'SF'); assert.equal(area[6], '7.2000'); assert.equal(area[7], 'param');
  assert.equal(line[4], 'LF'); assert.equal(line[5], 'manual');
  assert.equal(count[3], '3.00'); assert.equal(count[4], 'EA'); assert.equal(count[6], '7.2000');
  // system_type is appended last and blank until assigned — existing column
  // positions above are unchanged by its addition.
  assert.equal(area[8], '', 'unassigned area exports a blank system_type');
  assert.equal(line[8], '', 'a linear measurement takes no system');
});

// ---- flooring-system assignment (area measurements) ----

const drawArea = async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
};
const pickSystem = (page, key) => page.select('.measRow .measSystem', key);

await run('area system: dropdown offers exactly the four systems + Unassigned', async page => {
  await drawArea(page);
  const opts = await page.$$eval('.measRow .measSystem option',
    os => os.map(o => ({ value: o.value, label: o.textContent })));
  assert.deepEqual(opts, [
    { value: '', label: 'Unassigned' },
    { value: 'polish', label: 'Polish' },
    { value: 'seal', label: 'Seal' },
    { value: 'epoxy', label: 'Epoxy' },
    { value: 'polyurea', label: 'Polyurea' },
  ], 'catalog rendered verbatim, Unassigned first');
  // Default is null — never back-filled with a real system.
  assert.equal((await state(page)).meas[0].systemType, null, 'defaults to Unassigned');
  assert.ok(await page.$eval('.measRow .measSystem', el => el.classList.contains('unassigned')),
    'unassigned is visually marked');
});

await run('area system: only AREA rows get a picker', async page => {
  await drawArea(page);
  await setTool(page, 'line');
  for (const [x, y] of [[600, 300], [700, 300]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await setTool(page, 'count');
  await clickBase(page, 600, 400);
  await page.keyboard.press('Enter');

  const perRow = await page.$$eval('.measRow', rows => rows.map(r => ({
    text: r.textContent, hasPicker: !!r.querySelector('.measSystem'),
  })));
  assert.equal(perRow.length, 3, 'three measurements listed');
  assert.equal(perRow.filter(r => r.hasPicker).length, 1, 'exactly one picker');
  assert.ok(perRow.find(r => /^Area 1/.test(r.text)).hasPicker, 'the area row has it');
});

await run('area system: selection persists through reload and reaches CSV', async page => {
  await drawArea(page);
  await pickSystem(page, 'epoxy');
  assert.equal((await state(page)).meas[0].systemType, 'epoxy', 'state updated on change');

  const csv = await page.evaluate(() => window.__harness.buildCsv());
  const areaRow = csv.trim().split('\n').slice(3).find(l => l.split(',')[2] === 'area');
  assert.equal(areaRow.split(',')[8], 'epoxy', 'machine key, not the display label');

  await page.evaluate(() => window.__harness.flushSave());
  await page.reload();
  await waitReady(page);
  assert.equal((await state(page)).meas[0].systemType, 'epoxy', 'survives IndexedDB round-trip');
  assert.equal(await page.$eval('.measRow .measSystem', el => el.value), 'epoxy',
    'and the control reflects it after restore');
});

await run('area system: round-trips through JSON export/import', async page => {
  await drawArea(page);
  await pickSystem(page, 'polyurea');
  const json = await page.evaluate(() => window.__harness.exportJson());
  assert.match(json, /"systemType": "polyurea"/, 'present in the JSON backup');

  await page.evaluate(() => window.__harness.deleteProject(window.__harness.currentSha()));
  await page.reload();
  await waitReady(page);
  await page.evaluate(j => window.__harness.importJson(j), json);
  assert.equal((await state(page)).meas[0].systemType, 'polyurea', 'restored from backup');
});

await run('area system: legacy records with no systemType load as Unassigned', async page => {
  // A backup written BEFORE this field existed: same shape, key absent.
  await drawArea(page);
  const legacy = await page.evaluate(() => {
    const doc = JSON.parse(window.__harness.exportJson());
    for (const m of doc.measurements) delete m.systemType;
    return JSON.stringify(doc);
  });
  assert.ok(!legacy.includes('systemType'), 'fixture really has no systemType');

  await page.evaluate(j => window.__harness.importJson(j), legacy);
  assert.equal((await state(page)).meas[0].systemType, null,
    'absent key restores as null, not a guessed system');
  assert.ok(await page.$eval('.measRow .measSystem', el => el.classList.contains('unassigned')),
    'and renders as Unassigned');
});

await run('area system: reassigning back to Unassigned stores null, not ""', async page => {
  await drawArea(page);
  await pickSystem(page, 'seal');
  assert.equal((await state(page)).meas[0].systemType, 'seal');
  await pickSystem(page, '');
  assert.equal((await state(page)).meas[0].systemType, null, 'empty option clears to null');
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

// ---- quote screen helpers ----
// The screen renders engine output and computes nothing itself, so the cases
// below compare the DOM against `__harness.quote()` rather than against a
// number typed into this file: a hardcoded expectation here would only
// re-implement the cost buildup a third time.

const areaBlocks = page => page.$$eval('.qArea', bs => bs.length);

/// Type into a quote control. Every keystroke re-prices and rebuilds the view,
/// restoring focus by data-qkey — so REAL typing (not a scripted value set) is
/// also what proves a control survives its own re-render mid-entry.
async function typeQuote(page, selector, text) {
  await page.click(selector, { clickCount: 3 });
  await page.keyboard.type(text, { delay: 20 });
}

/// The smallest input the engine will price: one 100 SF area on a system,
/// with a crew and hours (it refuses to guess labor rather than quoting a
/// materials-only number that reads as legitimate).
async function priceOneArea(page, system) {
  await drawArea(page);
  await pickSystem(page, system);
  await gotoQuote(page);
  await typeQuote(page, '.qArea input[data-qkey^="crew-"]', '2');
  await typeQuote(page, '.qArea input[data-qkey^="hours-"]', '8');
  await page.waitForFunction(() => window.__harness.quote() !== null, { timeout: 5_000 });
}

// ---- quote handoff cases ----

/// One area (100 SF / 40 LF perimeter), one line (15 LF), one count (3 EA)
/// at the deep-linked fpi 7.2, where 10 pts = 1 ft.
async function drawQuoteFixture(page) {
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
}

const routeState = page => page.evaluate(() => ({
  route: window.__harness.route(),
  attr: document.querySelector('#appShell').dataset.route,
  hash: location.hash,
  // offsetParent is null for a display:none subtree — real visibility, not
  // just the attribute we set.
  quoteShown: !!document.querySelector('#quoteView').offsetParent,
  takeoffShown: !!document.querySelector('#workArea').offsetParent,
}));

// Navigation goes through location.hash, and `hashchange` fires as a queued
// task — the hash is current the instant the click returns, but applyRoute()
// has not run yet. Every navigation waits for the APPLIED route (the shell
// attribute) rather than racing the handler.
const waitRoute = (page, want) => page.waitForFunction(
  w => document.querySelector('#appShell').dataset.route === w, { timeout: 5_000 }, want);
const gotoQuote = async page => {
  await page.click('#quoteBtn');
  await waitRoute(page, 'quote');
  // The rate card is fetched ASYNCHRONOUSLY on the first visit and re-renders
  // the area controls when it lands (the grit picker only exists once the
  // card says which systems grind). Interacting before it settles resolves an
  // element handle the re-render then detaches.
  await page.waitForFunction(
    () => window.__harness.rateCard() !== null || window.__harness.rateCardError() !== null,
    { timeout: 5_000 });
};
const gotoTakeoff = async page => {
  await page.click('#quoteBackBtn');
  await waitRoute(page, 'takeoff');
};

await run('quote: button routes to #/quote, Back returns to takeoff', async page => {
  const before = await routeState(page);
  assert.equal(before.route, 'takeoff', 'boots on the takeoff route');
  assert.ok(before.takeoffShown && !before.quoteShown, 'takeoff visible at boot');

  await gotoQuote(page);
  const onQuote = await routeState(page);
  assert.equal(onQuote.route, 'quote');
  assert.equal(onQuote.attr, 'quote', 'shell carries the route attribute');
  assert.equal(onQuote.hash, '#/quote');
  assert.ok(onQuote.quoteShown && !onQuote.takeoffShown, 'quote view replaces the work area');
  // Same tab, same document — a new tab or external nav would break this.
  assert.equal(await page.evaluate(() => document.querySelectorAll('#quoteView').length), 1);

  await gotoTakeoff(page);
  const back = await routeState(page);
  assert.equal(back.route, 'takeoff');
  assert.ok(back.takeoffShown && !back.quoteShown, 'work area restored');
});

await run('quote: browser Back/Forward drives the same route swap', async page => {
  await gotoQuote(page);
  assert.equal((await routeState(page)).route, 'quote');
  await page.goBack();
  await waitRoute(page, 'takeoff');
  assert.ok((await routeState(page)).takeoffShown, 'history back leaves the quote route');
  await page.goForward();
  await waitRoute(page, 'quote');
  const fwd = await routeState(page);
  assert.equal(fwd.route, 'quote', 'history forward re-enters it');
  assert.ok(fwd.quoteShown, 'and the view actually re-renders');
});

await run('quote payload: quantity mirrors MeasurementInput per kind', async page => {
  await drawQuoteFixture(page);
  await gotoQuote(page);
  const p = await page.evaluate(() => window.__harness.lastQuotePayload());

  assert.equal(p.schema, 'takeoff-quote-payload@1');
  assert.equal(p.items.length, 3, 'one item per measurement');

  const area = p.items.find(i => i.kind === 'area');
  const line = p.items.find(i => i.kind === 'linear');
  const count = p.items.find(i => i.kind === 'count');
  assert.ok(area && line && count, 'one item per kind');

  // Area carries BOTH driving quantities; perimeter is a real field, not a
  // substring of the display text. Tolerances match the suite's existing
  // area cases — click→client-pixel rounding puts the drawn square a couple
  // of percent under a clean 100 SF.
  assert.ok(near(area.quantity.area_sf, 100, 3), `area_sf ${area.quantity.area_sf}`);
  assert.ok(near(area.quantity.perimeter_lf, 40, 1.5), `perimeter_lf ${area.quantity.perimeter_lf}`);
  assert.equal(area.quantity.length_lf, null, 'unused fields are null, never 0');
  assert.equal(area.quantity.count_ea, null);
  assert.equal(area.unit, 'SF');

  assert.ok(near(line.quantity.length_lf, 15, 0.5), `length_lf ${line.quantity.length_lf}`);
  assert.equal(line.quantity.area_sf, null);
  assert.equal(line.unit, 'LF');

  assert.equal(count.quantity.count_ea, 3);
  assert.equal(count.quantity.area_sf, null);
  assert.equal(count.unit, 'EA');

  // Provenance + scale ride along per item (§A2), as they do in CSV.
  assert.equal(area.scale.feet_per_paper_inch, 7.2);
  assert.equal(area.scale.source, 'param');
  assert.ok(area.origin, 'origin carried');
});

await run('quote payload: identifies the project, job, and category groups', async page => {
  await drawQuoteFixture(page);
  const sha = await page.evaluate(() => window.__harness.currentSha());
  await gotoQuote(page);
  const p = await page.evaluate(() => window.__harness.lastQuotePayload());

  assert.equal(p.projectId, sha, 'projectId is the content-addressed project key');
  assert.ok(p.job.name && p.job.address, 'job identity carried');
  assert.ok(p.pageScales.some(s => s.feet_per_paper_inch === 7.2), 'page scales carried');

  const cats = Object.fromEntries(p.groups.map(g => [g.category, g]));
  assert.ok(near(cats.Area.totals.SF, 100, 3), 'Area group totals SF');
  assert.ok(near(cats.Line.totals.LF, 15, 0.5), 'Line group totals LF');
  assert.equal(cats.Count.totals.EA, 3, 'Count group totals EA');
  assert.equal(cats.Area.totals.LF, undefined, 'totals never cross units');
  assert.deepEqual(cats.Count.itemIds, p.items.filter(i => i.kind === 'count').map(i => i.id));
});

await run('quote payload: area system carries key + resolved label', async page => {
  await drawArea(page);
  await pickSystem(page, 'polish');
  await setTool(page, 'line');
  for (const [x, y] of [[600, 300], [700, 300]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');

  await gotoQuote(page);
  const p = await page.evaluate(() => window.__harness.lastQuotePayload());
  const area = p.items.find(i => i.kind === 'area');
  const line = p.items.find(i => i.kind === 'linear');
  assert.deepEqual(area.system, { key: 'polish', label: 'Polish' },
    'machine key for pricing, label resolved at build time');
  assert.equal(line.system, null, 'a linear measurement carries no system');
});

await run('quote payload: rebuilt on each visit, never stale', async page => {
  await drawQuoteFixture(page);
  await gotoQuote(page);
  assert.equal((await page.evaluate(() => window.__harness.lastQuotePayload())).items.length, 3);
  assert.equal(await areaBlocks(page), 1, 'one priceable area so far');

  // A SECOND AREA, so both layers have to notice: the payload and the priced
  // view. Only areas are priced, so a new count would not prove the latter.
  await gotoTakeoff(page);
  await setTool(page, 'area');
  for (const [x, y] of [[600, 450], [660, 450], [660, 500], [600, 500]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');

  await gotoQuote(page);
  const p = await page.evaluate(() => window.__harness.lastQuotePayload());
  assert.equal(p.items.length, 4, 'the new measurement is in the second payload');
  assert.equal(await areaBlocks(page), 2, 'and the priced view shows it too');
});

await run('quote view: every figure on screen is the engine\'s, to the cent', async page => {
  await priceOneArea(page, 'epoxy');
  const seen = await page.evaluate(() => {
    const q = window.__harness.quote();
    const a = q.areas[0];
    const usd = v => v.toLocaleString('en-US',
      { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 2 });
    return {
      engine: {
        profit: usd(q.profit), price: usd(q.price), cost: usd(q.cost),
        margin: `${(q.margin * 100).toFixed(1)}%`,
        areaCost: usd(a.cost), materials: usd(a.materials), consumables: usd(a.consumables),
        labor: usd(a.labor), overhead: usd(a.overhead), lines: a.lines.length,
      },
      dom: {
        profit: document.querySelector('#qhProfitVal').textContent,
        price: document.querySelector('#qhPriceVal').textContent,
        cost: document.querySelector('#qhCostVal').textContent,
        margin: document.querySelector('#qhMarginVal').textContent,
        areaCost: document.querySelector('.qArea .qAreaCost').textContent,
        subtotals: [...document.querySelectorAll('.qArea .qSubtotals div')].map(d => d.textContent),
        rows: document.querySelectorAll('.qArea table.qLines tbody tr').length,
        jobPrice: document.querySelector('#qjPrice').textContent,
        jobCost: document.querySelector('#qjCost').textContent,
        jobProfit: document.querySelector('#qjProfit').textContent,
      },
    };
  });
  const { engine, dom } = seen;
  assert.equal(dom.profit, engine.profit, 'headline profit');
  assert.equal(dom.margin, engine.margin, 'headline margin');
  assert.equal(dom.price, engine.price, 'headline price');
  assert.equal(dom.cost, engine.cost, 'headline cost');
  assert.equal(dom.areaCost, engine.areaCost, 'per-area cost on the header');
  assert.deepEqual(
    [dom.jobCost, dom.jobPrice, dom.jobProfit],
    [engine.cost, engine.price, engine.profit],
    'job-level totals repeat the same figures, not a second computation',
  );
  for (const [label, want] of [['Material', engine.materials], ['Consumables', engine.consumables],
    ['Labor', engine.labor], ['Overhead', engine.overhead]]) {
    assert.ok(dom.subtotals.some(s => s.startsWith(label) && s.endsWith(want)),
      `${label} subtotal ${want} — got ${JSON.stringify(dom.subtotals)}`);
  }
  // Epoxy runs every consumable at full rate, so nothing is hidden and the
  // table is exactly as long as the engine's line list.
  assert.equal(dom.rows, engine.lines, 'one row per engine line');
});

await run('quote view: an area the engine will not price says why, and shows no number', async page => {
  await drawArea(page);
  await pickSystem(page, 'epoxy');   // system but no crew/hours
  await gotoQuote(page);
  const st = await page.evaluate(() => ({
    quote: window.__harness.quote(),
    err: window.__harness.quoteError(),
    alerts: [...document.querySelectorAll('#quoteAlerts .qAlert')].map(a => a.textContent),
    headlineHidden: document.querySelector('#quoteHeadline').hidden,
    blocks: document.querySelectorAll('.qArea').length,
    body: document.querySelector('.qArea .qAreaBody').textContent,
  }));
  assert.equal(st.quote, null, 'no quote at all, rather than a partial one');
  assert.match(st.err, /crew\/hours/, 'the engine names the missing input');
  assert.ok(st.alerts.some(a => a.includes('cannot be priced')), 'and the screen repeats it');
  assert.equal(st.headlineHidden, true, 'no profit headline beside an unpriceable job');
  assert.equal(st.blocks, 1, 'the area is still listed');
  assert.match(st.body, /Not priced/, 'and says so in place of a price');
});

await run('quote view: an unassigned area is listed and named, never dropped', async page => {
  await drawArea(page);                       // Area 1 — left unassigned
  await setTool(page, 'area');
  for (const [x, y] of [[600, 250], [660, 250], [660, 290], [600, 290]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const names = await page.$$eval('.measRow .measName', ns => ns.map(n => n.textContent));
  const pickers = await page.$$('.measRow .measSystem');
  await pickers[names.findIndex(n => n.includes('Area 2'))].select('epoxy');

  await gotoQuote(page);
  const st = await page.evaluate(() => ({
    alerts: [...document.querySelectorAll('#quoteAlerts .qAlert')].map(a => a.textContent),
    blocks: [...document.querySelectorAll('.qArea')].map(b => ({
      meta: b.querySelector('.qAreaMeta').textContent,
      cost: b.querySelector('.qAreaCost').textContent,
      body: b.querySelector('.qAreaBody') ? b.querySelector('.qAreaBody').textContent : '',
    })),
  }));
  assert.ok(st.alerts.some(a => /no system assigned/.test(a) && /Area 1/.test(a)),
    'the unassigned area is named in an alert');
  assert.equal(st.blocks.length, 2, 'both areas listed');
  const un = st.blocks.find(b => b.meta.startsWith('Unassigned'));
  assert.ok(un, 'the unassigned one is labelled as such');
  assert.equal(un.cost, '—', 'no cost invented for it');
  assert.match(un.body, /assign a system/, 'and it says what would fix that');
});

await run('quote view: consumables a system never uses hide behind a toggle, at $0.00', async page => {
  await priceOneArea(page, 'seal');
  const before = await page.evaluate(() => {
    const lines = window.__harness.quote().areas[0].lines;
    return {
      engineLines: lines.length,
      unused: lines.filter(l => l.source.kind === 'consumable' && l.multiplier === 0).length,
      rows: document.querySelectorAll('.qArea table.qLines tbody tr').length,
      toggle: document.querySelector('.qUnusedToggle').textContent,
    };
  });
  assert.equal(before.unused, 4, 'seal zeroes four of the eleven consumables');
  assert.equal(before.rows, before.engineLines - 4, 'which are hidden by default');
  assert.match(before.toggle, /^show 4 items not used/);

  await page.click('.qUnusedToggle');
  const after = await page.evaluate(() => ({
    toggle: document.querySelector('.qUnusedToggle').textContent,
    rows: [...document.querySelectorAll('.qArea table.qLines tbody tr')].map(r => ({
      cls: r.className,
      ext: r.children[4].textContent,
      readOnly: r.querySelector('input.qQty').readOnly,
      actions: r.children[5].children.length,
    })),
  }));
  assert.equal(after.rows.length, before.engineLines, 'every reported line is now on screen');
  assert.match(after.toggle, /^hide 4 items/);
  const unused = after.rows.filter(r => r.cls.includes('unused'));
  assert.equal(unused.length, 4);
  for (const r of unused) {
    assert.equal(r.ext, '$0.00', 'shown at nothing, not hidden');
    assert.equal(r.readOnly, true, 'not editable — every factor on zero is zero');
    assert.equal(r.actions, 0, 'and nothing to remove: it is a catalog fact');
  }
});

await run('quote view: removing a line keeps it visible at $0.00 with an undo', async page => {
  await priceOneArea(page, 'epoxy');
  const first = await page.evaluate(() => {
    const q = window.__harness.quote();
    return { id: q.areas[0].lines[0].product_id, cost: q.cost };
  });
  await page.click(`table.qLines tbody tr[data-product-id="${first.id}"] .qRowAct`);
  await page.waitForFunction(
    id => window.__harness.quote().areas[0].lines.find(l => l.product_id === id).suppressed,
    { timeout: 5_000 }, first.id);

  const after = await page.evaluate(id => {
    const row = document.querySelector(`tr[data-product-id="${id}"]`);
    return {
      present: !!row,
      cls: row.className,
      ext: row.children[4].textContent,
      act: row.querySelector('.qRowAct').textContent,
      cost: window.__harness.quote().cost,
      stored: window.__harness.measurements()[0].quantityOverrides,
    };
  }, first.id);
  assert.ok(after.present, 'the line stays on screen');
  assert.match(after.cls, /suppressed/, 'struck through rather than deleted');
  assert.equal(after.ext, '$0.00');
  assert.equal(after.act, '↺', 'with an undo where the remove was');
  assert.ok(after.cost < first.cost, 'and the job costs less than it did');
  assert.equal(after.stored[first.id], 0, 'stored as a per-job factor of 0');

  await page.click(`table.qLines tbody tr[data-product-id="${first.id}"] .qRowAct`);
  await page.waitForFunction(
    id => !window.__harness.quote().areas[0].lines.find(l => l.product_id === id).suppressed,
    { timeout: 5_000 }, first.id);
  const restored = await page.evaluate(() => ({
    cost: window.__harness.quote().cost,
    stored: window.__harness.measurements()[0].quantityOverrides,
  }));
  assert.ok(Math.abs(restored.cost - first.cost) < 0.005, 'undo restores the original cost');
  assert.equal(restored.stored[first.id], undefined, 'and clears the override entirely');
});

await run('quote view: margin re-prices through the engine and flags the floor', async page => {
  await priceOneArea(page, 'epoxy');
  const at35 = await page.evaluate(() => ({
    price: window.__harness.quote().price,
    below: window.__harness.quote().below_margin_floor,
    warnHidden: document.querySelector('#qhFloorWarn').hidden,
  }));
  assert.equal(at35.below, false, '35% clears the floor');
  assert.equal(at35.warnHidden, true, 'nothing to warn about');

  await typeQuote(page, '#qjMarginPct', '10');
  await page.waitForFunction(() => Math.abs(window.__harness.jobPricing().margin - 0.10) < 1e-9,
    { timeout: 5_000 });
  const low = await page.evaluate(() => {
    const q = window.__harness.quote();
    return {
      price: q.price, below: q.below_margin_floor,
      warnHidden: document.querySelector('#qhFloorWarn').hidden,
      domMargin: document.querySelector('#qhMarginVal').textContent,
      flagged: document.querySelector('#qhMarginBox').className.includes('below'),
    };
  });
  assert.ok(low.price < at35.price, 'a thinner margin prices lower');
  assert.equal(low.below, true, 'the ENGINE owns the 20% floor, not this screen');
  assert.equal(low.warnHidden, false, 'and the screen surfaces its flag');
  assert.ok(low.flagged, 'margin figure marked');
  assert.equal(low.domMargin, '10.0%');
});

await run('quote view: crew and hours survive the re-render they trigger', async page => {
  // Each keystroke re-prices and rebuilds the whole view; without focus
  // restoration by data-qkey the second digit lands somewhere else entirely.
  await drawArea(page);
  await pickSystem(page, 'epoxy');
  await gotoQuote(page);
  await typeQuote(page, '.qArea input[data-qkey^="crew-"]', '3');
  await typeQuote(page, '.qArea input[data-qkey^="hours-"]', '12');
  await page.waitForFunction(() => window.__harness.quote() !== null, { timeout: 5_000 });
  const st = await page.evaluate(() => ({
    labor: window.__harness.measurements()[0].labor,
    manHours: window.__harness.quote().areas[0].man_hours,
    focusKey: document.activeElement.dataset.qkey,
    hoursVal: document.querySelector('.qArea input[data-qkey^="hours-"]').value,
  }));
  assert.deepEqual(st.labor, { crew: 3, hours: 12 }, 'both digits landed in the same field');
  assert.equal(st.hoursVal, '12');
  assert.equal(st.manHours, 36, '3 × 12 man-hours reach the engine');
  assert.match(st.focusKey, /^hours-/, 'focus stayed on the field being typed into');
});

await run('quote view: empty takeoff shows an empty state, not a bare table', async page => {
  await gotoQuote(page);
  const view = await page.evaluate(() => ({
    empty: document.querySelector('#quoteEmpty') ? document.querySelector('#quoteEmpty').textContent : null,
    blocks: document.querySelectorAll('.qArea').length,
    headlineHidden: document.querySelector('#quoteHeadline').hidden,
    jobLevelHidden: document.querySelector('#quoteJobLevel').hidden,
    items: window.__harness.lastQuotePayload().items.length,
  }));
  assert.equal(view.items, 0);
  assert.match(view.empty, /draw an area/, 'empty state explains what to do');
  assert.equal(view.blocks, 0, 'no area blocks');
  assert.ok(view.headlineHidden, 'no headline with nothing to price');
  assert.ok(view.jobLevelHidden, 'and no job-level costs either');
});

// ---- hours plausibility ----
//
// One man-hour is $82.59 fully loaded, which makes hours the most leveraged
// input in the model by an order of magnitude. A real quote came out roughly
// 2.3x high on an hour figure nothing on screen questioned.

await run('quote view: the hours readout is the engine\'s, not a second multiplication', async page => {
  await priceOneArea(page, 'epoxy');
  const seen = await page.evaluate(() => {
    const a = window.__harness.quote().areas[0];
    const box = document.querySelector('.qArea .qProductivity');
    return {
      manHours: a.man_hours,
      sfPerMh: a.sf_per_man_hour,
      areaSf: a.area_sf,
      text: box.textContent,
      band: JSON.parse(window.__harness.productivityBand()),
    };
  });
  // 100 SF at 2 crew x 8 h = 16 man-hours = 6.25 SF/man-hour.
  assert.equal(seen.manHours, 16, 'engine man-hours');
  assert.ok(Math.abs(seen.sfPerMh - seen.areaSf / seen.manHours) < 1e-9,
    'the engine divides, the screen does not');
  assert.match(seen.text, /16\s*man-hours/, 'man-hours rendered from the engine figure');
  // Derived from the engine's own figure -- the square's SF depends on the
  // page scale in force, and hardcoding it would test the fixture, not the
  // readout.
  const ratio = String(Number(seen.sfPerMh.toFixed(4)));
  assert.ok(seen.text.includes(ratio),
    `the ratio belongs beside it: expected ${ratio} in ${JSON.stringify(seen.text)}`);
  // The median is ALWAYS shown, not only when the advisory fires: the real
  // 2.3x-high quote sat inside the band, and what catches that is seeing the
  // number next to its reference.
  assert.ok(seen.text.includes(String(seen.band.typical)),
    `the historical median belongs on screen — got ${seen.text}`);
  assert.equal(seen.band.typical, 22, 'band comes from engine constants');
});

await run('quote view: an implausible hour figure is flagged quietly and still prices', async page => {
  await priceOneArea(page, 'epoxy');   // 6.25 SF/mh — below the 8 floor
  const st = await page.evaluate(() => {
    const box = document.querySelector('.qArea .qProductivity');
    return {
      flag: window.__harness.quote().areas[0].productivity,
      outside: box.classList.contains('outside'),
      text: box.textContent,
      priced: window.__harness.quote().price,
      alerts: [...document.querySelectorAll('#quoteAlerts .qAlert')].map(a => a.textContent),
      // The sub-margin-floor warning is the loud one; this must not borrow it.
      borrowsFloorTreatment: box.className.includes('qhWarn') || box.className.includes('below'),
    };
  });
  assert.equal(st.flag, 'low', 'the ENGINE owns the band, the screen reads the flag');
  assert.ok(st.outside, 'and marks the readout');
  assert.match(st.text, /unusual/, 'says so in words');
  assert.ok(st.priced > 0, 'advisory only — the job still prices');
  assert.ok(!st.alerts.some(a => /hour|productiv|SF\//i.test(a)),
    `an implausible hour figure stays a per-area hint — got ${JSON.stringify(st.alerts)}`);
  assert.ok(!st.borrowsFloorTreatment, 'quieter than the margin-floor warning');
});

await run('quote view: no hours means no ratio, never Infinity', async page => {
  // Hours are routinely half-entered. Infinity or NaN beside a dollar figure
  // reads as a broken quote rather than an unfinished one.
  await drawArea(page);
  await pickSystem(page, 'epoxy');
  await gotoQuote(page);
  const st = await page.evaluate(() => ({
    quote: window.__harness.quote(),
    readout: document.querySelector('.qArea .qProductivity').textContent,
    screen: document.querySelector('#quoteView').textContent,
  }));
  assert.equal(st.quote, null, 'the engine refuses to price without hours');
  assert.match(st.readout, /—/, 'blank readout rather than a stale or invented ratio');
  assert.ok(!/Infinity|NaN/.test(st.screen), `no Infinity/NaN on screen: ${st.readout}`);
});

await run('quote view: job level blends man-hours across areas rather than averaging', async page => {
  // A 200 SF closet and a 20,000 SF warehouse are not equal votes on how fast
  // a job runs, so the job figure is total SF over total hours.
  await priceOneArea(page, 'epoxy');            // 100 SF, 16 mh
  await gotoTakeoff(page);
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of [[600, 250], [660, 250], [660, 290], [600, 290]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  const pickers = await page.$$('.measRow .measSystem');
  assert.equal(pickers.length, 2, 'both areas committed and offer a system picker');
  await pickers[pickers.length - 1].select('epoxy');

  await gotoQuote(page);
  await page.waitForFunction(
    () => document.querySelectorAll('.qArea input[data-qkey^="crew-"]').length === 2,
    { timeout: 5_000 });
  const crews = await page.$$('.qArea input[data-qkey^="crew-"]');
  // Deliberately SLOW: a small area soaking up more hours than the big one,
  // so the blended figure and the per-area mean cannot coincide by accident.
  await crews[1].click({ clickCount: 3 }); await page.keyboard.type('1', { delay: 20 });
  const hours2 = await page.$$('.qArea input[data-qkey^="hours-"]');
  await hours2[1].click({ clickCount: 3 }); await page.keyboard.type('40', { delay: 20 });
  await page.waitForFunction(() => window.__harness.quote() !== null
    && window.__harness.quote().areas.length === 2, { timeout: 5_000 });

  const st = await page.evaluate(() => {
    const q = window.__harness.quote();
    return {
      totalMh: q.man_hours,
      totalSf: q.area_sf,
      blended: q.sf_per_man_hour,
      perArea: q.areas.map(a => a.sf_per_man_hour),
      domMh: document.querySelector('#qjManHours').textContent,
      domBlend: document.querySelector('#qjSfPerMh').textContent,
    };
  });
  assert.equal(st.totalMh, 56, '16 + 40 man-hours');
  assert.ok(Math.abs(st.blended - st.totalSf / st.totalMh) < 1e-9, 'total over total');
  // The big fast area and the small slow one are NOT equal votes: the mean
  // flatters the job, the blend reports it.
  const mean = (st.perArea[0] + st.perArea[1]) / 2;
  assert.ok(Math.abs(st.blended - mean) / st.blended > 0.2,
    `blended ${st.blended} must not be the mean ${mean} of ${JSON.stringify(st.perArea)}`);
  assert.ok(st.blended < mean, 'the slow area drags the real figure down');
  assert.equal(st.domMh, String(st.totalMh), 'job man-hours on screen come from the engine total');
  assert.ok(!/Infinity|NaN|—/.test(st.domBlend), `blended figure on screen — got ${st.domBlend}`);
});

// ---- grit hook ----
//
// Wired end to end and completely inert: every multiplier on the shipped card
// is seeded at 1.0 until Jordan has timing data to populate it.

await run('quote: grit is offered only on systems that grind', async page => {
  await priceOneArea(page, 'polish');
  const polish = await page.evaluate(() => ({
    picker: !!document.querySelector('.qArea select[data-qkey^="grit-"]'),
    options: [...document.querySelectorAll('.qArea select[data-qkey^="grit-"] option')]
      .map(o => o.value),
  }));
  assert.ok(polish.picker, 'polish grinds, so a grit may be specified');
  assert.equal(polish.options[0], '', 'defaults to the system standard, not a guess');
  // Populated FROM THE RATE CARD — adding 1200 grit is a data edit, not a
  // code change, so the list must match the card rather than a literal.
  const card = await page.evaluate(() => window.__harness.rateCard().grit_levels.map(g => g.key));
  assert.deepEqual(polish.options.slice(1), card, 'options come from the card ladder');

  await gotoTakeoff(page);
  await pickSystem(page, 'epoxy');
  await gotoQuote(page);
  const epoxy = await page.evaluate(() =>
    !!document.querySelector('.qArea select[data-qkey^="grit-"]'));
  assert.equal(epoxy, false, 'epoxy has no standard grit, so no grit may be chosen on it');
});

await run('quote: selecting a grit changes no price while the ladder is seeded flat', async page => {
  await priceOneArea(page, 'polish');
  const before = await page.evaluate(() => {
    const q = window.__harness.quote();
    return { cost: q.cost, price: q.price, profit: q.profit, mh: q.man_hours };
  });
  await page.select('.qArea select[data-qkey^="grit-"]', '800');
  await page.waitForFunction(() => window.__harness.measurements()[0].gritLevel === '800',
    { timeout: 5_000 });
  const after = await page.evaluate(() => {
    const q = window.__harness.quote();
    return {
      cost: q.cost, price: q.price, profit: q.profit, mh: q.man_hours,
      mult: q.areas[0].grit_labor_multiplier,
      grit: q.areas[0].grit_level,
      base: q.areas[0].base_man_hours,
    };
  });
  assert.deepEqual(
    [after.cost, after.price, after.profit, after.mh],
    [before.cost, before.price, before.profit, before.mh],
    'the hook is inert at seed values — not one cent moves',
  );
  assert.equal(after.mult, 1, 'identity multiplier');
  assert.equal(after.grit, '800', 'but the selection did reach the engine');
  assert.equal(after.base, after.mh, 'entered hours and priced hours agree at 1.0x');
});

await run('quote: grit selection persists through reload', async page => {
  await priceOneArea(page, 'polish');
  await page.select('.qArea select[data-qkey^="grit-"]', '1200');
  await page.waitForFunction(() => window.__harness.measurements()[0].gritLevel === '1200',
    { timeout: 5_000 });
  await page.evaluate(() => window.__harness.flushSave());
  await page.reload();
  await waitReady(page);
  assert.equal(await page.evaluate(() => window.__harness.measurements()[0].gritLevel), '1200',
    'survives the IndexedDB round-trip');

  const json = await page.evaluate(() => window.__harness.exportJson());
  assert.match(json, /"gritLevel": "1200"/, 'and is in the JSON backup');
});

await run('quote: a grit left over from another system never poisons the quote', async page => {
  // Switching polish -> epoxy strands the grit key on the measurement. The
  // engine ERRORS on a grit it cannot apply, so sending it would take down
  // the whole quote over a field the user can no longer even see.
  await priceOneArea(page, 'polish');
  await page.select('.qArea select[data-qkey^="grit-"]', '800');
  await page.waitForFunction(() => window.__harness.measurements()[0].gritLevel === '800',
    { timeout: 5_000 });
  await gotoTakeoff(page);
  await pickSystem(page, 'epoxy');
  await gotoQuote(page);
  const st = await page.evaluate(() => ({
    stored: window.__harness.measurements()[0].gritLevel,
    sent: window.__harness.jobInput().areas[0].grit_level,
    quote: window.__harness.quote(),
    err: window.__harness.quoteError(),
  }));
  assert.equal(st.stored, '800', 'the selection is remembered for a switch back');
  assert.equal(st.sent, null, 'but never sent to a system that cannot use it');
  assert.equal(st.err, null, 'so the quote does not fail');
  assert.ok(st.quote && st.quote.price > 0, 'and still prices');
});

// ---- bid items: the grouping layer ----
//
// An area is a takeoff unit; a bid item is a line on the proposal. Grouping
// sits ABOVE pricing and never changes how an area is priced.

/// Click the nth match, resolving it immediately beforehand. Every edit on
/// the quote screen re-prices and rebuilds the view, which detaches any handle
/// resolved before the previous edit -- so a handle may never outlive one.
async function clickNth(page, selector, i) {
  const els = await page.$$(selector);
  assert.ok(els[i], `no element ${i} for ${selector} (found ${els.length})`);
  await els[i].click();
}
async function typeNth(page, selector, i, text) {
  const els = await page.$$(selector);
  assert.ok(els[i], `no element ${i} for ${selector} (found ${els.length})`);
  await els[i].click({ clickCount: 3 });
  await page.keyboard.type(text, { delay: 20 });
}

/// Bid item blocks on screen, in presentation order.
const bidBlocks = page => page.$$eval('.qBidItem', bs => bs.map(b => ({
  id: Number(b.dataset.bidItemId),
  name: b.querySelector('.qBidName').value,
  sum: b.querySelector('.qBidSum').textContent,
  alternate: b.classList.contains('alternate'),
  members: b.querySelectorAll('.qArea').length,
})));

/// Two priced areas on DIFFERENT pages, same system — the case bid items
/// exist for. Returns nothing; leaves the page on #/quote.
async function twoPagesPriced(page, system) {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  await gotoPage(page, +1);
  // The deep-linked fpi seeds page 1 ONLY; page 2 guards until it has a scale
  // of its own, so give it one before drawing.
  await openScaleTab(page);
  await page.select('#preset', '20');
  await page.click('.ribbonTab[data-tab="tools"]');
  await setTool(page, 'area');
  for (const [x, y] of [[600, 250], [660, 250], [660, 290], [600, 290]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  // Both rows are only visible together in the all-pages view.
  await page.evaluate(() => { const c = document.querySelector('#allPages'); if (!c.checked) c.click(); });
  const pickers = await page.$$('.measRow .measSystem');
  assert.equal(pickers.length, 2, 'two areas, one per page');
  for (const p of pickers) await p.select(system);
  await gotoQuote(page);
  await page.waitForFunction(
    () => document.querySelectorAll('.qArea input[data-qkey^="crew-"]').length === 2,
    { timeout: 5_000 });
  for (const i of [0, 1]) {
    await typeNth(page, '.qArea input[data-qkey^="crew-"]', i, '2');
    await typeNth(page, '.qArea input[data-qkey^="hours-"]', i, '8');
  }
  await page.waitForFunction(
    () => window.__harness.quote() !== null && window.__harness.quote().areas.length === 2,
    { timeout: 5_000 });
}

await run('bid items: every area gets one, named from the area, by default', async page => {
  // The compatibility guarantee. A project that has never been grouped -- and
  // every project saved before this layer existed -- looks exactly like this.
  await priceOneArea(page, 'epoxy');
  const items = await page.evaluate(() => window.__harness.bidItems());
  const meas = await page.evaluate(() => window.__harness.measurements());
  assert.equal(items.length, 1, 'one item per area');
  assert.deepEqual(items[0].areaIds, [meas[0].id]);
  assert.equal(items[0].name, meas[0].name, 'named from the area');
  assert.equal(items[0].alternate, false);

  // And the lump sum IS the area's price -- grouping changed nothing.
  const q = await page.evaluate(() => window.__harness.quote());
  assert.equal(q.bid_items.length, 1);
  assert.ok(Math.abs(q.bid_items[0].cost - q.areas[0].cost) < 0.005,
    'a single-member item costs exactly its area');
  assert.ok(Math.abs(q.bid_items[0].price + q.job_cost_price - q.price) < 0.005,
    'and its lump sum plus job costs is the quoted price');
});

await run('bid items: two areas from different pages group into one line at one price', async page => {
  await twoPagesPriced(page, 'polish');
  const before = await page.evaluate(() => {
    const q = window.__harness.quote();
    return { price: q.price, items: q.bid_items.length, pages: window.__harness.measurements().map(m => m.page) };
  });
  assert.equal(before.items, 2, 'ungrouped to start');
  assert.notEqual(before.pages[0], before.pages[1], 'the areas really are on different pages');

  for (const i of [0, 1]) await clickNth(page, '.qBidItem input[data-qkey^="pick-"]', i);
  await page.waitForFunction(() => !document.querySelector('#quoteGroupBtn').disabled, { timeout: 5_000 });
  await page.click('#quoteGroupBtn');
  await page.waitForFunction(() => window.__harness.bidItems().length === 1, { timeout: 5_000 });

  const after = await page.evaluate(() => window.__harness.quote());
  const blocks = await bidBlocks(page);
  assert.equal(blocks.length, 1, 'one line of work now');
  assert.equal(blocks[0].members, 2, 'with both areas still expandable inside it');
  assert.deepEqual((await page.evaluate(() => window.__harness.bidItems()))[0].areaIds.length, 2);
  // A bid item is NOT page-scoped, and grouping is a presentation act: the
  // money must not move.
  assert.ok(Math.abs(after.price - before.price) < 0.005,
    `grouping must not reprice: ${before.price} -> ${after.price}`);
  assert.ok(Math.abs(after.bid_items[0].cost - after.areas.reduce((s, a) => s + a.cost, 0)) < 0.005,
    'the lump sum is the sum of its members');
});

await run('bid items: ungroup returns each area to its own line', async page => {
  await twoPagesPriced(page, 'polish');
  for (const i of [0, 1]) await clickNth(page, '.qBidItem input[data-qkey^="pick-"]', i);
  await page.click('#quoteGroupBtn');
  await page.waitForFunction(() => window.__harness.bidItems().length === 1, { timeout: 5_000 });
  const grouped = await page.evaluate(() => window.__harness.quote().price);

  await page.click('.qBidItem button[data-qkey^="ungroup-"]');
  await page.waitForFunction(() => window.__harness.bidItems().length === 2, { timeout: 5_000 });
  const items = await page.evaluate(() => window.__harness.bidItems());
  const meas = await page.evaluate(() => window.__harness.measurements());
  assert.deepEqual(items.map(i => i.areaIds.length), [1, 1], 'one area each');
  assert.deepEqual(items.map(i => i.name).sort(), meas.map(m => m.name).sort(),
    're-named from their areas');
  assert.ok(Math.abs(await page.evaluate(() => window.__harness.quote().price) - grouped) < 0.005,
    'and ungrouping does not reprice either');
});

await run('bid items: grouping areas on different systems is refused, with a reason', async page => {
  // One lump sum carries one scope narrative. Refusing in the UI matters: the
  // engine would reject it too, but that would blank the whole quote instead
  // of explaining one bad grouping.
  await twoPagesPriced(page, 'polish');
  const pickers = await page.$$('.qArea select[data-qkey^="sys-"]');
  await pickers[1].select('epoxy');
  await page.waitForFunction(
    () => window.__harness.measurements().filter(m => m.systemType === 'epoxy').length === 1,
    { timeout: 5_000 });

  for (const i of [0, 1]) await clickNth(page, '.qBidItem input[data-qkey^="pick-"]', i);
  await page.click('#quoteGroupBtn');
  await page.waitForFunction(() => window.__harness.groupError() !== null, { timeout: 5_000 });

  const st = await page.evaluate(() => ({
    err: window.__harness.groupError(),
    items: window.__harness.bidItems().length,
    quote: window.__harness.quote(),
    shown: document.querySelector('#quoteGroupError').textContent,
  }));
  assert.equal(st.items, 2, 'nothing was merged');
  assert.match(st.err, /same system/, 'and it says why');
  assert.match(st.shown, /Polish|Epoxy/, 'naming the systems involved');
  assert.ok(st.quote && st.quote.price > 0, 'the quote is untouched, not blanked');
});

await run('bid items: renaming a line survives reload and does not reprice', async page => {
  await priceOneArea(page, 'epoxy');
  const before = await page.evaluate(() => window.__harness.quote().price);
  await typeQuote(page, '.qBidItem input[data-qkey^="name-"]', 'Front Showroom');
  await page.waitForFunction(() => window.__harness.bidItems()[0].name === 'Front Showroom',
    { timeout: 5_000 });
  assert.ok(Math.abs(await page.evaluate(() => window.__harness.quote().price) - before) < 0.005,
    'naming work is not pricing it');

  await page.evaluate(() => window.__harness.flushSave());
  await page.reload();
  await waitReady(page);
  assert.equal((await page.evaluate(() => window.__harness.bidItems()))[0].name, 'Front Showroom',
    'survives the IndexedDB round-trip');
  const json = await page.evaluate(() => window.__harness.exportJson());
  assert.match(json, /"name": "Front Showroom"/, 'and is in the JSON backup');
});

await run('bid items: flagging an alternate drops the base bid by exactly its price', async page => {
  await twoPagesPriced(page, 'polish');
  const before = await page.evaluate(() => {
    const q = window.__harness.quote();
    return { price: q.price, profit: q.profit, items: q.bid_items.map(b => ({ id: b.id, price: b.price })) };
  });

  await clickNth(page, '.qBidItem input[data-qkey^="alt-"]', 1);
  await page.waitForFunction(() => window.__harness.quote().alternate_price > 0, { timeout: 5_000 });

  const after = await page.evaluate(() => {
    const q = window.__harness.quote();
    return {
      price: q.price, altPrice: q.alternate_price,
      alt: q.bid_items.find(b => b.alternate),
      shown: document.querySelector('#quoteAlternates').hidden,
      rows: [...document.querySelectorAll('.qAltRow')].map(r => r.textContent),
      all: document.querySelector('#qAltAll').textContent,
      scopeHidden: document.querySelector('#qhScope').hidden,
      headline: document.querySelector('#qhPriceVal').textContent,
    };
  });
  const wasWorth = before.items.find(i => i.id === after.alt.id).price;
  assert.ok(Math.abs(after.price - (before.price - wasWorth)) < 0.005,
    `base bid must drop by exactly the alternate's lump sum: ${before.price} - ${wasWorth} != ${after.price}`);
  assert.ok(Math.abs(after.altPrice - wasWorth) < 0.005, 'and it is reported apart');
  assert.equal(after.shown, false, 'alternates are listed separately');
  assert.ok(after.rows.some(r => r.includes('+')), 'shown as an addition, not a subtotal');
  assert.equal(after.scopeHidden, false, 'the headline says which figure it is');
  // The headline is the BASE bid, never the two silently summed.
  assert.ok(!after.headline.includes(String(Math.round(before.price))),
    'the headline is no longer the everything-included figure');
});

await run('bid items: an alternate is still fully priced, at the same margin', async page => {
  await twoPagesPriced(page, 'polish');
  await clickNth(page, '.qBidItem input[data-qkey^="alt-"]', 1);
  await page.waitForFunction(() => window.__harness.quote().alternate_price > 0, { timeout: 5_000 });
  const q = await page.evaluate(() => window.__harness.quote());
  const a = q.bid_items.find(b => b.alternate);
  assert.ok(a.cost > 0 && a.price > 0, 'priced in full, not stubbed');
  assert.ok(Math.abs(a.profit / a.price - q.margin) < 1e-9, 'same margin as the base bid');
  // Its area still carries its own itemization on screen.
  assert.ok(await page.$('.qBidItem.alternate .qArea table.qLines'), 'itemization intact');
});

await run('bid items: deleting a member area leaves no dangling reference', async page => {
  await twoPagesPriced(page, 'polish');
  for (const i of [0, 1]) await clickNth(page, '.qBidItem input[data-qkey^="pick-"]', i);
  await page.click('#quoteGroupBtn');
  await page.waitForFunction(() => window.__harness.bidItems().length === 1, { timeout: 5_000 });

  // Delete one member from the takeoff, where the delete control lives.
  await gotoTakeoff(page);
  const doomed = await page.evaluate(() => window.__harness.measurements()[0].id);
  // The row controls are hover-revealed (display:none until :hover), so the
  // row has to be hovered before its delete button is a clickable element.
  await page.hover('.measRow');
  await page.click('.measRow .delToggle');
  await page.waitForFunction(id => !window.__harness.measurements().some(m => m.id === id),
    { timeout: 5_000 }, doomed);

  const items = await page.evaluate(() => window.__harness.bidItems());
  assert.equal(items.length, 1, 'the item survives its surviving member');
  assert.equal(items[0].areaIds.length, 1, 'and drops the dead reference');
  assert.ok(!items[0].areaIds.includes(doomed));

  await gotoQuote(page);
  const q = await page.evaluate(() => window.__harness.quote());
  assert.ok(q && q.price > 0, 'and the quote still prices rather than erroring');

  // Delete the LAST member: the item goes with it. An item is a name for some
  // work; with no work under it, it is not an empty item, it is not an item.
  await gotoTakeoff(page);
  await page.hover('.measRow');
  await page.click('.measRow .delToggle');
  await page.waitForFunction(() => window.__harness.measurements().length === 0, { timeout: 5_000 });
  assert.equal((await page.evaluate(() => window.__harness.bidItems())).length, 0,
    'no orphaned bid item left behind');
});

await run('bid items: a legacy project loads with one item per area', async page => {
  // Simulates a project saved before bid items existed: the stored record has
  // no bidItems key at all.
  await priceOneArea(page, 'epoxy');
  const json = await page.evaluate(() => window.__harness.exportJson());
  const legacy = JSON.parse(json);
  delete legacy.bidItems;
  assert.ok(!('bidItems' in legacy), 'the backup now looks pre-bid-item');

  await page.evaluate(() => window.__harness.deleteProject(window.__harness.currentSha()));
  // priceOneArea left us on #/quote, and the hash survives a reload -- come
  // back to takeoff so the Quote button is reachable again.
  await page.evaluate(() => { location.hash = '#/takeoff'; });
  await page.reload();
  await waitReady(page);
  await page.evaluate(j => window.__harness.importJson(j), JSON.stringify(legacy));

  const items = await page.evaluate(() => window.__harness.bidItems());
  const meas = await page.evaluate(() => window.__harness.measurements());
  assert.equal(items.length, 1, 'one item per area, synthesized on load');
  assert.deepEqual(items[0].areaIds, [meas[0].id]);
  assert.equal(items[0].name, meas[0].name, 'named from the area');
  await gotoQuote(page);
  assert.ok(await page.evaluate(() => window.__harness.quote().price) > 0,
    'and it prices without a migration step');
});

await run('quote: entering the route cancels an in-progress draft', async page => {
  await setTool(page, 'area');
  await snapOff(page);
  for (const [x, y] of SQ.slice(0, 2)) await clickBase(page, x, y);
  assert.equal((await state(page)).draftVerts, 2, 'draft in progress');
  await gotoQuote(page);
  assert.equal(await page.evaluate(() => window.__harness.draft()), null, 'draft cancelled');
});

await run('quote: a deep link onto #/quote boots into it with restored data', async page => {
  // The boot path must honour the hash rather than always landing on
  // takeoff — AND the view must pick up the deep-linked PDF, which finishes
  // loading after the first render.
  await drawQuoteFixture(page);
  await page.evaluate(() => window.__harness.flushSave());
  await page.evaluate(() => { location.hash = '#/quote'; });
  await page.reload();
  await waitReady(page);
  const st = await routeState(page);
  assert.equal(st.route, 'quote');
  assert.ok(st.quoteShown && !st.takeoffShown, 'boots directly into the quote view');
  assert.equal(await areaBlocks(page), 1, 'restored measurements priced, not an empty first pass');
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
  await openEngineDrawer(page);
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
  // A canvas edit attempt while blocked does nothing (guard + overlay). The
  // modal is viewport-centered, so probe a corner point — a center point
  // would land on the modal's own buttons rather than the canvas underneath.
  await clickBase(page, 20, 20);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 1, 'no new measurement while blocked');
  // Dismiss (the modal button is on top) → unblocked, edits resume.
  await page.click('#storageDismiss');
  assert.equal(await page.evaluate(() => window.__harness.storageBlocked()), false);
  for (const [x, y] of [[600, 300], [700, 300], [700, 400], [600, 400]]) await clickBase(page, x, y);
  await page.keyboard.press('Enter');
  assert.equal((await state(page)).meas.length, 2, 'edits resume after acknowledge');
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
  await openScaleTab(page);
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
