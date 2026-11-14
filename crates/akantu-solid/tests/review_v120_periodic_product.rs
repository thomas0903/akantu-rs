// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix_aij.cc
// (matVecMul), src/synchronizer/dof_synchronizer.cc (registerDOFs),
// src/model/common/dof_manager/dof_manager.cc (assembleMatMulVectToResidual);
// mesh and material of test/test_mesh_utils/test_mesh_periodic/.
//! Regressions of UD-872: the distributed matrix–vector
//! product (`assemble_mat_mul_vect_to_residual`, Newmark's `r −= M·ü`) on a
//! distributed **periodic** mesh. A rank that holds a periodic master only
//! as a pure-ghost copy (its local periodic slaves alias the master's
//! equation, made `Shared` by the distributed periodic numbering)
//! computes a partial row there; that row reaches the owner
//! only through the periodic node synchronizer (upstream's DOF synchronizer
//! is built from the node synchronizer's schemes alone,
//! `dof_synchronizer.cc:64-117`, so the row would be lost even with UD-003
//! corrected).
//!
//! * `Σ_owned (M·1)` per component is the total mass `Σ_ij M_ij`,
//!   independent of the partition: serial vs 2 / 4 / 8 ranks (local world,
//!   rank 0 reads, RCB, distributed `makePeriodic`, as `upstream_periodic`).
//!   Before the fix: 6825 instead of 7800 at 2 ranks. The same product per
//!   node (`assemble_mat_mul_vect_to_array`) at every node of every rank
//!   that is local or a parallel master (periodic slaves included) equals the serial value of that global
//!   node: a local slave of a pure-ghost master copy reads the owner's total
//!   only through the periodic owner → copy step.
//! * Non-periodic control: the product at 2 / 4 ranks equals, **bit for
//!   bit on every local row**, the node-synchronizer-only reduction of the
//!   rank-partial products (the reduction before the fix), computed here
//!   independently of the DOF manager's reducer: the periodic channel does
//!   not touch non-periodic meshes. On the periodic mesh the same
//!   node-only reduction loses exactly the rows the fix restores.
//! * One implicit-dynamic Newmark step (consistent mass): `Σu_y` and the
//!   consistent-mass kinetic energy, serial vs 2 / 4 ranks on the periodic
//!   square (before the fix: `NotConverged` at 2 and 4 ranks), and the
//!   non-periodic control.
//!
//! Tolerances: NUMERICAL_VALIDATION (round-off of a
//! reordered sum, measured ≤ 2e-16).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use akantu_comm::{CommunicationBuffer, Communicator, DataAccessor, ReduceOp, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SpatialDirection, SynchronizationTag};
use akantu_core::{Idx, NodeFlag, Real};
use akantu_linalg::layout::{EquationOwnership, NoReduction};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::periodic::make_periodic;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;

const MATERIAL: &str =
    "material elastic [\n name = steel\n rho = 7800\n E = 2.1e11\n nu = 0.3\n]\n";
const TIMEOUT: Duration = Duration::from_secs(300);
/// Relative bound on the round-off of a reordered sum of positive terms.
const SUM_TOLERANCE: Real = 1e-12;
/// Relative bound on `Σu_y` after one linear implicit step (serial vs
/// distributed solve).
const STEP_TOLERANCE: Real = 1e-9;

fn mesh_on(comm: Option<&Communicator>, periodic: bool) -> Mesh {
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
    if periodic {
        let _ = make_periodic(&mut mesh, SpatialDirection::X, None).unwrap();
    }
    mesh
}

/// Node → local equations of the displacement family (test-side copy of
/// the DOF manager's map).
fn node_equations(m: &SolidMechanicsModel) -> HashMap<Idx, Vec<usize>> {
    let fam = m.dof_manager().family("displacement").unwrap();
    let mut eqs: HashMap<Idx, Vec<usize>> = HashMap::new();
    for (&n, &e) in fam.associated_nodes.iter().zip(&fam.local_equation_numbers) {
        eqs.entry(n).or_default().push(usize::try_from(e).unwrap());
    }
    eqs
}

/// Rows of a local-equation vector per node (sum or copy on receipt).
struct Rows<'a> {
    eqs: &'a HashMap<Idx, Vec<usize>>,
    v: &'a mut [Real],
    add: bool,
}

