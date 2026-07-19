//! Hatch-structure classifier (eval-03 §6.A.1): flag segments that belong
//! to hatch/pattern fields so the wall mask can exclude them, by GEOMETRY
//! alone — no semantics. Hatch signature: many near-parallel segments at
//! near-uniform spacing in dense local clusters; walls are long, sparse,
//! and never form ≥5-rail uniform-pitch combs (a double-line wall is two
//! rails; abutting double walls reach four).
//!
//! Algorithm — "rail and comb" (a Hough (θ,ρ) accumulation + 1-D pitch
//! analysis): segments → angle families → RAILS (collinear members merged,
//! split at along-direction gaps) → COMBS (runs of rails at uniform pitch
//! with overlapping extent) → combs with enough rails classify as hatch.
//!
//! CONSERVATISM ASYMMETRY (governs every default): wrongly KEEPING hatch
//! costs a trapped seed — visible, recoverable, the user lowers a slider.
//! Wrongly DROPPING a wall costs a silent leak into the next room — a
//! wrong number that looks right. Every ambiguity therefore resolves to
//! "not hatch": tight pitch-regularity ceiling, high rail-count floor,
//! data-derivation that degrades to a no-op on thin evidence, and two
//! wall-rescue exemptions (end rails, outlier-extent rails). A comb of
//! stair treads or mullions CAN classify as hatch — deliberately: those
//! are not room boundaries, and dropping them makes stairwells fillable;
//! enclosure is carried by the perpendicular/outlier/end lines this pass
//! never flags. Known residual limitation: a single-line wall at exactly
//! the comb's pitch, phase, AND extent is indistinguishable from a hatch
//! line — the eval leak-check watches for it.

use crate::snap::Segment;
use std::collections::BTreeMap;

/// Angle clustering quantum (radians ≈ 0.5°). Like `WIDTH_QUANTUM`, this
/// is a float-noise collapse constant, not a tunable: CAD hatch angles
/// are exact and CTM noise is far below half a degree.
const ANGLE_QUANTUM: f64 = 0.008_726_646_259_971_648;

/// Gap-histogram quantum for `max_pitch_pts` derivation (pts).
const GAP_QUANTUM: f64 = 0.25;

/// Minimum gap-sample count before the derived `max_pitch_pts` is trusted;
/// below it derivation degrades to 0.0 = classifier no-op (never to
/// deleted walls — the `default_min_width` philosophy).
const MIN_GAP_EVIDENCE: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct HatchParams {
    /// ρ tolerance for merging collinear members into one rail. Derived:
    /// `clamp(2 × modal stroke width, 0.25, 1.0)` — the 1.0-pt cap keeps
    /// the two rails of a double-line wall (≥ ~3 pts apart on measured
    /// fixtures) unmergeable; the floor covers zero-width pen tables.
    pub rail_merge_tol_pts: f64,
    /// Max along-direction gap when merging dashes into a rail. Derived:
    /// `max(1.0, 2 × median segment length)` — poché dash gaps are
    /// comparable to dash length; a wall 50+ pts along the same infinite
    /// line never joins a dashed rail.
    pub dash_gap_tol_pts: f64,
    /// Comb size floor. Structural constant 5, not data-derived: a double
    /// wall is 2 rails, + a centerline 3, two abutting double walls in
    /// phase 4 — five uniform-pitch rails is the first count no wall
    /// assembly reaches.
    pub min_rails: usize,
    /// Pitch coefficient-of-variation ceiling. Default 0.10: CAD hatch
    /// pitch is exact (CV ≈ float noise); ambiguity → not hatch.
    pub pitch_cv_max: f64,
    /// Comb pitch ceiling in pts; `0.0` disables classification entirely.
    /// Derived from the page itself (engine-core has no scale): 3 × the
    /// modal consecutive-rail gap over families with ≥ `min_rails` rails,
    /// or 0.0 when fewer than [`MIN_GAP_EVIDENCE`] gaps contribute.
    /// Callers with scale context may override (e.g. 2 ft in pts).
    pub max_pitch_pts: f64,
    /// Adjacent rails join a comb only if their extents overlap at least
    /// this fraction of the shorter — the "dense local cluster" gate.
    pub min_overlap_frac: f64,
    /// Rails whose span exceeds the comb median span × this ratio stay
    /// unclassified: walls overrun the hatched field, hatch lines don't.
    pub extent_outlier_ratio: f64,
}

struct Prepared {
    /// (segment index, θ ∈ [0,π), midpoint ρ placeholder computed later)
    items: Vec<Item>,
    median_len: f64,
    modal_width: f64,
}

