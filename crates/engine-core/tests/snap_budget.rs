//! Timed build + query test at the addendum's 10⁴–10⁵ segment scale.
//!
//! Budget (approved plan): build 10⁵ segments ≤ 150 ms and 1,000 snap
//! queries ≤ 50 ms, asserted in RELEASE builds only (debug Rust is 10–50×
//! slower; there the test still runs and prints timings). CI runs
//! `cargo test --release --test snap_budget` to enforce it. Expected
//! actuals are ~40 ms build and single-digit-ms queries — the budget is
//! deliberately conservative against CI-runner noise while still proving
//! "milliseconds, not seconds".

use engine_core::{Point, Segment, SegmentIndex};
use std::time::Instant;

/// Deterministic LCG in [0, 1) — no `rand` dependency, no wall-clock seeds.
fn lcg(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 11) as f64) / ((1_u64 << 53) as f64)
}

#[test]
fn snap_index_build_and_query_budget() {
    const N: usize = 100_000;
    let mut state = 0x5eed_5eed_5eed_5eed_u64;
    let segments: Vec<Segment> = (0..N)
        .map(|_| {
            let x = lcg(&mut state) * 3000.0;
            let y = lcg(&mut state) * 2000.0;
            let dx = (lcg(&mut state) - 0.5) * 200.0;
            let dy = (lcg(&mut state) - 0.5) * 200.0;
            Segment {
                p1: Point::new(x, y),
                p2: Point::new(x + dx, y + dy),
                width: 0.5 + lcg(&mut state) * 5.0,
            }
        })
        .collect();

    let t_build = Instant::now();
    let index = SegmentIndex::build(segments);
    let build = t_build.elapsed();

    let t_query = Instant::now();
    let mut hits = 0_usize;
    for _ in 0..1000 {
        let cursor = Point::new(lcg(&mut state) * 3000.0, lcg(&mut state) * 2000.0);
        if index.snap(cursor, 10.0).is_some() {
            hits += 1;
        }
    }
    let queries = t_query.elapsed();

    println!(
        "snap budget: build {N} segments = {build:?}; 1000 queries = {queries:?}; {hits} hits"
    );
    assert!(hits > 0, "dense synthetic sheet should produce snap hits");

    if cfg!(debug_assertions) {
        eprintln!("debug build: budget not asserted (release-only, see CI step)");
        return;
    }
    assert!(
        build.as_millis() <= 150,
        "index build took {build:?}, budget 150 ms"
    );
    assert!(
        queries.as_millis() <= 50,
        "1000 queries took {queries:?}, budget 50 ms"
    );
}
