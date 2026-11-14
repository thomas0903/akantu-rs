// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_types.hh (eig / eigh with
// sort = true: the eigenvalues sorted descending with std::sort and the
// eigenvector columns permuted with them); oracle records of a probe
// (libstdc++'s sort observed black-box only, D-030).

//! CORE-019: the 72 tie cases of the
//! probe (`tests/data/oracle/v104_sort_ties.txt`, provenance in
//! its header): diagonal matrices `d_i = ((7 i) mod n) + 1` of sizes 16, 17,
//! 18, 20, 24 and 33 with exact ties (none, `pair01`, `pair2last`, `triple`,
//! `twopairs`, `pairmid`), through `eigh` and `eig` with `sort = true`.
//!
//! * Inside the bitwise scope (`n ≤ 16`, or no tie): the eigenvalues and the
//!   eigenvector column order (row of each column's non-zero entry, with its
//!   sign) are **identical** to the oracle's.
//! * Outside it (a tie at `n ≥ 17`): a valid decomposition under
//!   `docs/NUMERICAL_VALIDATION.md`: the same sorted eigenvalues, every
//!   column a signed unit vector `±e_r` with `A e_r = λ e_r` exactly, the
//!   columns orthonormal, and each tied eigenvalue's columns spanning the
//!   same coordinate subspace as the oracle's; the order inside a tie group
//!   may differ (libstdc++ introsort).

use akantu_core::tensor::MatrixX;
use akantu_linalg::dense::{SortOrder, Storage, eig_vectors, eigh_vectors};

const ORACLE: &str = include_str!("data/oracle/v104_sort_ties.txt");

/// Per column: (row of the non-zero entry, its sign, number of non-zeros).
fn columns(v: &MatrixX) -> Vec<(usize, bool, usize)> {
    let n = v.rows();
    (0..n)
        .map(|k| {
            let nz: Vec<usize> = (0..n).filter(|&i| v.at(i, k) != 0.0).collect();
            let r = *nz.last().unwrap();
            (r, v.at(r, k) < 0.0, nz.len())
        })
        .collect()
}

/// The oracle's `perm=` field in the same form.
fn oracle_columns(perm: &str) -> Vec<(usize, bool, usize)> {
    perm.split(',')
        .map(|t| {
            let several = t.starts_with('?');
            let t = t.trim_start_matches('?');
            let neg = t.ends_with('-');
            let r: usize = t.trim_end_matches('-').parse().unwrap();
            (r, neg, if several { 2 } else { 1 })
        })
        .collect()
}

#[test]
fn sort_ties_match_the_oracle_inside_the_scope_and_decompose_outside() {
    let (mut cases, mut bitwise, mut outside, mut reordered) = (0, 0, 0, 0);
    for l in ORACLE.lines().filter(|l| l.starts_with("SORT ")) {
        let f: Vec<&str> = l.split(' ').collect();
        let (kind, label) = (f[1], f[2]);
        let n: usize = f[3].trim_start_matches("n=").parse().unwrap();
        let oracle_values: Vec<f64> = f[4]
            .trim_start_matches("values=")
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect();
        let oracle_cols = oracle_columns(f[5].trim_start_matches("perm="));
        let mut d: Vec<f64> = (0..n).map(|i| ((i * 7) % n + 1) as f64).collect();
        let ties: Vec<(usize, usize)> = match label {
            "none" => vec![],
            "pair01" => vec![(0, 1)],
            "pair2last" => vec![(2, n - 1)],
            "triple" => vec![(0, 5), (0, 9)],
            "twopairs" => vec![(1, 3), (4, n - 2)],
            "pairmid" => vec![(n / 2, n / 2 + 1)],
            other => panic!("unknown case {other}"),
        };
        let tied = !ties.is_empty();
        for (a, b) in ties {
            d[b] = d[a];
        }
        let mut a = MatrixX::zeros(n, n);
        for (i, &di) in d.iter().enumerate() {
            *a.at_mut(i, i) = di;
        }
        let (values, v) = if kind == "eigh" {
            eigh_vectors(&a, Storage::Dynamic, SortOrder::Descending).unwrap()
        } else {
            eig_vectors(&a, Storage::Dynamic, SortOrder::Descending).unwrap()
        };
        let tag = format!("{kind} {label} n={n}");
        cases += 1;
        // the sorted spectrum is the same in every case (oracle printed with %g:
        // the values are small integers here)
        assert_eq!(values, oracle_values, "{tag}: eigenvalues");
        let mine = columns(&v);
        if n <= 16 || !tied {
            assert_eq!(mine, oracle_cols, "{tag}: inside the bitwise scope");
            bitwise += 1;
            continue;
        }
        outside += 1;
        if mine != oracle_cols {
            reordered += 1;
        }
        // The tie rule (docs/NUMERICAL_VALIDATION.md) on a diagonal matrix: signed unit columns, eigenpairs exact,
        // orthonormal, and per eigenvalue the same set of coordinate axes
        for (k, &(r, _, nnz)) in mine.iter().enumerate() {
            assert_eq!(nnz, 1, "{tag}: column {k} is not a coordinate vector");
            assert_eq!(v.at(r, k).abs(), 1.0, "{tag}: column {k} is not unit");
            assert_eq!(d[r], values[k], "{tag}: A e_r = λ e_r for column {k}");
        }
        let mut rows: Vec<usize> = mine.iter().map(|c| c.0).collect();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows.len(), n, "{tag}: columns not orthonormal");
        for lambda in values.iter() {
            let set = |cols: &[(usize, bool, usize)]| {
                let mut s: Vec<usize> = cols
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| values[*k] == *lambda)
                    .map(|(_, c)| c.0)
                    .collect();
                s.sort_unstable();
                s
            };
            assert_eq!(
                set(&mine),
                set(&oracle_cols),
                "{tag}: eigenspace of {lambda}"
            );
        }
    }
    eprintln!(
        "sort ties: {cases} cases; {bitwise} inside the bitwise scope (identical), {outside} \
         outside it (valid decompositions, {reordered} with another order inside a tie group)"
    );
    assert_eq!(cases, 72);
    // the 12 cases at n = 16 and the 12 tie-free cases (2 of them at n = 16)
    assert_eq!(bitwise, 22, "cases inside the bitwise scope");
    // 32 of the 48 tied cases at n >= 17 differ in column order
    assert_eq!(reordered, 32, "tie cases at n >= 17 ordered differently");
}
