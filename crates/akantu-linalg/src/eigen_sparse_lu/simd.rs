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
//! here). This module compiles them a second and third time with AVX and
//! AVX-512F enabled and calls the widest instance the CPU supports, so the
//! results are bitwise those of the portable instance on every CPU
//! (`dense::tests` compare every instance the test machine supports with
//! the reference kernels). `AKANTU_SPARSELU_ISA=portable` (read once)
//! forces the portable instance, for A/B timing.
//!
//! Every `unsafe` block here calls a `#[target_feature]` function after
//! `is_x86_feature_detected!` returned true for that feature (`UNSAFE.md`,
//! class L7).

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

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::dense::gemm_tiles;

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
        gemm_tiles::<16, true>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
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
        gemm_tiles::<8, true>(m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc);
    }
}
