//! CRDT spike bench: yrs 0.27 vs automerge 0.10 on the takeoff document
//! shape (final spec §9 decision gate). Deterministic: seeded LCG, fixed
//! actor ids, fixed timestamps. Output is the markdown source material for
//! docs/crdt-spike-report.md.

mod am_impl;
mod model;
mod yrs_impl;

use am_impl::AmSession;
use model::{gen_measurements, median, GenMeasurement, Lcg};
use std::time::Instant;
use yrs_impl::YrsSession;

const ACTOR_A: [u8; 16] = [0xaa; 16];
const ACTOR_B: [u8; 16] = [0xbb; 16];
const ACTOR_C: [u8; 16] = [0xcc; 16];

fn setup_pages_yrs(s: &YrsSession) {
    for p in 0..5 {
        s.add_page(&format!("p{p}"), 4.0, "calibrated", p == 0);
    }
    s.take_updates();
}

fn setup_pages_am(s: &mut AmSession) {
    for p in 0..5 {
        s.add_page(&format!("p{p}"), 4.0, "calibrated", p == 0);
    }
}

fn w1_update_size() {
    println!("\n## W1 — update size (bytes on the wire)\n");
    println!("| case | yrs | automerge |");
    println!("|---|---|---|");

    let mut rng = Lcg::new();
    let gms = gen_measurements(&mut rng, 3);
    // (a) in-progress trace: 1 initial vertex, then 29 single-vertex appends
    let trace = {
        let mut t = gms[0].clone();
        t.pts.truncate(1);
        t
    };
    let full30: GenMeasurement = {
        let mut t = gms[1].clone();
        while t.pts.len() < 30 {
            t.pts
                .push((rng.next_f64() * 3000.0, rng.next_f64() * 2000.0));
        }
        t.pts.truncate(30);
        t
    };

    let y = YrsSession::new(1);
    setup_pages_yrs(&y);
    let mut a = AmSession::new(&ACTOR_A);
    setup_pages_am(&mut a);

    y.create_measurement(&trace);
    a.create_measurement(&trace);
    let mut y_appends = Vec::new();
    let mut a_appends = Vec::new();
    for i in 0..29 {
        let (x, yy) = (100.0 + i as f64, 200.0 + i as f64);
        y_appends.push(y.append_vertex(&trace.id, x, yy) as f64);
        a_appends.push(a.append_vertex(&trace.id, x, yy) as f64);
    }
    println!(
        "| (a) first vertex append | {} | {} |",
        y_appends[0], a_appends[0]
    );
    println!(
        "| (a) steady-state per-vertex append (median of appends 10–29) | {} | {} |",
        median(y_appends[9..].to_vec()),
        median(a_appends[9..].to_vec())
    );
    println!(
        "| (a) 29 appends total | {} | {} |",
        y_appends.iter().sum::<f64>(),
        a_appends.iter().sum::<f64>()
    );
    println!(
        "| (b) completed 30-vertex measurement, one transaction | {} | {} |",
        y.create_measurement(&full30),
        a.create_measurement(&full30)
    );
    println!(
        "| (c) rename | {} | {} |",
        y.rename(&full30.id, "Conference Room 214"),
        a.rename(&full30.id, "Conference Room 214")
    );
    println!(
        "| (d) confirmedBy stamp | {} | {} |",
        y.confirm(&full30.id, "user-42"),
        a.confirm(&full30.id, "user-42")
    );
}

