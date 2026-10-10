//! Bloom filter for "does this file / link target exist?" checks (spec §9.9).
//! A negative answer is definitive and skips SQLite; a positive is confirmed.

use xxhash_rust::xxh3::xxh3_128;

pub struct Bloom {
    bits: Vec<u64>,
    m: u64,
    k: u32,
    len: usize,
}

impl Bloom {
    /// Sized for `capacity` items at ~1% false-positive rate.
    pub fn with_capacity(capacity: usize) -> Bloom {
        let n = capacity.max(1024) as f64;
        let m = (-(n * 0.01f64.ln()) / (2f64.ln().powi(2))).ceil() as u64;
        let m = m.next_multiple_of(64);
        let k = ((m as f64 / n) * 2f64.ln()).round().clamp(1.0, 16.0) as u32;
        Bloom { bits: vec![0; (m / 64) as usize], m, k, len: 0 }
    }

    fn positions(&self, key: &str) -> impl Iterator<Item = u64> + '_ {
        // Kirsch–Mitzenmacher double hashing from one 128-bit hash.
        let h = xxh3_128(key.to_lowercase().as_bytes());
        let h1 = h as u64;
        let h2 = (h >> 64) as u64 | 1;
        (0..self.k as u64).map(move |i| h1.wrapping_add(i.wrapping_mul(h2)) % self.m)
    }

    pub fn insert(&mut self, key: &str) {
        let pos: Vec<u64> = self.positions(key).collect();
        for p in pos {
            self.bits[(p / 64) as usize] |= 1 << (p % 64);
        }
        self.len += 1;
    }

    pub fn may_contain(&self, key: &str) -> bool {
        self.positions(key).all(|p| self.bits[(p / 64) as usize] & (1 << (p % 64)) != 0)
    }

    /// Inserts since the last rebuild; used to decide when to resize.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity_bits(&self) -> u64 {
        self.m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_negatives_low_false_positives() {
        let mut b = Bloom::with_capacity(10_000);
        for i in 0..10_000 {
            b.insert(&format!("notes/file-{i}.md"));
        }
        for i in 0..10_000 {
            assert!(b.may_contain(&format!("notes/file-{i}.md")));
        }
        let fp = (0..10_000).filter(|i| b.may_contain(&format!("other/{i}"))).count();
        assert!(fp < 300, "false positives: {fp}");
        assert!(b.may_contain("NOTES/FILE-1.MD"), "case-insensitive like the path column");
    }
}
