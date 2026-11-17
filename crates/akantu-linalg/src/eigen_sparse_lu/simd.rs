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

/// The executor's kernel choice for a batch: the SIMD level and, in
/// [`Precision::Fast`](akantu_core::exec::Precision::Fast) with the fast mode
/// enabled, the [`FastToken`] (FAST MODE kernels).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mode {
    pub simd: SimdToken,
    pub fast: Option<FastToken>,
}

impl Mode {
    /// The bitwise kernels of `simd`'s level.
    pub(crate) const fn bitwise(simd: SimdToken) -> Self {
        Self { simd, fast: None }
    }
}

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
#[cold] // FAST MODE: keeps the bitwise dispatch small
#[inline(never)]
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
    mode: Mode,
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
    match mode.fast {
        Some(fast) => {
            gemm_into_zero_fma(fast, m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc)
        }
        None => gemm_into_zero(
            mode.simd, m, n, d, a, a_off, lda, b, b_off, ldb, c, c_off, ldc,
        ),
    }
}

/// [`dense::gemv_tiles`] (`res += α·A·x`, Eigen's GEMV sequence per row)
/// with the instance of `simd`'s level.
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
pub(crate) fn gemv(
    mode: Mode,
    rows: usize,
    cols: usize,
    a: &[f64],
    a_off: usize,
    lda: usize,
    x: &[f64],
    res: &mut [f64],
    alpha: f64,
) {
    if let Some(fast) = mode.fast {
        return gemv_fma(fast, rows, cols, a, a_off, lda, x, res, alpha);
    }
    #[cfg(target_arch = "x86_64")]
    match mode.simd.level() {
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
    dense::gemv_tiles::<8, false>(rows, cols, a, a_off, lda, x, res, alpha);
}

/// FAST MODE: [`gemv`] with fused multiply-adds (each term and the final
/// `c·α + res`), the instance of the token's level (AVX-512F + FMA, AVX2 +
/// FMA; the bitwise kernel on the other levels).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[cold] // FAST MODE: keeps the bitwise dispatch small
#[inline(never)]
fn gemv_fma(
    fast: FastToken,
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
    match fast.simd().level() {
        SimdLevel::Avx512 => {
            // SAFETY: the fast token's level proves AVX-512F, AVX2 and FMA3.
            return unsafe { x86::gemv_avx512_fma(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        SimdLevel::Avx2 => {
            // SAFETY: the fast token's level proves AVX2 and FMA3.
            return unsafe { x86::gemv_avx2_fma(rows, cols, a, a_off, lda, x, res, alpha) };
        }
        SimdLevel::Scalar | SimdLevel::Neon => {}
    }
    gemv(
        Mode::bitwise(fast.simd()),
        rows,
        cols,
        a,
        a_off,
        lda,
        x,
        res,
        alpha,
    );
}

/// FAST MODE: `dense::trsm_unit_lower` with fused multiply-adds in its
/// `gebp` updates, the instance of the token's level (AVX-512F + FMA,
/// AVX2 + FMA; the bitwise kernel on the other levels).
#[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
#[cold] // FAST MODE: keeps the bitwise dispatch small
#[inline(never)]
pub(crate) fn trsm_fma(
    fast: FastToken,
    size: usize,
    cols: usize,
    t: &[f64],
    t_off: usize,
    ldt: usize,
    o: &mut [f64],
    o_off: usize,
    ldo: usize,
) {
    #[cfg(target_arch = "x86_64")]
    match fast.simd().level() {
        SimdLevel::Avx512 => {
            // SAFETY: the fast token's level proves AVX-512F, AVX2 and FMA3.
            return unsafe { x86::trsm_avx512_fma(size, cols, t, t_off, ldt, o, o_off, ldo) };
        }
        SimdLevel::Avx2 => {
            // SAFETY: the fast token's level proves AVX2 and FMA3.
            return unsafe { x86::trsm_avx2_fma(size, cols, t, t_off, ldt, o, o_off, ldo) };
        }
        SimdLevel::Scalar | SimdLevel::Neon => {}
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = fast;
    dense::trsm_unit_lower_bitwise(size, cols, t, t_off, ldt, o, o_off, ldo);
}

/// Places the hot loop of the AVX-512 gemm tile on a 64-byte boundary,
/// whatever address the linker gives the function: the default placement
/// put the loop head at 16 or 48 mod 64 in some builds and at 0 in others,
/// and those cost the gemm 8 % (`docs/benchmarks/perf-campaign/sparselu-threads`).
/// `.p2align 6` aligns this point (and raises the function's section to a
/// 64-byte alignment); the 16 bytes of NOPs compensate the loop's
/// preheader, which the compiler emits after this point (0x70 bytes with
/// rustc 1.99.1: the loop head then sits at 0 mod 64; checked in the
/// disassembly, see the benchmark README). Executed once per tile; no effect
/// on any value. Nothing on other architectures.
#[inline(always)]
pub(crate) fn align_next_loop() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: assembler alignment directives: they emit only NOP padding,
    // read and write no memory, registers or flags, and use no stack
    // (`nomem`, `nostack`, `preserves_flags`).
    unsafe {
        core::arch::asm!(
            ".p2align 6",
            ".nops 16",
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::dense::{gemm_tiles, gemv_tiles, trsm_unit_lower_fused};

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
        gemv_tiles::<32, false>(rows, cols, a, a_off, lda, x, res, alpha);
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
        gemv_tiles::<16, false>(rows, cols, a, a_off, lda, x, res, alpha);
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

    /// Fast mode: `gemv_tiles` with fused multiply-adds, AVX-512F.
    #[target_feature(enable = "avx512f,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemv_avx512_fma(
        rows: usize,
        cols: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        x: &[f64],
        res: &mut [f64],
        alpha: f64,
    ) {
        gemv_tiles::<32, true>(rows, cols, a, a_off, lda, x, res, alpha);
    }

    /// Fast mode: `gemv_tiles` with fused multiply-adds, AVX2.
    #[target_feature(enable = "avx2,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn gemv_avx2_fma(
        rows: usize,
        cols: usize,
        a: &[f64],
        a_off: usize,
        lda: usize,
        x: &[f64],
        res: &mut [f64],
        alpha: f64,
    ) {
        gemv_tiles::<16, true>(rows, cols, a, a_off, lda, x, res, alpha);
    }

    /// Fast mode: the triangular panel solve with fused multiply-adds in its
    /// `gebp` updates, AVX-512F.
    #[target_feature(enable = "avx512f,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn trsm_avx512_fma(
        size: usize,
        cols: usize,
        t: &[f64],
        t_off: usize,
        ldt: usize,
        o: &mut [f64],
        o_off: usize,
        ldo: usize,
    ) {
        trsm_unit_lower_fused(size, cols, t, t_off, ldt, o, o_off, ldo);
    }

    /// Fast mode: the triangular panel solve with fused multiply-adds, AVX2.
    #[target_feature(enable = "avx2,fma")]
    #[allow(clippy::too_many_arguments)] // BLAS-style kernel signature
    pub(super) fn trsm_avx2_fma(
        size: usize,
        cols: usize,
        t: &[f64],
        t_off: usize,
        ldt: usize,
        o: &mut [f64],
        o_off: usize,
        ldo: usize,
    ) {
        trsm_unit_lower_fused(size, cols, t, t_off, ldt, o, o_off, ldo);
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
    /// The FMA instances of the triangular panel solve and of the GEMV
    /// against their portable bodies with `FMA = true` (compiled without the
    /// `fma` feature, `f64::mul_add` is the correctly rounded software
    /// operation): the same per-entry sequence, so the same bits.
    #[test]
    fn fma_trsm_and_gemv_instances_match_the_portable_fused_bodies() {
        use akantu_core::bitwise_testing::Values;
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
        let mut v = Values::new(0x7E57_FA57_0000_0002);
        for case in 0..300usize {
            // trsm: sizes around the panel widths and the kc split
            let size = [3, 9, 17, 40, 100, 130][case % 6];
            let cols = 1 + case % 13;
            let ldt = size + case % 3;
            let t: Vec<f64> = (0..ldt * size + 1).map(|_| v.finite() / 8.0).collect();
            let o0: Vec<f64> = (0..size * cols).map(|_| v.finite()).collect();
            let mut want = o0.clone();
            super::dense::trsm_unit_lower_fused(size, cols, &t, 0, ldt, &mut want, 0, size);
            // gemv: rows, columns around the 128-column block rule
            let (rows, gc) = (1 + case % 70, [1, 7, 30, 127, 140][case % 5]);
            let a: Vec<f64> = (0..rows * gc).map(|_| v.finite()).collect();
            let x: Vec<f64> = (0..gc).map(|_| v.finite()).collect();
            let r0: Vec<f64> = (0..rows).map(|_| v.finite()).collect();
            let alpha = [1.0, -1.0, 0.5][case % 3];
            let mut gwant = r0.clone();
            super::dense::gemv_tiles::<8, true>(rows, gc, &a, 0, rows, &x, &mut gwant, alpha);
            for &fast in &tokens {
                let mut got = o0.clone();
                super::trsm_fma(fast, size, cols, &t, 0, ldt, &mut got, 0, size);
                for (p, q) in got.iter().zip(&want) {
                    assert_eq!(p.to_bits(), q.to_bits(), "trsm {fast:?} case {case}");
                }
                let mut g = r0.clone();
                let mode = super::Mode {
                    simd: fast.simd(),
                    fast: Some(fast),
                };
                super::gemv(mode, rows, gc, &a, 0, rows, &x, &mut g, alpha);
                for (p, q) in g.iter().zip(&gwant) {
                    assert_eq!(p.to_bits(), q.to_bits(), "gemv {fast:?} case {case}");
                }
            }
        }
    }
}
