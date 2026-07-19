//! Ad-hoc instrumentation for the hatch classifier against a real page's
//! segment dump (kept `#[ignore]`d; run explicitly with
//! `HATCH_DEBUG_FLAT=path cargo test --test hatch_debug -- --ignored --nocapture`).
//! Reads a JSON array of [x1,y1,x2,y2,w]×n and prints family/comb stats.

use engine_core::{classify_hatch, HatchParams, Point, Segment};

#[test]
#[ignore]
fn dump_hatch_stats() {
    let path = std::env::var("HATCH_DEBUG_FLAT").expect("set HATCH_DEBUG_FLAT");
    let raw = std::fs::read_to_string(path).unwrap();
    // Plain [n, n, …] array — split-parse, no JSON dependency needed.
    let vals: Vec<f64> = raw
        .trim_matches(|c| c == '[' || c == ']' || char::is_whitespace(c))
        .split(',')
        .map(|t| t.trim().parse().unwrap())
        .collect();
    let segments: Vec<Segment> = vals
        .chunks_exact(5)
        .map(|c| Segment {
            p1: Point::new(c[0], c[1]),
            p2: Point::new(c[2], c[3]),
            width: c[4],
        })
        .collect();
    let params = HatchParams::derive(&segments);
    println!("derived: {params:?}");
    let flags = classify_hatch(&segments, &params);
    println!("flagged: {}", flags.iter().filter(|&&f| f).count());

    // Horizontal tile courses in the kitchen region: how many flagged?
    let mut total = 0;
    let mut flagged = 0;
    let mut kept_examples = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let dy = (s.p2.y - s.p1.y).abs();
        let dx = (s.p2.x - s.p1.x).abs();
        if dy > 0.01 || dx < 20.0 || (s.width - 0.72).abs() > 0.05 {
            continue;
        }
        let my = (s.p1.y + s.p2.y) / 2.0;
        let mx = (s.p1.x + s.p2.x) / 2.0;
        if !(360.0..=910.0).contains(&my) || !(570.0..=1915.0).contains(&mx) {
            continue;
        }
        total += 1;
        if flags[i] {
            flagged += 1;
        } else if kept_examples.len() < 15 {
            kept_examples.push((i, s.p1.x, s.p1.y, s.p2.x, s.p2.y));
        }
    }
    println!("long horizontal 0.72 courses in kitchen: {total}, flagged {flagged}");
    for k in kept_examples {
        println!("  kept: {k:?}");
    }

    // Replicate rail clustering for the HORIZONTAL family in the kitchen
    // band to see the comb-formation picture (throwaway diagnostics).
    let mut items: Vec<(f64, f64, f64, usize)> = Vec::new(); // (rho=y, s_min, s_max, idx)
    for (i, s) in segments.iter().enumerate() {
        let dy = (s.p2.y - s.p1.y).abs();
        let dx = (s.p2.x - s.p1.x).abs();
        if dy > 0.01 * dx.max(1.0) || dx < 0.5 {
            continue;
        }
        let my = (s.p1.y + s.p2.y) / 2.0;
        if !(360.0..=910.0).contains(&my) {
            continue;
        }
        items.push((my, s.p1.x.min(s.p2.x), s.p1.x.max(s.p2.x), i));
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    // ρ-cluster at 0.6, split at dash gap 18.27
    let mut rails: Vec<(f64, f64, f64, usize)> = Vec::new(); // rho, smin, smax, count
    let mut cluster: Vec<(f64, f64, f64, usize)> = Vec::new();
    let flush = |cluster: &mut Vec<(f64, f64, f64, usize)>, rails: &mut Vec<(f64, f64, f64, usize)>| {
        if cluster.is_empty() { return; }
        cluster.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut cur: Option<(f64, f64, f64, usize)> = None;
        for &(rho, s0, s1, _) in cluster.iter() {
            match cur.as_mut() {
                Some(c) if s0 - c.2 <= 18.27 => { c.2 = c.2.max(s1); c.3 += 1; }
                _ => { if let Some(c) = cur.take() { rails.push(c); } cur = Some((rho, s0, s1, 1)); }
            }
        }
        if let Some(c) = cur.take() { rails.push(c); }
        cluster.clear();
    };
    for &it in &items {
        if let Some(last) = cluster.last() {
            if it.0 - last.0 > 0.6 { flush(&mut cluster, &mut rails); }
        }
        cluster.push(it);
    }
    flush(&mut cluster, &mut rails);
    rails.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("horizontal kitchen rails: {}", rails.len());
    for r in rails.iter().take(40) {
        println!("  rail rho {:7.2} s [{:7.1}..{:7.1}] segs {}", r.0, r.1, r.2, r.3);
    }
}
