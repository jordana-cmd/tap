//! yrs 0.27 session implementing the common spike surface.

use crate::model::{GenMeasurement, FIXED_AT};
use std::sync::{Arc, Mutex};
use yrs::types::array::ArrayPrelim;
use yrs::types::map::MapPrelim;
use yrs::undo::UndoManager;
use yrs::updates::decoder::Decode;
use yrs::{
    Any, Array, ArrayRef, Doc, Map, MapRef, Out, ReadTxn, StateVector, Subscription, Transact,
    Update,
};

pub struct YrsSession {
    pub doc: Doc,
    measurements: MapRef,
    pages: MapRef,
    updates: Arc<Mutex<Vec<Vec<u8>>>>,
    _sub: Subscription,
}

impl YrsSession {
    pub fn new(client_id: u64) -> YrsSession {
        let doc = Doc::with_client_id(client_id);
        let measurements = doc.get_or_insert_map("measurements");
        let pages = doc.get_or_insert_map("pages");
        let updates = Arc::new(Mutex::new(Vec::new()));
        let sink = updates.clone();
        let _sub = doc
            .observe_update_v1(move |_txn, e| sink.lock().unwrap().push(e.update.clone()))
            .unwrap();
        YrsSession {
            doc,
            measurements,
            pages,
            updates,
            _sub,
        }
    }

    pub fn last_update_len(&self) -> usize {
        self.updates.lock().unwrap().last().map_or(0, |u| u.len())
    }

