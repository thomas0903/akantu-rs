// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs performance tooling (not ported from Akantu): the hasher of the
// integer-keyed maps of the sparse matrix profile (upstream `irn_jcn_k`,
// sparse_matrix_aij.hh) and of the equation layout.

//! A cheap hasher for maps keyed by integers (sparse-matrix profile keys
//! `(row, column)`, global equation numbers).
//!
//! The standard library's default hasher (SipHash-1-3 with a random key)
//! resists hash flooding, which these maps do not need: their keys are
//! equation numbers of the model, not external input. Each written integer
//! is mixed with a 64 × 64 → 128-bit multiplication folded to 64 bits, so
//! both the low bits (bucket index) and the high bits (control byte) of the
//! hash depend on every bit of the key. The hash only places entries in the
//! table: no result of the library depends on it (the maps are never
//! iterated), so this choice cannot change a computed value.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Odd 64-bit constant of the mixing step (the golden-ratio constant).
const K: u64 = 0x9e37_79b9_7f4a_7c15;

/// Hasher of integer keys; see the module documentation.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntHasher(u64);

impl IntHasher {
    #[inline]
    fn mix(&mut self, x: u64) {
        let m = u128::from(self.0 ^ x) * u128::from(K);
        // Truncations intended: the fold of the 128-bit product.
        #[allow(clippy::cast_possible_truncation)]
        let (lo, hi) = (m as u64, (m >> 64) as u64);
        self.0 = lo ^ hi;
    }
}

impl Hasher for IntHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut b = [0u8; 8];
            b.copy_from_slice(c);
            self.mix(u64::from_le_bytes(b));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut b = [0u8; 8];
            b[..rest.len()].copy_from_slice(rest);
            self.mix(u64::from_le_bytes(b) ^ ((rest.len() as u64) << 56));
        }
    }

    #[inline]
    fn write_u64(&mut self, x: u64) {
        self.mix(x);
    }

    #[inline]
    fn write_i64(&mut self, x: i64) {
        // Reinterpretation intended: the bits are hashed.
        #[allow(clippy::cast_sign_loss)]
        self.mix(x as u64);
    }

    #[inline]
    fn write_usize(&mut self, x: usize) {
        self.mix(x as u64);
    }

    #[inline]
    fn write_u32(&mut self, x: u32) {
        self.mix(u64::from(x));
    }

    #[inline]
    fn write_i32(&mut self, x: i32) {
        // Reinterpretation intended: the bits are hashed.
        #[allow(clippy::cast_sign_loss)]
        self.mix(u64::from(x as u32));
    }
}

/// Builder of [`IntHasher`]s (deterministic: no random key).
pub type IntBuildHasher = BuildHasherDefault<IntHasher>;

/// A `HashMap` with the [`IntHasher`].
pub type IntHashMap<K, V> = HashMap<K, V, IntBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, Hash};

    fn h<T: Hash>(t: &T) -> u64 {
        IntBuildHasher::default().hash_one(t)
    }

    #[test]
    fn distinct_small_keys_spread_over_low_and_high_bits() {
        let mut low = std::collections::HashSet::new();
        let mut high = std::collections::HashSet::new();
        for i in 0..64i64 {
            for j in 0..64i64 {
                let x = h(&(i, j));
                low.insert(x & 0xfff);
                high.insert(x >> 57);
            }
        }
        // 4096 keys: most low-12-bit buckets distinct, every control value seen
        assert!(low.len() > 2400, "{}", low.len());
        assert_eq!(high.len(), 128);
        assert_ne!(h(&(1i64, 2i64)), h(&(2i64, 1i64)));
    }

    #[test]
    fn map_behaves_as_a_map() {
        let mut m: IntHashMap<(i64, i64), usize> = IntHashMap::default();
        for k in 0..10_000usize {
            let key = ((k % 97) as i64, (k / 97) as i64);
            assert!(m.insert(key, k).is_none());
        }
        for k in 0..10_000usize {
            assert_eq!(m[&((k % 97) as i64, (k / 97) as i64)], k);
        }
        let mut b = IntHasher::default();
        b.write(&[1, 2, 3]);
        let mut c = IntHasher::default();
        c.write(&[1, 2, 3, 0]);
        assert_ne!(b.finish(), c.finish());
    }
}
