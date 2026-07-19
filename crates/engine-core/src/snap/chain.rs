//! Collinear chain walk: from one segment, follow connected near-collinear
//! segments in both directions and measure the full run — the engine side
//! of the click-a-wall tool. Deterministic like [`SegmentIndex::snap`]:
//! candidate choice ties break by (angle deviation, join distance, id).

use super::{Segment, SegmentId, SegmentIndex};
use crate::geom::{distance, Point};
use rstar::AABB;
use std::collections::HashSet;

/// A run of near-collinear, near-connected segments.
#[derive(Debug, Clone, PartialEq)]
pub struct CollinearChain {
    /// Segments in walk order from `start` to `end` (the seed segment is
    /// included at its position along the run).
    pub ids: Vec<SegmentId>,
    /// Extreme endpoint at the minimum projection along the run axis.
    pub start: Point,
    /// Extreme endpoint at the maximum projection along the run axis.
    pub end: Point,
    /// `distance(start, end)` — end-to-end span INCLUDING bridged gaps,
    /// which is the wall-run length an estimator wants.
    pub run_length: f64,
}

/// Cycle/pathology backstop far above any real wall run.
const MAX_CHAIN_SEGMENTS: usize = 100_000;

struct Axis {
    origin: Point,
    ux: f64,
    uy: f64,
}

impl Axis {
    fn proj(&self, p: Point) -> f64 {
        (p.x - self.origin.x) * self.ux + (p.y - self.origin.y) * self.uy
    }
    /// Perpendicular distance of `p` from the axis line.
    fn perp(&self, p: Point) -> f64 {
        ((p.x - self.origin.x) * self.uy - (p.y - self.origin.y) * self.ux).abs()
    }
    /// |sin| of the angle between the axis and segment direction —
    /// orientation-agnostic (a wall drawn right-to-left still matches).
    fn sin_dev(&self, seg: &Segment) -> Option<f64> {
        let dx = seg.p2.x - seg.p1.x;
        let dy = seg.p2.y - seg.p1.y;
        let len = (dx * dx + dy * dy).sqrt();
        if !len.is_finite() || len == 0.0 {
            return None;
        }
        Some(((dx / len) * self.uy - (dy / len) * self.ux).abs())
    }
}

impl SegmentIndex {
    /// Walk the collinear chain containing `start` (see the click-a-wall
    /// tool, addendum §A3.3 follow-up). A neighbor joins the chain iff:
    /// its direction is within `angle_eps_rad` of the SEED segment's axis
    /// (fixed reference — no drift around gentle curves); it has an
    /// endpoint within `join_tol` of the current chain end; BOTH its
    /// endpoints lie within `join_tol` perpendicular of the axis line
    /// (rejects the offset parallel twin of a double-line wall); and it
    /// extends the run outward. Returns `None` for an out-of-range id, a
    /// non-finite or zero-length seed, or invalid tolerances.
    pub fn collinear_chain(
        &self,
        start: SegmentId,
        angle_eps_rad: f64,
        join_tol: f64,
    ) -> Option<CollinearChain> {
        if !(angle_eps_rad.is_finite() && angle_eps_rad > 0.0)
            || !(join_tol.is_finite() && join_tol > 0.0)
        {
            return None;
        }
        let seed = *self.segments.get(start.0 as usize)?;
        if !seed.is_finite() {
            return None;
        }
        let dx = seed.p2.x - seed.p1.x;
        let dy = seed.p2.y - seed.p1.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            return None;
        }
        let axis = Axis {
            origin: seed.p1,
            ux: dx / len,
            uy: dy / len,
        };
        let sin_eps = angle_eps_rad.min(std::f64::consts::FRAC_PI_2).sin();

        let mut visited: HashSet<u32> = HashSet::from([start.0]);
        // Track extremes over all chain endpoints (projection, point).
        let mut lo = (axis.proj(seed.p1), seed.p1);
        let mut hi = (axis.proj(seed.p2), seed.p2);
        if lo.0 > hi.0 {
            std::mem::swap(&mut lo, &mut hi);
        }

