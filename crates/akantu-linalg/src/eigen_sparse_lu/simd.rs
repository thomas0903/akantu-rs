// SPDX-License-Identifier: MPL-2.0
//
// Part of the Rust translation of Eigen 3.4.0's SparseLU for akantu-rs (a
// port of Akantu, EPFL/LSMS); see the notice of `eigen_sparse_lu/mod.rs`
// and `eigen_sparse_lu/dense.rs` for the Eigen files the kernels derive
// from. This file has no Eigen counterpart: it selects the instruction set
// of the kernels at run time.

//! The instruction-set instances of the SparseLU dense kernels and their
//! dispatch on the executor's SIMD level (D-123, `akantu_core::exec`).
//!
//! The kernels of `dense` are written once, with per-entry operation
//! sequences that do not depend on the vector width (independent entries in
//! the lanes, a separate multiply and add per term, no FMA: Rust never
//! contracts `a * b + c`, and the `fma` target feature is enabled only in
//! the explicit fast mode `gemm_into_zero_fma`). This module compiles them
//! a second and third time with AVX and AVX-512F enabled; a call takes the
//! executor's [`SimdToken`] (its level: `AKANTU_SIMD`, `ExecPolicy::with_simd`,
//! else the detected one) and runs the instance of that level:
//!
//! | `SimdLevel` | instance |
//! |---|---|
//! | `Avx512` | AVX-512F, 16-row gemm tiles, 32-row GEMV tiles |
//! | `Avx2` | AVX, 8-row gemm tiles, 16-row GEMV tiles |
//! | `Scalar`, `Neon` | the portable code (SSE2 on x86-64, NEON on aarch64), 4 / 8 rows |
//!
//! Every instance gives the bits of the portable one (`dense::tests` run
//! every supported level against the reference kernels). The fast mode
//! uses the [`FastToken`]'s level (AVX-512F + FMA, AVX2 + FMA; the bitwise
//! kernels on the other levels).
//!
//! Every `unsafe` block here calls a `#[target_feature]` function of the
//! token's level: a token exists only for a level the CPU supports
//! (`UNSAFE.md`, class L6).

#[cfg(any(target_arch = "x86_64", test))]
use akantu_core::exec::SimdLevel;
use akantu_core::exec::{FastToken, SimdToken};

use super::dense;

