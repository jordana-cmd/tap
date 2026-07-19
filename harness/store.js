// IndexedDB persistence for the throwaway harness.
//
// SCOPING (bounded, deliberate deviation — see harness/README.md): the
// PRODUCT rejects IndexedDB per addendum §A8 (OPFS + SQLite-WASM + yrs).
// This module is HARNESS-ONLY — a local measurement instrument that must
// survive a reload today, without waiting on the unbuilt React app. The
// stored shapes mirror §A2 field NAMES (Measurement label/origin/…,
// ScaleState feet_per_paper_inch/source) so migrating to the product
// store is a serialization mapping, not a rewrite. Browser-only module;
// the Node test rig drives it through the page, never imports it.

const DB_NAME = 'takeoff-harness';
const DB_VERSION = 1;

/// Hex SHA-256 of the PDF bytes — the content-addressed project key.
export async function sha256(bytes) {
  const buf = await crypto.subtle.digest('SHA-256',
    bytes instanceof ArrayBuffer ? bytes : bytes.buffer ?? bytes);
  return [...new Uint8Array(buf)].map(b => b.toString(16).padStart(2, '0')).join('');
}

let dbPromise = null;
export function openDb() {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains('files')) db.createObjectStore('files', { keyPath: 'sha' });
      if (!db.objectStoreNames.contains('projects')) db.createObjectStore('projects', { keyPath: 'sha' });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

function tx(db, store, mode, fn) {
  return new Promise((resolve, reject) => {
    const t = db.transaction(store, mode);
    const s = t.objectStore(store);
    let out;
    const r = fn(s);
    if (r) r.onsuccess = () => { out = r.result; };
    t.oncomplete = () => resolve(out);
    t.onerror = () => reject(t.error);
    t.onabort = () => reject(t.error);
  });
}

export async function putFile(rec) {
  return tx(await openDb(), 'files', 'readwrite', s => s.put(rec));
}
export async function getFile(sha) {
  return tx(await openDb(), 'files', 'readonly', s => s.get(sha));
}
export async function putProject(rec) {
  return tx(await openDb(), 'projects', 'readwrite', s => s.put(rec));
}
export async function getProject(sha) {
  return tx(await openDb(), 'projects', 'readonly', s => s.get(sha));
}
export async function listProjects() {
  const all = await tx(await openDb(), 'projects', 'readonly', s => s.getAll());
  return (all ?? []).sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
}
export async function deleteProject(sha) {
  const db = await openDb();
  await tx(db, 'projects', 'readwrite', s => s.delete(sha));
  await tx(db, 'files', 'readwrite', s => s.delete(sha));
}

/// Best-effort storage estimate (bytes used / quota); nulls if unsupported.
export async function estimate() {
  if (!navigator.storage?.estimate) return { usage: null, quota: null };
  try {
    const e = await navigator.storage.estimate();
    return { usage: e.usage ?? null, quota: e.quota ?? null };
  } catch { return { usage: null, quota: null }; }
}

/// Ask the browser to keep our storage across eviction pressure.
export async function requestPersist() {
  if (!navigator.storage?.persist) return false;
  try { return await navigator.storage.persist(); } catch { return false; }
}
