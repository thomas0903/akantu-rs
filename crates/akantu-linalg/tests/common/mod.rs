// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance helpers (docs/NUMERICAL_VALIDATION.md): the "GEMM order" tolerance class of upstream's Eigen products
// (src/common/aka_types.hh) and the fixture record format of
// oracle/probes/dense/.

//! Shared helpers of the dense acceptance tests.

#![allow(dead_code)] // each test binary uses a subset of the helpers

/// Unit roundoff `u = 2⁻⁵³`.
pub const U: f64 = f64::EPSILON / 2.0;

/// `γ_n = n u / (1 − n u)` (Higham, *Accuracy and Stability*, §3.1).
pub fn gamma(n: usize) -> f64 {
    let nu = n as f64 * U;
    nu / (1.0 - nu)
}

/// Outcome of a GEMM-order acceptance check for one result entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GemmVerdict {
    /// Exact under D-046 (bit identity, or default NaNs of either sign).
    Exact,
    /// Within `2 γ_n Σ|t_k|`; carries `err / bound` (`< 1`).
    WithinBound(f64),
}

/// Accept `got` as a **different summation order** of the same rounded
/// products `terms[k] = fl(a_ik · b_kj)` that produced `want`.
///
/// Both orders sum identical rounded products, so each differs from the
/// exact sum `S = Σ t_k` by at most `γ_{n−1} Σ|t_k|`; hence
/// `|got − want| ≤ 2 γ_{n−1} Σ|t_k|`. The bound is evaluated with `γ_n` and
/// divided by `(1 − γ_n)` to cover the rounding of `Σ|t_k|` itself.
///
/// The bound assumes finite, normal arithmetic. Outside it the entry must
/// match **exactly** (D-046: bit identity, or default NaNs of either sign):
/// any non-finite term, `got` or
/// `want`; a non-zero term below the normal range (underflow/subnormal: the
/// relative error model fails); an overflowing `Σ|t_k|`. A NaN error can
/// never pass (`!(err <= bound)`), and an all-zero term list requires
/// `got == want` (signed zeros may differ between orders and compare equal).
pub fn gemm_entry(terms: &[f64], got: f64, want: f64) -> Result<GemmVerdict, String> {
    // D-046: bit identity, or default NaNs of either sign
    let exact = akantu_validation::exact_eq(want, got);
    if exact {
        return Ok(GemmVerdict::Exact);
    }
    let outside_model = !got.is_finite()
        || !want.is_finite()
        || terms
            .iter()
            .any(|t| !t.is_finite() || (*t != 0.0 && t.abs() < f64::MIN_POSITIVE));
    if outside_model {
        return Err(format!(
            "outside the bound's assumptions (non-finite or subnormal): got {got:e}, want {want:e}"
        ));
    }
    let abs_sum: f64 = terms.iter().map(|t| t.abs()).sum();
    if !abs_sum.is_finite() {
        return Err(format!(
            "sum of |terms| overflows: got {got:e}, want {want:e}"
        ));
    }
    let n = terms.len();
    let g = gamma(n);
    let bound = 2.0 * g * abs_sum / (1.0 - g);
    let err = (got - want).abs();
    if bound == 0.0 {
        return if got == want {
            Ok(GemmVerdict::WithinBound(0.0))
        } else {
            Err(format!("all terms zero but got {got:e} != want {want:e}"))
        };
    }
    if err <= bound {
        Ok(GemmVerdict::WithinBound(err / bound))
    } else {
        Err(format!("err {err:e} > bound {bound:e} (or NaN)"))
    }
}

/// Parse one hexadecimal `f64` of a fixture record.
pub fn hex(tok: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(tok, 16).unwrap_or_else(|_| panic!("bad hex {tok}")))
}

/// A `matmul`/`trmatmul` fixture record as `(R × C) · (C × K)` operands
/// (column-major), whether the upstream operands were fixed-size, whether
/// it was a transposed-lhs product, and the expected `R × K` result.
pub struct ProductRecord {
    pub line: usize,
    pub fixed: bool,
    pub transposed: bool,
    pub r: usize,
    pub c: usize,
    pub k: usize,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub want: Vec<f64>,
}

/// All product records of the dense fixture.
pub fn product_records(fixture: &str) -> Vec<ProductRecord> {
    let mut out = Vec::new();
    for (idx, line) in fixture.lines().enumerate() {
        let tr = line.starts_with("trmatmul ");
        if !(line.starts_with("matmul ") || tr) {
            continue;
        }
        let mut parts = line.split(" | ");
        let head: Vec<&str> = parts.next().unwrap_or("").split_whitespace().collect();
        let d: Vec<f64> = parts
            .next()
            .unwrap_or("")
            .split_whitespace()
            .map(hex)
            .collect();
        let want: Vec<f64> = parts
            .next()
            .unwrap_or("")
            .split_whitespace()
            .map(hex)
            .collect();
        let dims: Vec<usize> = head[2..5].iter().map(|s| s.parse().unwrap()).collect();
        // matmul: a = R×C, b = C×K; trmatmul: stored at = C×R, b = C×K.
        let (r, c, k) = if tr {
            (dims[1], dims[0], dims[2])
        } else {
            (dims[0], dims[1], dims[2])
        };
        let mut a = vec![0.0; r * c];
        for i in 0..r {
            for l in 0..c {
                a[i + l * r] = if tr { d[l + i * c] } else { d[i + l * r] };
            }
        }
        out.push(ProductRecord {
            line: idx + 1,
            fixed: head[1] == "F",
            transposed: tr,
            r,
            c,
            k,
            a,
            b: d[r * c..r * c + c * k].to_vec(),
            want,
        });
    }
    out
}
