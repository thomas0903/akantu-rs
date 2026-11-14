// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the Array<double>::printself block of
// test/test_solver/test_sparse_matrix_product.verified (copied verbatim to
// tests/data/upstream/), produced by src/common/aka_array_tmpl.hh
// (Array::printself at debug level dblTest).

//! CORE-013, GOLD-001/TEST-157: the core printer
//! reproduces, byte for byte, the `Array<double>` block that upstream's
//! golden file compares (`std::cout << dof_vector << std::endl` at
//! `dblTest`: 121 tuples `{2, 2}`, id `vector`, allocated size 121, memory
//! size with the UD-457 factor). The progress lines before it belong to the
//! test program (`akantu-solver`).

use akantu_core::Array;
use akantu_core::array_print::ArrayPrint;
use akantu_core::debug::{DebugLevel, DebugLevelContext};

const VERIFIED: &str = include_str!("data/upstream/test_sparse_matrix_product.verified");

#[test]
fn test_sparse_matrix_product_verified_block() {
    let block = VERIFIED
        .split_once("Array<double> [")
        .map(|(_, rest)| format!("Array<double> [{rest}"))
        .unwrap();
    let mut v = Array::from_vec(vec![2.0_f64; 121 * 2], 2).unwrap();
    v.set_id("vector");
    // debug::setDebugLevel(dblTest); std::cout << dof_vector << std::endl;
    let _ctx = DebugLevelContext::new(DebugLevel::TEST);
    let printed = v.print_self(&ArrayPrint {
        allocated_size: Some(121),
        ..ArrayPrint::default()
    }) + "\n";
    assert_eq!(printed, block);
}