#[derive(Clone, Copy)]
struct Item {
    idx: usize,
    theta: f64,
    mx: f64,
    my: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

fn prepare(segments: &[Segment]) -> Prepared {
    let mut items = Vec::with_capacity(segments.len());
    let mut lens = Vec::with_capacity(segments.len());
    let mut width_counts: BTreeMap<i64, usize> = BTreeMap::new();
    for (idx, s) in segments.iter().enumerate() {
        let (x1, y1, x2, y2) = (s.p1.x, s.p1.y, s.p2.x, s.p2.y);
        if ![x1, y1, x2, y2].iter().all(|v| v.is_finite()) {
            continue;
        }
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 || !len.is_finite() {
            continue;
        }
        let mut theta = dy.atan2(dx);
        if theta < 0.0 {
            theta += std::f64::consts::PI;
        }
        if theta >= std::f64::consts::PI {
            theta -= std::f64::consts::PI;
        }
        items.push(Item {
            idx,
            theta,
            mx: (x1 + x2) / 2.0,
            my: (y1 + y2) / 2.0,
            x1,
            y1,
            x2,
            y2,
        });
        lens.push(len);
        if s.width.is_finite() && s.width >= 0.0 {
            *width_counts.entry((s.width / 0.01).round() as i64).or_insert(0) += 1;
        }
    }
    lens.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_len = if lens.is_empty() { 0.0 } else { lens[lens.len() / 2] };
    let modal_width = width_counts
        .iter()
        .max_by_key(|(_, c)| **c)
        .map_or(0.0, |(k, _)| *k as f64 * 0.01);
    Prepared {
        items,
        median_len,
        modal_width,
    }
}

/// Angle families: indices into `items`, clustered by θ with circular wrap
/// (θ near 0 and near π are the same direction).
fn angle_families(items: &[Item]) -> Vec<Vec<usize>> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[a].theta.partial_cmp(&items[b].theta).unwrap());
    let mut families: Vec<Vec<usize>> = vec![vec![order[0]]];
    for w in order.windows(2) {
        if items[w[1]].theta - items[w[0]].theta <= ANGLE_QUANTUM {
            families.last_mut().unwrap().push(w[1]);
        } else {
            families.push(vec![w[1]]);
        }
    }
    // Circular wrap: merge last into first when the gap across π closes.
    if families.len() > 1 {
        let first_theta = items[*families[0].first().unwrap()].theta;
        let last_theta = items[*families.last().unwrap().last().unwrap()].theta;
        if first_theta + std::f64::consts::PI - last_theta <= ANGLE_QUANTUM {
            let last = families.pop().unwrap();
            families[0].extend(last);
        }
    }
    families
}

struct Rail {
    rho: f64,
    s_min: f64,
    s_max: f64,
    members: Vec<usize>, // segment indices (original)
}

impl Rail {
    fn span(&self) -> f64 {
        self.s_max - self.s_min
    }
}

/// Build rails for one angle family (collinear dash-merge + gap split).
fn build_rails(items: &[Item], family: &[usize], params: &HatchParams) -> Vec<Rail> {
    // Family direction: mean angle with wrap normalization (members of a
    // wrapped family sit near both 0 and π — shift the high ones down).
    let base = items[family[0]].theta;
    let mean_theta = family
        .iter()
        .map(|&i| {
            let mut t = items[i].theta;
            if (t - base).abs() > std::f64::consts::FRAC_PI_2 {
                t -= std::f64::consts::PI * (t - base).signum();
            }
            t
        })
        .sum::<f64>()
        / family.len() as f64;
    let (ux, uy) = (mean_theta.cos(), mean_theta.sin());
    let (nx, ny) = (-uy, ux);

    // (ρ, s_min, s_max, idx) per member.
    let mut m: Vec<(f64, f64, f64, usize)> = family
        .iter()
        .map(|&i| {
            let it = items[i];
            let rho = it.mx * nx + it.my * ny;
            let sa = it.x1 * ux + it.y1 * uy;
            let sb = it.x2 * ux + it.y2 * uy;
            (rho, sa.min(sb), sa.max(sb), it.idx)
        })
        .collect();
    m.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut rails: Vec<Rail> = Vec::new();
    let mut cluster: Vec<(f64, f64, f64, usize)> = Vec::new();
    let flush = |cluster: &mut Vec<(f64, f64, f64, usize)>, rails: &mut Vec<Rail>| {
        if cluster.is_empty() {
            return;
        }
        // Split the ρ-cluster into rails at along-direction gaps.
        cluster.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut rail: Option<Rail> = None;
        for &(rho, s0, s1, idx) in cluster.iter() {
            match rail.as_mut() {
                Some(r) if s0 - r.s_max <= params.dash_gap_tol_pts => {
                    r.s_max = r.s_max.max(s1);
                    r.rho += (rho - r.rho) / (r.members.len() + 1) as f64;
                    r.members.push(idx);
                }
                _ => {
                    if let Some(r) = rail.take() {
                        rails.push(r);
                    }
                    rail = Some(Rail {
                        rho,
                        s_min: s0,
                        s_max: s1,
                        members: vec![idx],
                    });
                }
            }
        }
        if let Some(r) = rail.take() {
            rails.push(r);
        }
        cluster.clear();
    };
    for &entry in &m {
        if let Some(last) = cluster.last() {
            if entry.0 - last.0 > params.rail_merge_tol_pts {
                flush(&mut cluster, &mut rails);
            }
        }
        cluster.push(entry);
    }
    flush(&mut cluster, &mut rails);
    rails.sort_by(|a, b| a.rho.partial_cmp(&b.rho).unwrap());
    rails
}

