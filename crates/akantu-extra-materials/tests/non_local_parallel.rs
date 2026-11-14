// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative_non_local_parallel.cc,
// test_material_damage_iterative_non_local_parallel.sh (serial run, then
// `mpirun -np 4`), two_materials.dat, one_circular_inclusion.msh}; and the
// extra non-local laws on distributed meshes.

//! The extra non-local laws on distributed meshes:
//!
//! * TEST-187 — upstream's `test_material_damage_iterative_non_local_parallel`
//!   with its own criterion: the serial run, then the distributed run (2 and
//!   4 ranks: upstream's `.sh` runs `mpirun -np 4`), two static solves
//!   (modified Newton–Raphson, increment criterion `1e-12`, two iterations)
//!   with the iterative damage of the law holding the larger maximum in
//!   between; at both solves the displacement of every owned node of the
//!   distributed run differs from the serial one by at most `1e-9`
//!   (upstream's `checkDisplacement`, which matches elements by barycentre;
//!   here by global connectivity). The package never compiled at the
//!   reference commit: no oracle run exists; the criterion is
//!   upstream's.
//! * the four extra non-local laws (`damage_iterative_non_local`,
//!   `brittle_non_local`, `vreepeerlings_non_local`,
//!   `damage_orthotropic_iterative_non_local`) on a distributed plate: the
//!   averaged variables of every owned point equal the serial ones within
//!   the summation-order bound of NUMERICAL_VALIDATION.

mod common;
mod par_xmat;

use std::time::Duration;

use akantu_comm::run_local;
use par_xmat::{laws_vs_serial, test_187, xn11};

#[test]
fn test_material_damage_iterative_non_local_parallel() {
    for np in [2, 4] {
        for (r, o) in run_local(np, Duration::from_secs(300), test_187)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}

#[test]
fn extra_non_local_laws_distributed_vs_serial() {
    for np in [2, 4] {
        for (r, o) in run_local(np, Duration::from_secs(300), laws_vs_serial)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}

#[test]
fn xn11_rank_local_maximum_with_the_global_rule_is_refused() {
    for np in [1, 2, 4] {
        for (r, o) in run_local(np, Duration::from_secs(120), xn11)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("np {np} rank {r}: {e}");
            }
        }
    }
}
