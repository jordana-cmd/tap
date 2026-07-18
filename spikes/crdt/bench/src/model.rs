//! Shared deterministic workload data (no wall-clock, no thread ids).

/// Fixed timestamp used for every `at` field — documents must be
/// byte-reproducible across runs.
pub const FIXED_AT: &str = "1752800000";

pub struct Lcg(pub u64);

impl Lcg {
    pub fn new() -> Lcg {
        Lcg(0xC4D7_5EED_0000_0001)
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
    }
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next_f64() * (hi - lo) as f64) as usize
    }
}

#[derive(Clone)]
pub struct GenMeasurement {
    pub id: String,
    pub page: String,
    pub kind: &'static str,
    pub name: String,
    pub color: String,
    pub origin: &'static str,
    pub pts: Vec<(f64, f64)>,
}

pub const KINDS: [&str; 3] = ["area", "linear", "count"];

/// Deterministic measurement stream: page cycles p0..p4, kind cycles,
/// trace length 4..30 vertices (mean ≈ 17), coords in a 3000×2000 pt page.
pub fn gen_measurements(rng: &mut Lcg, count: usize) -> Vec<GenMeasurement> {
    (0..count)
        .map(|i| {
            let n_pts = rng.range(4, 30);
            let pts = (0..n_pts)
                .map(|_| (rng.next_f64() * 3000.0, rng.next_f64() * 2000.0))
                .collect();
            GenMeasurement {
                id: format!("m{i:04}"),
                page: format!("p{}", i % 5),
                kind: KINDS[i % 3],
                name: format!("Room {i}"),
                color: format!("#{:06x}", (rng.next_f64() * 16_777_215.0) as u32),
                origin: "manual",
                pts,
            }
        })
        .collect()
}

pub fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}