fn overlap_frac(a: &Rail, b: &Rail) -> f64 {
    let overlap = a.s_max.min(b.s_max) - a.s_min.max(b.s_min);
    let shorter = a.span().min(b.span()).max(f64::MIN_POSITIVE);
    overlap / shorter
}

fn pitch_cv(pitches: &[f64]) -> f64 {
    if pitches.len() < 2 {
        return 0.0;
    }
    let mean = pitches.iter().sum::<f64>() / pitches.len() as f64;
    if mean <= 0.0 {
        return f64::INFINITY;
    }
    let var = pitches.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / pitches.len() as f64;
    var.sqrt() / mean
}

/// Flag one comb's members, applying the two wall-rescue exemptions.
fn flag_comb(rails: &[Rail], comb: &[usize], params: &HatchParams, flags: &mut [bool]) {
    if comb.len() < params.min_rails {
        return;
    }
    let mut spans: Vec<f64> = comb.iter().map(|&r| rails[r].span()).collect();
    spans.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_span = spans[spans.len() / 2];
    for (pos, &r) in comb.iter().enumerate() {
        // End-rail exemption: never classify the comb's first/last rail —
        // a wall abutting the field at exactly one pitch cannot be
        // absorbed from the edge.
        if pos == 0 || pos == comb.len() - 1 {
            continue;
        }
        // Outlier-extent exemption: walls overrun the hatched field.
        if rails[r].span() > median_span * params.extent_outlier_ratio {
            continue;
        }
        for &m in &rails[r].members {
            flags[m] = true;
        }
    }
}

/// Sweep rails (sorted by ρ) into uniform-pitch combs and flag them.
///
/// Multiple combs stay OPEN simultaneously: a rail that fails only the
/// extent-overlap gate against one comb belongs to a different locality
/// (side-by-side fields, or annotation elsewhere on the sheet at an
/// interleaving offset) and must neither join nor SPLIT that comb — it
/// tries the other open combs and otherwise seeds its own. Only a
/// structural failure (pitch too large / irregular) against every
/// overlapping comb starts a new comb. Unjoined single rails stay
/// unclassified — the conservative direction.
fn combs_over_rails(rails: &[Rail], params: &HatchParams, flags: &mut [bool]) {
    struct Comb {
        members: Vec<usize>,
        pitches: Vec<f64>,
    }
    let mut combs: Vec<Comb> = Vec::new();
    for r in 0..rails.len() {
        let mut joined = false;
        for comb in combs.iter_mut() {
            let &prev = comb.members.last().unwrap();
            if overlap_frac(&rails[prev], &rails[r]) < params.min_overlap_frac {
                continue; // different locality: neither join nor split
            }
            let pitch = rails[r].rho - rails[prev].rho;
            let mut trial = comb.pitches.clone();
            trial.push(pitch);
            if pitch <= params.max_pitch_pts && pitch_cv(&trial) <= params.pitch_cv_max {
                comb.members.push(r);
                comb.pitches = trial;
                joined = true;
                break;
            }
        }
        if !joined {
            combs.push(Comb {
                members: vec![r],
                pitches: Vec::new(),
            });
        }
    }
    for comb in &combs {
        flag_comb(rails, &comb.members, params, flags);
    }
}

