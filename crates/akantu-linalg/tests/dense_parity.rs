// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): parity of src/common/aka_types.hh (eig,
// eigh, determinant, inverse, products, norms), aka_voigthelper.hh and the
// std::complex division used by Eigen, against fixtures produced by the real
// upstream code (oracle/probes/dense/).

//! Bit-level parity of the dense layer with upstream (Eigen 3.4.0, strict
//! oracle flags). Every record of `fixtures/dense/dense_strict.txt` is
//! recomputed and compared **bit for bit** (two NaNs compare equal whatever
//! their payload), including the products Eigen evaluates with its blocked
//! GEMM kernel (`gebp`), reproduced by `akantu_core::eigen_gebp`
//! (`docs/NUMERICAL_VALIDATION.md`). The former GEMM tolerance class
//! (60 records) no longer applies to any record; the number of products
//! Eigen routes to `gebp` is reported.

use std::collections::BTreeMap;

use akantu_core::eigen_gebp::{self, ProductShape, Route};

use akantu_core::tensor::{Matrix, MatrixExt, MatrixX, SquareMatrixExt, Vector, VectorExt};
use akantu_core::voigt;
use akantu_linalg::dense::{self, Complex, ComplexDivision, SortOrder, Storage};

const FIXTURE: &str = include_str!("fixtures/dense/dense_strict.txt");
const LARGE_EIG_FIXTURE: &str = include_str!("fixtures/dense/dense_eig_large_strict.txt");

fn parse(tok: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(tok, 16).unwrap_or_else(|_| panic!("bad hex {tok}")))
}

/// Exact match (decision D-046: bit identity, or default quiet NaNs of
/// either sign; a looser "any two NaNs" match would hide payload
/// differences).
fn same(a: f64, b: f64) -> bool {
    akantu_validation::exact_eq(a, b)
}

/// Parsed field: numbers, or an upstream exception.
enum Field {
    Num(Vec<f64>),
    Exc,
}

fn fields(line: &str) -> (Vec<String>, Vec<Field>) {
    let mut parts = line.split(" | ");
    let head: Vec<String> = parts
        .next()
        .unwrap_or("")
        .split_whitespace()
        .map(String::from)
        .collect();
    let rest = parts
        .map(|p| {
            let p = p.trim();
            if p == "EXC" {
                Field::Exc
            } else {
                Field::Num(p.split_whitespace().map(parse).collect())
            }
        })
        .collect();
    (head, rest)
}

fn num(f: &Field) -> &[f64] {
    match f {
        Field::Num(v) => v,
        Field::Exc => panic!("unexpected EXC"),
    }
}

#[derive(Default)]
struct Stats {
    checked: usize,
    mismatches: Vec<String>,
}

impl Stats {
    fn cmp(&mut self, what: &str, line_no: usize, got: &[f64], want: &[f64]) {
        self.checked += 1;
        if got.len() != want.len() {
            self.mismatches.push(format!(
                "line {line_no} {what}: length {} != {}",
                got.len(),
                want.len()
            ));
            return;
        }
        if let Some(i) = (0..got.len()).find(|&i| !same(got[i], want[i])) {
            self.mismatches.push(format!(
                "line {line_no} {what}: entry {i}: got {:e} ({:016x}) want {:e} ({:016x})",
                got[i],
                got[i].to_bits(),
                want[i],
                want[i].to_bits()
            ));
        }
    }
}

fn mx(n: usize, m: usize, d: &[f64]) -> MatrixX {
    MatrixX::from_col_major(n, m, d.to_vec()).unwrap()
}

fn cvals(v: &[Complex]) -> Vec<f64> {
    v.iter().flat_map(|z| [z.re, z.im]).collect()
}

