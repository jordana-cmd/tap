//! SegmentIndex + snapping (addendum §A3.3): a per-page spatial index over
//! stroked segments and the prioritized snap query. Upgrades precision from
//! "how steady is your mouse" to CAD-exact, and is the prerequisite for
//! trusting AI-proposed geometry later.
//!
//! PDF operator-list extraction is NOT here — segments arrive via
//! [`SegmentSource`] (the seam the future pdf module fills). Scanned PDFs
//! yield no vectors → empty index → [`SegmentIndex::snap`] returns `None`,
//! silently. Degrade, don't warn-spam.

mod intersect;

use crate::geom::{distance, Point};
use intersect::{project_onto_segment, segment_intersection};
use rstar::{RTree, RTreeObject, AABB};

/// Identifier of a segment within one page's index: its position in the Vec
/// passed to [`SegmentIndex::build`] (stable even when some entries are
/// skipped as non-finite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentId(pub u32);

/// A stroked segment in base units (PDF points), page-local, top-left
/// origin. `width` is the stroke width from extraction (walls are typically
/// the widest strokes on a sheet) — carried for later wall inference, unused
/// by snapping itself. Zero-length segments are valid point features:
/// endpoint-snappable, with projection degenerating to the point.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Segment {
    pub p1: Point,
    pub p2: Point,
    pub width: f64,
}

impl Segment {
    fn is_finite(&self) -> bool {
        self.p1.x.is_finite()
            && self.p1.y.is_finite()
            && self.p2.x.is_finite()
            && self.p2.y.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapKind {
    /// Cursor snapped to an endpoint (p1 or p2) of [`Snap::segment`].
    Endpoint,
    /// Snapped to the lazily computed intersection of [`Snap::segment`]
    /// with `other` (ids ordered: segment < other).
    Intersection { other: SegmentId },
    /// Perpendicular projection onto [`Snap::segment`].
    Projection,
}

/// Where the cursor lands and why. Measurements created from a Snap carry
/// `origin:'snap'` provenance (addendum §A2) — `segment` (plus `other` for
/// intersections) is that provenance reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    pub point: Point,
    pub kind: SnapKind,
    pub segment: SegmentId,
}

/// The seam for PDF operator-list extraction (future pdf module or server
/// ingest). This step ships only the trait; tests use a canned stub.
pub trait SegmentSource {
    fn segments_for_page(&self, page_index: u32) -> Vec<Segment>;
}

#[derive(Debug, Clone)]
struct IndexedSegment {
    id: u32,
    env: AABB<[f64; 2]>,
}

impl RTreeObject for IndexedSegment {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> AABB<[f64; 2]> {
        self.env
    }
}

/// Per-page spatial index over stroked segments: built once from the full
/// extraction, immutable thereafter, queried on every pointer-down. Backed
/// by an rstar bulk-loaded R*-tree (final spec §9 decision log).
pub struct SegmentIndex {
    segments: Vec<Segment>,
    tree: RTree<IndexedSegment>,
}

impl SegmentIndex {
    /// Bulk-build from the page's segments. Entries with non-finite
    /// coordinates stay in the id space but are never indexed or returned
    /// by [`SegmentIndex::snap`] (defensive against extraction bugs).
    pub fn build(segments: Vec<Segment>) -> SegmentIndex {
        let entries: Vec<IndexedSegment> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_finite())
            .map(|(i, s)| IndexedSegment {
                id: i as u32,
                env: AABB::from_corners(
                    [s.p1.x.min(s.p2.x), s.p1.y.min(s.p2.y)],
                    [s.p1.x.max(s.p2.x), s.p1.y.max(s.p2.y)],
                ),
            })
            .collect();
        SegmentIndex {
            segments,
            tree: RTree::bulk_load(entries),
        }
    }

    /// Number of segments provided to [`SegmentIndex::build`] (the id
    /// space), including any non-finite entries that are never returned.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The source segment for an id (None if out of range).
    pub fn segment(&self, id: SegmentId) -> Option<Segment> {
        self.segments.get(id.0 as usize).copied()
    }