impl HatchParams {
    /// Data-derived defaults for one page's segments (see field docs for
    /// each derivation). Runs a bootstrap rail pass to build the
    /// consecutive-rail-gap histogram behind `max_pitch_pts`.
    pub fn derive(segments: &[Segment]) -> HatchParams {
        let prep = prepare(segments);
        let mut params = HatchParams {
            rail_merge_tol_pts: (2.0 * prep.modal_width).clamp(0.25, 1.0),
            dash_gap_tol_pts: (2.0 * prep.median_len).max(1.0),
            min_rails: 5,
            pitch_cv_max: 0.10,
            max_pitch_pts: 0.0,
            min_overlap_frac: 0.5,
            extent_outlier_ratio: 1.5,
        };
        // Bootstrap gap histogram over families that could form combs.
        let mut gap_counts: BTreeMap<i64, usize> = BTreeMap::new();
        let mut total_gaps = 0usize;
        for family in angle_families(&prep.items) {
            let rails = build_rails(&prep.items, &family, &params);
            if rails.len() < params.min_rails {
                continue;
            }
            for w in rails.windows(2) {
                let gap = w[1].rho - w[0].rho;
                if gap.is_finite() && gap > 0.0 {
                    *gap_counts.entry((gap / GAP_QUANTUM).round() as i64).or_insert(0) += 1;
                    total_gaps += 1;
                }
            }
        }
        if total_gaps >= MIN_GAP_EVIDENCE {
            let modal_gap = gap_counts
                .iter()
                .max_by_key(|(_, c)| **c)
                .map_or(0.0, |(k, _)| *k as f64 * GAP_QUANTUM);
            params.max_pitch_pts = 3.0 * modal_gap;
        }
        params
    }
}

