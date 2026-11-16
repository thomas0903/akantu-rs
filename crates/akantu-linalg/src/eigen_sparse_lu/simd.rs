// SPDX-License-Identifier: MPL-2.0
//
// Part of the Rust translation of Eigen 3.4.0's SparseLU for akantu-rs (a
// port of Akantu, EPFL/LSMS); see the notice of `eigen_sparse_lu/mod.rs`
// and `eigen_sparse_lu/dense.rs` for the Eigen files the kernels derive
// from. This file has no Eigen counterpart: it selects the instruction set
// of the kernels at run time.

//! Run-time choice of the instruction set of the SparseLU dense kernels.
//!
//! The kernels of `dense` are written once, with per-entry operation
//! sequences that do not depend on the vector width (independent entries in
//! the lanes, a separate multiply and add per term, no FMA: Rust never
//! contracts `a * b + c`, and the `fma` target feature is never enabled
//! here, except in the explicit fast mode `gemm_into_zero_fma`). This
//! module compiles them a second and third time with AVX and
//! AVX-512F enabled and calls the widest instance the CPU supports, so the
//! results are bitwise those of the portable instance on every CPU
//! (`dense::tests` compare every instance the test machine supports with
//! the reference kernels). `AKANTU_SPARSELU_ISA=portable` (read once)
//! forces the portable instance, for A/B timing.
//!
//! Every `unsafe` block here calls a `#[target_feature]` function after
//! `is_x86_feature_detected!` returned true for that feature (`UNSAFE.md`,
//! class L6).

use std::sync::OnceLock;

use super::dense;

/// The instruction-set instances of the kernels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))] // x86-64 instances
pub(crate) enum Isa {
    /// The baseline of the target (SSE2 on x86-64, NEON on aarch64).
    Portable,
    /// x86-64 with AVX (256-bit vectors).
    Avx,
    /// x86-64 with AVX-512F (512-bit vectors).
    Avx512,
}

/// The widest instance this CPU runs, or `Portable` when the environment
/// variable `AKANTU_SPARSELU_ISA` is `portable`.
pub(crate) fn isa() -> Isa {
    static ISA: OnceLock<Isa> = OnceLock::new();
    *ISA.get_or_init(|| {
        if std::env::var("AKANTU_SPARSELU_ISA").is_ok_and(|v| v == "portable") {
            return Isa::Portable;
        }
        available().last().copied().unwrap_or(Isa::Portable)
    })
}

/// Every instance this CPU can run, narrowest first.
pub(crate) fn available() -> Vec<Isa> {
    #[allow(unused_mut)] // only x86-64 adds instances
    let mut v = vec![Isa::Portable];
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx") {
            v.push(Isa::Avx);
        }
        if std::arch::is_x86_feature_detected!("avx512f") {
            v.push(Isa::Avx512);
        }
    }
    v
}