/// One `eig` record; `division`: the oracle runtime's complex division, for
/// the complex eigenvectors (the only field that depends on it).
fn check_eig(
    st: &mut Stats,
    line_no: usize,
    storage: Storage,
    n: usize,
    f: &[Field],
    division: ComplexDivision,
) {
    let a = mx(n, n, num(&f[0]));
    let tag = |s: &str| format!("eig {storage:?} {n} {s}");
    st.cmp(
        &tag("complex values"),
        line_no,
        &cvals(&dense::eig_complex(&a, storage).unwrap()),
        num(&f[1]),
    );
    st.cmp(
        &tag("real values"),
        line_no,
        &dense::eig(&a, storage).unwrap(),
        num(&f[2]),
    );
    for (k, order) in [(3, SortOrder::Descending), (4, SortOrder::Natural)] {
        let r = dense::eig_vectors(&a, storage, order);
        match (&f[k], r) {
            (Field::Exc, Err(_)) => st.checked += 1,
            (Field::Exc, Ok(_)) => st.mismatches.push(format!(
                "line {line_no} {}: expected exception",
                tag("vectors")
            )),
            (Field::Num(_), Err(e)) => st.mismatches.push(format!(
                "line {line_no} {}: unexpected error {e}",
                tag("vectors")
            )),
            (Field::Num(want), Ok((v, m))) => {
                let mut got = v.clone();
                got.extend_from_slice(m.as_slice());
                st.cmp(&tag(&format!("vectors {order:?}")), line_no, &got, want);
            }
        }
    }
    let (cv, cm) = dense::eig_vectors_complex_with(&a, storage, division).unwrap();
    let mut got = cvals(&cv);
    for col in &cm {
        got.extend(cvals(col));
    }
    st.cmp(&tag("complex vectors"), line_no, &got, num(&f[5]));
    st.cmp(
        &tag("eigh values"),
        line_no,
        &dense::eigh(&a, storage).unwrap(),
        num(&f[6]),
    );
    for (k, order) in [(7, SortOrder::Descending), (8, SortOrder::Natural)] {
        let (v, m) = dense::eigh_vectors(&a, storage, order).unwrap();
        let mut got = v;
        got.extend_from_slice(m.as_slice());
        st.cmp(
            &tag(&format!("eigh vectors {order:?}")),
            line_no,
            &got,
            num(&f[k]),
        );
    }
}

fn fixed<const N: usize>(d: &[f64]) -> Matrix<N, N> {
    std::array::from_fn(|j| std::array::from_fn(|i| d[i + j * N]))
}

fn rect<const R: usize, const C: usize>(d: &[f64]) -> Matrix<R, C> {
    std::array::from_fn(|j| std::array::from_fn(|i| d[i + j * R]))
}

fn detinv_fixed<const N: usize>(d: &[f64]) -> Vec<f64> {
    let m = fixed::<N>(d);
    let mut out = vec![m.det()];
    out.extend_from_slice(m.inverse_unchecked().as_flat());
    out
}

fn matmul_fixed<const R: usize, const C: usize, const K: usize>(d: &[f64]) -> Vec<f64> {
    let a = rect::<R, C>(&d[..R * C]);
    let b = rect::<C, K>(&d[R * C..]);
    a.matmul(&b).as_flat().to_vec()
}

fn trmatmul_fixed<const C: usize, const R: usize, const K: usize>(d: &[f64]) -> Vec<f64> {
    // at: C x R, b: C x K, result at^T b: R x K
    let at = rect::<C, R>(&d[..C * R]);
    let b = rect::<C, K>(&d[C * R..]);
    at.tr_matmul(&b).as_flat().to_vec()
}

fn reduce_fixed<const R: usize, const C: usize>(d: &[f64]) -> Vec<f64> {
    let a = rect::<R, C>(&d[..R * C]);
    let b = rect::<R, C>(&d[R * C..]);
    let (trace, dd) = if R == C {
        let s = MatrixX::from_col_major(R, C, a.as_flat().to_vec()).unwrap();
        let _ = s;
        (trace_of::<R, C>(&a), a.double_dot(&b))
    } else {
        (0.0, 0.0)
    };
    vec![
        a.norm(),
        a.squared_norm(),
        a.sum(),
        a.double_dot(&b),
        trace,
        dd,
    ]
}

