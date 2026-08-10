//! Locally-unique identifiers.
//!
//! Not UUIDs: these only need to be unique within one Codex home, and readable
//! when they show up in a filename or a branch name. A nanosecond timestamp
//! gives ordering; a process-wide counter breaks ties when two are minted inside
//! the same nanosecond.

use std::sync::atomic::{AtomicU64, Ordering};

use super::time::unix_nanos;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A short, collision-resistant `<nanos>-<counter>` suffix in hex.
pub(crate) fn unique_suffix() -> String {
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{count:x}", unix_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successive_ids_differ_and_are_hex_pairs() {
        let one = unique_suffix();
        let two = unique_suffix();
        assert_ne!(one, two);
        let (nanos, count) = one.split_once('-').expect("two hex parts");
        assert!(u128::from_str_radix(nanos, 16).is_ok());
        assert!(u64::from_str_radix(count, 16).is_ok());
    }

    #[test]
    fn ids_stay_unique_in_a_tight_loop() {
        let ids: std::collections::HashSet<_> = (0..500).map(|_| unique_suffix()).collect();
        assert_eq!(ids.len(), 500);
    }
}
