// Vector segment extraction from a pdf.js page (addendum §A3.3).
//
// NOTE (§A8 guard): client-side pdf.js — including this operator-list walk —
// is a measurement INSTRUMENT for the throwaway harness only. The product
// extracts vectors server-side during ingest, or on-demand per region; the
// §A8 carve-out permits exactly that, not browser-primary PDF handling.
//
// This module is deliberately DOM-free (pure pdf.js API + typed arrays) so
// the headless Node eval rig imports the SAME code path as the browser
// harness — one extraction implementation, no fork.

/**
 * Walk a page's operator list and return its stroked line segments in
 * top-left base units (PDF points), each with its CTM-corrected stroke
 * width.
 *
 * @param page PDFPageProxy
 * @param OPS  the pdf.js OPS enum (passed in so this module works with the
 *             vendored browser build AND the npm package in Node)
 * @returns {Promise<{flat: Float64Array, pathCount: number,
 *           curveCount: number, skipped: Record<string, number>}>}
 *   flat = [x1, y1, x2, y2, width_pts] × n — the PageGeometry input format.
 */
export async function extractSegments(page, OPS) {
  const opList = await page.getOperatorList();
  // Applies m2 first, then m1 (pdf.js Util.transform convention).
  const mul = (m1, m2) => [
    m1[0] * m2[0] + m1[2] * m2[1],
    m1[1] * m2[0] + m1[3] * m2[1],
    m1[0] * m2[2] + m1[2] * m2[3],
    m1[1] * m2[2] + m1[3] * m2[3],
    m1[0] * m2[4] + m1[2] * m2[5] + m1[4],
    m1[1] * m2[4] + m1[3] * m2[5] + m1[5],
  ];
  const apply = (m, x, y) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];

  // Device space = top-left base units: the scale-1 viewport transform
  // performs the y-flip (and any /Rotate) from PDF user space.
  const vp = page.getViewport({ scale: 1 }).transform;

  const strokeOps = new Set([
    OPS.stroke, OPS.closeStroke, OPS.fillStroke,
    OPS.eoFillStroke, OPS.closeFillStroke, OPS.closeEOFillStroke,
  ]);
  const opName = Object.fromEntries(Object.entries(OPS).map(([k, v]) => [v, k]));
  const handled = new Set([
    OPS.save, OPS.restore, OPS.transform, OPS.setLineWidth, OPS.setGState,
    OPS.constructPath,
    // Style/state ops that cannot affect geometry or width — not "skipped".
    OPS.setStrokeRGBColor, OPS.setFillRGBColor, OPS.setLineCap,
    OPS.setLineJoin, OPS.setMiterLimit, OPS.setDash, OPS.dependency,
  ]);

  let ctm = [1, 0, 0, 1, 0, 0];
  let lineWidth = 1;
  const stack = [];
  const out = [];
  const skipped = {};
  let pathCount = 0;
  let curveCount = 0;

  const { fnArray, argsArray } = opList;
  for (let i = 0; i < fnArray.length; i++) {
    const fn = fnArray[i];
    const args = argsArray[i];
    if (fn === OPS.save) {
      stack.push([ctm, lineWidth]);
    } else if (fn === OPS.restore) {
      if (stack.length) [ctm, lineWidth] = stack.pop();
    } else if (fn === OPS.transform) {
      ctm = mul(ctm, args);
    } else if (fn === OPS.setLineWidth) {
      lineWidth = args[0];
    } else if (fn === OPS.setGState) {
      for (const [key, value] of args[0] ?? []) {
        if (key === 'LW') lineWidth = value;
      }
    } else if (fn === OPS.paintFormXObjectBegin) {
      // pdf.js inlines the form's ops until paintFormXObjectEnd; the form
      // matrix composes into the CTM for that span (previously ignored —
      // a latent misplacement defect, eval-03 queue A.2). BBox clipping
      // is deliberately not applied: this walk collects geometry/widths,
      // not painted pixels. Nested forms work via the ordinary stack.
      stack.push([ctm, lineWidth]);
      // Matrix may be a plain Array OR a Float32Array (pdf.js 6.x).
      if (args?.[0]?.length === 6) {
        ctm = mul(ctm, args[0]);
      }
    } else if (fn === OPS.paintFormXObjectEnd) {
      if (stack.length) [ctm, lineWidth] = stack.pop();
    } else if (fn === OPS.constructPath) {
      // pdf.js 6.x fused form: [paintOp, [Float32Array subpath…], minMax].
      const [paintOp, subpaths] = args;
      if (!strokeOps.has(paintOp)) continue; // fills/clips emit no segments
      pathCount++;
      const m = mul(vp, ctm);
      const widthPts = lineWidth * Math.sqrt(Math.abs(m[0] * m[3] - m[1] * m[2]));
      const emit = (x1, y1, x2, y2) => {
        const [ax, ay] = apply(m, x1, y1);
        const [bx, by] = apply(m, x2, y2);
        if ([ax, ay, bx, by, widthPts].every(Number.isFinite)) {
          out.push(ax, ay, bx, by, widthPts);
        }
      };
      for (const path of subpaths) {
        let cx = 0, cy = 0, sx = 0, sy = 0;
        for (let j = 0; j < path.length;) {
          const op = path[j];
          if (op === 0) {          // moveTo x y — starts a new subpath
            cx = sx = path[j + 1]; cy = sy = path[j + 2]; j += 3;
          } else if (op === 1) {   // lineTo x y
            emit(cx, cy, path[j + 1], path[j + 2]);
            cx = path[j + 1]; cy = path[j + 2]; j += 3;
          } else if (op === 2) {   // curveTo c1x c1y c2x c2y x y — skipped,
            curveCount++;          // but the current point MUST advance or
            cx = path[j + 5];      // every lineTo after an arc anchors wrong
            cy = path[j + 6]; j += 7;
          } else if (op === 3) {   // closePath
            if (cx !== sx || cy !== sy) emit(cx, cy, sx, sy);
            cx = sx; cy = sy; j += 1;
          } else {
            j += 1;                // unknown mini-op: resynchronize
          }
        }
      }
    } else if (!handled.has(fn)) {
      const name = opName[fn] ?? String(fn);
      skipped[name] = (skipped[name] ?? 0) + 1;
    }
  }
  return { flat: Float64Array.from(out), pathCount, curveCount, skipped };
}