impl DataAccessor<Idx> for Rows<'_> {
    fn nb_data(&self, nodes: &[Idx], _: SynchronizationTag) -> usize {
        8 * nodes
            .iter()
            .map(|n| self.eqs.get(n).map_or(0, Vec::len))
            .sum::<usize>()
    }
    fn pack_data(
        &self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for n in nodes {
            for &e in self.eqs.get(n).into_iter().flatten() {
                buf.pack(&self.v[e]);
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for n in nodes {
            for &e in self.eqs.get(n).into_iter().flatten() {
                let x: Real = buf.unpack()?;
                if self.add {
                    self.v[e] += x;
                } else {
                    self.v[e] = x;
                }
            }
        }
        Ok(())
    }
}

struct Product {
    /// `Σ_owned (M·1)` per component, summed over the ranks.
    total: [Real; 2],
    /// The same for the node-synchronizer-only reduction.
    node_only_total: [Real; 2],
    /// Local rows of the DOF manager's product equal, bit for bit, to the
    /// node-only reduction (on this rank).
    bitwise_node_only: bool,
    /// `(global node, (M·1)_n)` of every local or parallel-master node
    /// (periodic slaves included) through `assemble_mat_mul_vect_to_array`: a local
    /// periodic slave reads its master's row, so its value needs the owner →
    /// copy step when the master is a pure-ghost copy on this rank.
    per_node: Vec<(u64, [Real; 2])>,
}

fn owned_sums(m: &SolidMechanicsModel, v: &[Real]) -> [Real; 2] {
    let dm = m.dof_manager();
    let layout = dm.layout();
    let fam = dm.family("displacement").unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut sums = [0.0, 0.0];
    for (k, &eq) in fam.local_equation_numbers.iter().enumerate() {
        let eq = usize::try_from(eq).unwrap();
        if seen.insert(eq) && layout.is_owned(eq) {
            sums[k % 2] += v[eq];
        }
    }
    sums
}

fn reduce_sum(comm: Option<&Communicator>, mut v: [Real; 2]) -> [Real; 2] {
    if let Some(c) = comm {
        c.all_reduce(&mut v, ReduceOp::Sum).unwrap();
    }
    v
}

/// `residual = M · 1` through the DOF manager, and the independent
/// node-only reduction of the rank-partial product.
fn product(comm: Option<&Communicator>, periodic: bool) -> Product {
    let mesh = mesh_on(comm, periodic);
    let nb_nodes = mesh.nb_nodes();
    let parser = Parser::parse_str(MATERIAL, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))
        .unwrap();
    m.assemble_mass().unwrap();
    let ones = vec![1.0; nb_nodes * 2];
    {
        let dm = m.dof_manager_mut();
        dm.zero_residual();
        dm.assemble_mat_mul_vect_to_residual("displacement", "M", &ones, 1.0)
            .unwrap();
    }
    let r = m.dof_manager().residual().values().to_vec();
    let mut per_node = akantu_core::Array::new(2);
    m.dof_manager_mut()
        .assemble_mat_mul_vect_to_array("displacement", "M", &ones, &mut per_node, 1.0)
        .unwrap();
    let per_node: Vec<(u64, [Real; 2])> = {
        let mesh = m.physics.base.mesh.read().unwrap();
        per_node
            .as_slice()
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(n, v)| {
                let n = Idx::try_from(n).unwrap();
                // local or parallel master, periodic bits ignored: every
                // global node once over the ranks
                let shared = mesh.node_flag(n).bits() & NodeFlag::SHARED_MASK.bits();
                (shared == NodeFlag::NORMAL.bits() || shared == NodeFlag::MASTER.bits())
                    .then(|| (u64::try_from(mesh.node_global_id(n)).unwrap(), *v))
            })
            .collect()
    };
    // Independent node-only reduction: the rank-partial product of the
    // (rank-partial) matrix with the cache of `1` (every row of a
    // non-periodic mesh; periodic slaves alias their master's row, so the
    // cache of a master counts its local slaves), then slaves → owner (sum)
    // and owner → slaves through the node synchronizer.
    let eqs = node_equations(&m);
    let dm = m.dof_manager();
    let layout = dm.layout().clone();
    let mut cache = vec![0.0; layout.nb_local()];
    {
        let fam = dm.family("displacement").unwrap();
        let mesh = m.physics.base.mesh.read().unwrap();
        for (&n, &e) in fam.associated_nodes.iter().zip(&fam.local_equation_numbers) {
            if !mesh.is_periodic_slave(n).unwrap() {
                cache[usize::try_from(e).unwrap()] += 1.0;
            }
        }
    }
    let mut partial = vec![0.0; layout.nb_local()];
    dm.matrix("M")
        .unwrap()
        .mat_vec_mul(&layout, &NoReduction, &cache, &mut partial, 1.0, 1.0)
        .unwrap();
    let node_sync = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .synchronizers()
        .node
        .clone();
    if let Some(sync) = &node_sync {
        let mut acc = Rows {
            eqs: &eqs,
            v: &mut partial,
            add: true,
        };
        sync.slave_reduction(&mut acc, SynchronizationTag::SmmRes)
            .unwrap();
        acc.add = false;
        sync.synchronize(&mut acc, SynchronizationTag::SmmRes)
            .unwrap();
    }
    let bitwise_node_only = r
        .iter()
        .zip(&partial)
        .all(|(a, b)| a.to_bits() == b.to_bits());
    Product {
        total: reduce_sum(comm, owned_sums(&m, &r)),
        node_only_total: reduce_sum(comm, owned_sums(&m, &partial)),
        bitwise_node_only,
        per_node,
    }
}

