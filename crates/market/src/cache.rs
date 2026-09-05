//! A small time-to-live cache in front of the providers.
//!
//! Without it, a dashboard showing eight positions would hit the network eight
//! times a minute per source and burn a free tier in an afternoon.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct TtlCache<K, V> {
    ttl: Duration,
    entries: Mutex<HashMap<K, Entry<V>>>,
}

#[derive(Debug, Clone)]
struct Entry<V> {
    value: V,
    stored_at: Instant,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.stored_at.elapsed() > self.ttl {
            entries.remove(key);
            return None;
        }
        Some(entry.value.clone())
    }

    pub fn insert(&self, key: K, value: V) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        // Bound the map by dropping what has already expired. The working set is
        // a few dozen symbols, so this stays cheap and needs no LRU machinery.
        entries.retain(|_, e| e.stored_at.elapsed() <= self.ttl);
        entries.insert(
            key,
            Entry {
                value,
                stored_at: Instant::now(),
            },
        );
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unchecked_time_subtraction,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[test]
    fn a_value_comes_back_within_its_lifetime() {
        let cache = TtlCache::new(Duration::from_secs(60));
        cache.insert("btc", 42);
        assert_eq!(cache.get(&"btc"), Some(42));
    }

    #[test]
    fn an_expired_value_is_gone_not_stale() {
        let cache = TtlCache::new(Duration::ZERO);
        cache.insert("btc", 42);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(cache.get(&"btc"), None);
    }

    #[test]
    fn expired_entries_do_not_pile_up() {
        let cache = TtlCache::new(Duration::from_millis(1));
        for i in 0..100 {
            cache.insert(i, i);
        }
        std::thread::sleep(Duration::from_millis(5));
        cache.insert(999, 999);
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
    }
}