    pub fn take_updates(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.updates.lock().unwrap())
    }

    pub fn add_page(&self, id: &str, fpi: f64, source: &str, with_verification: bool) {
        let mut txn = self.doc.transact_mut();
        let page: MapRef = self.pages.insert(
            &mut txn,
            id,
            MapPrelim::from([("source", Any::from(source))]),
        );
        page.insert(&mut txn, "fpi", fpi);
        if with_verification {
            let v: MapRef = page.insert(
                &mut txn,
                "verification",
                MapPrelim::from([
                    ("method", Any::from("auto")),
                    ("verdict", Any::from("verified")),
                    ("at", Any::from(FIXED_AT)),
                ]),
            );
            v.insert(&mut txn, "medianDeviationPct", 0.8_f64);
            let samples: ArrayRef =
                v.insert(&mut txn, "samples", ArrayPrelim::from([] as [Any; 0]));
            for i in 0..3 {
                samples.push_back(
                    &mut txn,
                    MapPrelim::from([
                        ("printedText", Any::from(format!("{}'-0\"", 10 + i))),
                        ("printedFt", Any::from((10 + i) as f64)),
                        ("measuredFt", Any::from((10 + i) as f64 + 0.05)),
                        ("deviationPct", Any::from(0.5_f64)),
                    ]),
                );
            }
        }
    }

    /// One transaction creating the full measurement. Returns wire bytes.
    pub fn create_measurement(&self, gm: &GenMeasurement) -> usize {
        let mut txn = self.doc.transact_mut();
        let m: MapRef = self.measurements.insert(
            &mut txn,
            gm.id.as_str(),
            MapPrelim::from([
                ("id", gm.id.as_str()),
                ("page", gm.page.as_str()),
                ("kind", gm.kind),
                ("name", gm.name.as_str()),
                ("color", gm.color.as_str()),
                ("origin", gm.origin),
            ]),
        );
        let flat: Vec<f64> = gm.pts.iter().flat_map(|&(x, y)| [x, y]).collect();
        m.insert(&mut txn, "pts", ArrayPrelim::from(flat));
        drop(txn);
        self.last_update_len()
    }

    fn measurement(&self, txn: &impl ReadTxn, id: &str) -> Option<MapRef> {
        match self.measurements.get(txn, id) {
            Some(Out::YMap(m)) => Some(m),
            _ => None,
        }
    }

    /// Append one vertex (x, y) in its own transaction. Returns wire bytes.
    pub fn append_vertex(&self, id: &str, x: f64, y: f64) -> usize {
        let mut txn = self.doc.transact_mut();
        let m = self.measurement(&txn, id).unwrap();
        let Some(Out::YArray(pts)) = m.get(&txn, "pts") else {
            panic!("pts missing")
        };
        pts.push_back(&mut txn, x);
        pts.push_back(&mut txn, y);
        drop(txn);
        self.last_update_len()
    }

    pub fn rename(&self, id: &str, name: &str) -> usize {
        let mut txn = self.doc.transact_mut();
        let m = self.measurement(&txn, id).unwrap();
        m.insert(&mut txn, "name", name);
        drop(txn);
        self.last_update_len()
    }

    pub fn confirm(&self, id: &str, user: &str) -> usize {
        let mut txn = self.doc.transact_mut();
        let m = self.measurement(&txn, id).unwrap();
        m.insert(
            &mut txn,
            "confirmedBy",
            MapPrelim::from([("userId", user), ("at", FIXED_AT)]),
        );
        drop(txn);
        self.last_update_len()
    }

    pub fn delete(&self, id: &str) -> usize {
        let mut txn = self.doc.transact_mut();
        self.measurements.remove(&mut txn, id);
        drop(txn);
        self.last_update_len()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    pub fn load(client_id: u64, snapshot: &[u8]) -> YrsSession {
        let s = YrsSession::new(client_id);
        s.apply(snapshot);
        s.take_updates(); // loading isn't part of any measured wire traffic
        s
    }

    pub fn apply(&self, update: &[u8]) {
        let mut txn = self.doc.transact_mut();
        txn.apply_update(Update::decode_v1(update).unwrap())
            .unwrap();
    }

    /// Canonical, sorted, human-readable state for merge-outcome reporting.
    pub fn summary(&self) -> String {
        let txn = self.doc.transact();
        let mut ids: Vec<String> = self
            .measurements
            .keys(&txn)
            .map(|k| k.to_string())
            .collect();
        ids.sort();
        let mut out = String::new();
        for id in ids {
            let m = self.measurement(&txn, &id).unwrap();
            let name = match m.get(&txn, "name") {
                Some(Out::Any(Any::String(s))) => s.to_string(),
                other => format!("{other:?}"),
            };
            let pts_len = match m.get(&txn, "pts") {
                Some(Out::YArray(a)) => a.len(&txn) / 2,
                _ => 0,
            };
            let confirmed = m.get(&txn, "confirmedBy").is_some();
            out.push_str(&format!(
                "{id}: name={name:?} vertices={pts_len} confirmed={confirmed}\n"
            ));
        }
        if out.is_empty() {
            out.push_str("<no measurements>\n");
        }
        out
    }

    pub fn measurement_count(&self) -> usize {
        let txn = self.doc.transact();
        self.measurements.len(&txn) as usize
    }
}

/// Undo/redo demo using the native UndoManager.
pub fn undo_demo() -> String {
    let s = YrsSession::new(1);
    let gm = GenMeasurement {
        id: "m0000".into(),
        page: "p0".into(),
        kind: "area",
        name: "Original".into(),
        color: "#112233".into(),
        origin: "manual",
        pts: vec![(0.0, 0.0), (10.0, 0.0)],
    };
    s.create_measurement(&gm);
    let mut undo: UndoManager<()> = UndoManager::new();
    undo.expand_scope(&s.doc, &s.measurements);
    s.rename("m0000", "Renamed");
    s.append_vertex("m0000", 10.0, 10.0);
    let after_edits = s.summary();
    let undid_append = undo.undo_blocking();
    let undid_rename = undo.undo_blocking();
    let after_undo = s.summary();
    let redid = undo.redo_blocking();
    let after_redo = s.summary();
    format!(
        "after edits:   {after_edits}undo x2 (returned {undid_append}, {undid_rename}): {after_undo}redo x1 (returned {redid}): {after_redo}"
    )
}
