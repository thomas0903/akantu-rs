// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a port of Akantu (EPFL/LSMS): the engine behind
// src/common/aka_random_generator.hh (`std::default_random_engine`).
// Clean-room implementation (D-030) written from the ISO C++ specification
// ([rand.req.urng], [rand.eng.lcg], [rand.predef], [rand.util.canonical]) and
// black-box experiments against the oracle toolchain; see CLEANROOM_LOG.md.

//! `std::minstd_rand0` and `std::generate_canonical<double, 53>`.

/// A uniform random bit generator (C++ `[rand.req.urng]`): every call
/// returns a value in `[MIN, MAX]`.
///
/// Implementations must satisfy `MIN < MAX`.
pub trait UniformRandomBitGenerator {
    /// Smallest value returned by [`next_value`](Self::next_value) (`G::min()`).
    const MIN: u64;
    /// Largest value returned by [`next_value`](Self::next_value) (`G::max()`).
    const MAX: u64;
    /// The next value (`g()`).
    fn next_value(&mut self) -> u64;
}

/// `std::minstd_rand0`: the linear congruential engine
/// `linear_congruential_engine<uint_fast32_t, 16807, 0, 2147483647>`
/// (`[rand.predef]`), i.e. `x_{i+1} = 16807 · x_i mod (2³¹ − 1)`.
///
/// The oracle's `uint_fast32_t` is 64 bits wide (LP64), which matters for the
/// seeding of [`with_seed`](Self::with_seed) / [`with_seed_long`](Self::with_seed_long).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MinstdRand0 {
    state: u64,
}

/// `std::default_random_engine` of the oracle's standard library.
pub type DefaultRandomEngine = MinstdRand0;

impl MinstdRand0 {
    /// Multiplier `a`.
    pub const MULTIPLIER: u64 = 16_807;
    /// Increment `c`.
    pub const INCREMENT: u64 = 0;
    /// Modulus `m` = 2³¹ − 1.
    pub const MODULUS: u64 = 2_147_483_647;
    /// `default_seed`.
    pub const DEFAULT_SEED: u64 = 1;
    /// `min()`: `c == 0 ? 1 : 0`.
    pub const MIN: u64 = if Self::INCREMENT == 0 { 1 } else { 0 };
    /// `max()`: `m − 1`.
    pub const MAX: u64 = Self::MODULUS - 1;

    /// Default constructor (seed [`DEFAULT_SEED`](Self::DEFAULT_SEED)).
    #[must_use]
    pub const fn new() -> Self {
        Self::with_seed(Self::DEFAULT_SEED)
    }

    /// `[rand.eng.lcg]`: the state becomes `s mod m`, or 1 if both
    /// `c mod m` and `s mod m` are zero.
    #[must_use]
    pub const fn with_seed(s: u64) -> Self {
        let r = s % Self::MODULUS;
        let state = if Self::INCREMENT % Self::MODULUS == 0 && r == 0 {
            1
        } else {
            r
        };
        Self { state }
    }

    /// Seeding with a C `long` (as `RandomGenerator::seed(long)` does): the
    /// implicit conversion to the 64-bit `result_type` is modulo 2⁶⁴, so a
    /// negative seed `s` acts as `2⁶⁴ + s`.
    #[must_use]
    pub const fn with_seed_long(s: i64) -> Self {
        Self::with_seed(s.cast_unsigned())
    }

    /// `seed(s)`: same state as [`with_seed`](Self::with_seed).
    pub fn seed(&mut self, s: u64) {
        *self = Self::with_seed(s);
    }

    /// `seed(long)`: same state as [`with_seed_long`](Self::with_seed_long).
    pub fn seed_long(&mut self, s: i64) {
        *self = Self::with_seed_long(s);
    }

    /// The current state `x_i` (what `operator<<` writes).
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// `operator()`: advances the state and returns it.
    pub fn next_u32(&mut self) -> u32 {
        // a · x < 16807 · 2³¹ < 2⁴⁵: exact in u64.
        self.state = (Self::MULTIPLIER * self.state + Self::INCREMENT) % Self::MODULUS;
        // The state is below 2³¹.
        u32::try_from(self.state).unwrap_or(u32::MAX)
    }

    /// `discard(z)`: advances the state `z` times.
    pub fn discard(&mut self, z: u64) {
        for _ in 0..z {
            let _ = self.next_u32();
        }
    }
}

impl Default for MinstdRand0 {
    fn default() -> Self {
        Self::new()
    }
}

