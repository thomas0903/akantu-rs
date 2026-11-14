// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion/diffusion_model.cc
// (assembleCapacityLumped: makeConsistentForPeriodicity of the nodal
// capacity, then assembleToLumpedMatrix); mesh of
// test/test_mesh_utils/test_mesh_periodic/.
//! The heat-transfer model's lumped capacity on a
//! distributed **periodic** mesh
//! (`make_consistent_for_periodicity_values_in` through the DOF manager's
//! scope, `crates/akantu-diffusion/src/model/assembly.rs`). On the periodic
//! `square.msh` (x) at 2 / 4 ranks (local world, rank 0 reads, RCB, the
//! distributed `makePeriodic`), the lumped capacity per DOF at every node
//! of every rank that is local or a parallel master (periodic slaves
//! included: they read their master's equation) equals the serial value
//! of that global node, and the total over the independent nodes is
//! `ρ c |Ω|`. Before the distributed periodic wiring the assembly was refused
//! (`NotImplemented`); without the periodic reduction the slaves' halves
//! would be missing (the values at the periodic boundary would be about
//! half the serial ones).
//!
//! Tolerance: relative `1e-12` (NUMERICAL_VALIDATION: a
//! reordered sum of a few positive terms).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use akantu_comm::{Communicator, ReduceOp, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SpatialDirection};
use akantu_core::{Idx, NodeFlag, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::periodic::make_periodic;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_validation::fixtures_root;

const DAT: &str = "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    \
                   name = mat\n    density = 2\n    capacity = 3\n    \
                   conductivity = [[2.0, 0.3], [0.2, 1.5]]\n  ]\n]\n";
const TOLERANCE: Real = 1e-12;

fn periodic_square(comm: Option<&Communicator>) -> Mesh {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic/square.msh");
    let mut mesh = match comm {
        None => {
            let mut m = Mesh::new(2, "mesh").unwrap();
            akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
            m
        }
        Some(c) => {
            let mut m = Mesh::with_communicator(2, "mesh", c.clone()).unwrap();
            if c.rank() == 0 {
                akantu_io::read_mesh(&mut m, &f, MeshIoType::Gmsh).unwrap();
            }
            let mut p = RcbPartitioner;
            let _ = distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
            m
        }
    };
    let _ = make_periodic(&mut mesh, SpatialDirection::X, None).unwrap();
    mesh
}

/// `(global node, lumped capacity)` of every local or parallel-master node
/// (periodic slaves included), and the total over the independent nodes
/// (local or master, periodic slaves excluded) summed over the ranks.
fn lumped_capacity(comm: Option<&Communicator>) -> (Vec<(u64, Real)>, Real) {
    let mesh = periodic_square(comm);
    let parser = Parser::parse_str(DAT, "heat.dat").unwrap();
    let shared = SharedMesh::new(mesh);
    let mut model =
        HeatTransferModel::with_parser(shared.clone(), None, "heat_transfer", parser).unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_capacity_matrix_lumped().unwrap();
    let cap = model.lumped_capacity().unwrap();
    let mesh = shared.read().unwrap();
    let mut per_node = Vec::new();
    let mut total = [0.0];
    for (n, &c) in cap.as_slice().iter().enumerate() {
        let n = Idx::try_from(n).unwrap();
        let flag = mesh.node_flag(n);
        let shared_bits = flag.bits() & NodeFlag::SHARED_MASK.bits();
        if shared_bits == NodeFlag::NORMAL.bits() || shared_bits == NodeFlag::MASTER.bits() {
            per_node.push((u64::try_from(mesh.node_global_id(n)).unwrap(), c));
            if !flag.is_periodic_slave() {
                total[0] += c;
            }
        }
    }
    if let Some(c) = comm {
        c.all_reduce(&mut total, ReduceOp::Sum).unwrap();
    }
    (per_node, total[0])
}

fn rel(a: Real, b: Real) -> Real {
    ((a - b) / b).abs()
}

#[test]
fn distributed_periodic_lumped_capacity_matches_serial_on_2_and_4_ranks() {
    let (serial, serial_total) = lumped_capacity(None);
    let want: HashMap<u64, Real> = serial.iter().copied().collect();
    // independent check: Σ over the independent nodes = ρ c |Ω| (unit
    // square, ρ = 2, c = 3)
    assert!(
        rel(serial_total, 6.0) < TOLERANCE,
        "serial total {serial_total}"
    );
    for n in [2, 4] {
        let out = run_local(n, Duration::from_secs(300), |c| lumped_capacity(Some(&c)));
        let mut checked = 0;
        for (r, (per_node, total)) in out.iter().enumerate() {
            assert!(
                rel(*total, serial_total) < TOLERANCE,
                "{n} ranks, rank {r}: total {total} vs serial {serial_total}"
            );
            for (g, c) in per_node {
                assert!(
                    rel(*c, want[g]) < TOLERANCE,
                    "{n} ranks, rank {r}, global node {g}: lumped capacity {c} vs serial {}",
                    want[g]
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            want.len(),
            "{n} ranks: nodes checked vs serial nodes"
        );
        eprintln!(
            "{n} ranks: {checked} nodes, total {:.17e} (serial {serial_total:.17e})",
            out[0].1
        );
    }
}
