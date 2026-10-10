//! ULID-style ids: 48-bit ms timestamp + 80 random bits, Crockford base32.
//! Lexicographically sortable by creation time.

use std::sync::atomic::{AtomicU64, Ordering};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn random_u128() -> u128 {
    // std-only entropy: RandomState is seeded from the OS per process;
    // mix in time and a counter so ids within one process never repeat.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut a = RandomState::new().build_hasher();
    a.write_u64(c);
    a.write_u128(nanos);
    let mut b = RandomState::new().build_hasher();
    b.write_u64(!c);
    b.write_u128(nanos.rotate_left(17));
    ((a.finish() as u128) << 64) | b.finish() as u128
}

pub fn new_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u128).unwrap_or(0);
    let v: u128 = (ms << 80) | (random_u128() & ((1u128 << 80) - 1));
    (0..26).rev().map(|i| ALPHABET[((v >> (i * 5)) & 31) as usize] as char).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn unique_sortable() {
        let a = super::new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = super::new_id();
        assert_eq!(a.len(), 26);
        assert!(a < b);
        let set: std::collections::HashSet<_> = (0..1000).map(|_| super::new_id()).collect();
        assert_eq!(set.len(), 1000);
    }
}
