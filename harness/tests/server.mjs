// Dependency-free static server for the interaction suite: serves harness/,
// the rate cards at /data/ (as serve.ps1 does — the quote screen fetches its
// card at runtime and prices nothing without it), plus the generated synthetic
// test PDF at /test/walls.pdf. Ephemeral port.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeWallsPdf, makeOtherPdf } from './make-fixture.mjs';

const HARNESS_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DATA_DIR = path.resolve(HARNESS_DIR, '..', 'data');
const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json',
  '.pdf': 'application/pdf', '.map': 'application/json', '.ts': 'application/typescript',
};

export function startServer() {
  const walls = makeWallsPdf();
  const other = makeOtherPdf();
  const server = http.createServer((req, res) => {
    const urlPath = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (urlPath === '/test/walls.pdf') {
      res.writeHead(200, { 'Content-Type': 'application/pdf' });
      res.end(walls);
      return;
    }
    if (urlPath === '/test/other.pdf') {
      res.writeHead(200, { 'Content-Type': 'application/pdf' });
      res.end(other);
      return;
    }
    const fromData = urlPath.startsWith('/data/');
    const base = fromData ? DATA_DIR : HARNESS_DIR;
    const rel = urlPath === '/' ? 'index.html'
      : fromData ? urlPath.slice('/data/'.length) : urlPath.slice(1);
    const file = path.join(base, rel);
    if (file.startsWith(base) && fs.existsSync(file) && fs.statSync(file).isFile()) {
      res.writeHead(200, {
        'Content-Type': MIME[path.extname(file).toLowerCase()] ?? 'application/octet-stream',
        'Cache-Control': 'no-store',
      });
      res.end(fs.readFileSync(file));
    } else {
      res.writeHead(404);
      res.end('not found');
    }
  });
  return new Promise(resolve => {
    server.listen(0, '127.0.0.1', () => {
      resolve({ port: server.address().port, close: () => server.close() });
    });
  });
}
