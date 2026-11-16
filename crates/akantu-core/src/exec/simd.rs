// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: SIMD level detection and the dispatch convention of
// multi-versioned kernels (D-123).

//! SIMD levels and the convention for a multi-versioned kernel.
//!
//! [`SimdLevel::detected`] is the highest level the CPU supports, found once
//! per process (`is_x86_feature_detected!`; NEON is part of every aarch64
//! target). A [`SimdToken`] can only be made for a supported level, so a
//! kernel that receives one (from [`crate::exec::ExecPolicy::simd_token`])
//! may call its `#[target_feature]` version for that level.
//!
//! **Convention for a kernel** (example: [`scaled_add`]):
//!
//! 1. one portable body, `#[inline(always)]`, generic or written once per
//!    lane width, that performs per lane the scalar sequence of operations
//!    (no FMA, no reassociation, no horizontal sums in
//!    [`crate::exec::Precision::Bitwise`]; same NaN, signed-zero and min/max
//!    behaviour);
//! 2. one `#[target_feature(enable = "…")] unsafe fn` per level that calls
//!    the body, so the compiler generates it with the level's instructions;
//! 3. one dispatch point per batch (`match token.level()`), each `unsafe`
//!    call justified by the token's invariant and listed in the crate's
//!    `UNSAFE.md`;
//! 4. a property test that runs every level of [`SimdLevel::supported`] on
//!    random inputs including signed zeros, infinities, NaNs and subnormals
//!    and requires the scalar level's bits.
//!
//! Tests and timings select a level with `AKANTU_SIMD=scalar|avx2|avx512|neon`
//! or [`crate::exec::ExecPolicy::with_simd`].

// The example kernel calls its `#[target_feature]` versions and their
// intrinsics: every block carries its `SAFETY:` comment and its row in
// `UNSAFE.md` (the rest of the crate denies `unsafe`).
#![allow(unsafe_code)]

use std::sync::OnceLock;

use crate::error::{Error, Result};
use crate::types::Real;

/// Instruction-set level of a multi-versioned kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SimdLevel {
    /// Portable scalar code (the compiler's baseline target).
    Scalar,
    /// x86-64 AVX2 with FMA3 (256-bit vectors; every AVX2 processor of
    /// Intel and AMD has both).
    Avx2,
    /// x86-64 AVX-512F (512-bit vectors), with AVX2 and FMA3.
    Avx512,
    /// aarch64 Advanced SIMD (128-bit vectors).
    Neon,
}

impl SimdLevel {
    /// Every level, supported or not.
    pub const ALL: [Self; 4] = [Self::Scalar, Self::Avx2, Self::Avx512, Self::Neon];

    /// The name used by `AKANTU_SIMD` and Python.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Avx2 => "avx2",
            Self::Avx512 => "avx512",
            Self::Neon => "neon",
        }
    }

    /// `true` when this CPU can run the level.
    #[must_use]
    pub fn is_supported(self) -> bool {
        match self {
            Self::Scalar => true,
            #[cfg(target_arch = "x86_64")]
            Self::Avx2 => {
                std::arch::is_x86_feature_detected!("avx2")
                    && std::arch::is_x86_feature_detected!("fma")
            }
            #[cfg(target_arch = "x86_64")]
            Self::Avx512 => {
                std::arch::is_x86_feature_detected!("avx512f") && Self::Avx2.is_supported()
            }
            #[cfg(target_arch = "aarch64")]
            Self::Neon => std::arch::is_aarch64_feature_detected!("neon"),
            #[allow(unreachable_patterns)] // the levels of the other architecture
            _ => false,
        }
    }

    /// The highest supported level (detected once per process).
    #[must_use]
    pub fn detected() -> Self {
        static DETECTED: OnceLock<SimdLevel> = OnceLock::new();
        *DETECTED.get_or_init(|| {
            Self::ALL
                .into_iter()
                .rev()
                .find(|l| l.is_supported())
                .unwrap_or(Self::Scalar)
        })
    }

    /// Every supported level, lowest first (for tests over all levels).
    #[must_use]
    pub fn supported() -> Vec<Self> {
        Self::ALL.into_iter().filter(|l| l.is_supported()).collect()
    }

    /// A token of this level, `None` when the CPU does not support it.
    #[must_use]
    pub fn token(self) -> Option<SimdToken> {
        self.is_supported().then_some(SimdToken(self))
    }
}

impl std::str::FromStr for SimdLevel {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|l| l.name() == s)
            .ok_or_else(|| {
                Error::invalid(format!(
                    "SIMD level `{s}`: expected `auto`, `scalar`, `avx2`, `avx512` or `neon`"
                ))
            })
    }
}

/// A SIMD level the CPU supports. Only [`SimdLevel::token`] and
/// [`SimdToken::detected`] make one, so the `unsafe` call of a kernel's
/// `#[target_feature]` version for `token.level()` is sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimdToken(SimdLevel);