        let mut forward: Vec<SegmentId> = Vec::new();
        let mut backward: Vec<SegmentId> = Vec::new();
        for dir in [1.0_f64, -1.0] {
            // Walk end: the current extreme point in this direction.
            loop {
                let e = if dir > 0.0 { hi.1 } else { lo.1 };
                let extent = if dir > 0.0 { hi.0 } else { lo.0 };
                let window = AABB::from_corners(
                    [e.x - join_tol, e.y - join_tol],
                    [e.x + join_tol, e.y + join_tol],
                );
                // Best: (sin deviation, join distance, id).
                let mut best: Option<(f64, f64, u32)> = None;
                for entry in self.tree.locate_in_envelope_intersecting(&window) {
                    let id = entry.id;
                    if visited.contains(&id) {
                        continue;
                    }
                    let seg = self.segments[id as usize];
                    let Some(sin_dev) = axis.sin_dev(&seg) else {
                        continue;
                    };
                    if sin_dev > sin_eps
                        || axis.perp(seg.p1) > join_tol
                        || axis.perp(seg.p2) > join_tol
                    {
                        continue;
                    }
                    let join = distance(seg.p1, e).min(distance(seg.p2, e));
                    if join > join_tol {
                        continue;
                    }
                    // Must extend the run outward in the walk direction.
                    let (pa, pb) = (axis.proj(seg.p1), axis.proj(seg.p2));
                    let far = if dir > 0.0 { pa.max(pb) } else { pa.min(pb) };
                    if (far - extent) * dir <= 0.0 {
                        continue;
                    }
                    let replace = match best {
                        None => true,
                        Some((bs, bj, bid)) => {
                            (sin_dev, join, id) < (bs, bj, bid)
                        }
                    };
                    if replace {
                        best = Some((sin_dev, join, id));
                    }
                }
                let Some((_, _, id)) = best else {
                    break;
                };
                visited.insert(id);
                if dir > 0.0 {
                    forward.push(SegmentId(id));
                } else {
                    backward.push(SegmentId(id));
                }
                let seg = self.segments[id as usize];
                for p in [seg.p1, seg.p2] {
                    let pr = axis.proj(p);
                    if pr < lo.0 {
                        lo = (pr, p);
                    }
                    if pr > hi.0 {
                        hi = (pr, p);
                    }
                }
                if visited.len() >= MAX_CHAIN_SEGMENTS {
                    break;
                }
            }
        }

