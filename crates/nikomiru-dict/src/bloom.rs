// SPDX-License-Identifier: GPL-3.0-only

//! A Bloom filter over the keys of a dictionary's indexes. Most strings a
//! lookup tries are in no dictionary (prefixes, deinflections, mixed-spelling
//! readings: thousands per click); the filter answers those without touching
//! the B-trees (NFR-PERF-04). Fushi keeps a `bloom.filter` for the same reason.

/// Bits per key: about 0.8% false positives with [`HASHES`].
const BITS_PER_KEY: usize = 10;
/// Probes per key.
const HASHES: u64 = 7;

/// The filter. Its bytes are stored in the dictionary file, so the hash must
/// never change: FNV-1a, mixed by SplitMix64 for the second hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloom {
    bits: Vec<u64>,
}

fn hashes(key: &str) -> (u64, u64) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut z = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (h, (z ^ (z >> 31)) | 1)
}

impl Bloom {
    /// An empty filter sized for `keys` keys.
    pub fn with_capacity(keys: usize) -> Self {
        Self {
            bits: vec![0; (keys.max(1) * BITS_PER_KEY).div_ceil(64)],
        }
    }

    fn probes(&self, key: &str) -> impl Iterator<Item = (usize, u64)> {
        let m = self.bits.len() as u64 * 64;
        let (h1, h2) = hashes(key);
        (0..HASHES).map(move |i| {
            let bit = h1.wrapping_add(i.wrapping_mul(h2)) % m;
            ((bit / 64) as usize, 1 << (bit % 64))
        })
    }

    /// Adds a key.
    pub fn insert(&mut self, key: &str) {
        for (word, mask) in self.probes(key).collect::<Vec<_>>() {
            self.bits[word] |= mask;
        }
    }

    /// `false` if the key was never added; `true` if it probably was.
    pub fn may_contain(&self, key: &str) -> bool {
        self.probes(key)
            .all(|(word, mask)| self.bits[word] & mask != 0)
    }

    /// The bytes to store.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A filter from stored bytes; `None` if they cannot be one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || !bytes.len().is_multiple_of(8) {
            return None;
        }
        let bits = bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect();
        Some(Self { bits })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_keys_are_always_found_and_others_rarely() {
        let mut bloom = Bloom::with_capacity(10_000);
        for i in 0..10_000 {
            bloom.insert(&format!("語{i}"));
        }
        assert!((0..10_000).all(|i| bloom.may_contain(&format!("語{i}"))));
        let false_positives = (0..10_000)
            .filter(|i| bloom.may_contain(&format!("後{i}")))
            .count();
        assert!(
            false_positives < 200,
            "{false_positives} false positives in 10000"
        );
        let back = Bloom::from_bytes(&bloom.to_bytes()).unwrap();
        assert_eq!(back, bloom);
        assert!(Bloom::from_bytes(&[1, 2, 3]).is_none());
    }

    #[test]
    fn the_hash_never_changes() {
        // the filter is stored in dictionary files; FNV-1a of the UTF-8 bytes
        assert_eq!(hashes("食べる").0, 0xfa29_c919_497d_dd65);
    }
}
