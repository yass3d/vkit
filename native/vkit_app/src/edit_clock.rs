use std::sync::atomic::{AtomicU64, Ordering};

static CLOCK: AtomicU64 = AtomicU64::new(1);

pub fn next_seq() -> u64 {
    CLOCK.fetch_add(1, Ordering::Relaxed)
}
