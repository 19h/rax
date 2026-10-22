//! Exact issued-thread identity history. Inclusive ranges keep ordinary
//! sequential allocation compact without claiming gaps from clone3 set_tid.

use std::collections::BTreeMap;

/// Numeric identities issued to this virtual process, including retired tasks.
/// Membership/insertion are O(log R), retained space O(R), for R disjoint runs.
/// Sequential allocation retains one run rather than one node per old thread.
#[derive(Debug, Default)]
pub struct ThreadIds {
    runs: BTreeMap<i32, i32>,
}

impl ThreadIds {
    pub(crate) fn new(leader: i32) -> Self {
        let mut ids = Self::default();
        ids.record(leader);
        ids
    }

    pub(crate) fn contains(&self, id: i32) -> bool {
        self.runs
            .range(..=id)
            .next_back()
            .is_some_and(|(_, &end)| id <= end)
    }

    pub(crate) fn record(&mut self, id: i32) {
        let (mut start, mut end) = (id, id);
        if let Some((&left, &right)) = self.runs.range(..=id).next_back() {
            if id <= right {
                return;
            }
            if right.checked_add(1) == Some(id) {
                start = left;
                self.runs.remove(&left);
            }
        }
        if let Some((&left, &right)) = self.runs.range(id..).next() {
            if id.checked_add(1) == Some(left) {
                end = right;
                self.runs.remove(&left);
            }
        }
        self.runs.insert(start, end);
    }

    pub(crate) fn reset(&mut self, leader: i32) {
        self.runs.clear();
        self.record(leader);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn procfs_thread_ids_match_exact_membership_at_sparse_and_integer_boundaries() {
        let mut ids = ThreadIds::default();
        let mut expected = BTreeSet::new();
        for id in [
            i32::MIN,
            i32::MAX,
            4_000_000,
            100,
            102,
            101,
            99,
            103,
            -1,
            0,
            1,
            i32::MIN + 1,
            i32::MAX - 1,
            101,
            4_000_000,
        ] {
            ids.record(id);
            expected.insert(id);
            for probe in [
                i32::MIN,
                i32::MIN + 1,
                i32::MIN + 2,
                -2,
                -1,
                0,
                1,
                2,
                98,
                99,
                100,
                101,
                102,
                103,
                104,
                3_999_999,
                4_000_000,
                4_000_001,
                i32::MAX - 2,
                i32::MAX - 1,
                i32::MAX,
            ] {
                assert_eq!(
                    ids.contains(probe),
                    expected.contains(&probe),
                    "{id}/{probe}"
                );
            }
        }
    }

    #[test]
    fn procfs_thread_ids_keep_sequential_lifetimes_in_one_run_and_reset_fork_ownership() {
        let mut ids = ThreadIds::new(1000);
        for id in 1001..11000 {
            ids.record(id);
        }
        assert_eq!(ids.runs.len(), 1);
        assert!(ids.contains(1000) && ids.contains(10999));
        assert!(!ids.contains(999) && !ids.contains(11000));
        ids.reset(42);
        assert_eq!(ids.runs.len(), 1);
        assert!(ids.contains(42));
        assert!(!ids.contains(1000) && !ids.contains(10999));
    }
}
