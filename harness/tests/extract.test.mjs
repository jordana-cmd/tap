// Extraction probe: runs the REAL harness extract.js against the synthetic
// fixture PDF in headless Chromium (same vendored pdf.js the harness uses)
// and asserts exact segment coordinates — including the Form-XObject
// segment, which lands correctly only if extract.js applies /Matrix.
// Run: node extract.test.mjs
import puppeteer from 'puppeteer';
import assert from 'node:assert/strict';
import { startServer } from './server.mjs';

const server = await startServer();
const browser = await puppeteer.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', e => errors.push(String(e)));
await page.goto(`http://127.0.0.1:${server.port}/`);

const res = await page.evaluate(async () => {
  const pdfjs = await import('/vendor/pdfjs/pdf.min.mjs');
  pdfjs.GlobalWorkerOptions.workerSrc = '/vendor/pdfjs/pdf.worker.min.mjs';
  const { extractSegments } = await import('/extract.js');
  const buf = await (await fetch('/test/walls.pdf')).arrayBuffer();
  const doc = await pdfjs.getDocument({ data: new Uint8Array(buf) }).promise;
  const ex = await extractSegments(await doc.getPage(1), pdfjs.OPS);
  return { flat: Array.from(ex.flat), skipped: ex.skipped, pathCount: ex.pathCount };
});

const segs = [];
for (let o = 0; o < res.flat.length; o += 5) {
  segs.push(res.flat.slice(o, o + 5).map(v => +v.toFixed(3)));
}
const has = (x1, y1, x2, y2, w) => segs.some(s =>
  Math.abs(s[0] - x1) < 0.01 && Math.abs(s[1] - y1) < 0.01 &&
  Math.abs(s[2] - x2) < 0.01 && Math.abs(s[3] - y2) < 0.01 &&
  Math.abs(s[4] - w) < 0.01);

let failures = 0;
function check(name, cond) {
  if (cond) console.log(`PASS  ${name}`);
  else { failures++; console.log(`FAIL  ${name}`); }
}

// Direct page geometry unmoved (top-left base units, y' = 612 − y).
check('wall-run segment at (100,400)-(250,400) w3.6', has(100, 400, 250, 400, 3.6));
check('room edge at (100,300)-(300,300) w3.6', has(100, 300, 300, 300, 3.6));
check('hairline at (120,232)-(280,232) w0.24', has(120, 232, 280, 232, 0.24));
// Form-XObject segment: correct ONLY when /Matrix [1 0 0 1 60 -40] is
// applied — lands at (60,602)-(160,602). Without the fix it would sit at
// (0,562)-(100,562).
check('form segment at (60,602)-(160,602) w3.6', has(60, 602, 160, 602, 3.6));
check('un-transformed form position absent', !has(0, 562, 100, 562, 3.6));
check('form ops no longer tallied as skipped',
  !('paintFormXObjectBegin' in res.skipped) && !('paintFormXObjectEnd' in res.skipped));
check('no page errors', errors.length === 0);

await browser.close();
server.close();
if (failures) { console.log(`\n${failures} FAILED`); process.exit(1); }
console.log('\nextraction probe passed');