fn trace_of<const R: usize, const C: usize>(a: &Matrix<R, C>) -> f64 {
    // only called with R == C
    match (R, C) {
        (1, 1) => fixed::<1>(a.as_flat()).trace(),
        (2, 2) => fixed::<2>(a.as_flat()).trace(),
        (3, 3) => fixed::<3>(a.as_flat()).trace(),
        (6, 6) => fixed::<6>(a.as_flat()).trace(),
        _ => panic!("no trace dispatch for {R}x{C}"),
    }
}

fn dot_fixed<const N: usize>(d: &[f64]) -> Vec<f64> {
    let a: Vector<N> = std::array::from_fn(|i| d[i]);
    let b: Vector<N> = std::array::from_fn(|i| d[N + i]);
    vec![a.dot(&b), a.norm()]
}

fn voigt_record<const D: usize, const S: usize>(nb: usize, d: &[f64]) -> Vec<f64> {
    let m = fixed::<D>(&d[..D * D]);
    let b = mx(D, nb, &d[D * D..D * D + D * nb]);
    let g = mx(D, D, &d[D * D + D * nb..]);
    let mut out = Vec::new();
    let v1 = voigt::matrix_to_voigt::<D, S>(&m);
    out.extend_from_slice(&v1);
    out.extend_from_slice(&voigt::matrix_to_voigt_with_factors::<D, S>(&m));
    out.extend_from_slice(voigt::voigt_to_matrix::<D, S>(&v1).as_flat());
    let mut bv = MatrixX::zeros(S, D * nb);
    voigt::b_matrix_to_sym_voigt(D, &b, &mut bv, nb).unwrap();
    out.extend_from_slice(bv.as_slice());
    let mut bnl = MatrixX::zeros(D * D, D * nb);
    voigt::b_matrix_to_bnl(D, &b, &mut bnl, nb).unwrap();
    out.extend_from_slice(bnl.as_slice());
    let mut bl2 = MatrixX::zeros(S, D * nb);
    voigt::b_matrix_to_bl2(D, &b, &g, &mut bl2, nb).unwrap();
    out.extend_from_slice(bl2.as_slice());
    out
}