/// `C = 0; C += A·B` ([`dense::gemm_tiles`] with a zero destination) with
/// the instance of `simd`'s level.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemm_into_zero(
    simd: SimdToken,
    m: usize,
    n: usize,
    d: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    b: &[f64],
    b_off: usize,
    ldb: usize,
    c: &mut [f64],
    c_off: usize,
    ldc: usize,
) {
    #[cfg(target_arch = "x86_64")]
    match simd.level() {
        SimdLevel::Avx512 => {
            // SAFETY: the token proves that the CPU supports AVX-512F;
            // `x86::gemm_avx512` has no precondition beyond its target
            // feature (safe code, bounds-checked slices).
            return unsafe {
                x86::gemm_avx512(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
        SimdLevel::Avx2 => {
            // SAFETY: the token proves AVX2 (which includes AVX);
            // `x86::gemm_avx` has no precondition beyond its target feature.
            return unsafe { x86::gemm_avx(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc) };
        }
        SimdLevel::Scalar | SimdLevel::Neon => {}
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = simd;
    dense::sparselu_gemm_into_zero(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// FAST MODE (`SparseLu::set_fast_mode`): [`gemm_into_zero`] with every
/// term as a fused multiply-add (one rounding instead of two), so the
/// result differs from Eigen's in the last bits. The instance of the
/// token's level: AVX-512F + FMA, AVX2 + FMA; on the other levels the
/// bitwise kernel.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemm_into_zero_fma(
    fast: FastToken,
    m: usize,
    n: usize,
    d: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    b: &[f64],
    b_off: usize,
    ldb: usize,
    c: &mut [f64],
    c_off: usize,
    ldc: usize,
) {
    #[cfg(target_arch = "x86_64")]
    match fast.simd().level() {
        SimdLevel::Avx512 => {
            // SAFETY: the token proves AVX-512F with AVX2 and FMA3; the
            // callee has no other precondition.
            return unsafe {
                x86::gemm_avx512_fma(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
        SimdLevel::Avx2 => {
            // SAFETY: the token proves AVX2 with FMA3.
            return unsafe {
                x86::gemm_avx2_fma(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
        SimdLevel::Scalar | SimdLevel::Neon => {}
    }
    gemm_into_zero(
        fast.simd(),
        m,
        n,
        d,
        a,
        a_off,
        lda,
        b,
        b_off,
        ldb,
        c,
        c_off,
        ldc,
    );
}

/// Whether [`gemm_into_zero_fma`] runs an FMA instance for `fast`.
#[cfg(test)]
pub(crate) fn fma_instance(fast: FastToken) -> bool {
    matches!(fast.simd().level(), SimdLevel::Avx512 | SimdLevel::Avx2)
}

/// [`gemm_into_zero_fma`] with `fast`, else [`gemm_into_zero`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline]
pub(crate) fn gemm_into_zero_mode(
    simd: SimdToken,
    fast: Option<FastToken>,
    m: usize,
    n: usize,
    d: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    b: &[f64],
    b_off: usize,
    ldb: usize,
    c: &mut [f64],
    c_off: usize,
    ldc: usize,
) {
    match fast {
        Some(fast) => {
            gemm_into_zero_fma(fast, m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
        }
        None => gemm_into_zero(simd, m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc),
    }
}

/// [`dense::gemv_tiles`] (`res += α·A·x`, Eigen's GEMV sequence per row)
/// with the instance of `simd`'s level.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv(
    simd: SimdToken,
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &[f64],
    res: &mut [f64],
    alpha: f64,
) {
    #[cfg(target_arch = "x86_64")]
    match simd.level() {
        SimdLevel::Avx512 => {
            // SAFETY: the token proves that the CPU supports AVX-512F;
            // `x86::gemv_avx512` has no precondition beyond its target
            // feature (safe code, bounds-checked slices).
            return unsafe { x86::gemv_avx512(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        SimdLevel::Avx2 => {
            // SAFETY: the token proves AVX2 (which includes AVX);
            // `x86::gemv_avx` has no precondition beyond its target feature.
            return unsafe { x86::gemv_avx(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        SimdLevel::Scalar | SimdLevel::Neon => {}
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = simd;
    dense::gemv_tiles::<8>(rows, cols, a, a_off, lda, x, res, alpha);
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::dense::{gemm_tiles, gemv_tiles};

    /// `gemv_tiles` compiled with AVX-512F: tiles of 32 rows (four vectors).
    #[target_feature(enable = "avx512f")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemv_avx512(
        rows: usize,
        cols: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        x: &[f64],
        res: &mut [f64],
        alpha: f64,
    ) {
        gemv_tiles::<32>(rows, cols, a, a_off, lda, x, res, alpha);
    }

    /// `gemv_tiles` compiled with AVX: tiles of 16 rows (four vectors).
    #[target_feature(enable = "avx")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemv_avx(
        rows: usize,
        cols: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        x: &[f64],
        res: &mut [f64],
        alpha: f64,
    ) {
        gemv_tiles::<16>(rows, cols, a, a_off, lda, x, res, alpha);
    }

    /// `gemm_tiles` compiled with AVX-512F: tiles of 16 rows (two vectors).
    #[target_feature(enable = "avx512f")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemm_avx512(
        m: usize,
        n: usize,
        d: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        b: &[f64],
        b_off: usize,
        ldb: usize,
        c: &mut [f64],
        c_off: usize,
        ldc: usize,
    ) {
        gemm_tiles::<16, true, false>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }

    /// `gemm_tiles` compiled with AVX: tiles of 8 rows (two vectors).
    #[target_feature(enable = "avx")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemm_avx(
        m: usize,
        n: usize,
        d: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        b: &[f64],
        b_off: usize,
        ldb: usize,
        c: &mut [f64],
        c_off: usize,
        ldc: usize,
    ) {
        gemm_tiles::<8, true, false>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }

    /// Fast mode: `gemm_tiles` with fused multiply-adds, AVX-512F.
    #[target_feature(enable = "avx512f,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemm_avx512_fma(
        m: usize,
        n: usize,
        d: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        b: &[f64],
        b_off: usize,
        ldb: usize,
        c: &mut [f64],
        c_off: usize,
        ldc: usize,
    ) {
        gemm_tiles::<16, true, true>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }

    /// Fast mode: `gemm_tiles` with fused multiply-adds, AVX2.
    #[target_feature(enable = "avx2,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemm_avx2_fma(
        m: usize,
        n: usize,
        d: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        b: &[f64],
        b_off: usize,
        ldb: usize,
        c: &mut [f64],
        c_off: usize,
        ldc: usize,
    ) {
        gemm_tiles::<8, true, true>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }
}

#[cfg(test)]
mod tests {
    use akantu_core::exec::{Exec, ExecPolicy, Precision, SimdLevel};

    /// Each FMA instance against a scalar reference with the same sequence
    /// per entry (`c = fma(a, b, c)` for `k` in order, the tail
    /// `fma(b, a, c)`), bit for bit, on finite inputs: `f64::mul_add` is
    /// the correctly rounded fused multiply-add on every platform.
    #[test]
    fn fma_instances_match_a_scalar_fused_reference() {
        use akantu_core::bitwise_testing::Values;
        // a fast token of every supported level with an FMA instance
        let tokens: Vec<_> = SimdLevel::supported()
            .into_iter()
            .filter_map(|level| {
                let policy = ExecPolicy::serial()
                    .with_precision(Precision::Fast)
                    .with_simd(level)
                    .ok()?;
                Exec::new(policy).ok()?.fast_token()
            })
            .filter(|&t| super::fma_instance(t))
            .collect();
        let mut v = Values::new(0x51AB_1E5E_ED00_0001);
        for case in 0..400usize {
            let (m, n, d) = (1 + case % 37, 1 + case % 11, case % 21);
            let lda = m + case % 3;
            let mut a = vec![0.0; lda * d.max(1) + m];
            let mut b = vec![0.0; d * n + 1];
            (a.iter_mut().chain(b.iter_mut())).for_each(|x| *x = v.finite());
            let d_end = d & !1;
            let mut want = vec![0.0; m * n];
            for j in 0..n {
                for i in 0..m {
                    let mut c = 0.0f64;
                    for k in 0..d_end {
                        c = a[i + k * lda].mul_add(b[k + j * d], c);
                    }
                    if d_end < d {
                        c = b[d_end + j * d].mul_add(a[i + d_end * lda], c);
                    }
                    want[i + j * m] = c;
                }
            }
            for &fast in &tokens {
                let mut got = vec![f64::NAN; m * n];
                super::gemm_into_zero_fma(fast, m, n, d, &a, 0, lda, &b, 0, d, &mut got, 0, m);
                for (x, y) in got.iter().zip(&want) {
                    assert_eq!(x.to_bits(), y.to_bits(), "{fast:?} case {case}");
                }
            }
        }
        // every x86-64 CPU with AVX2 has an FMA instance to test
        if SimdLevel::Avx2.is_supported() {
            assert!(!tokens.is_empty());
        }
    }
}