fn rel(a: Real, b: Real) -> Real {
    ((a - b) / b).abs()
}

/// The total of `M · 1` on the distributed periodic square equals
/// the serial total at 2 / 4 / 8 ranks (the 8-rank invariant), while the
/// node-only reduction loses the pure-ghost periodic master copies' rows.
#[test]
fn periodic_consistent_mass_product_total_matches_serial_on_2_4_8_ranks() {
    let serial = product(None, true);
    eprintln!("serial: Σ(M·1) = {:?}", serial.total);
    for n in [2, 4, 8] {
        let out = run_local(n, TIMEOUT, |c| product(Some(&c), true));
        let p = &out[0];
        eprintln!(
            "{n} ranks: Σ(M·1) = {:?} (rel {:.3e}); node-only reduction {:?}",
            p.total,
            rel(p.total[0], serial.total[0]),
            p.node_only_total
        );
        for c in 0..2 {
            assert!(
                rel(p.total[c], serial.total[c]) < SUM_TOLERANCE,
                "{n} ranks, component {c}: Σ(M·1) {} vs serial {}",
                p.total[c],
                serial.total[c]
            );
        }
        let want: HashMap<u64, [Real; 2]> = serial.per_node.iter().copied().collect();
        let mut checked = 0;
        for (r, q) in out.iter().enumerate() {
            for (g, v) in &q.per_node {
                let w = want[g];
                for c in 0..2 {
                    assert!(
                        rel(v[c], w[c]) < SUM_TOLERANCE,
                        "{n} ranks, rank {r}, global node {g}, component {c}: (M·1) {} vs serial {}",
                        v[c],
                        w[c]
                    );
                }
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            want.len(),
            "{n} ranks: nodes checked vs serial nodes"
        );
        // the periodic channel is needed: the node synchronizer alone loses
        // rows (UD-872), at least at 2 ranks where rank 1 holds masters as
        // pure ghosts
        if n == 2 {
            assert!(
                rel(p.node_only_total[0], serial.total[0]) > 1e-3,
                "2 ranks: the node-only reduction no longer loses rows ({:?}): the regression \
                 lost its discriminating case",
                p.node_only_total
            );
        }
    }
}

/// Control: on the non-periodic square the product is the node-only
/// reduction bit for bit on every local row of every rank (2 / 4 ranks),
/// and its total is the serial one.
#[test]
fn non_periodic_product_is_the_node_only_reduction_bit_for_bit() {
    let serial = product(None, false);
    assert!(serial.bitwise_node_only);
    for n in [2, 4] {
        let out = run_local(n, TIMEOUT, |c| product(Some(&c), false));
        for (r, p) in out.iter().enumerate() {
            assert!(
                p.bitwise_node_only,
                "{n} ranks, rank {r}: rows differ from the node-only reduction"
            );
        }
        let p = &out[0];
        for c in 0..2 {
            assert!(
                rel(p.total[c], serial.total[c]) < SUM_TOLERANCE,
                "non-periodic, {n} ranks, component {c}: {} vs {}",
                p.total[c],
                serial.total[c]
            );
        }
    }
}

/// `(Σ_{local or master} u_y over the ranks, consistent-mass kinetic
/// energy)` after one implicit-dynamic step from rest (corner and `y-minus`
/// fixed, traction 1e9 on `y-plus`, as `TestPeriodicStatic` (2, 1)). The
/// kinetic energy `½ Σ v·(M v)` goes through `assemble_mat_mul_vect_to_array`
/// and reads `M v` at local periodic slaves whose master is a pure-ghost
/// copy: it needs the owner → copy step of the periodic channel.
fn implicit_step(comm: Option<&Communicator>, periodic: bool) -> (Real, Real) {
    let mesh = mesh_on(comm, periodic);
    let parser = Parser::parse_str(MATERIAL, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))
        .unwrap();
    m.set_time_step(1e-5, None).unwrap();
    for d in [SpatialDirection::X, SpatialDirection::Y] {
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: d,
            },
            "corner",
        )
        .unwrap();
    }
    m.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::Y,
        },
        "y-minus",
    )
    .unwrap();
    m.apply_neumann_bc(&FromTraction::new(vec![0.0, 1e9]), "y-plus")
        .unwrap();
    if let Err(e) = m.solve_step(None) {
        panic!(
            "periodic = {periodic}, rank {:?}: {e}",
            comm.map(Communicator::rank)
        );
    }
    let mesh_h = m.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let u = m.displacement().unwrap().as_slice().to_vec();
    let mut s = [0.0];
    for (n, un) in u.as_chunks::<2>().0.iter().enumerate() {
        if mesh
            .is_local_or_master_node(Idx::try_from(n).unwrap())
            .unwrap()
        {
            s[0] += un[1];
        }
    }
    drop(mesh);
    if let Some(c) = comm {
        c.all_reduce(&mut s, ReduceOp::Sum).unwrap();
    }
    (s[0], m.kinetic_energy().unwrap())
}