/// [`dense::sparselu_gemm_into_zero`] with the instance [`isa`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline]
pub(crate) fn gemm_into_zero(
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
    gemm_into_zero_with(isa(), m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// [`dense::sparselu_gemm_into_zero`] with a given instance; an instance the
/// CPU does not support runs the portable one.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemm_into_zero_with(
    isa: Isa,
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
    match isa {
        Isa::Avx512 if std::arch::is_x86_feature_detected!("avx512f") => {
            // SAFETY: the CPU supports AVX-512F (checked in the match guard);
            // `x86::gemm_avx512` has no precondition beyond its target feature
            // (safe code, bounds-checked slices).
            return unsafe {
                x86::gemm_avx512(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
        Isa::Avx if std::arch::is_x86_feature_detected!("avx") => {
            // SAFETY: the CPU supports AVX (checked in the match guard);
            // `x86::gemm_avx` has no precondition beyond its target feature.
            return unsafe { x86::gemm_avx(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc) };
        }
        _ => {}
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = isa;
    dense::sparselu_gemm_into_zero(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// The fast mode of [`gemm_into_zero`] (`SparseLu::set_fast_mode`):
/// every term as a fused multiply-add (one rounding instead of two), so the
/// result differs from Eigen's in the last bits. Needs AVX-512F or AVX2
/// with FMA; on other CPUs it is [`gemm_into_zero`] (bitwise).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemm_into_zero_fma(
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
    gemm_into_zero_fma_with(isa(), m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// [`gemm_into_zero_fma`] with the FMA instance of a given width:
/// `Avx512` (AVX-512F + FMA), `Avx` (AVX2 + FMA); an instance the CPU does
/// not support, or `Portable`, runs [`gemm_into_zero_with`] (bitwise).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemm_into_zero_fma_with(
    isa: Isa,
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
    {
        let fma = std::arch::is_x86_feature_detected!("fma");
        if isa == Isa::Avx512 && fma && std::arch::is_x86_feature_detected!("avx512f") {
            // SAFETY: the CPU supports AVX-512F and FMA (checked on the
            // line above); the callee has no other precondition.
            return unsafe {
                x86::gemm_avx512_fma(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
        if isa != Isa::Portable && fma && std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: the CPU supports AVX2 and FMA (checked on the line
            // above).
            return unsafe {
                x86::gemm_avx2_fma(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
            };
        }
    }
    gemm_into_zero_with(isa, m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
}

/// Whether [`gemm_into_zero_fma`] runs an FMA instance on this CPU.
#[cfg(test)]
pub(crate) fn fma_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("fma")
            && (std::arch::is_x86_feature_detected!("avx512f")
                || std::arch::is_x86_feature_detected!("avx2"))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// [`gemm_into_zero_fma`] when `fma`, else [`gemm_into_zero`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline]
pub(crate) fn gemm_into_zero_mode(
    fma: bool,
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
    if fma {
        gemm_into_zero_fma(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    } else {
        gemm_into_zero(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }
}

/// [`dense::gemv_tiles`] (`res += α·A·x`, Eigen's GEMV sequence per row)
/// with the instance [`isa`].
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[inline]
pub(crate) fn gemv(
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &[f64],
    res: &mut [f64],
    alpha: f64,
) {
    gemv_with(isa(), rows, cols, a, a_off, lda, x, res, alpha);
}

/// [`gemv`] with a given instance; an instance the CPU does not support runs
/// the portable one.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv_with(
    isa: Isa,
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
    match isa {
        Isa::Avx512 if std::arch::is_x86_feature_detected!("avx512f") => {
            // SAFETY: the CPU supports AVX-512F (checked in the match guard);
            // `x86::gemv_avx512` has no precondition beyond its target feature
            // (safe code, bounds-checked slices).
            return unsafe { x86::gemv_avx512(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        Isa::Avx if std::arch::is_x86_feature_detected!("avx") => {
            // SAFETY: the CPU supports AVX (checked in the match guard);
            // `x86::gemv_avx` has no precondition beyond its target feature.
            return unsafe { x86::gemv_avx(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        _ => {}
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = isa;
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
    /// Each FMA instance against a scalar reference with the same sequence
    /// per entry (`c = fma(a, b, c)` for `k` in order, the tail
    /// `fma(b, a, c)`), bit for bit, on finite inputs: `f64::mul_add` is
    /// the correctly rounded fused multiply-add on every platform.
    #[test]
    fn fma_instances_match_a_scalar_fused_reference() {
        #[cfg(target_arch = "x86_64")]
        {
            use akantu_core::bitwise_testing::Values;
            let mut v = Values::new(0x51AB_1E5E_ED00_0001);
            let mut ran = 0;
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
                let fma = std::arch::is_x86_feature_detected!("fma");
                let mut kernels = Vec::new();
                if fma && std::arch::is_x86_feature_detected!("avx512f") {
                    kernels.push(super::Isa::Avx512);
                }
                if fma && std::arch::is_x86_feature_detected!("avx2") {
                    kernels.push(super::Isa::Avx);
                }
                for &isa in &kernels {
                    let mut got = vec![f64::NAN; m * n];
                    super::gemm_into_zero_fma_with(
                        isa, m, n, d, &a, 0, lda, &b, 0, d, &mut got, 0, m,
                    );
                    for (x, y) in got.iter().zip(&want) {
                        assert_eq!(x.to_bits(), y.to_bits(), "{isa:?} FMA case {case}");
                    }
                    ran += 1;
                }
            }
            if std::arch::is_x86_feature_detected!("fma") {
                assert!(ran > 0);
            }
        }
    }
}
