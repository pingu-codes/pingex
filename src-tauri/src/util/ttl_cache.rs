//! A small in-process cache whose entries always expire.
//!
//! Every backend cache uses this, including the ones holding answers that can
//! never change (anything keyed by a commit SHA), so memory stays bounded and a
//! surprising answer cannot outlive `max_age`. Mutable git state is never
//! cached here: the frontend's query cache owns that, because only it knows
//! when a mutation, a focus change or a finished turn made it stale.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Entry<V> {
    value: V,
    inserted: Instant,
    weight: usize,
}

pub(crate) struct TtlCache<K, V> {
    entries: Mutex<HashMap<K, Entry<V>>>,
    max_age: Duration,
    max_entries: usize,
    /// Upper bound on the summed `weight` of all entries; `usize::MAX` when the
    /// cache is bounded by count alone.
    max_weight: usize,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    pub(crate) fn new(max_age: Duration, max_entries: usize) -> Self {
        Self::weighted(max_age, max_entries, usize::MAX)
    }

    /// A cache also bounded by the summed weight of its entries (bytes, for
    /// diffs and file contents), as given to `insert_weighted`.
    pub(crate) fn weighted(max_age: Duration, max_entries: usize, max_weight: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_age,
            max_entries,
            max_weight,
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<V> {
        self.get_at(key, Instant::now())
    }

    pub(crate) fn insert(&self, key: K, value: V) {
        self.insert_at(key, value, 1, Instant::now());
    }

    /// Insert with a weight; a value heavier than the whole budget is not kept.
    pub(crate) fn insert_weighted(&self, key: K, value: V, weight: usize) {
        self.insert_at(key, value, weight, Instant::now());
    }

    /// The cached value, or `compute`'s answer, which is kept only when it is
    /// `Ok`, so a failure is retried on the next call.
    pub(crate) fn get_or_try<E>(
        &self,
        key: K,
        compute: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let value = compute()?;
        self.insert(key, value.clone());
        Ok(value)
    }

    fn get_at(&self, key: &K, now: Instant) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if now.saturating_duration_since(entry.inserted) >= self.max_age {
            entries.remove(key);
            return None;
        }
        Some(entry.value.clone())
    }

    fn insert_at(&self, key: K, value: V, weight: usize, now: Instant) {
        if weight > self.max_weight || self.max_entries == 0 {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.remove(&key);
        entries.retain(|_, entry| now.saturating_duration_since(entry.inserted) < self.max_age);
        let mut total: usize = entries.values().map(|entry| entry.weight).sum();
        while !entries.is_empty()
            && (entries.len() >= self.max_entries || total + weight > self.max_weight)
        {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.inserted)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = entries.remove(&oldest) {
                total -= removed.weight;
            }
        }
        entries.insert(
            key,
            Entry {
                value,
                inserted: now,
                weight,
            },
        );
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn an_entry_expires_after_max_age() {
        let cache = TtlCache::new(MINUTE, 10);
        let start = Instant::now();
        cache.insert_at("a", 1, 1, start);
        assert_eq!(cache.get_at(&"a", start + Duration::from_secs(59)), Some(1));
        assert_eq!(cache.get_at(&"a", start + MINUTE), None);
        assert_eq!(cache.len(), 0, "an expired entry is dropped when read");
    }

    #[test]
    fn the_oldest_entry_goes_when_the_cache_is_full() {
        let cache = TtlCache::new(MINUTE, 2);
        let start = Instant::now();
        cache.insert_at("a", 1, 1, start);
        cache.insert_at("b", 2, 1, start + Duration::from_secs(1));
        cache.insert_at("c", 3, 1, start + Duration::from_secs(2));
        let now = start + Duration::from_secs(3);
        assert_eq!(cache.get_at(&"a", now), None);
        assert_eq!(cache.get_at(&"b", now), Some(2));
        assert_eq!(cache.get_at(&"c", now), Some(3));
    }

    #[test]
    fn replacing_a_key_does_not_evict_another() {
        let cache = TtlCache::new(MINUTE, 2);
        let start = Instant::now();
        cache.insert_at("a", 1, 1, start);
        cache.insert_at("b", 2, 1, start);
        cache.insert_at("a", 9, 1, start + Duration::from_secs(1));
        let now = start + Duration::from_secs(2);
        assert_eq!(cache.get_at(&"a", now), Some(9));
        assert_eq!(cache.get_at(&"b", now), Some(2));
    }

    #[test]
    fn weight_bounds_the_cache_and_oversized_values_are_skipped() {
        let cache = TtlCache::weighted(MINUTE, 100, 10);
        let start = Instant::now();
        cache.insert_at("a", 1, 6, start);
        cache.insert_at("b", 2, 6, start + Duration::from_secs(1));
        let now = start + Duration::from_secs(2);
        assert_eq!(cache.get_at(&"a", now), None, "a goes to make room for b");
        assert_eq!(cache.get_at(&"b", now), Some(2));
        cache.insert_at("huge", 3, 11, now);
        assert_eq!(cache.get_at(&"huge", now), None);
        assert_eq!(
            cache.get_at(&"b", now),
            Some(2),
            "a value that never fits evicts nothing"
        );
    }

    #[test]
    fn expired_entries_are_swept_on_insert() {
        let cache = TtlCache::new(MINUTE, 10);
        let start = Instant::now();
        cache.insert_at("a", 1, 1, start);
        cache.insert_at("b", 2, 1, start + MINUTE * 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn get_or_try_keeps_only_successes() {
        let cache: TtlCache<&str, i32> = TtlCache::new(MINUTE, 10);
        assert_eq!(cache.get_or_try("a", || Err::<i32, ()>(())), Err(()));
        assert_eq!(cache.get_or_try("a", || Ok::<_, ()>(5)), Ok(5));
        assert_eq!(cache.get_or_try("a", || Ok::<_, ()>(6)), Ok(5));
    }
}
