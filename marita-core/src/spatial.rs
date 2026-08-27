//! Uniform-grid spatial index for dynamic 2D entities.
//!
//! The index divides the simulation plane into square cells. Insertions and
//! queries are O(1) per entity, making it cheap to rebuild every tick.

use glam::DVec2;
use std::collections::HashMap;
use std::collections::HashSet;

/// A uniform-grid spatial index for entities with a position and radius.
pub struct SpatialIndex {
    cell_size: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl SpatialIndex {
    /// Create an index with the given cell size. A good default is the largest
    /// entity radius expected, plus a safety margin.
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size: cell_size.max(1.0),
            cells: HashMap::new(),
        }
    }

    /// Clear the index for reuse.
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Insert an entity index at `position`.
    pub fn insert(&mut self, position: DVec2, entity_index: usize) {
        let key = self.key(position);
        self.cells.entry(key).or_default().push(entity_index);
    }

    /// Query all candidate entity indices whose cells overlap the given
    /// axis-aligned bounding box. The caller must still perform its own
    /// exact distance/geometry test.
    pub fn query_aabb(&self, min: DVec2, max: DVec2) -> impl Iterator<Item = usize> + '_ {
        let min_x = (min.x / self.cell_size).floor() as i64;
        let min_y = (min.y / self.cell_size).floor() as i64;
        let max_x = (max.x / self.cell_size).ceil() as i64;
        let max_y = (max.y / self.cell_size).ceil() as i64;

        let mut seen = HashSet::new();
        let mut results = Vec::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(indices) = self.cells.get(&(x, y)) {
                    for &idx in indices {
                        if seen.insert(idx) {
                            results.push(idx);
                        }
                    }
                }
            }
        }
        results.into_iter()
    }

    /// Query candidate entity indices that may lie within `radius` of `center`.
    pub fn query_circle(&self, center: DVec2, radius: f64) -> impl Iterator<Item = usize> + '_ {
        let r = radius.abs();
        let min = DVec2::new(center.x - r, center.y - r);
        let max = DVec2::new(center.x + r, center.y + r);
        self.query_aabb(min, max)
    }

    fn key(&self, position: DVec2) -> (i64, i64) {
        (
            (position.x / self.cell_size).floor() as i64,
            (position.y / self.cell_size).floor() as i64,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_queries_candidates() {
        let mut index = SpatialIndex::new(10.0);
        index.insert(DVec2::new(5.0, 5.0), 0);
        index.insert(DVec2::new(100.0, 100.0), 1);

        let found: Vec<_> = index.query_circle(DVec2::new(5.0, 5.0), 10.0).collect();
        assert!(found.contains(&0));
        assert!(!found.contains(&1));
    }

    #[test]
    fn query_returns_unique_indices() {
        let mut index = SpatialIndex::new(1.0);
        index.insert(DVec2::new(0.5, 0.5), 0);
        index.insert(DVec2::new(1.5, 1.5), 1);
        // Point (1.0, 1.0) lies on the corner of four cells; index 0 and 1
        // are in two of those cells.
        let found: Vec<_> = index.query_circle(DVec2::new(1.0, 1.0), 2.0).collect();
        assert_eq!(found.len(), 2);
    }
}