impl SimdToken {
    /// The token of [`SimdLevel::detected`].
    #[must_use]
    pub fn detected() -> Self {
        Self(SimdLevel::detected())
    }

    /// The scalar level (always supported).
    #[must_use]
    pub const fn scalar() -> Self {
        Self(SimdLevel::Scalar)
    }

    /// The level.
    #[must_use]
    pub const fn level(self) -> SimdLevel {
        self.0
    }
}

/// Proof that the executor of a batch is in
/// [`crate::exec::Precision::Fast`]: only [`crate::exec::Exec::fast_token`]
/// hands one out, so a fast-mode entry point that takes it cannot run in
/// the bitwise default. Carries the executor's SIMD level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastToken(SimdToken);

impl FastToken {
    /// Made by `Exec::fast_token` only.
    pub(crate) const fn new(simd: SimdToken) -> Self {
        Self(simd)
    }

    /// The executor's SIMD level.
    #[must_use]
    pub const fn simd(self) -> SimdToken {
        self.0
    }
}

/// A computation that [`with_fma`] runs with fused multiply-add
/// instructions enabled. Its `run` must be `#[inline(always)]` (and every
/// function it calls in its hot loop), so that it is compiled inside the
/// FMA-enabled entry point.
pub trait FmaTask {
    /// Result.
    type Output;
    /// The computation.
    fn run(self) -> Self::Output;
}