fn w2_growth() {
    println!("\n## W2 — document growth (session: create 2000, edit 500, delete 500)\n");
    let mut rng = Lcg::new();
    let gms = gen_measurements(&mut rng, 2000);

    // yrs
    let y = YrsSession::new(1);
    setup_pages_yrs(&y);
    for gm in &gms {
        y.create_measurement(gm);
    }
    for gm in gms.iter().take(500) {
        y.rename(&gm.id, &format!("{} edited", gm.name));
        y.append_vertex(&gm.id, 1.0, 2.0);
        y.append_vertex(&gm.id, 3.0, 4.0);
    }
    for gm in gms.iter().skip(500).take(500) {
        y.delete(&gm.id);
    }
    let y_log: usize = y.take_updates().iter().map(|u| u.len()).sum();
    let y_snap = y.snapshot();
    let y_reload = YrsSession::load(9, &y_snap);
    let y_compact = y_reload.snapshot();
    assert_eq!(y_reload.measurement_count(), 1500);

    // automerge
    let mut a = AmSession::new(&ACTOR_A);
    setup_pages_am(&mut a);
    let mut a_log = 0usize;
    for gm in &gms {
        a_log += a.create_measurement(gm);
    }
    for gm in gms.iter().take(500) {
        a_log += a.rename(&gm.id, &format!("{} edited", gm.name));
        a_log += a.append_vertex(&gm.id, 1.0, 2.0);
        a_log += a.append_vertex(&gm.id, 3.0, 4.0);
    }
    for gm in gms.iter().skip(500).take(500) {
        a_log += a.delete(&gm.id);
    }
    let a_snap = a.snapshot();
    let mut a_reload = AmSession::load(&ACTOR_C, &a_snap);
    let a_compact = a_reload.snapshot();
    assert_eq!(a_reload.measurement_count(), 1500);

    println!("| metric | yrs | automerge |");
    println!("|---|---|---|");
    println!("| cumulative update-log bytes | {y_log} | {a_log} |");
    println!(
        "| snapshot after session | {} | {} |",
        y_snap.len(),
        a_snap.len()
    );
    println!(
        "| snapshot after reload+re-save (compaction) | {} | {} |",
        y_compact.len(),
        a_compact.len()
    );
}

fn scenario_base_yrs() -> Vec<u8> {
    let base = YrsSession::new(1);
    setup_pages_yrs(&base);
    let mut rng = Lcg::new();
    let mut gms = gen_measurements(&mut rng, 2);
    for gm in &mut gms {
        gm.pts.truncate(5);
        base.create_measurement(gm);
    }
    base.snapshot()
}

fn scenario_base_am() -> Vec<u8> {
    let mut base = AmSession::new(&ACTOR_C);
    setup_pages_am(&mut base);
    let mut rng = Lcg::new();
    let mut gms = gen_measurements(&mut rng, 2);
    for gm in &mut gms {
        gm.pts.truncate(5);
        base.create_measurement(gm);
    }
    base.snapshot()
}

fn merge_yrs(label: &str, diverge_a: impl Fn(&YrsSession), diverge_b: impl Fn(&YrsSession)) {
    let snap = scenario_base_yrs();
    let a = YrsSession::load(11, &snap);
    let b = YrsSession::load(12, &snap);
    diverge_a(&a);
    diverge_b(&b);
    let (sa, sb) = (a.snapshot(), b.snapshot());
    let c1 = YrsSession::load(91, &snap);
    c1.apply(&sa);
    c1.apply(&sb);
    let c2 = YrsSession::load(92, &snap);
    c2.apply(&sb);
    c2.apply(&sa);
    let deterministic = c1.summary() == c2.summary();
    println!(
        "**yrs — {label}** (order-independent: {deterministic})\n```\n{}```",
        c1.summary()
    );
}

fn merge_am(label: &str, diverge_a: impl Fn(&mut AmSession), diverge_b: impl Fn(&mut AmSession)) {
    let snap = scenario_base_am();
    let mut a = AmSession::load(&ACTOR_A, &snap);
    let mut b = AmSession::load(&ACTOR_B, &snap);
    diverge_a(&mut a);
    diverge_b(&mut b);
    let mut c1 = AmSession::load(&ACTOR_C, &snap);
    c1.doc.merge(&mut a.doc).unwrap();
    c1.doc.merge(&mut b.doc).unwrap();
    let mut c2 = AmSession::load(&ACTOR_C, &snap);
    c2.doc.merge(&mut b.doc).unwrap();
    c2.doc.merge(&mut a.doc).unwrap();
    let deterministic = c1.summary() == c2.summary();
    println!(
        "**automerge — {label}** (order-independent: {deterministic})\n```\n{}```",
        c1.summary()
    );
}