#[test]
fn dense_layer_matches_upstream_bit_for_bit() {
    let mut st = Stats::default();
    let mut per_op: BTreeMap<String, usize> = BTreeMap::new();
    let mut gebp_records = 0usize;
    for (idx, line) in FIXTURE.lines().enumerate() {
        let line_no = idx + 1;
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let (head, f) = fields(line);
        let op = head[0].as_str();
        let storage = if head[1] == "F" {
            Storage::Fixed
        } else {
            Storage::Dynamic
        };
        let dims: Vec<usize> = head[2..].iter().map(|s| s.parse().unwrap()).collect();
        *per_op.entry(format!("{op} {}", head[1])).or_default() += 1;
        match op {
            "eig" => check_eig(
                &mut st,
                line_no,
                storage,
                dims[0],
                &f,
                ComplexDivision::Fused,
            ),
            "detinv" => {
                let n = dims[0];
                let input = num(&f[0]);
                let got = if storage == Storage::Fixed {
                    match n {
                        1 => detinv_fixed::<1>(input),
                        2 => detinv_fixed::<2>(input),
                        3 => detinv_fixed::<3>(input),
                        4 => detinv_fixed::<4>(input),
                        5 => detinv_fixed::<5>(input),
                        6 => detinv_fixed::<6>(input),
                        8 => detinv_fixed::<8>(input),
                        _ => panic!("no fixed detinv dispatch for {n}"),
                    }
                } else {
                    let a = mx(n, n, input);
                    let mut v = vec![a.det().unwrap()];
                    v.extend_from_slice(a.inverse_unchecked().unwrap().as_slice());
                    v
                };
                let mut want = num(&f[1]).to_vec();
                want.extend_from_slice(num(&f[2]));
                st.cmp(&format!("detinv {storage:?} {n}"), line_no, &got, &want);
            }
            "matmul" if storage == Storage::Fixed => {
                let d = num(&f[0]);
                let got = match (dims[0], dims[1], dims[2]) {
                    (1, 1, 1) => matmul_fixed::<1, 1, 1>(d),
                    (1, 3, 1) => matmul_fixed::<1, 3, 1>(d),
                    (3, 1, 3) => matmul_fixed::<3, 1, 3>(d),
                    (2, 2, 2) => matmul_fixed::<2, 2, 2>(d),
                    (3, 3, 3) => matmul_fixed::<3, 3, 3>(d),
                    (2, 3, 4) => matmul_fixed::<2, 3, 4>(d),
                    (3, 2, 4) => matmul_fixed::<3, 2, 4>(d),
                    (6, 6, 6) => matmul_fixed::<6, 6, 6>(d),
                    (6, 3, 6) => matmul_fixed::<6, 3, 6>(d),
                    (3, 6, 3) => matmul_fixed::<3, 6, 3>(d),
                    (1, 6, 6) => matmul_fixed::<1, 6, 6>(d),
                    (6, 6, 1) => matmul_fixed::<6, 6, 1>(d),
                    (8, 8, 1) => matmul_fixed::<8, 8, 1>(d),
                    (1, 8, 8) => matmul_fixed::<1, 8, 8>(d),
                    (9, 9, 1) => matmul_fixed::<9, 9, 1>(d),
                    (3, 9, 3) => matmul_fixed::<3, 9, 3>(d),
                    (4, 27, 1) => matmul_fixed::<4, 27, 1>(d),
                    (2, 30, 2) => matmul_fixed::<2, 30, 2>(d),
                    (8, 8, 8) => matmul_fixed::<8, 8, 8>(d),
                    (24, 6, 6) => matmul_fixed::<24, 6, 6>(d),
                    s => panic!("no fixed matmul dispatch for {s:?}"),
                };
                let (r, c, k) = (dims[0], dims[1], dims[2]);
                gebp_records += is_gebp(&ProductShape::fixed(r, c, k));
                st.cmp(&format!("matmul F {r} {c} {k}"), line_no, &got, num(&f[1]));
            }
            "matmul" => {
                let d = num(&f[0]);
                let (r, c, k) = (dims[0], dims[1], dims[2]);
                let a = mx(r, c, &d[..r * c]);
                let b = mx(c, k, &d[r * c..]);
                let got = a.matmul(&b).unwrap();
                gebp_records += is_gebp(&ProductShape::dynamic(r, c, k));
                st.cmp(
                    &format!("matmul D {r} {c} {k}"),
                    line_no,
                    got.as_slice(),
                    num(&f[1]),
                );
            }
            "trmatmul" => {
                let d = num(&f[0]);
                let got = match (dims[0], dims[1], dims[2]) {
                    (1, 1, 1) => trmatmul_fixed::<1, 1, 1>(d),
                    (3, 1, 1) => trmatmul_fixed::<3, 1, 1>(d),
                    (1, 3, 3) => trmatmul_fixed::<1, 3, 3>(d),
                    (2, 2, 2) => trmatmul_fixed::<2, 2, 2>(d),
                    (3, 3, 3) => trmatmul_fixed::<3, 3, 3>(d),
                    (3, 2, 4) => trmatmul_fixed::<3, 2, 4>(d),
                    (2, 3, 4) => trmatmul_fixed::<2, 3, 4>(d),
                    (6, 6, 6) => trmatmul_fixed::<6, 6, 6>(d),
                    (3, 6, 6) => trmatmul_fixed::<3, 6, 6>(d),
                    (6, 3, 3) => trmatmul_fixed::<6, 3, 3>(d),
                    (6, 1, 6) => trmatmul_fixed::<6, 1, 6>(d),
                    (6, 6, 1) => trmatmul_fixed::<6, 6, 1>(d),
                    (8, 8, 1) => trmatmul_fixed::<8, 8, 1>(d),
                    (8, 1, 8) => trmatmul_fixed::<8, 1, 8>(d),
                    (9, 9, 1) => trmatmul_fixed::<9, 9, 1>(d),
                    (9, 3, 3) => trmatmul_fixed::<9, 3, 3>(d),
                    (27, 4, 1) => trmatmul_fixed::<27, 4, 1>(d),
                    (30, 2, 2) => trmatmul_fixed::<30, 2, 2>(d),
                    (8, 8, 8) => trmatmul_fixed::<8, 8, 8>(d),
                    (6, 24, 6) => trmatmul_fixed::<6, 24, 6>(d),
                    s => panic!("no fixed trmatmul dispatch for {s:?}"),
                };
                let (c, r, k) = (dims[0], dims[1], dims[2]);
                gebp_records += is_gebp(&ProductShape::fixed(r, c, k).lhs_transposed());
                st.cmp(
                    &format!("trmatmul F {c} {r} {k}"),
                    line_no,
                    &got,
                    num(&f[1]),
                );
            }
            "reduce" => {
                let d = num(&f[0]);
                let got = match (dims[0], dims[1]) {
                    (1, 1) => reduce_fixed::<1, 1>(d),
                    (2, 2) => reduce_fixed::<2, 2>(d),
                    (3, 3) => reduce_fixed::<3, 3>(d),
                    (2, 3) => reduce_fixed::<2, 3>(d),
                    (6, 6) => reduce_fixed::<6, 6>(d),
                    (4, 9) => reduce_fixed::<4, 9>(d),
                    s => panic!("no reduce dispatch for {s:?}"),
                };
                st.cmp(&format!("reduce F {dims:?}"), line_no, &got, num(&f[1]));
            }
            "dot" => {
                let d = num(&f[0]);
                let got = match dims[0] {
                    1 => dot_fixed::<1>(d),
                    2 => dot_fixed::<2>(d),
                    3 => dot_fixed::<3>(d),
                    4 => dot_fixed::<4>(d),
                    6 => dot_fixed::<6>(d),
                    9 => dot_fixed::<9>(d),
                    27 => dot_fixed::<27>(d),
                    28 => dot_fixed::<28>(d),
                    37 => dot_fixed::<37>(d),
                    38 => dot_fixed::<38>(d),
                    n => panic!("no dot dispatch for {n}"),
                };
                st.cmp(&format!("dot F {}", dims[0]), line_no, &got, num(&f[1]));
            }
            "voigt" => {
                let d = num(&f[0]);
                let got = match dims[0] {
                    1 => voigt_record::<1, 1>(dims[1], d),
                    2 => voigt_record::<2, 3>(dims[1], d),
                    3 => voigt_record::<3, 6>(dims[1], d),
                    n => panic!("no voigt dispatch for {n}"),
                };
                st.cmp(&format!("voigt {dims:?}"), line_no, &got, num(&f[1]));
            }
            "cdiv" => {
                let d = num(&f[0]);
                let z = Complex::new(d[0], d[1]).quotient(Complex::new(d[2], d[3]));
                st.cmp("cdiv", line_no, &[z.re, z.im], num(&f[1]));
            }
            other => panic!("unknown op {other}"),
        }
    }
    eprintln!("records per op: {per_op:?}");
    eprintln!(
        "checked {} comparisons, all bit for bit; {gebp_records} product records routed to gebp",
        st.checked
    );
    if !st.mismatches.is_empty() {
        for m in st.mismatches.iter().take(40) {
            eprintln!("MISMATCH {m}");
        }
        panic!(
            "{} mismatches out of {} comparisons",
            st.mismatches.len(),
            st.checked
        );
    }
    assert!(st.checked > 5000, "fixture unexpectedly small");
    // 48 of the former 60 GEMM-tolerance records are gebp products; the
    // other 12 (fixed 3x9x3) are coefficient-based GemmProducts.
    assert_eq!(gebp_records, 48, "gebp coverage of the fixture changed");
}

