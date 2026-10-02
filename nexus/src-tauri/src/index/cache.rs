//! Hot tier of the two-tier index (spec §9.8): the most recently used files'
//! contents in RAM. Entries are validated against (mtime, len) on every hit,
//! so an external edit can never serve stale content.

use std::collections::HashMap;
use std::sync::Arc;

pub const HOT_CAPACITY: usize = 500;

#[derive(Clone)]
pub struct HotEntry {
    pub content: Arc<String>,
    pub mtime: i64,
    pub len: u64,
}

pub struct HotCache {
    cap: usize,
    tick: u64,
    map: HashMap<String, (HotEntry, u64)>,
}

impl HotCache {
    pub fn new(cap: usize) -> Self {
        HotCache { cap, tick: 0, map: HashMap::with_capacity(cap + 1) }
    }

    fn key(rel: &str) -> String {
        rel.to_lowercase()
    }

    pub fn get(&mut self, rel: &str, mtime: i64, len: u64) -> Option<Arc<String>> {
        self.tick += 1;
        let tick = self.tick;
        let key = Self::key(rel);
        match self.map.get_mut(&key) {
            Some((e, t)) if e.mtime == mtime && e.len == len => {
                *t = tick;
                Some(e.content.clone())
            }
            Some(_) => {
                self.map.remove(&key);
                None
            }
            None => None,
        }
    }

    pub fn put(&mut self, rel: &str, entry: HotEntry) {
        self.tick += 1;
        self.map.insert(Self::key(rel), (entry, self.tick));
        if self.map.len() > self.cap {
            // Evict the oldest ~10% in one pass to amortise the scan.
            let mut ticks: Vec<u64> = self.map.values().map(|(_, t)| *t).collect();
            let cut_idx = (self.cap / 10).max(1);
            ticks.select_nth_unstable(cut_idx);
            let cut = ticks[cut_idx];
            self.map.retain(|_, (_, t)| *t > cut);
        }
    }

    pub fn invalidate(&mut self, rel: &str) {
        self.map.remove(&Self::key(rel));
    }

    pub fn invalidate_prefix(&mut self, rel_dir: &str) {
        let p = format!("{}/", Self::key(rel_dir));
        self.map.retain(|k, _| !k.starts_with(&p));
    }

    pub fn clear_all(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(s: &str, mtime: i64) -> HotEntry {
        HotEntry { content: Arc::new(s.into()), mtime, len: s.len() as u64 }
    }

    #[test]
    fn lru_and_validation() {
        let mut c = HotCache::new(10);
        for i in 0..10 {
            c.put(&format!("f{i}"), entry("x", 1));
        }
        assert!(c.get("f0", 1, 1).is_some()); // touch f0 so it survives
        c.put("f10", entry("x", 1));
        assert!(c.len() <= 10);
        assert!(c.get("f0", 1, 1).is_some());
        assert!(c.get("f0", 2, 1).is_none(), "mtime mismatch evicts");
        assert!(c.get("f0", 1, 1).is_none());
    }
}
