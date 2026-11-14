// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_FE2/solid_mechanics_model_RVE.hh (the `srand(0.)` / `rand()`
// calls of `GelMaterialSelector`). Clean-room (D-030): written from the
// published descriptions of the C library generator named in the module
// documentation, verified black-box; no C library source was read.

//! The C library's `srand`/`rand` as used by upstream's
//! `GelMaterialSelector` (UD-586): the additive lagged-Fibonacci generator
//! of the GNU C library's default `TYPE_3` state (degree 31, separation 3),
//! written from its published description (no library source read):
//!
//! * `r₀ = seed` (a seed of 0 is replaced by 1), `rᵢ = 16807·rᵢ₋₁ mod
//!   (2³¹ − 1)` for `i = 1…30` (computed without overflow, negative results
//!   wrapped by `+ (2³¹ − 1)`), `rᵢ = rᵢ₋₃₁` for `i = 31…33`;
//! * `rᵢ = rᵢ₋₃₁ + rᵢ₋₃ (mod 2³²)` for `i ≥ 34`; the first 310 values are
//!   discarded and the `k`-th output is `r₃₄₃₊ₖ >> 1` (`k ≥ 1`).
//!
//! Public sources (D-030 clean-room statement):
//!
//! * the recurrence with lags 31 and 3, the 310 discarded values and the
//!   `>> 1` output: P. Selinger, "The GLIBC pseudo-random number
//!   generator" (online article, 2007), which states the generator as the
//!   formulas above; lagged additive generators in general: D. E. Knuth,
//!   *The Art of Computer Programming*, vol. 2, §3.2.2;
//! * the seeding sequence: the "minimal standard" multiplicative generator
//!   `16807·x mod (2³¹ − 1)` of S. K. Park and K. W. Miller, "Random number
//!   generators: good ones are hard to find", Commun. ACM 31(10),
//!   1192–1201 (1988), evaluated without overflow by L. Schrage's
//!   factorisation (L. Schrage, "A more portable Fortran random number
//!   generator", ACM Trans. Math. Softw. 5(2), 132–138, 1979: `q = ⌊m/a⌋ =
//!   127773`, `r = m mod a = 2836`, `a·(x mod q) − r·⌊x/q⌋`, plus `m` when
//!   negative), on the seed read as a signed 32-bit value;
//! * the replacement of the seed 0 by 1: black-box evidence (`srand(0)` and
//!   `srand(1)` give the same stream in the vectors below).
//!
//! No C library source (GPL/LGPL glibc code) was read to write or review
//! this module.
//!
//! Evidence: the first 64 outputs after `srand(0)`, `srand(1)` and
//! `srand(12345)` equal those of glibc 2.39 (Ubuntu 24.04) and of the
//! oracle image's Debian glibc (`tests/data/glibc_rand_srand_0_1_12345.txt`,
//! test `c_rand_matches_glibc_black_box`). Upstream's selector is therefore
//! platform dependent (another C library gives other gel pockets); the port
//! reproduces the oracle platform's.

/// State of the C library generator.
#[derive(Clone, Debug)]
pub struct CRand {
    r: Vec<u32>,
}

impl CRand {
    /// `srand(seed)`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let seed = if seed == 0 { 1 } else { seed };
        let mut r: Vec<u32> = Vec::with_capacity(400);
        // the seed is used as a signed 32-bit value
        let mut word = i64::from(seed as i32);
        r.push(word as u32);
        for _ in 1..31 {
            let hi = word / 127_773;
            let lo = word % 127_773;
            word = 16_807 * lo - 2_836 * hi;
            if word < 0 {
                word += 2_147_483_647;
            }
            r.push(word as u32);
        }
        for i in 31..34 {
            let v = r[i - 31];
            r.push(v);
        }
        let mut g = Self { r };
        for _ in 34..344 {
            g.step();
        }
        g
    }

    fn step(&mut self) -> u32 {
        let n = self.r.len();
        let v = self.r[n - 31].wrapping_add(self.r[n - 3]);
        self.r.push(v);
        // keep the window bounded
        if self.r.len() > 1024 {
            self.r.drain(..self.r.len() - 31);
        }
        v
    }

    /// `rand()`: a value in `[0, 2³¹ − 1]`.
    pub fn next_value(&mut self) -> u32 {
        self.step() >> 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_first_values() {
        let mut g = CRand::new(1);
        assert_eq!(g.next_value(), 1_804_289_383);
        assert_eq!(g.next_value(), 846_930_886);
        let mut z = CRand::new(0);
        assert_eq!(z.next_value(), 1_804_289_383);
    }
}