/// CORE-019: dynamic `eig` / `eigh`
/// records for `13 ≤ n ≤ 48` (`fixtures/dense/dense_eig_large_strict.txt`,
/// probe `--eig-large`, x86_64 oracle with the D-050 pin, provenance in its
/// header): tie-free symmetric, SPD, general and complex-pair matrices up
/// to `n = 48`, and tied spectra (repeated eigenvalues, diagonal ties with
/// `-0.0`) at `n ≤ 16`, all inside the bitwise scope of `sort = true`.
/// Every field of every record (values, sorted and natural vectors,
/// `eigh`, complex vectors) is compared bit for bit as in the main fixture.
/// The records come from the x86_64 runtime, so the complex eigenvectors
/// are computed with its complex division ([`ComplexDivision::Unfused`],
/// D-043); with the aarch64 runtime's ([`ComplexDivision::Fused`]) 1,238
/// complex-eigenvector entries of the two general records with complex
/// pairs (n = 17, 40) differ in their last bits
/// (`fused_division_differs_on_the_x86_64_records`).
#[test]
fn large_eig_records_match_upstream_bit_for_bit() {
    let mut st = Stats::default();
    let mut sizes = Vec::new();
    for (line_no, n, f) in large_records() {
        check_eig(
            &mut st,
            line_no,
            Storage::Dynamic,
            n,
            &f,
            ComplexDivision::Unfused,
        );
        sizes.push(n);
    }
    eprintln!(
        "large eig records: sizes {sizes:?}, {} comparisons, all bit for bit",
        st.checked
    );
    if !st.mismatches.is_empty() {
        for m in st.mismatches.iter().take(40) {
            eprintln!("MISMATCH {m}");
        }
        panic!(
            "{} mismatches out of {} comparisons",
            st.mismatches.len(),
            st.checked
        );
    }
    // the probe's case list (oracle/probes/dense/probe.cc, eig_large)
    assert_eq!(sizes, [13, 17, 24, 48, 33, 17, 40, 20, 13, 16, 16]);
}

