// Generates a minimal synthetic vector PDF (one 792×612 page) with known
// stroked geometry, so interaction tests exercise the REAL extraction path
// (extract.js → PageGeometry) without any confidential fixture.
//
// Geometry (PDF user space, y-up; the scale-1 viewport flip puts it in
// top-left base units as y' = 612 − y):
//   wall run  : 3 collinear 3.6 pt segments at y=212 (base y' 400),
//               x 100–250 / 252–400 / 402–550 (2 pt gaps → chain bridges)
//   room rect : 3.6 pt outline, x 100–300, y 312–462 (base y' 150–300)
//   hairline  : 0.24 pt dimension line inside the room
//
// At deep-link fpi 7.2, 10 pts = 1 ft: wall run = 450 pts = 45.0 LF.
//
// Also included: a Form XObject with a translation /Matrix drawing one
// 3.6-pt segment — form space (0,50)-(100,50), matrix [1 0 0 1 60 -40]
// → user (60,10)-(160,10) → TOP-LEFT base units (60,602)-(160,602).
// Regression coverage for the extract.js form-matrix CTM fix.

export function makeWallsPdf() {
  const content = [
    '3.6 w',
    '100 212 m 250 212 l S',
    '252 212 m 400 212 l S',
    '402 212 m 550 212 l S',
    '100 312 m 300 312 l S',
    '300 312 m 300 462 l S',
    '300 462 m 100 462 l S',
    '100 462 m 100 312 l S',
    '0.24 w',
    '120 380 m 280 380 l S',
    '/F1 Do',
    // Second room (base units x 600-750, y 150-300) filled with a dense
    // 6-pt-pitch hatch field at wall stroke — the hide-hatch suite case.
    '3.6 w',
    '600 312 m 750 312 l S',
    '750 312 m 750 462 l S',
    '750 462 m 600 462 l S',
    '600 462 m 600 312 l S',
    ...Array.from({ length: 24 }, (_, i) => {
      const y = 318 + i * 6;
      return `603 ${y} m 747 ${y} l S`;
    }),
  ].join('\n');
  const formContent = '3.6 w\n0 50 m 100 50 l S';

  // Page 2 (for page-scoping tests): one plain room, base units
  // x 400-560, y' 252-412, no hatch — distinct from page 1's geometry.
  const page2 = [
    '3.6 w',
    '400 200 m 560 200 l S',
    '560 200 m 560 360 l S',
    '560 360 m 400 360 l S',
    '400 360 m 400 200 l S',
  ].join('\n');

  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Contents 4 0 R '
      + '/Resources << /XObject << /F1 5 0 R >> >> >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
    '<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] '
      + `/Matrix [1 0 0 1 60 -40] /Length ${formContent.length} >>\n`
      + `stream\n${formContent}\nendstream`,
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Contents 7 0 R >>',
    `<< /Length ${page2.length} >>\nstream\n${page2}\nendstream`,
  ];

  let pdf = '%PDF-1.4\n';
  const offsets = [];
  objects.forEach((body, i) => {
    offsets.push(pdf.length);
    pdf += `${i + 1} 0 obj\n${body}\nendobj\n`;
  });
  const xrefStart = pdf.length;
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const off of offsets) {
    pdf += `${String(off).padStart(10, '0')} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;
  return Buffer.from(pdf, 'latin1');
}

/// A minimal, DISTINCT one-page PDF (different bytes → different SHA →
/// separate project) for the content-addressing test.
export function makeOtherPdf() {
  const content = '3.6 w\n200 200 m 400 200 l S';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Contents 4 0 R >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
  ];
  let pdf = '%PDF-1.4\n';
  const offsets = [];
  objects.forEach((body, i) => { offsets.push(pdf.length); pdf += `${i + 1} 0 obj\n${body}\nendobj\n`; });
  const xrefStart = pdf.length;
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const off of offsets) pdf += `${String(off).padStart(10, '0')} 00000 n \n`;
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;
  return Buffer.from(pdf, 'latin1');
}
