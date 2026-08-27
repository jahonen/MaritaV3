//! Adaptive 2D quadtree for spatial indexing of points and regions.
//!
//! The tree is rebuilt deterministically each tick. Items are stored in the
//! smallest node that fully contains their axis-aligned bounding box. This
//! naturally handles objects of vastly different scales (e.g., metre-scale
//! ship emitters and AU-scale sunlight) without duplicating large items into
//! many leaves.

use glam::DVec2;

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: DVec2,
    pub max: DVec2,
}

impl Aabb {
    /// Create a degenerate Aabb from a single point.
    pub fn from_point(p: DVec2) -> Self {
        Self { min: p, max: p }
    }

    /// Create a square Aabb around a circle.
    pub fn from_circle(center: DVec2, radius: f64) -> Self {
        let r = radius.abs();
        Self {
            min: DVec2::new(center.x - r, center.y - r),
            max: DVec2::new(center.x + r, center.y + r),
        }
    }

    /// Expand the Aabb uniformly in every direction.
    pub fn expanded(&self, margin: f64) -> Self {
        Self {
            min: DVec2::new(self.min.x - margin, self.min.y - margin),
            max: DVec2::new(self.max.x + margin, self.max.y + margin),
        }
    }

    /// Union of two Aabbs.
    pub fn union(&self, other: Aabb) -> Self {
        Self {
            min: DVec2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: DVec2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    /// True if `other` is completely inside this Aabb.
    pub fn contains(&self, other: Aabb) -> bool {
        other.min.x >= self.min.x
            && other.min.y >= self.min.y
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    /// True if this Aabb and `other` overlap.
    pub fn overlaps(&self, other: Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }

    pub fn center(&self) -> DVec2 {
        (self.min + self.max) * 0.5
    }

    pub fn half_size(&self) -> DVec2 {
        (self.max - self.min) * 0.5
    }
}

#[derive(Debug, Clone)]
struct Entry {
    index: usize,
    aabb: Aabb,
}

#[derive(Debug, Clone)]
struct Node {
    bounds: Aabb,
    children: Option<[Box<Node>; 4]>,
    items: Vec<Entry>,
}

impl Node {
    fn new(bounds: Aabb) -> Self {
        Self {
            bounds,
            children: None,
            items: Vec::new(),
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    fn child_bounds(&self) -> [Aabb; 4] {
        let c = self.bounds.center();
        [
            // bottom-left
            Aabb {
                min: self.bounds.min,
                max: c,
            },
            // bottom-right
            Aabb {
                min: DVec2::new(c.x, self.bounds.min.y),
                max: DVec2::new(self.bounds.max.x, c.y),
            },
            // top-left
            Aabb {
                min: DVec2::new(self.bounds.min.x, c.y),
                max: DVec2::new(c.x, self.bounds.max.y),
            },
            // top-right
            Aabb {
                min: c,
                max: self.bounds.max,
            },
        ]
    }

    fn split(&mut self) {
        if self.children.is_some() {
            return;
        }
        let child_bounds = self.child_bounds();
        self.children = Some([
            Box::new(Node::new(child_bounds[0])),
            Box::new(Node::new(child_bounds[1])),
            Box::new(Node::new(child_bounds[2])),
            Box::new(Node::new(child_bounds[3])),
        ]);
        // Redistribute current items into children where possible.
        let items = std::mem::take(&mut self.items);
        for entry in items {
            self.insert_child(entry);
        }
    }

    fn insert_child(&mut self, entry: Entry) {
        if let Some(children) = &mut self.children {
            // An AABB can be fully contained by at most one child.
            let target = children
                .iter()
                .position(|child| child.bounds.contains(entry.aabb));
            if let Some(idx) = target {
                children[idx].items.push(entry);
            } else {
                // Item straddles a split boundary; keep it at this level.
                self.items.push(entry);
            }
        }
    }

    fn insert(&mut self, entry: Entry, capacity: usize, max_depth: u32, depth: u32) {
        if depth == max_depth {
            self.items.push(entry);
            return;
        }

        if self.is_leaf() {
            self.items.push(entry);
            if self.items.len() > capacity && depth < max_depth {
                self.split();
            }
            return;
        }

        // Already has children. Try to push item into a single child.
        self.insert_child(entry);

        // If the item straddles children it stays in self.items.
    }
}

/// Adaptive 2D quadtree for spatial indexing.
#[derive(Debug, Clone)]
pub struct Quadtree {
    root: Node,
    capacity: usize,
    max_depth: u32,
}

impl Quadtree {
    /// Build a tree with fixed bounds.
    pub fn new(bounds: Aabb, capacity: usize, max_depth: u32) -> Self {
        Self {
            root: Node::new(bounds),
            capacity,
            max_depth,
        }
    }

    /// Build a tree from item AABBs, computing bounds automatically.
    pub fn build(items: &[(usize, Aabb)], capacity: usize, max_depth: u32) -> Self {
        if items.is_empty() {
            // Use a unit AABB around the origin when there are no items.
            return Self::new(
                Aabb {
                    min: DVec2::new(-1.0, -1.0),
                    max: DVec2::new(1.0, 1.0),
                },
                capacity,
                max_depth,
            );
        }
        let mut bounds = items[0].1;
        for (_, aabb) in items.iter().skip(1) {
            bounds = bounds.union(*aabb);
        }
        // Add a small margin so edge items do not sit exactly on the boundary.
        let margin = (bounds.max - bounds.min).length() * 1e-6 + 1.0;
        let bounds = bounds.expanded(margin);
        let mut tree = Self::new(bounds, capacity, max_depth);
        for (index, aabb) in items {
            tree.insert(*index, *aabb);
        }
        tree
    }

    /// Insert a single item. Deterministic rebuild is preferred; this is
    /// exposed for incremental construction in tests.
    pub fn insert(&mut self, index: usize, aabb: Aabb) {
        let entry = Entry { index, aabb };
        self.root.insert(entry, self.capacity, self.max_depth, 0);
    }

    /// Query all item indices whose AABB overlaps `region`.
    pub fn query_region(&self, region: Aabb) -> Vec<usize> {
        let mut out = Vec::new();
        Self::query_region_node(&self.root, region, &mut out);
        out
    }

    fn query_region_node(node: &Node, region: Aabb, out: &mut Vec<usize>) {
        if !node.bounds.overlaps(region) {
            return;
        }
        for entry in &node.items {
            if entry.aabb.overlaps(region) {
                out.push(entry.index);
            }
        }
        if let Some(children) = &node.children {
            for child in children {
                Self::query_region_node(child, region, out);
            }
        }
    }

    /// Query all item indices whose AABB overlaps the circle centered at
    /// `center` with radius `radius`.
    pub fn query_circle(&self, center: DVec2, radius: f64) -> Vec<usize> {
        self.query_region(Aabb::from_circle(center, radius))
    }

    /// Query all item indices whose AABB contains `point`.
    pub fn query_point(&self, point: DVec2) -> Vec<usize> {
        let point_aabb = Aabb::from_point(point);
        self.query_region(point_aabb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_point_finds_inserted_point() {
        let mut tree = Quadtree::new(
            Aabb {
                min: DVec2::new(-100.0, -100.0),
                max: DVec2::new(100.0, 100.0),
            },
            4,
            8,
        );
        tree.insert(0, Aabb::from_point(DVec2::new(10.0, 10.0)));
        tree.insert(1, Aabb::from_point(DVec2::new(-50.0, 50.0)));

        let found: Vec<_> = tree.query_point(DVec2::new(10.0, 10.0));
        assert!(found.contains(&0));
        assert!(!found.contains(&1));
    }

    #[test]
    fn query_region_finds_overlapping_items() {
        let mut tree = Quadtree::new(
            Aabb {
                min: DVec2::new(-100.0, -100.0),
                max: DVec2::new(100.0, 100.0),
            },
            4,
            8,
        );
        tree.insert(0, Aabb::from_circle(DVec2::new(10.0, 10.0), 5.0));
        tree.insert(1, Aabb::from_circle(DVec2::new(-50.0, 50.0), 5.0));

        let found: Vec<_> = tree.query_region(Aabb::from_circle(DVec2::new(12.0, 12.0), 2.0));
        assert!(found.contains(&0));
        assert!(!found.contains(&1));
    }

    #[test]
    fn large_region_stored_at_coarse_node() {
        // A region larger than child quadrants should be stored at the root
        // or a coarse internal node, and still be found by a point query.
        let mut tree = Quadtree::new(
            Aabb {
                min: DVec2::new(-100.0, -100.0),
                max: DVec2::new(100.0, 100.0),
            },
            2,
            8,
        );
        tree.insert(0, Aabb::from_circle(DVec2::new(0.0, 0.0), 60.0));
        tree.insert(1, Aabb::from_point(DVec2::new(10.0, 10.0)));

        let found: Vec<_> = tree.query_point(DVec2::new(50.0, 50.0));
        assert!(found.contains(&0));
        assert!(!found.contains(&1));
    }

    #[test]
    fn build_computes_bounds() {
        let items = vec![
            (0usize, Aabb::from_point(DVec2::new(1.0, 2.0))),
            (1usize, Aabb::from_point(DVec2::new(-3.0, 4.0))),
        ];
        let tree = Quadtree::build(&items, 4, 8);
        assert!(tree
            .root
            .bounds
            .contains(Aabb::from_point(DVec2::new(1.0, 2.0))));
        assert!(tree
            .root
            .bounds
            .contains(Aabb::from_point(DVec2::new(-3.0, 4.0))));

        let found: Vec<_> = tree.query_point(DVec2::new(1.0, 2.0));
        assert!(found.contains(&0));
    }
}