impl UniformRandomBitGenerator for MinstdRand0 {
    const MIN: u64 = MinstdRand0::MIN;
    const MAX: u64 = MinstdRand0::MAX;

    fn next_value(&mut self) -> u64 {
        u64::from(self.next_u32())
    }
}

/// Number of generator calls of `generate_canonical<double, 53>` for a
/// generator range `r = MAX − MIN + 1` (`[rand.util.canonical]`:
/// `k = max(1, ⌈b / log₂ R⌉)`, b = 53).
///
/// Determined by experiment (`CLEANROOM_LOG.md`, E2): the oracle uses
/// `L = ⌊log₂ R⌋`, except that its floating-point logarithm yields `L − 1`
/// for R = 8, 64 and 2⁵³ (for the other exact powers of two the difference
/// does not change the call count).
const fn canonical_calls(r: u128) -> u32 {
    const B: u32 = 53;
    let mut l = r.ilog2();
    if r == 8 || r == 64 || r == 1 << 53 {
        l -= 1;
    }
    let k = B.div_ceil(l);
    if k == 0 { 1 } else { k }
}

/// `std::generate_canonical<double, 53>(g)` (`[rand.util.canonical]`):
/// with `R = MAX − MIN + 1` and k calls `g_0, …, g_{k−1}`,
/// `S = Σ (g_i − MIN) · R^i` and `R^k` are accumulated in `double`
/// arithmetic, digit 0 (the first call) first, and the result is `S / R^k`;
/// a quotient that rounds to 1.0 is replaced by the largest double below 1,
/// so the result lies in `[0, 1)`.
///
/// For `MinstdRand0` (R = 2³¹ − 2) this takes two engine calls.
pub fn generate_canonical<G: UniformRandomBitGenerator>(g: &mut G) -> f64 {
    const {
        assert!(G::MIN < G::MAX, "a generator needs MIN < MAX");
    }
    let range = u128::from(G::MAX - G::MIN) + 1;
    let k = canonical_calls(range);
    // Correctly rounded conversion of the exact range.
    #[allow(clippy::cast_precision_loss)] // ranges above 2^53 round, like the oracle's
    let r = range as f64;
    let mut sum = 0.0_f64;
    let mut tmp = 1.0_f64;
    for _ in 0..k {
        #[allow(clippy::cast_precision_loss)] // digits above 2^53 round, like the oracle's
        let digit = (g.next_value() - G::MIN) as f64;
        sum += digit * tmp;
        tmp *= r;
    }
    let ret = sum / tmp;
    if ret >= 1.0 {
        // Largest double below 1 (0x1.fffffffffffffp-1).
        f64::from_bits(0x3fef_ffff_ffff_ffff)
    } else {
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_10000th_value() {
        // [rand.predef]: the 10000th consecutive invocation of a
        // default-constructed minstd_rand0 produces 1043618065.
        let mut e = MinstdRand0::new();
        e.discard(9999);
        assert_eq!(e.next_u32(), 1_043_618_065);
    }

    #[test]
    fn seeding_reduces_modulo_m_and_maps_zero_to_one() {
        assert_eq!(MinstdRand0::with_seed(0).state(), 1);
        assert_eq!(MinstdRand0::with_seed(MinstdRand0::MODULUS).state(), 1);
        assert_eq!(MinstdRand0::with_seed(MinstdRand0::MODULUS + 5).state(), 5);
        assert_eq!(
            MinstdRand0::with_seed_long(-1).state(),
            u64::MAX % MinstdRand0::MODULUS
        );
        let mut e = MinstdRand0::new();
        e.seed_long(42);
        assert_eq!(e, MinstdRand0::with_seed(42));
    }

    #[test]
    fn canonical_call_counts() {
        // Observed on the oracle (CLEANROOM_LOG.md, E2).
        assert_eq!(canonical_calls(2_147_483_646), 2);
        assert_eq!(canonical_calls(2), 53);
        assert_eq!(canonical_calls(8), 27);
        assert_eq!(canonical_calls(9), 18);
        assert_eq!(canonical_calls(64), 11);
        assert_eq!(canonical_calls(65), 9);
        assert_eq!(canonical_calls(256), 7);
        assert_eq!(canonical_calls(1000), 6);
        assert_eq!(canonical_calls(110_000_000), 3);
        assert_eq!(canonical_calls(1 << 27), 2);
        assert_eq!(canonical_calls((1 << 27) - 1), 3);
        assert_eq!(canonical_calls(1 << 53), 2);
        assert_eq!(canonical_calls((1 << 53) + 1), 1);
        assert_eq!(canonical_calls(1 << 64), 1);
    }
}
