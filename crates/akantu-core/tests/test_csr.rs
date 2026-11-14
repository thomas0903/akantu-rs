// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_common/test_csr.cc
// (TestCsrFixture.{CheckInsertion, Iteration, ReverseIteration}).

//! Port of `test_csr.cc` (TEST-002, CORE-029): a `CSR<Idx>` with `N = 1000`
//! rows of random length in `[0, N)` is built with the two-pass protocol
//! (count, `countToCSR`, `resizeCols`, `beginInsertions`, insertions of
//! `nb_cols − j`, `endInsertions`) and read back forwards and backwards.
//!
//! Upstream draws the row lengths with `rand()`; the intent (rows of
//! arbitrary, partly empty, lengths) is kept with the libstdc++-compatible
//! `minstd_rand0` engine of `akantu_core::random`, scaled exactly as
//! upstream scales `rand()` (`Int(r · N / (MAX + 1.))`).

use akantu_core::csr::Csr;
use akantu_core::random::MinstdRand0;
use akantu_core::{Idx, Int};

const N: usize = 1000;

struct Fixture {
    nb_cols_per_row: Vec<usize>,
    csr: Csr<Idx>,
}

fn set_up() -> Fixture {
    let mut csr = Csr::new(0);
    csr.resize_rows(N);
    csr.clear_rows();
    let mut rng = MinstdRand0::new();
    let max = MinstdRand0::MAX as f64;
    let mut nb_cols_per_row = Vec::with_capacity(N);
    for i in 0..N {
        let r = f64::from(rng.next_u32());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in [0, N)
        let nb_cols = (r * N as f64 / (max + 1.0)) as usize;
        nb_cols_per_row.push(nb_cols);
        for _ in 0..nb_cols {
            *csr.row_offset_mut(i).unwrap() += 1;
        }
    }
    csr.count_to_csr();
    csr.resize_cols();
    csr.begin_insertions();
    for (i, &nb_cols) in nb_cols_per_row.iter().enumerate() {
        for j in 0..nb_cols {
            csr.insert_in_row(i, Idx::try_from(nb_cols - j).unwrap())
                .unwrap();
        }
    }
    csr.end_insertions();
    Fixture {
        nb_cols_per_row,
        csr,
    }
}

#[test]
fn check_insertion() {
    let f = set_up();
    assert_eq!(N, f.csr.nb_rows());
    // the draws give empty rows as well as long ones (the intent of rand())
    assert!(f.nb_cols_per_row.contains(&0) || f.nb_cols_per_row.iter().any(|&c| c < 10));
    assert!(f.nb_cols_per_row.iter().any(|&c| c > N / 2));
}

#[test]
fn iteration() {
    let f = set_up();
    for i in 0..f.csr.nb_rows() {
        let mut nb_cols = Int::try_from(f.nb_cols_per_row[i]).unwrap();
        for &v in f.csr.row(i).unwrap() {
            assert_eq!(nb_cols, v);
            nb_cols -= 1;
        }
        assert_eq!(0, nb_cols);
    }
}

#[test]
fn reverse_iteration() {
    let f = set_up();
    for i in 0..f.csr.nb_rows() {
        let nb_cols = Int::try_from(f.nb_cols_per_row[i]).unwrap();
        let mut j = nb_cols;
        for &v in f.csr.row(i).unwrap().iter().rev() {
            assert_eq!(nb_cols - j + 1, v);
            j -= 1;
        }
        assert_eq!(0, j);
    }
}

/// The upstream draws give an empty row 0, so the
/// fixture cannot see `countToCSR` skipping the first row of the prefix
/// sum; the same protocol on rows whose first one is not empty.
#[test]
fn count_to_csr_accumulates_from_the_first_row() {
    let counts = [3_usize, 1, 0, 2];
    let mut csr: Csr<Idx> = Csr::new(0);
    csr.resize_rows(counts.len());
    csr.clear_rows();
    for (i, &c) in counts.iter().enumerate() {
        for _ in 0..c {
            *csr.row_offset_mut(i).unwrap() += 1;
        }
    }
    csr.count_to_csr();
    assert_eq!(csr.offsets(), &[0, 3, 4, 4, 6]);
    csr.resize_cols();
    csr.begin_insertions();
    for (i, &c) in counts.iter().enumerate() {
        for j in 0..c {
            csr.insert_in_row(i, Idx::try_from(c - j).unwrap()).unwrap();
        }
    }
    csr.end_insertions();
    for (i, &c) in counts.iter().enumerate() {
        assert_eq!(csr.nb_cols(i).unwrap(), c);
    }
    assert_eq!(csr.row(1).unwrap(), &[1]);
}
