//! automerge 0.10 (AutoCommit) session implementing the common spike surface.

use crate::model::{GenMeasurement, FIXED_AT};
use automerge::transaction::Transactable;
use automerge::{ActorId, AutoCommit, ObjId, ObjType, ReadDoc, Value, ROOT};

pub struct AmSession {
    pub doc: AutoCommit,
    measurements: ObjId,
    pages: ObjId,
}

impl AmSession {
    pub fn new(actor: &[u8]) -> AmSession {
        let mut doc = AutoCommit::new();
        doc.set_actor(ActorId::from(actor));
        let measurements = doc.put_object(ROOT, "measurements", ObjType::Map).unwrap();
        let pages = doc.put_object(ROOT, "pages", ObjType::Map).unwrap();
        doc.commit();
        let _ = doc.save_incremental(); // flush setup; workload bytes start clean
        AmSession {
            doc,
            measurements,
            pages,
        }
    }

    fn flush(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.save_incremental()
    }

    pub fn add_page(&mut self, id: &str, fpi: f64, source: &str, with_verification: bool) {
        let page = self.doc.put_object(&self.pages, id, ObjType::Map).unwrap();
        self.doc.put(&page, "fpi", fpi).unwrap();
        self.doc.put(&page, "source", source).unwrap();
        if with_verification {
            let v = self
                .doc
                .put_object(&page, "verification", ObjType::Map)
                .unwrap();
            self.doc.put(&v, "method", "auto").unwrap();
            self.doc.put(&v, "verdict", "verified").unwrap();
            self.doc.put(&v, "at", FIXED_AT).unwrap();
            self.doc.put(&v, "medianDeviationPct", 0.8).unwrap();
            let samples = self.doc.put_object(&v, "samples", ObjType::List).unwrap();
            for i in 0..3 {
                let s = self.doc.insert_object(&samples, i, ObjType::Map).unwrap();
                self.doc
                    .put(&s, "printedText", format!("{}'-0\"", 10 + i))
                    .unwrap();
                self.doc.put(&s, "printedFt", (10 + i) as f64).unwrap();
                self.doc
                    .put(&s, "measuredFt", (10 + i) as f64 + 0.05)
                    .unwrap();
                self.doc.put(&s, "deviationPct", 0.5).unwrap();
            }
        }
        self.flush();
    }

    /// One change creating the full measurement. Returns wire bytes.
    pub fn create_measurement(&mut self, gm: &GenMeasurement) -> usize {
        let m = self
            .doc
            .put_object(&self.measurements, gm.id.as_str(), ObjType::Map)
            .unwrap();
        self.doc.put(&m, "id", gm.id.as_str()).unwrap();
        self.doc.put(&m, "page", gm.page.as_str()).unwrap();
        self.doc.put(&m, "kind", gm.kind).unwrap();
        self.doc.put(&m, "name", gm.name.as_str()).unwrap();
        self.doc.put(&m, "color", gm.color.as_str()).unwrap();
        self.doc.put(&m, "origin", gm.origin).unwrap();
        let pts = self.doc.put_object(&m, "pts", ObjType::List).unwrap();
        for (i, &(x, y)) in gm.pts.iter().enumerate() {
            self.doc.insert(&pts, 2 * i, x).unwrap();
            self.doc.insert(&pts, 2 * i + 1, y).unwrap();
        }
        self.flush().len()
    }

    fn measurement(&self, id: &str) -> Option<ObjId> {
        self.doc
            .get(&self.measurements, id)
            .unwrap()
            .map(|(_, obj)| obj)
    }

    fn pts_of(&self, m: &ObjId) -> ObjId {
        self.doc.get(m, "pts").unwrap().map(|(_, obj)| obj).unwrap()
    }

    /// Append one vertex (x, y) as its own change. Returns wire bytes.
    pub fn append_vertex(&mut self, id: &str, x: f64, y: f64) -> usize {
        let m = self.measurement(id).unwrap();
        let pts = self.pts_of(&m);
        let len = self.doc.length(&pts);
        self.doc.insert(&pts, len, x).unwrap();
        self.doc.insert(&pts, len + 1, y).unwrap();
        self.flush().len()
    }

    pub fn rename(&mut self, id: &str, name: &str) -> usize {
        let m = self.measurement(id).unwrap();
        self.doc.put(&m, "name", name).unwrap();
        self.flush().len()
    }

    pub fn confirm(&mut self, id: &str, user: &str) -> usize {
        let m = self.measurement(id).unwrap();
        let c = self
            .doc
            .put_object(&m, "confirmedBy", ObjType::Map)
            .unwrap();
        self.doc.put(&c, "userId", user).unwrap();
        self.doc.put(&c, "at", FIXED_AT).unwrap();
        self.flush().len()
    }

    pub fn delete(&mut self, id: &str) -> usize {
        self.doc.delete(&self.measurements, id).unwrap();
        self.flush().len()
    }

    pub fn snapshot(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.save()
    }

    pub fn load(actor: &[u8], snapshot: &[u8]) -> AmSession {
        let mut doc = AutoCommit::load(snapshot).unwrap();
        doc.set_actor(ActorId::from(actor));
        let measurements = doc
            .get(ROOT, "measurements")
            .unwrap()
            .map(|(_, obj)| obj)
            .unwrap();
        let pages = doc.get(ROOT, "pages").unwrap().map(|(_, obj)| obj).unwrap();
        let _ = doc.save_incremental(); // future increments start at the load point
        AmSession {
            doc,
            measurements,
            pages,
        }
    }

    /// Canonical, sorted, human-readable state for merge-outcome reporting.
    /// Conflicting `name` registers are shown with every surviving value
    /// (automerge keeps losers queryable via get_all).
    pub fn summary(&self) -> String {
        let mut ids: Vec<String> = self.doc.keys(&self.measurements).collect();
        ids.sort();
        let mut out = String::new();
        for id in ids {
            let m = self.measurement(&id).unwrap();
            let names: Vec<String> = self
                .doc
                .get_all(&m, "name")
                .unwrap()
                .into_iter()
                .map(|(v, _)| match v {
                    Value::Scalar(s) => s.to_string(),
                    other => format!("{other:?}"),
                })
                .collect();
            let name = if names.len() == 1 {
                format!("{:?}", names[0])
            } else {
                format!(
                    "winner {:?} (all surviving: {names:?})",
                    names.last().unwrap()
                )
            };
            let pts_len = self.doc.length(&self.pts_of(&m)) / 2;
            let confirmed = self.doc.get(&m, "confirmedBy").unwrap().is_some();
            out.push_str(&format!(
                "{id}: name={name} vertices={pts_len} confirmed={confirmed}\n"
            ));
        }
        if out.is_empty() {
            out.push_str("<no measurements>\n");
        }
        out
    }

    pub fn measurement_count(&self) -> usize {
        self.doc.keys(&self.measurements).count()
    }
}