/// Per-segment hatch flags, indexed like `segments`. Non-finite and
/// zero-length segments are never flagged. `max_pitch_pts <= 0` is the
/// documented no-op (thin evidence → keep everything).
pub fn classify_hatch(segments: &[Segment], params: &HatchParams) -> Vec<bool> {
    let mut flags = vec![false; segments.len()];
    if !(params.max_pitch_pts > 0.0) || params.min_rails == 0 {
        return flags;
    }
    let prep = prepare(segments);
    for family in angle_families(&prep.items) {
        if family.len() < params.min_rails {
            continue;
        }
        let rails = build_rails(&prep.items, &family, params);
        combs_over_rails(&rails, params, &mut flags);
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;

    fn seg(x1: f64, y1: f64, x2: f64, y2: f64) -> Segment {
        Segment {
            p1: Point::new(x1, y1),
            p2: Point::new(x2, y2),
            width: 0.72,
        }
    }

    fn params() -> HatchParams {
        HatchParams {
            rail_merge_tol_pts: 0.5,
            dash_gap_tol_pts: 4.0,
            min_rails: 5,
            pitch_cv_max: 0.10,
            max_pitch_pts: 20.0,
            min_overlap_frac: 0.5,
            extent_outlier_ratio: 1.5,
        }
    }

    /// N horizontal rails at pitch `p`, spanning x 0..len, starting y0.
    fn field(n: usize, p: f64, len: f64, y0: f64) -> Vec<Segment> {
        (0..n).map(|i| seg(0.0, y0 + i as f64 * p, len, y0 + i as f64 * p)).collect()
    }

    #[test]
    fn rails_merge_collinear_dashes_into_one_rail() {
        // 7 rails; the middle one drawn as 4 dashes with 2-pt gaps. If the
        // dashes did NOT merge, the interior rails would flag separately
        // and dash members would still be flagged — assert instead on the
        // rail structure: all 4 dashes share the middle rail's fate.
        let mut segs = field(7, 9.0, 100.0, 0.0);
        segs.remove(3);
        for k in 0..4 {
            let x0 = k as f64 * 26.0;
            segs.push(seg(x0, 27.0, x0 + 24.0, 27.0));
        }
        let flags = classify_hatch(&segs, &params());
        // Dashes (last 4 entries) are an interior rail → all flagged.
        assert!(flags[segs.len() - 4..].iter().all(|&f| f));
    }

    #[test]
    fn rail_splits_on_large_gap_along_direction() {
        // A "rail" made of two far-apart pieces (gap 50 ≫ dash tol) is two
        // rails; with only 4 other lines the comb never reaches 5 uniform
        // members spanning both pieces' extents (overlap gate) — the far
        // piece (a wall further along the same infinite line) stays kept.
        let mut segs = field(5, 9.0, 40.0, 0.0); // x 0..40
        segs.push(seg(90.0, 18.0, 130.0, 18.0)); // far piece on rail 2's line
        let flags = classify_hatch(&segs, &params());
        assert!(!flags[segs.len() - 1], "far collinear piece must not inherit the comb flag");
    }

    #[test]
    fn angle_family_clusters_across_quantum_boundary() {
        // 7 rails whose angles jitter within ±0.2° — must be ONE family
        // (sweep clustering, not fixed bins), hence interior rails flag.
        let segs: Vec<Segment> = (0..7)
            .map(|i| {
                let y = i as f64 * 9.0;
                let jitter = (i as f64 - 3.0) * 0.0006; // radians, ±0.1°
                seg(0.0, y, 100.0, y + 100.0 * jitter.tan())
            })
            .collect();
        let flags = classify_hatch(&segs, &params());
        assert!(flags.iter().skip(1).take(5).all(|&f| f), "{flags:?}");
    }

    #[test]
    fn double_wall_two_rails_never_hatch() {
        // Two parallel wall faces 5 pts apart — and even four faces from
        // two abutting double walls — stay below min_rails.
        let segs = vec![
            seg(0.0, 0.0, 200.0, 0.0),
            seg(0.0, 5.0, 200.0, 5.0),
            seg(0.0, 100.0, 200.0, 100.0),
            seg(0.0, 105.0, 200.0, 105.0),
        ];
        let flags = classify_hatch(&segs, &params());
        assert!(flags.iter().all(|&f| !f));
    }

    #[test]
    fn comb_breaks_at_pitch_irregularity() {
        // 5 uniform rails, then a rail at half pitch (a wall face), then 5
        // more uniform: pitch CV break splits the comb; each side has 5
        // rails, interior members flag, the irregular rail does not.
        let mut segs = field(5, 9.0, 100.0, 0.0); // y 0..36
        segs.push(seg(0.0, 40.0, 100.0, 40.0)); // wall face, Δρ=4
        for i in 0..5 {
            segs.push(seg(0.0, 49.0 + i as f64 * 9.0, 100.0, 49.0 + i as f64 * 9.0));
        }
        let flags = classify_hatch(&segs, &params());
        assert!(!flags[5], "irregular-pitch rail must not be hatch");
        assert!(flags[2], "interior of first comb flags");
        assert!(flags[8], "interior of second comb flags");
    }

    #[test]
    fn end_rails_exempt() {
        let segs = field(7, 9.0, 100.0, 0.0);
        let flags = classify_hatch(&segs, &params());
        assert!(!flags[0] && !flags[6], "end rails must stay unclassified");
        assert!(flags[1..6].iter().all(|&f| f), "interior rails flag");
    }

    #[test]
    fn outlier_extent_rail_exempt() {
        // Interior rail 3 spans 3× the field length (a wall crossing the
        // comb at pitch phase) — exempted by the extent-outlier rule.
        let mut segs = field(7, 9.0, 100.0, 0.0);
        segs[3] = seg(-100.0, 27.0, 200.0, 27.0);
        let flags = classify_hatch(&segs, &params());
        assert!(!flags[3], "outlier-extent rail must stay unclassified");
        assert!(flags[2] && flags[4]);
    }

    #[test]
    fn derive_max_pitch_from_modal_gap() {
        // 30 rails at pitch 9 → modal gap 9 → max_pitch 27.
        let segs = field(30, 9.0, 100.0, 0.0);
        let p = HatchParams::derive(&segs);
        assert!((p.max_pitch_pts - 27.0).abs() < 1e-9, "{}", p.max_pitch_pts);
    }

    #[test]
    fn derive_no_op_on_sparse_evidence() {
        // A lone double wall: 1 gap ≪ MIN_GAP_EVIDENCE → no-op params.
        let segs = vec![seg(0.0, 0.0, 200.0, 0.0), seg(0.0, 5.0, 200.0, 5.0)];
        let p = HatchParams::derive(&segs);
        assert_eq!(p.max_pitch_pts, 0.0);
        assert!(classify_hatch(&segs, &p).iter().all(|&f| !f));
    }

    #[test]
    fn nonfinite_segments_never_flagged() {
        let mut segs = field(7, 9.0, 100.0, 0.0);
        segs.push(Segment {
            p1: Point::new(f64::NAN, 27.0),
            p2: Point::new(100.0, 27.0),
            width: 0.72,
        });
        let flags = classify_hatch(&segs, &params());
        assert!(!flags[segs.len() - 1]);
    }
}
