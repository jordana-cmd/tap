//! Wasm-footprint instrument: exercises automerge through the common
//! surface so dead-code elimination cannot strip the library (create doc,
//! build a measurement map + pts list, incremental save, incremental load,
//! full save, full load).
//!
//! automerge → uuid needs a randomness source on wasm32-unknown-unknown;
//! this instrument supplies a deterministic custom getrandom backend
//! (built with RUSTFLAGS --cfg getrandom_backend="custom"). Size cost is a
//! few dozen bytes — a stand-in for the JS randomness glue production
//! builds would carry. Never production code.

use automerge::transaction::Transactable;
use automerge::{ActorId, AutoCommit, ObjType, ROOT};

#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    for i in 0..len {
        unsafe { *dest.add(i) = (i as u8).wrapping_mul(31).wrapping_add(7) };
    }
    Ok(())
}

#[no_mangle]
pub extern "C" fn crdt_roundtrip(n: u32) -> u32 {
    let mut doc = AutoCommit::new();
    doc.set_actor(ActorId::from(&[1_u8; 16][..]));
    let measurements = doc.put_object(ROOT, "measurements", ObjType::Map).unwrap();
    let m = doc.put_object(&measurements, "m0", ObjType::Map).unwrap();
    doc.put(&m, "name", "Room").unwrap();
    doc.put(&m, "origin", "manual").unwrap();
    let pts = doc.put_object(&m, "pts", ObjType::List).unwrap();
    for i in 0..n as usize * 2 {
        doc.insert(&pts, i, i as f64).unwrap();
    }
    doc.commit();
    let inc = doc.save_incremental();

    let mut doc2 = AutoCommit::new();
    doc2.set_actor(ActorId::from(&[2_u8; 16][..]));
    doc2.load_incremental(&inc).unwrap();
    let snap = doc2.save();

    let mut doc3 = AutoCommit::load(&snap).unwrap();
    (snap.len() + doc3.get_heads().len()) as u32
}
