//! Wasm-footprint instrument: exercises yrs through the common surface so
//! dead-code elimination cannot strip the library (create doc, build a
//! measurement map + pts list, encode update, apply update, snapshot, load).

use yrs::types::array::ArrayPrelim;
use yrs::types::map::MapPrelim;
use yrs::updates::decoder::Decode;
use yrs::{Array, Doc, Map, MapRef, ReadTxn, StateVector, Transact, Update};

#[no_mangle]
pub extern "C" fn crdt_roundtrip(n: u32) -> u32 {
    let doc = Doc::with_client_id(1);
    let measurements = doc.get_or_insert_map("measurements");
    {
        let mut txn = doc.transact_mut();
        let m: MapRef = measurements.insert(
            &mut txn,
            "m0",
            MapPrelim::from([("name", "Room"), ("origin", "manual")]),
        );
        let flat: Vec<f64> = (0..n as usize * 2).map(|i| i as f64).collect();
        let pts = m.insert(&mut txn, "pts", ArrayPrelim::from(flat));
        pts.push_back(&mut txn, 42.0_f64);
    }
    let update = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());

    let doc2 = Doc::with_client_id(2);
    {
        let mut txn = doc2.transact_mut();
        txn.apply_update(Update::decode_v1(&update).unwrap())
            .unwrap();
    }
    let snap = doc2
        .transact()
        .encode_state_as_update_v1(&StateVector::default());

    let doc3 = Doc::with_client_id(3);
    {
        let mut txn = doc3.transact_mut();
        txn.apply_update(Update::decode_v1(&snap).unwrap()).unwrap();
    }
    snap.len() as u32
}