/// End to end: before the fix the step did not converge at 2 and 4
/// ranks (error 7.2e-8 after 10 iterations).
#[test]
fn periodic_implicit_dynamic_step_matches_serial_on_2_and_4_ranks() {
    let serial = implicit_step(None, true);
    for n in [2, 4] {
        let out = run_local(n, TIMEOUT, |c| implicit_step(Some(&c), true));
        let (u, e) = out[0];
        eprintln!(
            "periodic, {n} ranks: Σu_y = {u:.17e} (rel {:.3e}), E_kin = {e:.17e} (rel {:.3e})",
            rel(u, serial.0),
            rel(e, serial.1)
        );
        assert!(
            rel(u, serial.0) < STEP_TOLERANCE,
            "{n} ranks: Σu_y {u} vs serial {}",
            serial.0
        );
        assert!(
            rel(e, serial.1) < STEP_TOLERANCE,
            "{n} ranks: E_kin {e} vs serial {}",
            serial.1
        );
    }
}

/// Control: the same step on the non-periodic square.
#[test]
fn non_periodic_implicit_dynamic_step_matches_serial_on_2_and_4_ranks() {
    let serial = implicit_step(None, false);
    for n in [2, 4] {
        let out = run_local(n, TIMEOUT, |c| implicit_step(Some(&c), false));
        let (u, e) = out[0];
        assert!(
            rel(u, serial.0) < STEP_TOLERANCE && rel(e, serial.1) < STEP_TOLERANCE,
            "non-periodic, {n} ranks: Σu_y {u}, E_kin {e} vs serial {serial:?}"
        );
    }
}

/// The shared rows that the periodic channel carries exist: at 2 ranks a
/// rank turns pure-ghost periodic master copies into `Shared` equations
/// (otherwise the regression above would not discriminate).
#[test]
fn pure_ghost_periodic_master_copies_are_shared_equations_at_2_ranks() {
    let out = run_local(2, TIMEOUT, |c| {
        let mesh = mesh_on(Some(&c), true);
        let pure_ghost_masters: Vec<Idx> = (0..mesh.nb_nodes())
            .map(|n| Idx::try_from(n).unwrap())
            .filter(|&n| mesh.node_flag(n).is_pure_ghost() && mesh.is_periodic_master(n).unwrap())
            .collect();
        let parser = Parser::parse_str(MATERIAL, "material.dat").unwrap();
        let mut m =
            SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
        m.init_full(&ModelOptions::new(AnalysisMethod::Static))
            .unwrap();
        let eqs = node_equations(&m);
        let layout = m.dof_manager().layout();
        pure_ghost_masters
            .iter()
            .flat_map(|n| eqs.get(n).into_iter().flatten())
            .filter(|&&e| layout.ownership(e) == Some(EquationOwnership::Shared))
            .count()
    });
    assert!(
        out.iter().sum::<usize>() > 0,
        "no pure-ghost periodic master copy: {out:?}"
    );
}
