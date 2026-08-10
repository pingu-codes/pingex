//! Wall-clock helpers. Each returns 0 if the clock somehow predates the epoch,
//! which never happens in practice but keeps these total.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time in Unix seconds.
pub(crate) fn unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Current wall-clock time in Unix milliseconds.
pub(crate) fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Current wall-clock time in Unix nanoseconds, used where an identifier needs
/// more entropy than a second-resolution stamp gives.
pub(crate) fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_agree_on_magnitude() {
        let secs = unix_secs();
        let millis = unix_millis();
        let nanos = unix_nanos();
        // Sanity: all three read the same instant, so they agree once scaled.
        assert!(secs > 1_600_000_000, "clock looks wrong: {secs}");
        assert!((millis / 1000 - secs).abs() <= 1);
        assert!(((nanos / 1_000_000_000) as i64 - secs).abs() <= 1);
    }
}