/// The runtime matters: with the aarch64 complex division, exactly the
/// complex-eigenvector fields of the general records with complex pairs
/// (lines 15 and 16: n = 17 and n = 40) differ from the x86_64 records, and
/// every other field is still bit for bit.
#[test]
fn fused_division_differs_on_the_x86_64_records() {
    let mut st = Stats::default();
    for (line_no, n, f) in large_records() {
        check_eig(
            &mut st,
            line_no,
            Storage::Dynamic,
            n,
            &f,
            ComplexDivision::Fused,
        );
    }
    let lines: Vec<&str> = st
        .mismatches
        .iter()
        .map(|m| m.split(':').next().unwrap())
        .collect();
    assert_eq!(
        lines,
        [
            "line 15 eig Dynamic 17 complex vectors",
            "line 16 eig Dynamic 40 complex vectors"
        ]
    );
}

/// The `eig` records of the large fixture: (line, n, fields).
fn large_records() -> Vec<(usize, usize, Vec<Field>)> {
    LARGE_EIG_FIXTURE
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty())
        .map(|(idx, line)| {
            let (head, f) = fields(line);
            assert_eq!(
                (head[0].as_str(), head[1].as_str()),
                ("eig", "D"),
                "line {}: only dynamic eig records",
                idx + 1
            );
            (idx + 1, head[2].parse().unwrap(), f)
        })
        .collect()
}

fn is_gebp(shape: &ProductShape) -> usize {
    usize::from(matches!(eigen_gebp::route(shape), Route::Gebp { .. }))
}