        backward.reverse();
        let mut ids = backward;
        ids.push(start);
        ids.extend(forward);
        Some(CollinearChain {
            ids,
            start: lo.1,
            end: hi.1,
            run_length: distance(lo.1, hi.1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(x1: f64, y1: f64, x2: f64, y2: f64) -> Segment {
        Segment {
            p1: Point::new(x1, y1),
            p2: Point::new(x2, y2),
            width: 1.0,
        }
    }

    const EPS: f64 = 0.03; // ≈1.7°
    const TOL: f64 = 3.0;

    fn ids(chain: &CollinearChain) -> Vec<u32> {
        chain.ids.iter().map(|s| s.0).collect()
    }

    #[test]
    fn chain_straight_run_of_five_segments() {
        // Five end-to-end horizontal segments spanning x 0..100.
        let index = SegmentIndex::build(
            (0..5).map(|i| seg(i as f64 * 20.0, 50.0, i as f64 * 20.0 + 20.0, 50.0)).collect(),
        );
        let c = index.collinear_chain(SegmentId(2), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0, 1, 2, 3, 4]);
        assert_eq!(c.start, Point::new(0.0, 50.0));
        assert_eq!(c.end, Point::new(100.0, 50.0));
        assert_eq!(c.run_length, 100.0);
    }

    #[test]
    fn chain_walks_both_directions_from_middle() {
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 30.0, 0.0),
            seg(30.0, 0.0, 60.0, 0.0),
            seg(60.0, 0.0, 90.0, 0.0),
        ]);
        let c = index.collinear_chain(SegmentId(1), EPS, TOL).unwrap();
        // Walk order: backward-most first, seed in the middle.
        assert_eq!(ids(&c), vec![0, 1, 2]);
        assert_eq!(c.run_length, 90.0);
    }

    #[test]
    fn chain_bridges_gap_within_join_tol() {
        // 2-unit gap at x=50 (≤ TOL) — bridged; run includes the gap.
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 50.0, 0.0),
            seg(52.0, 0.0, 100.0, 0.0),
        ]);
        let c = index.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0, 1]);
        assert_eq!(c.run_length, 100.0);
    }

    #[test]
    fn chain_stops_at_gap_beyond_join_tol() {
        // 10-unit gap (> TOL): the run ends at the gap.
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 50.0, 0.0),
            seg(60.0, 0.0, 100.0, 0.0),
        ]);
        let c = index.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0]);
        assert_eq!(c.run_length, 50.0);
    }

    #[test]
    fn chain_does_not_turn_corner() {
        // An L: horizontal then vertical sharing the corner point.
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 60.0, 0.0),
            seg(60.0, 0.0, 60.0, 40.0),
        ]);
        let c = index.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0]);
        assert_eq!(c.run_length, 60.0);
    }

    #[test]
    fn chain_near_collinear_boundary() {
        // Continuation tilted by slope 0.02 (sin ≈ 0.02 < EPS 0.03): joins.
        let just_inside = SegmentIndex::build(vec![
            seg(0.0, 0.0, 50.0, 0.0),
            seg(50.0, 0.0, 100.0, 1.0),
        ]);
        let c = just_inside.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0, 1]);

        // Tilted by slope 0.1 (sin ≈ 0.0995 > EPS): rejected.
        let just_outside = SegmentIndex::build(vec![
            seg(0.0, 0.0, 50.0, 0.0),
            seg(50.0, 0.0, 100.0, 5.0),
        ]);
        let c = just_outside.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0]);
    }

    #[test]
    fn chain_rejects_offset_parallel_twin() {
        // Double-line wall: the twin runs parallel 5 units off-axis
        // (> TOL perpendicular) and even touches the chain end region at a
        // corner return. It must NOT be absorbed.
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 60.0, 0.0),
            seg(0.0, 5.0, 60.0, 5.0),   // offset twin
            seg(60.0, 0.0, 120.0, 0.0), // true continuation
        ]);
        let c = index.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0, 2]);
        assert_eq!(c.run_length, 120.0);
    }

    #[test]
    fn chain_single_segment_run() {
        let index = SegmentIndex::build(vec![seg(3.0, 4.0, 33.0, 44.0)]);
        let c = index.collinear_chain(SegmentId(0), EPS, TOL).unwrap();
        assert_eq!(ids(&c), vec![0]);
        assert_eq!(c.run_length, 50.0);
    }

    #[test]
    fn chain_zero_length_or_invalid_start_is_none() {
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 0.0, 0.0),
            seg(f64::NAN, 0.0, 1.0, 0.0),
        ]);
        assert!(index.collinear_chain(SegmentId(0), EPS, TOL).is_none());
        assert!(index.collinear_chain(SegmentId(1), EPS, TOL).is_none());
        assert!(index.collinear_chain(SegmentId(99), EPS, TOL).is_none());
        // Invalid tolerances.
        let ok = SegmentIndex::build(vec![seg(0.0, 0.0, 10.0, 0.0)]);
        assert!(ok.collinear_chain(SegmentId(0), 0.0, TOL).is_none());
        assert!(ok.collinear_chain(SegmentId(0), EPS, f64::NAN).is_none());
    }
}
