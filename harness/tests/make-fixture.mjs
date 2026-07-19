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
  ].join('\n');

  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 792 612] /Contents 4 0 R >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
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
