//! Two-pass connected-component labeling with union-find (addendum §A3.2),
//! 4-connectivity, iterative throughout.

use super::raster::Mask;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Component {
    pub(crate) label: u32,
    pub(crate) pixels: usize,
    pub(crate) min_x: u32,
    pub(crate) min_y: u32,
    pub(crate) max_x: u32,
    pub(crate) max_y: u32,
    pub(crate) touches_border: bool,
}

struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new() -> UnionFind {
        UnionFind { parent: vec![0] } // label 0 = background sentinel
    }

    fn make(&mut self) -> u32 {
        let label = self.parent.len() as u32;
        self.parent.push(label);
        label
    }

    /// Path-halving find.
    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let grandparent = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = grandparent;
            x = grandparent;
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi as usize] = lo;
        }
    }
}

/// Label 4-connected components of set pixels. Returns the resolved
/// row-major label grid (0 = background) and per-component stats, ordered
/// by label for determinism.
pub(crate) fn label_components(foreground: &Mask) -> (Vec<u32>, Vec<Component>) {
    let w = foreground.width() as usize;
    let h = foreground.height() as usize;
    let fg = foreground.as_slice();
    let mut labels = vec![0u32; w * h];
    let mut uf = UnionFind::new();

    // Pass 1: provisional labels from left/up neighbors; record equivalences.
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !fg[i] {
                continue;
            }
            let left = if x > 0 { labels[i - 1] } else { 0 };
            let up = if y > 0 { labels[i - w] } else { 0 };
            labels[i] = match (left, up) {
                (0, 0) => uf.make(),
                (l, 0) => l,
                (0, u) => u,
                (l, u) => {
                    uf.union(l, u);
                    l.min(u)
                }
            };
        }
    }

    // Pass 2: resolve to roots and accumulate stats.
    let mut by_root: HashMap<u32, Component> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if labels[i] == 0 {
                continue;
            }
            let root = uf.find(labels[i]);
            labels[i] = root;
            let comp = by_root.entry(root).or_insert(Component {
                label: root,
                pixels: 0,
                min_x: x as u32,
                min_y: y as u32,
                max_x: x as u32,
                max_y: y as u32,
                touches_border: false,
            });
            comp.pixels += 1;
            comp.min_x = comp.min_x.min(x as u32);
            comp.min_y = comp.min_y.min(y as u32);
            comp.max_x = comp.max_x.max(x as u32);
            comp.max_y = comp.max_y.max(y as u32);
            comp.touches_border |= x == 0 || y == 0 || x == w - 1 || y == h - 1;
        }
    }
    let mut components: Vec<Component> = by_root.into_values().collect();
    components.sort_by_key(|c| c.label);
    (labels, components)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ccl_labels_two_separate_regions() {
        let mut m = Mask::new(8, 4);
        // 2×2 block at (1,1); 3×1 run at (5,2).
        for y in 1..=2 {
            for x in 1..=2 {
                m.set(x, y, true);
            }
        }
        for x in 5..=7 {
            m.set(x, 2, true);
        }
        let (labels, comps) = label_components(&m);
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].pixels, 4);
        assert!(!comps[0].touches_border);
        assert_eq!(comps[1].pixels, 3);
        assert!(comps[1].touches_border); // x = 7 is the last column
        assert_eq!((comps[0].min_x, comps[0].max_y), (1, 2));
        // Pixels of the same run resolve to the same label.
        assert_eq!(labels[2 * 8 + 5], labels[2 * 8 + 7]);
    }

    #[test]
    fn ccl_union_find_merges_u_shape() {
        // U: two vertical arms joined at the bottom — the classic case where
        // pass 1 assigns two provisional labels that pass 2 must merge.
        let mut m = Mask::new(5, 4);
        for y in 0..=2 {
            m.set(1, y, true);
            m.set(3, y, true);
        }
        for x in 1..=3 {
            m.set(x, 3, true);
        }
        let (labels, comps) = label_components(&m);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].pixels, 9);
        assert_eq!(labels[1], labels[3]); // both arms resolved to one root
    }
}
