//! A tiny bounded TTL cache for API response bodies.
//!
//! The launcher caches raw response text keyed by request URL rather than
//! deserialised structs: it keeps the cache free of generics, and re-parsing a
//! cached body costs microseconds against a network round trip. Entries are
//! capped so a long session browsing thousands of servers cannot grow without
//! bound.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a cached entry stays fresh, per class of request.
#[derive(Debug, Clone, Copy)]
pub struct Ttl(pub Duration);

impl Ttl {
    /// Server list and detail pages. The indexer publishes a new snapshot about
    /// once a minute; 15s keeps the view current without hammering the API when
    /// the user flips between filters.
    pub const SNAPSHOT: Ttl = Ttl(Duration::from_secs(15));
    /// Population history. The narrowest bucket the API produces is 5 minutes,
    /// so a fresh request inside a minute cannot show the user anything new.
    pub const HISTORY: Ttl = Ttl(Duration::from_secs(60));
    /// Ping sites change rarely; the response carries its own `updatedAt`.
    pub const STABLE: Ttl = Ttl(Duration::from_secs(60 * 60));
}

struct Entry {
    stored_at: Instant,
    body: String,
}

/// A process-lifetime cache of response bodies.
pub struct ResponseCache {
    entries: Mutex<HashMap<String, Entry>>,
    capacity: usize,
}

impl ResponseCache {
    /// Creates a cache holding at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity: capacity.max(1),
        }
    }

    /// Returns the cached body for `key` when it is still within `ttl`.
    pub fn get(&self, key: &str, ttl: Ttl) -> Option<String> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        (entry.stored_at.elapsed() < ttl.0).then(|| entry.body.clone())
    }

    /// Stores `body` under `key`, evicting the oldest entry when full.
    pub fn put(&self, key: &str, body: String) {
        let Ok(mut entries) = self.entries.lock() else {
            // A poisoned cache is a cache miss, never a failed request.
            return;
        };
        if entries.len() >= self.capacity && !entries.contains_key(key) {
            // Linear scan for the oldest entry. At this capacity that is far
            // cheaper than maintaining a second index.
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, e)| e.stored_at)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key.to_owned(),
            Entry {
                stored_at: Instant::now(),
                body,
            },
        );
    }

    /// Drops every entry. Used when the API base URL changes.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    /// Number of entries currently held, fresh or not.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// True when the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANT: Ttl = Ttl(Duration::ZERO);

    #[test]
    fn stores_and_reads_back() {
        let cache = ResponseCache::new(4);
        cache.put("a", "body".into());
        assert_eq!(cache.get("a", Ttl::SNAPSHOT).as_deref(), Some("body"));
    }

    #[test]
    fn misses_on_unknown_key() {
        let cache = ResponseCache::new(4);
        assert!(cache.get("nope", Ttl::SNAPSHOT).is_none());
    }

    #[test]
    fn expired_entries_are_not_returned() {
        let cache = ResponseCache::new(4);
        cache.put("a", "body".into());
        // A zero TTL means every entry is already stale.
        assert!(cache.get("a", INSTANT).is_none());
    }

    #[test]
    fn eviction_keeps_the_cache_bounded() {
        let cache = ResponseCache::new(2);
        cache.put("a", "1".into());
        cache.put("b", "2".into());
        cache.put("c", "3".into());
        assert_eq!(cache.len(), 2);
        // "a" was oldest, so it went first.
        assert!(cache.get("a", Ttl::SNAPSHOT).is_none());
        assert!(cache.get("c", Ttl::SNAPSHOT).is_some());
    }

    #[test]
    fn overwriting_an_existing_key_does_not_evict() {
        let cache = ResponseCache::new(2);
        cache.put("a", "1".into());
        cache.put("b", "2".into());
        cache.put("a", "updated".into());
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("a", Ttl::SNAPSHOT).as_deref(), Some("updated"));
        assert!(cache.get("b", Ttl::SNAPSHOT).is_some());
    }

    #[test]
    fn clear_empties_the_cache() {
        let cache = ResponseCache::new(4);
        cache.put("a", "1".into());
        assert!(!cache.is_empty());
        cache.clear();
        assert!(cache.is_empty());
    }
}