/// Fast mode only (D-122; `fast` proves the executor is in `Fast`): run
/// `task` compiled with the FMA instructions of the executor's level when it has them (`Avx2`, `Avx512`; every aarch64
/// processor has them in its baseline), so that `f64::mul_add` in it is one
/// instruction instead of a library call. Without FMA (`Scalar`) the task
/// runs as compiled for the baseline (`mul_add` stays fused, but slower).
pub fn with_fma<T: FmaTask>(fast: FastToken, task: T) -> T::Output {
    match fast.simd().level() {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 | SimdLevel::Avx512 => {
            // SAFETY: a token of `Avx2` or `Avx512` exists only when the CPU
            // reports avx2 and fma (`SimdLevel::is_supported`).
            unsafe { run_with_fma(task) }
        }
        _ => task.run(),
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn run_with_fma<T: FmaTask>(task: T) -> T::Output {
    task.run()
}

/// Example of the convention: `out[i] += a · x[i]` (the product rounded,
/// then the sum rounded, as the scalar statement `*o += a * x`), with one
/// version per level. Every level gives the scalar bits.
///
/// # Panics
/// `out` and `x` of different lengths.
pub fn scaled_add(token: SimdToken, out: &mut [Real], x: &[Real], a: Real) {
    assert_eq!(out.len(), x.len(), "scaled_add: lengths differ");
    match token.level() {
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx512 => {
            // SAFETY: a token of `Avx512` exists only when the CPU reports
            // avx512f (`SimdLevel::token`, `SimdToken::detected`); the
            // lengths are equal (asserted above).
            unsafe { scaled_add_avx512(out, x, a) }
        }
        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => {
            // SAFETY: a token of `Avx2` exists only when the CPU reports
            // avx2; the lengths are equal (asserted above).
            unsafe { scaled_add_avx2(out, x, a) }
        }
        #[cfg(target_arch = "aarch64")]
        SimdLevel::Neon => {
            // SAFETY: a token of `Neon` exists only when the CPU reports
            // neon; the lengths are equal (asserted above).
            unsafe { scaled_add_neon(out, x, a) }
        }
        _ => scaled_add_scalar(out, x, a),
    }
}

/// The scalar sequence (also the tail of the vector versions).
#[inline(always)]
fn scaled_add_scalar(out: &mut [Real], x: &[Real], a: Real) {
    for (o, &xi) in out.iter_mut().zip(x) {
        *o += a * xi;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scaled_add_avx2(out: &mut [Real], x: &[Real], a: Real) {
    use std::arch::x86_64::{
        _mm256_add_pd, _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd, _mm256_storeu_pd,
    };
    let n = out.len().min(x.len());
    let body = n - n % 4;
    let va = _mm256_set1_pd(a);
    let mut i = 0;
    while i < body {
        // SAFETY: `i + 4 ≤ body ≤ len` of both slices; unaligned loads and
        // stores; avx2 enabled on this function.
        unsafe {
            let p = _mm256_mul_pd(va, _mm256_loadu_pd(x.as_ptr().add(i)));
            let o = _mm256_add_pd(_mm256_loadu_pd(out.as_ptr().add(i)), p);
            _mm256_storeu_pd(out.as_mut_ptr().add(i), o);
        }
        i += 4;
    }
    scaled_add_scalar(&mut out[body..n], &x[body..n], a);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn scaled_add_avx512(out: &mut [Real], x: &[Real], a: Real) {
    use std::arch::x86_64::{
        _mm512_add_pd, _mm512_loadu_pd, _mm512_mul_pd, _mm512_set1_pd, _mm512_storeu_pd,
    };
    let n = out.len().min(x.len());
    let body = n - n % 8;
    let va = _mm512_set1_pd(a);
    let mut i = 0;
    while i < body {
        // SAFETY: `i + 8 ≤ body ≤ len` of both slices; unaligned loads and
        // stores; avx512f enabled on this function.
        unsafe {
            let p = _mm512_mul_pd(va, _mm512_loadu_pd(x.as_ptr().add(i)));
            let o = _mm512_add_pd(_mm512_loadu_pd(out.as_ptr().add(i)), p);
            _mm512_storeu_pd(out.as_mut_ptr().add(i), o);
        }
        i += 8;
    }
    scaled_add_scalar(&mut out[body..n], &x[body..n], a);
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn scaled_add_neon(out: &mut [Real], x: &[Real], a: Real) {
    use std::arch::aarch64::{vaddq_f64, vdupq_n_f64, vld1q_f64, vmulq_f64, vst1q_f64};
    let n = out.len().min(x.len());
    let body = n - n % 2;
    let va = vdupq_n_f64(a);
    let mut i = 0;
    while i < body {
        // SAFETY: `i + 2 ≤ body ≤ len` of both slices; `vld1q_f64` and
        // `vst1q_f64` have no alignment requirement beyond `f64`; neon
        // enabled on this function.
        unsafe {
            let p = vmulq_f64(va, vld1q_f64(x.as_ptr().add(i)));
            let o = vaddq_f64(vld1q_f64(out.as_ptr().add(i)), p);
            vst1q_f64(out.as_mut_ptr().add(i), o);
        }
        i += 2;
    }
    scaled_add_scalar(&mut out[body..n], &x[body..n], a);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test values: specials (signed zeros, infinities, NaNs, subnormals,
    /// extremes), then arbitrary bit patterns.
    fn values(n: usize, seed: u64) -> Vec<Real> {
        const SPECIAL: [Real; 12] = [
            0.0,
            -0.0,
            Real::INFINITY,
            Real::NEG_INFINITY,
            Real::NAN,
            Real::MIN_POSITIVE,
            -Real::MIN_POSITIVE / 4.0,
            5e-324,
            Real::MAX,
            -Real::MAX,
            1.0,
            -3.5,
        ];
        let mut s = seed;
        (0..n)
            .map(|i| {
                s = s
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                if i % 5 == 0 {
                    SPECIAL[(s >> 33) as usize % SPECIAL.len()]
                } else if i % 5 == 1 {
                    Real::from_bits(s)
                } else {
                    ((s >> 11) as Real / (1u64 << 53) as Real - 0.5) * 1e3
                }
            })
            .collect()
    }

    /// Bit identity, NaNs compared as NaNs (their payloads are not part of
    /// the contract, D-046).
    fn same(a: Real, b: Real) -> bool {
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
    }

    #[test]
    fn every_level_gives_the_scalar_bits() {
        let levels = SimdLevel::supported();
        assert!(levels.contains(&SimdLevel::Scalar));
        assert!(levels.contains(&SimdLevel::detected()));
        for (n, seed) in [(0, 1), (1, 2), (7, 3), (8, 4), (33, 5), (1000, 6)] {
            for a in values(9, seed ^ 0xa5) {
                let x = values(n, seed);
                let o0 = values(n, seed + 100);
                let mut reference = o0.clone();
                scaled_add(SimdToken::scalar(), &mut reference, &x, a);
                for &level in &levels {
                    let mut out = o0.clone();
                    scaled_add(level.token().unwrap(), &mut out, &x, a);
                    for (i, (&r, &o)) in reference.iter().zip(&out).enumerate() {
                        assert!(same(r, o), "{level:?} n={n} i={i}: {r:e} vs {o:e}");
                    }
                }
            }
        }
    }

    struct Fused(Real, Real, Real);
    impl FmaTask for Fused {
        type Output = Real;
        #[inline(always)]
        fn run(self) -> Real {
            self.0.mul_add(self.1, self.2)
        }
    }

    #[test]
    fn fma_tasks_are_fused_at_every_level() {
        // (1 + 2⁻³⁰)² − 1 rounded once keeps the 2⁻⁶⁰ term
        let x = 1.0 + (2.0 as Real).powi(-30);
        let exact = (2.0 as Real).powi(-29) + (2.0 as Real).powi(-60);
        for level in SimdLevel::supported() {
            assert_eq!(
                with_fma(FastToken::new(level.token().unwrap()), Fused(x, x, -1.0)),
                exact,
                "{level:?}"
            );
        }
    }

    #[test]
    fn names_round_trip() {
        for l in SimdLevel::ALL {
            assert_eq!(l.name().parse::<SimdLevel>().unwrap(), l);
        }
        assert!("sse".parse::<SimdLevel>().is_err());
    }
}