fn w3_merge_semantics() {
    println!("\n## W3 — merge semantics (base: m0000 \"Room 0\" 5 vtx, m0001 \"Room 1\" 5 vtx)\n");

    println!("### (a) two actors edit different measurements\n");
    merge_yrs(
        "A edits m0000, B edits m0001",
        |a| {
            a.rename("m0000", "Kitchen A");
            a.append_vertex("m0000", 9.0, 9.0);
        },
        |b| {
            b.rename("m0001", "Bath B");
            b.append_vertex("m0001", 8.0, 8.0);
        },
    );
    merge_am(
        "A edits m0000, B edits m0001",
        |a| {
            a.rename("m0000", "Kitchen A");
            a.append_vertex("m0000", 9.0, 9.0);
        },
        |b| {
            b.rename("m0001", "Bath B");
            b.append_vertex("m0001", 8.0, 8.0);
        },
    );

    println!("### (b) both rename the same measurement\n");
    merge_yrs(
        "A: m0000 → \"Kitchen A\"; B: m0000 → \"Kitchen B\"",
        |a| {
            a.rename("m0000", "Kitchen A");
        },
        |b| {
            b.rename("m0000", "Kitchen B");
        },
    );
    merge_am(
        "A: m0000 → \"Kitchen A\"; B: m0000 → \"Kitchen B\"",
        |a| {
            a.rename("m0000", "Kitchen A");
        },
        |b| {
            b.rename("m0000", "Kitchen B");
        },
    );

    println!("### (c) A deletes m0000 while B is mid-trace on it\n");
    merge_yrs(
        "A deletes m0000; B appends 3 vertices + renames it",
        |a| {
            a.delete("m0000");
        },
        |b| {
            b.append_vertex("m0000", 1.0, 1.0);
            b.append_vertex("m0000", 2.0, 2.0);
            b.append_vertex("m0000", 3.0, 3.0);
            b.rename("m0000", "In Progress");
        },
    );
    merge_am(
        "A deletes m0000; B appends 3 vertices + renames it",
        |a| {
            a.delete("m0000");
        },
        |b| {
            b.append_vertex("m0000", 1.0, 1.0);
            b.append_vertex("m0000", 2.0, 2.0);
            b.append_vertex("m0000", 3.0, 3.0);
            b.rename("m0000", "In Progress");
        },
    );
}

fn w4_snapshot_load() {
    println!("\n## W4 — snapshot + cold load at 5000 measurements\n");
    let mut rng = Lcg::new();
    let gms = gen_measurements(&mut rng, 5000);

    let y = YrsSession::new(1);
    setup_pages_yrs(&y);
    for gm in &gms {
        y.create_measurement(gm);
    }
    let y_snap = y.snapshot();
    let mut y_times = Vec::new();
    for i in 0..10 {
        let t = Instant::now();
        let loaded = YrsSession::load(100 + i, &y_snap);
        y_times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(loaded.measurement_count(), 5000);
    }

    let mut a = AmSession::new(&ACTOR_A);
    setup_pages_am(&mut a);
    for gm in &gms {
        a.create_measurement(gm);
    }
    let a_snap = a.snapshot();
    let mut a_times = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let loaded = AmSession::load(&ACTOR_C, &a_snap);
        a_times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(loaded.measurement_count(), 5000);
    }

    println!("| metric | yrs | automerge |");
    println!("|---|---|---|");
    println!("| snapshot bytes | {} | {} |", y_snap.len(), a_snap.len());
    println!(
        "| cold load, median of 10 (ms) | {:.1} | {:.1} |",
        median(y_times),
        median(a_times)
    );
}

fn w6_undo() {
    println!("\n## W6 — undo/redo\n");
    println!(
        "**yrs (native UndoManager):**\n```\n{}```",
        yrs_impl::undo_demo()
    );
    println!(
        "**automerge 0.10:** no undo/redo API exists on Automerge or AutoCommit \
         (verified against the 0.10.0 source; no `undo` symbol in the public API). \
         Undo would be application-built: capture inverse operations or diff \
         against prior heads per local change, excluding remote changes."
    );
}

fn main() {
    println!("# CRDT spike raw results — yrs 0.27.3 vs automerge 0.10.0");
    w1_update_size();
    w2_growth();
    w3_merge_semantics();
    w4_snapshot_load();
    w6_undo();
}
