//! A small in-process cache whose entries always expire.
//!
//! Every backend cache uses this, including the ones holding answers that can
//! never change (anything keyed by a commit SHA), so memory stays bounded and a
//! surprising answer cannot outlive `max_age`. Mutable git state is never
//! cached here: the frontend's query cache owns that, because only it knows
//! when a mutation, a focus change or a finished turn made it stale.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
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
    /// One lock per key `get_or_try` is computing, so callers that miss at
    /// the same moment run `compute` once rather than each shelling out.
    computing: Mutex<HashMap<K, Arc<Mutex<()>>>>,
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
            computing: Mutex::new(HashMap::new()),
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
    ///
    /// Concurrent callers for one key wait for whichever got there first and
    /// take its answer: two commands a page fires together (the inbox and the
    /// stacks both need the repository's slug) cost one `gh` call, not two.
    /// It blocks, so callers run it on blocking threads, as for `compute`.
    pub(crate) fn get_or_try<E>(
        &self,
        key: K,
        compute: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let Some(slot) = self
            .computing
            .lock()
            .ok()
            .map(|mut computing| computing.entry(key.clone()).or_default().clone())
        else {
            return self.compute_and_insert(key, compute);
        };
        let guard = slot.lock();
        // Whoever held the slot may have just filled the cache.
        let result = match self.get(&key) {
            Some(value) => Ok(value),
            None => self.compute_and_insert(key.clone(), compute),
        };
        drop(guard);
        if let Ok(mut computing) = self.computing.lock() {
            // Two strong references: the map's and this caller's; nobody else
            // is waiting on the slot, so it can go.
            if computing
                .get(&key)
                .is_some_and(|held| Arc::strong_count(held) <= 2)
            {
                computing.remove(&key);
            }
        }
        result
    }

    fn compute_and_insert<E>(
        &self,
        key: K,
        compute: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E> {
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

    #[test]
    fn concurrent_misses_compute_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache: Arc<TtlCache<&str, i32>> = Arc::new(TtlCache::new(MINUTE, 10));
        let calls = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (cache, calls) = (cache.clone(), calls.clone());
                std::thread::spawn(move || {
                    cache.get_or_try("slug", || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        // Long enough that every thread misses before it ends.
                        std::thread::sleep(Duration::from_millis(100));
                        Ok::<_, ()>(7)
                    })
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap(), Ok(7));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(cache.computing.lock().unwrap().is_empty());
    }
}
