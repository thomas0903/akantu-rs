// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_periodic.cc
// (makeConsistentForPeriodicity on a distributed mesh); mesh of
// test/test_mesh_utils/test_mesh_periodic/.
//! Regressions: the own-scope entry points of the
//! distributed `makeConsistentForPeriodicity` (model level
//! `make_consistent_for_periodicity_values`, mesh level
//! `make_consistent_for_periodicity_{real,bool}`) checked the array length
//! **before** opening their collective scope, so a rank-specific wrong array
//! (a caller bug) made that rank return while the others entered the
//! exchange and waited (local world: timeout; MPI: hang). The checks are
//! now agreed inside the scope: at 2 ranks with a wrong array on rank 1,
//! every rank returns an error well before the timeout (rank 1 its own
//! `DimensionMismatch`, rank 0 "failed on another rank") and rank 0's array
//! is unchanged. Control: right arrays on every rank succeed.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MeshIoType, ReduceOperation as MeshReduce, SpatialDirection};
use akantu_core::{Array, Real};
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::periodic::{
    make_consistent_for_periodicity_bool, make_consistent_for_periodicity_real, make_periodic,
};
use akantu_model::model::{ReduceOperation, make_consistent_for_periodicity_values};
use akantu_validation::fixtures_root;

const TIMEOUT: Duration = Duration::from_secs(15);
const PROMPT: f64 = 5.0;

fn periodic_square(comm: &Communicator) -> Mesh {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic/square.msh");
    let mut m = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
    }
    let mut p = RcbPartitioner;
    let _ = distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    let _ = make_periodic(&mut m, SpatialDirection::X, None).unwrap();
    m
}

#[derive(Clone, Copy, Debug)]
enum Entry {
    ModelValues,
    MeshReal,
    MeshBool,
}

/// `(seconds, outcome, rank 0's array unchanged)` of one call of `entry`
/// with one tuple too many on rank 1 when `wrong`.
fn call(comm: Communicator, entry: Entry, wrong: bool) -> (f64, Result<(), String>, bool) {
    let mesh = periodic_square(&comm);
    let n = mesh.nb_nodes() + usize::from(wrong && comm.rank() == 1);
    let t0 = Instant::now();
    let (r, unchanged) = match entry {
        Entry::ModelValues => {
            let mut a: Vec<Real> = (0..n * 2).map(|i| i as Real).collect();
            let before = a.clone();
            let r = make_consistent_for_periodicity_values(
                &mesh,
                &mut a,
                n,
                2,
                ReduceOperation::Sum,
                |x, y| x + y,
            );
            (r, a == before)
        }
        Entry::MeshReal => {
            let mut a = Array::<Real>::try_zeros(n, 2).unwrap();
            for (i, v) in a.as_mut_slice().iter_mut().enumerate() {
                *v = i as Real;
            }
            let before = a.as_slice().to_vec();
            let r = make_consistent_for_periodicity_real(&mesh, &mut a, MeshReduce::Sum);
            (r, a.as_slice() == before.as_slice())
        }
        Entry::MeshBool => {
            let mut a = Array::<bool>::new(1);
            for i in 0..n {
                a.push_tuple(&[i % 3 == 0]);
            }
            let before = a.as_slice().to_vec();
            let r = make_consistent_for_periodicity_bool(&mesh, &mut a, MeshReduce::Or);
            (r, a.as_slice() == before.as_slice())
        }
    };
    (
        t0.elapsed().as_secs_f64(),
        r.map_err(|e| e.to_string()),
        unchanged,
    )
}

#[test]
fn wrong_array_on_one_rank_is_an_agreed_refusal() {
    for entry in [Entry::ModelValues, Entry::MeshReal, Entry::MeshBool] {
        let out = run_local(2, TIMEOUT, move |c| call(c, entry, true));
        for (r, (dt, res, unchanged)) in out.iter().enumerate() {
            eprintln!("{entry:?}: rank {r}: {dt:.2} s: {res:?}");
            assert!(*dt < PROMPT, "{entry:?}: rank {r} returned after {dt:.2} s");
            let err = res
                .as_ref()
                .expect_err(&format!("{entry:?}: rank {r} succeeded"));
            if r == 1 {
                assert!(
                    err.contains("makeConsistentForPeriodicity"),
                    "{entry:?}: {err}"
                );
            } else {
                assert!(err.contains("failed on another rank"), "{entry:?}: {err}");
                assert!(*unchanged, "{entry:?}: rank 0 changed its array");
            }
        }
    }
}

#[test]
fn right_arrays_are_made_consistent_on_every_rank() {
    for entry in [Entry::ModelValues, Entry::MeshReal, Entry::MeshBool] {
        let out = run_local(2, TIMEOUT, move |c| call(c, entry, false));
        for (r, (_, res, _)) in out.iter().enumerate() {
            assert_eq!(res, &Ok(()), "{entry:?}: rank {r}");
        }
    }
}