    /// All segments in id order (the Vec passed to [`SegmentIndex::build`]).
    /// Lets one owned copy serve the histogram, the wall-mask rasterizer,
    /// and overlay filtering without duplicating ~10⁵ segments.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Snap `cursor` to nearby geometry within `tolerance` BASE UNITS.
    ///
    /// engine-core has no zoom concept: the addendum's screen-space
    /// `10 / zoom` tolerance is converted to base units by engine-web
    /// BEFORE calling. Distances compare inclusively (d ≤ tolerance snaps).
    /// Returns `None` when the index is empty (scanned-PDF degrade path),
    /// nothing is in tolerance, or `tolerance` is non-finite or ≤ 0.
    pub fn snap(&self, cursor: Point, tolerance: f64) -> Option<Snap> {
        if !(tolerance.is_finite() && tolerance > 0.0)
            || !cursor.x.is_finite()
            || !cursor.y.is_finite()
        {
            return None;
        }
        // Any endpoint, intersection, or projection within tolerance lies on
        // a segment whose AABB intersects this window, so the candidate set
        // is sufficient for all three passes.
        let window = AABB::from_corners(
            [cursor.x - tolerance, cursor.y - tolerance],
            [cursor.x + tolerance, cursor.y + tolerance],
        );
        let candidates: Vec<u32> = self
            .tree
            .locate_in_envelope_intersecting(&window)
            .map(|e| e.id)
            .collect();
        if candidates.is_empty() {
            return None;
        }

        // Priority is categorical per addendum §A3.3: endpoint >
        // intersection > projection — an in-tolerance endpoint wins even
        // when an intersection or projection is strictly CLOSER. Known UX
        // consequence: a farther endpoint outranks a nearer target by
        // design; if tracing ever feels sticky, per-kind tolerances
        // (engine-web-era tuning) are the adjustment path, not a reorder
        // of this priority.

        // Pass 1 — endpoints. Ties: smaller distance, lower id, p1 before p2.
        let mut best_end: Option<(f64, u32, u8, Point)> = None;
        for &id in &candidates {
            let seg = self.segments[id as usize];
            for (slot, p) in [(0_u8, seg.p1), (1_u8, seg.p2)] {
                let d = distance(cursor, p);
                if d <= tolerance {
                    let replace = match best_end {
                        None => true,
                        Some((bd, bid, bslot, _)) => {
                            d < bd || (d == bd && (id, slot) < (bid, bslot))
                        }
                    };
                    if replace {
                        best_end = Some((d, id, slot, p));
                    }
                }
            }
        }
        if let Some((_, id, _, point)) = best_end {
            return Some(Snap {
                point,
                kind: SnapKind::Endpoint,
                segment: SegmentId(id),
            });
        }

        // Pass 2 — intersections, computed lazily over candidate pairs only
        // (worst case k(k−1)/2 for k candidates in the window — never the
        // O(n²) precompute the addendum's 10⁵-segment scale forbids).
        // Ties: smaller distance, then lexicographic (id, other).
        let mut best_int: Option<(f64, u32, u32, Point)> = None;
        for (i, &ida) in candidates.iter().enumerate() {
            let a = self.segments[ida as usize];
            for &idb in &candidates[i + 1..] {
                let b = self.segments[idb as usize];
                let Some(p) = segment_intersection(a.p1, a.p2, b.p1, b.p2) else {
                    continue;
                };
                let d = distance(cursor, p);
                if d <= tolerance {
                    let (lo, hi) = if ida < idb { (ida, idb) } else { (idb, ida) };
                    let replace = match best_int {
                        None => true,
                        Some((bd, blo, bhi, _)) => d < bd || (d == bd && (lo, hi) < (blo, bhi)),
                    };
                    if replace {
                        best_int = Some((d, lo, hi, p));
                    }
                }
            }
        }
        if let Some((_, lo, hi, point)) = best_int {
            return Some(Snap {
                point,
                kind: SnapKind::Intersection {
                    other: SegmentId(hi),
                },
                segment: SegmentId(lo),
            });
        }

        // Pass 3 — perpendicular projection. Ties: smaller distance, lower
        // id (what makes duplicates and collinear overlaps deterministic).
        let mut best_proj: Option<(f64, u32, Point)> = None;
        for &id in &candidates {
            let seg = self.segments[id as usize];
            let p = project_onto_segment(cursor, seg.p1, seg.p2);
            let d = distance(cursor, p);
            if d <= tolerance {
                let replace = match best_proj {
                    None => true,
                    Some((bd, bid, _)) => d < bd || (d == bd && id < bid),
                };
                if replace {
                    best_proj = Some((d, id, p));
                }
            }
        }
        best_proj.map(|(_, id, point)| Snap {
            point,
            kind: SnapKind::Projection,
            segment: SegmentId(id),
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

    #[test]
    fn snap_empty_index_returns_none() {
        // Scanned PDFs yield no vectors → snapping silently off.
        let index = SegmentIndex::build(vec![]);
        assert!(index.is_empty());
        assert_eq!(index.snap(Point::new(10.0, 10.0), 50.0), None);
    }

    #[test]
    fn snap_endpoint_beats_projection_when_both_in_tolerance() {
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 100.0, 0.0),  // projection (10,0), d = 5
            seg(10.0, 9.0, 40.0, 40.0), // endpoint (10,9), d = 4
        ]);
        let snap = index.snap(Point::new(10.0, 5.0), 6.0).unwrap();
        assert_eq!(snap.kind, SnapKind::Endpoint);
        assert_eq!(snap.segment, SegmentId(1));
        assert_eq!(snap.point, Point::new(10.0, 9.0));
    }

