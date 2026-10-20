//! Opt-in address ranges whose write accesses acquire private-copy charge.
//!
//! This metadata is independent of guest permissions and OS-personality flags.
//! Range changes require the same owner lock as the containing VMA map.
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(super) struct PrivateRanges {
    ranges: BTreeMap<u64, u64>,
}

impl PrivateRanges {
    /// Include a nonempty half-open range, coalescing touching neighbours.
    /// O(k log n) time and O(k) temporary space for k affected boundaries.
    pub(super) fn enable(&mut self, start: u64, end: u64) {
        debug_assert!(start < end);
        let first = self
            .ranges
            .range(..=start)
            .next_back()
            .filter(|(_, previous_end)| **previous_end >= start)
            .map_or(start, |(&previous_start, _)| previous_start);
        let keys: Vec<u64> = self
            .ranges
            .range(first..=end)
            .map(|(&key, _)| key)
            .collect();
        let mut begin = start;
        let mut limit = end;
        for key in keys {
            let previous_end = self.ranges.remove(&key).expect("selected range exists");
            begin = begin.min(key);
            limit = limit.max(previous_end);
        }
        self.ranges.insert(begin, limit);
    }

    /// Stop tracking a half-open range while preserving both outside pieces.
    pub(super) fn remove(&mut self, start: u64, end: u64) {
        let keys: Vec<u64> = self
            .intersections(start, end)
            .into_iter()
            .map(|(begin, _)| {
                *self
                    .ranges
                    .range(..=begin)
                    .next_back()
                    .expect("overlap exists")
                    .0
            })
            .collect();
        for key in keys {
            let previous_end = self.ranges.remove(&key).expect("selected range exists");
            if key < start {
                self.ranges.insert(key, start);
            }
            if previous_end > end {
                self.ranges.insert(end, previous_end);
            }
        }
    }

    pub(super) fn contains(&self, address: u64) -> bool {
        self.ranges
            .range(..=address)
            .next_back()
            .is_some_and(|(_, &end)| address < end)
    }

    /// Clipped tracked pieces in ascending order. No absent page is visited.
    pub(super) fn intersections(&self, start: u64, end: u64) -> Vec<(u64, u64)> {
        if start >= end {
            return Vec::new();
        }
        let first = self
            .ranges
            .range(..start)
            .next_back()
            .filter(|(_, previous_end)| **previous_end > start)
            .map_or(start, |(&previous_start, _)| previous_start);
        self.ranges
            .range(first..end)
            .filter_map(|(&begin, &limit)| {
                (limit > start).then_some((begin.max(start), limit.min(end)))
            })
            .collect()
    }

    pub(super) fn bytes(&self, start: u64, end: u64) -> u64 {
        self.intersections(start, end)
            .into_iter()
            .map(|(begin, limit)| limit - begin)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::PrivateRanges;

    #[test]
    fn private_ranges_preserve_gaps_and_both_sides_of_partial_removal() {
        let mut ranges = PrivateRanges::default();
        ranges.enable(0x1000, 0x3000);
        ranges.enable(0x5000, 0x7000);
        assert_eq!(ranges.bytes(0, 0x8000), 0x4000);
        assert!(!ranges.contains(0x3000));
        ranges.enable(0x3000, 0x5000);
        assert_eq!(ranges.intersections(0, 0x8000), [(0x1000, 0x7000)]);
        ranges.remove(0x2000, 0x6000);
        assert_eq!(
            ranges.intersections(0, 0x8000),
            [(0x1000, 0x2000), (0x6000, 0x7000)]
        );
        ranges.remove(0, 0x1800);
        ranges.remove(0x6800, u64::MAX);
        assert_eq!(
            ranges.intersections(0, u64::MAX),
            [(0x1800, 0x2000), (0x6000, 0x6800)]
        );
        assert_eq!(ranges.bytes(0, u64::MAX), 0x1000);
        assert!(ranges.contains(0x1800));
        assert!(!ranges.contains(0x2000));
        assert!(!ranges.contains(0x6800));
    }
}