    #[test]
    fn snap_priority_holds_when_projection_strictly_closer() {
        // Projection d = 2 is strictly closer than endpoint d = 5 — the
        // endpoint still wins (categorical priority per §A3.3).
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 100.0, 0.0),  // projection (10,0), d = 2
            seg(14.0, 5.0, 60.0, 60.0), // endpoint (14,5), d = 5
        ]);
        let snap = index.snap(Point::new(10.0, 2.0), 6.0).unwrap();
        assert_eq!(snap.kind, SnapKind::Endpoint);
        assert_eq!(snap.point, Point::new(14.0, 5.0));
    }

    #[test]
    fn snap_intersection_beats_projection() {
        // Projections (52,10) d=3 and (50,13) d=2 are both closer than the
        // intersection (50,10) d=√13≈3.61 — the intersection still wins.
        let index = SegmentIndex::build(vec![
            seg(0.0, 10.0, 100.0, 10.0),
            seg(50.0, 0.0, 50.0, 100.0),
        ]);
        let snap = index.snap(Point::new(52.0, 13.0), 5.0).unwrap();
        assert_eq!(
            snap.kind,
            SnapKind::Intersection {
                other: SegmentId(1)
            }
        );
        assert_eq!(snap.segment, SegmentId(0));
        assert_eq!(snap.point, Point::new(50.0, 10.0));
    }

    #[test]
    fn snap_projection_when_nothing_else() {
        let index = SegmentIndex::build(vec![seg(0.0, 0.0, 100.0, 0.0)]);
        let snap = index.snap(Point::new(30.0, 4.0), 5.0).unwrap();
        assert_eq!(snap.kind, SnapKind::Projection);
        assert_eq!(snap.point, Point::new(30.0, 0.0));
    }

    #[test]
    fn snap_tolerance_boundary() {
        let index = SegmentIndex::build(vec![seg(0.0, 0.0, 100.0, 0.0)]);
        let cursor = Point::new(50.0, 5.0); // d = 5 exactly
        assert!(index.snap(cursor, 5.001).is_some()); // just inside
        assert!(index.snap(cursor, 5.0).is_some()); // inclusive boundary
        assert_eq!(index.snap(cursor, 4.999), None); // just outside
    }

    #[test]
    fn snap_vertical_horizontal_diagonal() {
        let index = SegmentIndex::build(vec![
            seg(10.0, 0.0, 10.0, 100.0),   // vertical
            seg(0.0, 200.0, 100.0, 200.0), // horizontal
            seg(0.0, 0.0, 100.0, 100.0),   // diagonal
        ]);
        let v = index.snap(Point::new(13.0, 50.0), 4.0).unwrap();
        assert_eq!((v.point, v.segment), (Point::new(10.0, 50.0), SegmentId(0)));
        let h = index.snap(Point::new(40.0, 203.0), 4.0).unwrap();
        assert_eq!(
            (h.point, h.segment),
            (Point::new(40.0, 200.0), SegmentId(1))
        );
        let d = index.snap(Point::new(53.0, 47.0), 5.0).unwrap();
        assert_eq!((d.point, d.segment), (Point::new(50.0, 50.0), SegmentId(2)));
        assert_eq!(d.kind, SnapKind::Projection);
    }

    #[test]
    fn snap_duplicate_segments_deterministic() {
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 100.0, 0.0),
            seg(0.0, 0.0, 100.0, 0.0), // exact duplicate
        ]);
        // Endpoint tie between ids 0 and 1 → lowest id wins.
        let snap = index.snap(Point::new(2.0, 2.0), 5.0).unwrap();
        assert_eq!(
            (snap.kind, snap.segment),
            (SnapKind::Endpoint, SegmentId(0))
        );
        // Duplicates are parallel → no bogus intersection mid-span.
        let snap = index.snap(Point::new(50.0, 2.0), 5.0).unwrap();
        assert_eq!(
            (snap.kind, snap.segment),
            (SnapKind::Projection, SegmentId(0))
        );
    }

    #[test]
    fn snap_t_junction_endpoint_on_segment() {
        // B's endpoint (50,0) lies ON A: it is both an endpoint and the A∩B
        // intersection — the endpoint pass wins by priority.
        let index =
            SegmentIndex::build(vec![seg(0.0, 0.0, 100.0, 0.0), seg(50.0, 0.0, 50.0, 80.0)]);
        let snap = index.snap(Point::new(51.0, 3.0), 5.0).unwrap();
        assert_eq!(snap.kind, SnapKind::Endpoint);
        assert_eq!(snap.segment, SegmentId(1));
        assert_eq!(snap.point, Point::new(50.0, 0.0));
    }

    #[test]
    fn snap_parallel_segments_no_intersection() {
        let index = SegmentIndex::build(vec![seg(0.0, 0.0, 100.0, 0.0), seg(0.0, 3.0, 100.0, 3.0)]);
        // Equidistant projections (d = 1.5 to both) → tie-break to id 0;
        // crucially no fabricated "intersection" from the parallel pair.
        let snap = index.snap(Point::new(50.0, 1.5), 5.0).unwrap();
        assert_eq!(
            (snap.kind, snap.segment),
            (SnapKind::Projection, SegmentId(0))
        );
    }

    #[test]
    fn snap_collinear_overlapping_no_intersection() {
        let index = SegmentIndex::build(vec![
            seg(0.0, 0.0, 60.0, 0.0),
            seg(40.0, 0.0, 100.0, 0.0), // overlaps A on [40, 60]
        ]);
        let snap = index.snap(Point::new(50.0, 2.0), 5.0).unwrap();
        assert_eq!(
            (snap.kind, snap.segment),
            (SnapKind::Projection, SegmentId(0))
        );
        assert_eq!(snap.point, Point::new(50.0, 0.0));
    }

    #[test]
    fn snap_degenerate_zero_length_segment() {
        let index = SegmentIndex::build(vec![seg(5.0, 5.0, 5.0, 5.0)]);
        let snap = index.snap(Point::new(6.0, 6.0), 2.0).unwrap();
        assert_eq!(snap.kind, SnapKind::Endpoint);
        assert_eq!(snap.point, Point::new(5.0, 5.0));
    }

    #[test]
    fn snap_invalid_tolerance_returns_none() {
        let index = SegmentIndex::build(vec![seg(0.0, 0.0, 100.0, 0.0)]);
        let cursor = Point::new(50.0, 1.0);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(index.snap(cursor, bad), None);
        }
    }

    #[test]
    fn snap_skips_nonfinite_segments_ids_stable() {
        let index = SegmentIndex::build(vec![
            seg(f64::NAN, 0.0, 10.0, 0.0), // never indexed, id 0 reserved
            seg(0.0, 20.0, 100.0, 20.0),
        ]);
        assert_eq!(index.len(), 2);
        let snap = index.snap(Point::new(50.0, 22.0), 5.0).unwrap();
        assert_eq!(snap.segment, SegmentId(1));
        // The NaN entry is still addressable (id space stable), just inert.
        assert!(index.segment(SegmentId(0)).is_some());
    }

    #[test]
    fn index_accessors() {
        let index = SegmentIndex::build(vec![seg(0.0, 0.0, 1.0, 1.0)]);
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
        assert_eq!(
            index.segment(SegmentId(0)).unwrap().p2,
            Point::new(1.0, 1.0)
        );
        assert_eq!(index.segment(SegmentId(7)), None);
    }

    #[test]
    fn segment_source_stub() {
        // The extraction seam: a canned source feeds build() end-to-end.
        struct CannedSource;
        impl SegmentSource for CannedSource {
            fn segments_for_page(&self, page_index: u32) -> Vec<Segment> {
                match page_index {
                    0 => vec![Segment {
                        p1: Point::new(0.0, 0.0),
                        p2: Point::new(100.0, 0.0),
                        width: 6.0,
                    }],
                    _ => vec![], // scanned page: no vectors
                }
            }
        }
        let index = SegmentIndex::build(CannedSource.segments_for_page(0));
        assert!(index.snap(Point::new(50.0, 3.0), 5.0).is_some());
        let empty = SegmentIndex::build(CannedSource.segments_for_page(1));
        assert_eq!(empty.snap(Point::new(50.0, 3.0), 5.0), None);
    }
}
