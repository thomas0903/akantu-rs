// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_solid_mechanics_model_fixture.hh (rank 0 reads, mesh->distribute()),
// test/test_model/test_common/test_dof_mesh_distribute.cc; akantu-rs
// scenarios (no direct upstream counterpart): distributed vs
// serial SolidMechanicsModel runs.

//! Solid mechanics on distributed meshes, shared by the local-world tests
//! (`smm_parallel.rs`) and the real-MPI binary (`mpi_smm.rs`).
//!
//! The mesh is a structured cube of `n³` hexahedra split into 6
//! tetrahedra each, built on rank 0 and distributed with a partitioner;
//! the same mesh is solved serially on rank 0 (serial communicator) and
//! the results are compared per **global node** on rank 0 (owned values
//! gathered with the node synchronizer).

#![allow(dead_code)]

use akantu_comm::{Communicator, Synchronizer};
use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_mesh::distribution::{
    MeshPartitioner, PrescribedPartition, RcbPartitioner, distribute_with,
};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::reduce::NanMinMax;

pub type Outcome = std::result::Result<(), String>;

pub fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

pub const DAT: &str = "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";

/// Two laws: law 0 `small` (small strain) and
/// law 1 `fd` (finite deformation: St Venant–Kirchhoff), different
/// densities and moduli so that a ghost on the wrong law changes the run;
/// elements chosen by [`CheckerboardSelector`].
pub const DAT_TWO_LAWS: &str = "material elastic [\n  name = small\n  rho = 1.5\n  E = 2\n  \
                                nu = 0.25\n]\n\nmaterial elastic [\n  name = fd\n  rho = 1\n  E = 1\n  \
                                nu = 0.3\n  finite_deformation = true\n]\n";

/// The laws of a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Laws {
    /// [`DAT`]: one small-strain elastic law.
    One,
    /// [`DAT_TWO_LAWS`] with the [`CheckerboardSelector`].
    TwoWithFiniteDeformation,
    /// A material file of its own, default selector (the laws
    /// with their own exchanges).
    Dat(&'static str),
}

impl Laws {
    pub fn dat(self) -> &'static str {
        match self {
            Laws::One => DAT,
            Laws::TwoWithFiniteDeformation => DAT_TWO_LAWS,
            Laws::Dat(dat) => dat,
        }
    }

    /// Model on `shared` with these laws (and their selector).
    pub fn model(self, shared: &SharedMesh) -> std::result::Result<SolidMechanicsModel, String> {
        let parser = ok(Parser::parse_str(self.dat(), "material.dat"), "parser")?;
        let mut model = ok(
            SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
            "model",
        )?;
        if self == Laws::TwoWithFiniteDeformation {
            model.set_material_selector(Box::new(CheckerboardSelector));
        }
        Ok(model)
    }
}

/// Law `(⌊2x̄⌋ + ⌊2ȳ⌋ + ⌊2z̄⌋) mod 2` of the element's barycentre (mean of
/// its nodes, upstream `Mesh::getBarycenter`): a 2 × 2 × 2 checkerboard of
/// the unit cube, so that both laws meet every partition interface. The
/// barycentre coordinates of the cube's Kuhn tetrahedra are `(4i + m) h / 4`
/// with `m ∈ {1, 2, 3}`, never on a checkerboard plane (`n` = 2, 3, 4).
/// Upstream runs the selector on local elements only; ghost elements
/// receive their owner's law (`_constitutive_law_id`).
#[derive(Clone, Copy, Debug)]
pub struct CheckerboardSelector;

impl akantu_model::constitutive_law::ConstitutiveLawSelector for CheckerboardSelector {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        element: akantu_core::Element,
    ) -> Result<Option<usize>> {
        let b = ctx.mesh.barycenter(element)?;
        #[allow(clippy::cast_possible_truncation)] // floor of a coordinate in [0, 2]
        let k: i64 = b.iter().map(|x| (2.0 * x).floor() as i64).sum();
        Ok(Some(usize::from(k.rem_euclid(2) == 1)))
    }
}

/// Structured cube `[0, 1]³` of `n³` hexahedra, each split into 6
/// tetrahedra (Kuhn triangulation): `(nodes, Tet4 connectivity)`.
pub fn cube_tet4(n: usize) -> (Vec<[Real; 3]>, Vec<[Idx; 4]>) {
    let h = 1.0 / n as Real;
    let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as Idx;
    let mut nodes = Vec::new();
    for k in 0..=n {
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([i as Real * h, j as Real * h, k as Real * h]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let v = [
                    id(i, j, k),
                    id(i + 1, j, k),
                    id(i + 1, j + 1, k),
                    id(i, j + 1, k),
                    id(i, j, k + 1),
                    id(i + 1, j, k + 1),
                    id(i + 1, j + 1, k + 1),
                    id(i, j + 1, k + 1),
                ];
                // Kuhn: 6 tets around the diagonal v0–v6
                for t in [
                    [0, 1, 2, 6],
                    [0, 2, 3, 6],
                    [0, 3, 7, 6],
                    [0, 7, 4, 6],
                    [0, 4, 5, 6],
                    [0, 5, 1, 6],
                ] {
                    tets.push([v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                }
            }
        }
    }
    (nodes, tets)
}

/// The cube mesh on `comm` (full on rank 0, empty elsewhere).
pub fn cube_mesh(comm: Communicator, n: usize, full: bool) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(3, "cube", comm), "mesh")?;
    if full {
        let (nodes, tets) = cube_tet4(n);
        let mut a = MeshAccessor::new(&mut m);
        for x in &nodes {
            ok(a.push_node(x), "node")?;
        }
        let mut c = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
        for t in &tets {
            c.push_tuple(t);
        }
    }
    ok(m.compute_bounding_box(), "bbox")?;
    Ok(m)
}

/// Partitioner choice of a scenario.
#[derive(Clone, Debug)]
pub enum Part {
    /// Pure-Rust bisection.
    Rcb,
    /// Linearised parts (fixture).
    Prescribed(Vec<Idx>),
}

/// Distributed cube (`comm.size()` parts).
pub fn distributed_cube(
    comm: &Communicator,
    n: usize,
    part: &Part,
) -> std::result::Result<Mesh, String> {
    let mut m = cube_mesh(comm.clone(), n, comm.rank() == 0)?;
    let r = match part {
        Part::Rcb => {
            let mut p = RcbPartitioner;
            distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0)
        }
        Part::Prescribed(parts) => {
            let mut p = PrescribedPartition::new(parts.clone());
            distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0)
        }
    };
    let _ = ok(r, "distribute")?;
    Ok(m)
}

/// Kind of run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// Explicit central differences, lumped mass.
    Explicit,
    /// Linear static, faer (centralised).
    Static,
    /// Implicit dynamics (trapezoidal rule), faer.
    ImplicitDynamic,
}

/// Result of a run: per global node (sorted by global id) displacement and
/// velocity, and the energies after the last step.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub u: Vec<Real>,
    pub v: Vec<Real>,
    pub kinetic: Real,
    pub potential: Real,
}

/// Set up and run the model on `mesh` (distributed or serial); returns the
/// gathered snapshot on rank 0 (`None` elsewhere).
pub fn run(mesh: Mesh, kind: Run, steps: usize) -> std::result::Result<Option<Snapshot>, String> {
    run_laws(mesh, kind, steps, Laws::One)
}

/// [`run`] with the material file `dat` (the laws with their own
/// exchanges).
pub fn run_with(
    mesh: Mesh,
    kind: Run,
    steps: usize,
    dat: &'static str,
) -> std::result::Result<Option<Snapshot>, String> {
    run_laws(mesh, kind, steps, Laws::Dat(dat))
}

/// [`run`] with the given laws.
pub fn run_laws(
    mesh: Mesh,
    kind: Run,
    steps: usize,
    laws: Laws,
) -> std::result::Result<Option<Snapshot>, String> {
    let comm = mesh.communicator().clone();
    let parallel = mesh.synchronizers().node.is_some() && comm.size() > 1;
    let shared = SharedMesh::new(mesh);
    let mut model = laws.model(&shared)?;
    let method = match kind {
        Run::Explicit => AnalysisMethod::ExplicitLumpedMass,
        Run::Static => AnalysisMethod::Static,
        Run::ImplicitDynamic => AnalysisMethod::ImplicitDynamic,
    };
    let mut options = ModelOptions::new(method);
    if kind != Run::Explicit {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    ok(model.init_full(&options), "init_full")?;
    let (coords, nb) = {
        let m = ok(shared.read(), "read")?;
        (m.nodes().as_slice().to_vec(), m.nb_nodes())
    };
    // boundary conditions: z = 0 face clamped; load / initial state
    {
        let mut b = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 0.0 {
                b[3 * n] = true;
                b[3 * n + 1] = true;
                b[3 * n + 2] = true;
            }
        }
    }
    match kind {
        Run::Explicit | Run::ImplicitDynamic => {
            // initial displacement: a smooth bending mode
            let mut u_g = ok(model.displacement_mut(), "u")?;
            let u = u_g.as_mut_slice();
            for n in 0..nb {
                let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
                u[3 * n] = 1e-3 * z * z * (1.0 + 0.3 * y);
                u[3 * n + 1] = 5e-4 * z * (0.5 - x);
            }
            drop(u_g);
            let dt = ok(model.stable_time_step(), "stable time step")?;
            let dt = if kind == Run::Explicit {
                0.8 * dt
            } else {
                4.0 * dt
            };
            ok(model.set_time_step(dt, None), "time step")?;
        }
        Run::Static => {
            let mut f = ok(model.external_force_mut(), "f_ext")?;
            let f = f.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 1.0 {
                    f[3 * n] = 1e-3 * (1.0 + coords[3 * n + 1]);
                    f[3 * n + 2] = -2e-3;
                }
            }
        }
    }
    let steps = if kind == Run::Static { 1 } else { steps };
    for s in 0..steps {
        ok(model.solve_step(None), &format!("solve step {s}"))?;
    }
    let kinetic = if kind == Run::Static {
        0.0
    } else {
        ok(model.energy("kinetic"), "kinetic")?
    };
    let potential = ok(model.energy("potential"), "potential")?;
    let u = ok(model.displacement(), "u")?.as_slice().to_vec();
    let v = if kind == Run::Static {
        vec![0.0; u.len()]
    } else {
        ok(model.velocity(), "v")?.as_slice().to_vec()
    };
    let m = ok(shared.read(), "read")?;
    let (gu, gv) = if parallel {
        let ns: &Synchronizer<Idx> = m.synchronizers().node.as_ref().ok_or("no node sync")?;
        let e = NodeEntities { mesh: &m };
        (
            ok(ns.gather(&e, &u, 3), "gather u")?,
            ok(ns.gather(&e, &v, 3), "gather v")?,
        )
    } else if comm.rank() == 0 {
        // serial or one rank: global ids are the local ones (1 rank:
        // identity too, upstream does not renumber)
        let mut gu = vec![0.0; u.len()];
        let mut gv = vec![0.0; v.len()];
        for n in 0..m.nb_nodes() {
            let g = usize::try_from(m.node_global_id(n as Idx)).unwrap_or(0);
            gu[3 * g..3 * g + 3].copy_from_slice(&u[3 * n..3 * n + 3]);
            gv[3 * g..3 * g + 3].copy_from_slice(&v[3 * n..3 * n + 3]);
        }
        (Some(gu), Some(gv))
    } else {
        (None, None)
    };
    Ok(match (gu, gv) {
        (Some(u), Some(v)) => Some(Snapshot {
            u,
            v,
            kinetic,
            potential,
        }),
        _ => None,
    })
}

/// Largest absolute difference and largest magnitude of two fields.
pub fn diff(a: &[Real], b: &[Real]) -> (Real, Real, usize) {
    let mut d = 0.0;
    let mut mag = 0.0;
    let mut bitwise_equal = 0;
    for (x, y) in a.iter().zip(b) {
        d = NanMinMax::nan_max(d, (x - y).abs());
        mag = NanMinMax::nan_max(mag, x.abs());
        if x.to_bits() == y.to_bits() {
            bitwise_equal += 1;
        }
    }
    (d, mag, bitwise_equal)
}

/// Distributed vs serial run of `kind` on the cube: per global node, the
/// displacement and velocity differ by at most `rel_tol` relative to the
/// largest value (summation-order differences at interface nodes,
/// NUMERICAL_VALIDATION); prints the
/// measured differences.
pub fn distributed_vs_serial(
    comm: Communicator,
    kind: Run,
    n: usize,
    steps: usize,
    part: Part,
    rel_tol: Real,
) -> Outcome {
    distributed_vs_serial_laws(comm, kind, n, steps, part, rel_tol, Laws::One)
}

/// [`distributed_vs_serial`] with the given laws.
pub fn distributed_vs_serial_laws(
    comm: Communicator,
    kind: Run,
    n: usize,
    steps: usize,
    part: Part,
    rel_tol: Real,
    laws: Laws,
) -> Outcome {
    let rank = comm.rank();
    let size = comm.size();
    let m = distributed_cube(&comm, n, &part)?;
    let par = run_laws(m, kind, steps, laws)?;
    if rank != 0 {
        return Ok(());
    }
    let par = par.ok_or("no gathered result")?;
    let serial_mesh = cube_mesh(Communicator::serial(), n, true)?;
    let ser = run_laws(serial_mesh, kind, steps, laws)?.ok_or("no serial result")?;
    let (du, mu, eu) = diff(&par.u, &ser.u);
    let (dv, mv, ev) = diff(&par.v, &ser.v);
    println!(
        "INFO {kind:?} {laws:?} n={n} ranks={size}: |du|max={du:.3e} (|u|max={mu:.3e}, {eu}/{} bitwise), \
         |dv|max={dv:.3e} (|v|max={mv:.3e}, {ev}/{} bitwise), Ek {:.17e} vs {:.17e}, Ep {:.17e} vs {:.17e}",
        par.u.len(),
        par.v.len(),
        par.kinetic,
        ser.kinetic,
        par.potential,
        ser.potential
    );
    if du > rel_tol * mu.max(Real::MIN_POSITIVE) || dv > rel_tol * mv.max(Real::MIN_POSITIVE) {
        return Err(format!(
            "{kind:?} at {size} ranks differs from serial: du {du:e} (|u| {mu:e}), dv {dv:e} (|v| {mv:e})"
        ));
    }
    let de = (par.potential - ser.potential).abs();
    if de > rel_tol * ser.potential.abs().max(Real::MIN_POSITIVE) {
        return Err(format!(
            "potential energy {} vs {}",
            par.potential, ser.potential
        ));
    }
    // One rank: the distributed mesh equals the serial one (no ghost,
    // same numbering), so every run is bitwise identical, implicit dynamics
    // included: the distributed product `M·a` accumulates in place in
    // upstream's order on rows the reduction does not touch
    // (it used to be exempt here).
    if size == 1 && (du != 0.0 || dv != 0.0) {
        return Err("one rank must equal the serial run bitwise".into());
    }
    Ok(())
}

/// Upstream `TestMesh.TestMeshDistribute` (`test_dof_mesh_distribute.cc`):
/// distributing a mesh that already has a consumer (a DOF manager / model)
/// fails; distributing first, then creating the model, succeeds.
pub fn dof_mesh_distribute(comm: Communicator) -> Outcome {
    let m1 = cube_mesh(comm.clone(), 2, comm.rank() == 0)?;
    let s1 = SharedMesh::new(m1);
    let _model = ok(
        SolidMechanicsModel::with_parser(
            s1.clone(),
            None,
            "smm1",
            ok(Parser::parse_str(DAT, "m.dat"), "parser")?,
        ),
        "model 1",
    )?;
    // a set-up operation (distribution) is refused while consumers exist
    let r = s1.setup(|m| {
        let mut p = RcbPartitioner;
        distribute_with(m, Some(&mut p as &mut dyn MeshPartitioner), 0)
    });
    if r.is_ok() {
        return Err("distributing a mesh with a registered model must fail".into());
    }
    let m2 = cube_mesh(comm.clone(), 2, comm.rank() == 0)?;
    let s2 = SharedMesh::new(m2);
    ok(
        s2.setup(|m| {
            let mut p = RcbPartitioner;
            distribute_with(m, Some(&mut p as &mut dyn MeshPartitioner), 0)
        }),
        "distribute 2",
    )?;
    ok(
        SolidMechanicsModel::with_parser(
            s2,
            None,
            "smm2",
            ok(Parser::parse_str(DAT, "m.dat"), "parser")?,
        ),
        "model 2",
    )?;
    Ok(())
}

/// Bit patterns of the model's nodal arrays (u, v, a, f_int, f_ext, lumped
/// mass, blocked flags as 0/1).
fn nodal_state(model: &SolidMechanicsModel) -> std::result::Result<Vec<Vec<u64>>, String> {
    let bits = |a: &[Real]| a.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
    Ok(vec![
        bits(ok(model.displacement(), "u")?.as_slice()),
        bits(ok(model.velocity(), "v")?.as_slice()),
        bits(ok(model.acceleration(), "a")?.as_slice()),
        bits(ok(model.internal_force(), "f_int")?.as_slice()),
        bits(ok(model.external_force(), "f_ext")?.as_slice()),
        bits(ok(model.mass(), "mass")?.as_slice()),
        ok(model.blocked_dofs(), "blocked")?
            .as_slice()
            .iter()
            .map(|&b| u64::from(b))
            .collect(),
    ])
}

const NODAL_NAMES: [&str; 7] = ["u", "v", "a", "f_int", "f_ext", "mass", "blocked"];

/// `tag`'s element exchange leaves every nodal array bit-identical on every
/// rank (UD-233: element-tag nodal data is packed but never unpacked; for
/// `_smm_for_gradu` nothing is exchanged at all, as upstream).
fn element_tag_leaves_nodal_state(
    model: &mut SolidMechanicsModel,
    tag: akantu_core::enums::SynchronizationTag,
) -> Outcome {
    let before = nodal_state(model)?;
    ok(model.synchronize(tag), &format!("synchronize {tag}"))?;
    let after = nodal_state(model)?;
    for ((b, a), name) in before.iter().zip(&after).zip(NODAL_NAMES) {
        if let Some(i) = (0..b.len()).find(|&i| b[i] != a[i]) {
            return Err(format!("synchronize {tag} changed {name}[{i}]"));
        }
    }
    Ok(())
}

/// The owner's (master or local node's) copy of every node, gathered from
/// every rank, as bit patterns per global id.
fn owner_bits(
    comm: &Communicator,
    flags: &[akantu_core::NodeFlag],
    gids: &[i64],
    values: &[Real],
    dim: usize,
) -> std::result::Result<std::collections::HashMap<i64, Vec<u64>>, String> {
    let mut mine = Vec::new();
    for (n, f) in flags.iter().enumerate() {
        if f.is_slave() || f.is_pure_ghost() {
            continue;
        }
        mine.push(gids[n]);
        for k in 0..dim {
            mine.push(values[dim * n + k].to_bits() as i64);
        }
    }
    let all = ok(comm.all_gather_v(&mine), "all_gather_v")?;
    let mut out = std::collections::HashMap::new();
    for rank in all {
        for chunk in rank.chunks_exact(dim + 1) {
            let bits = chunk[1..].iter().map(|&b| b as u64).collect();
            if out.insert(chunk[0], bits).is_some() {
                return Err(format!("global node {} has two owners", chunk[0]));
            }
        }
    }
    Ok(out)
}

/// Every slave copy of `values` equals its master's bit for bit (collective).
fn slaves_equal_masters(
    comm: &Communicator,
    flags: &[akantu_core::NodeFlag],
    gids: &[i64],
    values: &[Real],
    dim: usize,
    what: &str,
) -> Outcome {
    let owners = owner_bits(comm, flags, gids, values, dim)?;
    for (n, f) in flags.iter().enumerate() {
        if !f.is_slave() {
            continue;
        }
        let want = owners
            .get(&gids[n])
            .ok_or_else(|| format!("{what}: slave node {n} has no master"))?;
        let have: Vec<u64> = (0..dim).map(|k| values[dim * n + k].to_bits()).collect();
        if &have != want {
            return Err(format!(
                "{what}: slave node {n} (global {}) = {have:x?}, master {want:x?}",
                gids[n]
            ));
        }
    }
    Ok(())
}

/// The model's registered tags (MPI-026…036). Element tags: every nodal
/// array is unchanged on every rank (UD-233, `_smm_for_gradu` a no-op as
/// upstream: MPI-031), and the exchange itself takes place (observed with
/// a live collective scope that refuses it);
/// `_smm_gradu` / `_smm_stress` give every ghost element its owner's
/// `grad_u` / `stress` (MPI-035, MPI-034 small strain); ghost elements
/// carry their owner's law (`_constitutive_law_id`). DOF tags (node
/// synchronizer): slave copies of the exchanged arrays — garbage beforehand
/// — equal their masters' bit for bit afterwards and the other arrays are
/// untouched (`_smm_uv`, `_smm_res`, `_for_dump`, `_smm_mass`;
/// MPI-026…029). Local checks are agreed on every rank before the next
/// collective, so a failing rank does not leave its peers waiting for the
/// collective timeout.
pub fn model_synchronize_tags(comm: Communicator) -> Outcome {
    use akantu_core::enums::SynchronizationTag as T;
    let m = distributed_cube(&comm, 3, &Part::Rcb)?;
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init",
    )?;
    ok(model.assemble_internal_forces(), "f_int")?;
    ok(model.assemble_mass_lumped(), "mass")?;
    let (flags, gids): (Vec<akantu_core::NodeFlag>, Vec<i64>) = {
        let mesh = ok(shared.read(), "read")?;
        (
            mesh.nodes_flags().as_slice().to_vec(),
            (0..mesh.nb_nodes())
                .map(|n| mesh.node_global_id(n as Idx))
                .collect(),
        )
    };
    // distinct values everywhere, so an exchange that wrote anything would
    // be seen
    {
        let fill = |a: &mut [Real], off: Real| {
            for (i, x) in a.iter_mut().enumerate() {
                *x = off + gids[i / 3] as Real + 0.125 * (i % 3) as Real;
            }
        };
        fill(ok(model.displacement_mut(), "u")?.as_mut_slice(), 1000.0);
        fill(ok(model.velocity_mut(), "v")?.as_mut_slice(), 2000.0);
        fill(ok(model.acceleration_mut(), "a")?.as_mut_slice(), 3000.0);
        fill(
            ok(model.external_force_mut(), "f_ext")?.as_mut_slice(),
            4000.0,
        );
    }
    for tag in [
        T::SmmStress,
        T::SmmDensity,
        T::SmmMass,
        T::ForDump,
        T::SmmBoundary,
        T::SmmForGradu,
        T::ConstitutiveLawId,
    ] {
        let r = element_tag_leaves_nodal_state(&mut model, tag);
        agree(&comm, r)?;
    }
    if model.synchronize(T::Test).is_ok() {
        return Err("an unregistered tag must be refused".into());
    }
    // UD-553 B: the compat switch reproduces upstream's silent no-op (an
    // empty range of registered synchronizers): no error, no exchange, no
    // state change; the default refuses again once it is cleared
    model
        .physics
        .upstream_compat
        .unregistered_synchronization_tag_is_a_noop = true;
    for tag in [T::Test, T::ClhInitCl, T::SmmUv, T::SmmRes] {
        let before = nodal_state(&model)?;
        let r = model
            .synchronize(tag)
            .map_err(|e| format!("UD-553 compat: {tag}: {e}"));
        let r = r.and_then(|()| {
            if nodal_state(&model)? == before {
                Ok(())
            } else {
                Err(format!("UD-553 compat: {tag} changed the nodal state"))
            }
        });
        agree(&comm, r)?;
    }
    model
        .physics
        .upstream_compat
        .unregistered_synchronization_tag_is_a_noop = false;
    if model.synchronize(T::Test).is_ok() {
        return Err("an unregistered tag must be refused by default".into());
    }
    let r = element_tags_exchange(&comm, &mut model);
    agree(&comm, r)?;
    // _smm_gradu / _smm_stress: every ghost element's grad_u / stress =
    // its owner's (MPI-035; MPI-034, small strain: the stress only)
    ghost_field_takes_owner_values(&comm, &shared, &mut model, T::SmmGradu, "grad_u")?;
    ghost_field_takes_owner_values(&comm, &shared, &mut model, T::SmmStress, "stress")?;
    // ghost elements have a law
    let r = (|| -> Outcome {
        let mesh = ok(shared.read(), "read")?;
        for e in mesh.elements(
            akantu_core::TypeFilter::new()
                .dim(3)
                .ghost(GhostType::Ghost),
        ) {
            ok(
                model.physics.materials.law_of(e),
                &format!("law of ghost {e}"),
            )?;
        }
        Ok(())
    })();
    agree(&comm, r)?;
    // the DOF tags below need slave nodes somewhere
    let mut nb_slaves = [flags.iter().filter(|f| f.is_slave()).count() as u64];
    ok(
        comm.all_reduce(&mut nb_slaves, akantu_comm::ReduceOp::Sum),
        "slaves",
    )?;
    if comm.size() > 1 && nb_slaves[0] == 0 {
        return Err("no slave node on any rank".into());
    }
    // DOF tags: garbage on slaves, restored from masters, other arrays kept
    let garbage = |a: &mut [Real]| {
        for (n, f) in flags.iter().enumerate() {
            if f.is_slave() {
                a[3 * n..3 * n + 3].fill(-1.0);
            }
        }
    };
    type Getter = fn(&SolidMechanicsModel) -> Result<akantu_core::array_shared::ArrayRef<'_, Real>>;
    let getters: [(&str, Getter); 5] = [
        ("u", SolidMechanicsModel::displacement),
        ("v", SolidMechanicsModel::velocity),
        ("a", SolidMechanicsModel::acceleration),
        ("f_int", SolidMechanicsModel::internal_force),
        ("f_ext", SolidMechanicsModel::external_force),
    ];
    for (tag, exchanged) in [
        (T::SmmUv, &["u", "v"][..]),
        (T::SmmRes, &["f_int"][..]),
        (T::ForDump, &["u", "v", "a", "f_int", "f_ext"][..]),
    ] {
        for &name in exchanged {
            let mut a = match name {
                "u" => ok(model.displacement_mut(), "u")?,
                "v" => ok(model.velocity_mut(), "v")?,
                "a" => ok(model.acceleration_mut(), "a")?,
                "f_int" => ok(model.internal_force_mut(), "f_int")?,
                _ => ok(model.external_force_mut(), "f_ext")?,
            };
            garbage(a.as_mut_slice());
        }
        let before = nodal_state(&model)?;
        ok(model.synchronize_nodal(tag), &format!("nodal {tag}"))?;
        let after = nodal_state(&model)?;
        let r = (|| -> Outcome {
            for (k, name) in NODAL_NAMES.iter().enumerate() {
                if !exchanged.contains(name) && before[k] != after[k] {
                    return Err(format!("nodal {tag} changed {name}"));
                }
            }
            for (name, get) in getters {
                if exchanged.contains(&name)
                    && ok(get(&model), name)?.as_slice().iter().any(|&x| x == -1.0)
                {
                    return Err(format!("nodal {tag}: {name} keeps a garbage slave value"));
                }
            }
            Ok(())
        })();
        agree(&comm, r)?;
        for (name, get) in getters {
            if exchanged.contains(&name) {
                let a_g = ok(get(&model), name)?;
                let a = a_g.as_slice();
                slaves_equal_masters(&comm, &flags, &gids, a, 3, &format!("nodal {tag} {name}"))?;
            }
        }
    }
    // _smm_mass (MPI-028): garbage in the slave masses
    // (written through `mass_mut`, upstream's writeable Python view),
    // restored from the masters by the exchange; nothing else changes
    garbage(ok(model.mass_mut(), "mass")?.as_mut_slice());
    let before = nodal_state(&model)?;
    ok(model.synchronize_nodal(T::SmmMass), "nodal smm_mass")?;
    let after = nodal_state(&model)?;
    let mass = ok(model.mass(), "mass")?.as_slice().to_vec();
    let r = (|| -> Outcome {
        for (k, name) in NODAL_NAMES.iter().enumerate() {
            if *name != "mass" && before[k] != after[k] {
                return Err(format!("nodal smm_mass changed {name}"));
            }
        }
        if mass.iter().any(|&x| x == -1.0) {
            return Err("nodal smm_mass: a slave keeps its garbage mass".into());
        }
        if !mass.iter().all(|&x| x > 0.0) {
            return Err("nodal smm_mass: non-positive lumped mass".into());
        }
        Ok(())
    })();
    agree(&comm, r)?;
    slaves_equal_masters(&comm, &flags, &gids, &mass, 3, "nodal smm_mass")?;
    Ok(())
}

/// Agree on a local outcome before the next collective:
/// every rank returns an error if any rank failed (its own
/// message, or "failed on another rank"), instead of the peers of a
/// failing rank blocking in their next collective until the timeout.
fn agree(comm: &Communicator, local: Outcome) -> Outcome {
    let mut failed = [u64::from(local.is_err())];
    ok(
        comm.all_reduce(&mut failed, akantu_comm::ReduceOp::Max),
        "agree",
    )?;
    match local {
        Err(e) => Err(e),
        Ok(()) if failed[0] != 0 => Err("failed on another rank".into()),
        Ok(()) => Ok(()),
    }
}

/// The element tags exchange (MPI-030, -032, -033, -034, -035,
/// `_smm_density`, `_constitutive_law_id`) even where upstream's exchange
/// has no observable effect on the state (element-nodal tags: packed,
/// never unpacked, UD-233/238), and `_smm_for_gradu` does not (MPI-031).
/// Observed through the communicator: while a collective scope is live on
/// it, every further scope is refused locally without communicating
/// (`akantu_comm::phase`), so a call that exchanges fails with a refusal on
/// every rank and a call that does not communicate succeeds. Removing an
/// element exchange from `SolidMechanicsModel::synchronize` fails this
/// check. Nothing to observe at one rank (no element synchronizer
/// neighbours).
fn element_tags_exchange(comm: &Communicator, model: &mut SolidMechanicsModel) -> Outcome {
    use akantu_core::enums::SynchronizationTag as T;
    if comm.size() == 1 {
        return Ok(());
    }
    let before = nodal_state(model)?;
    let probe = akantu_comm::phase::Phases::new(comm, "test:element-exchange-probe");
    let mut errs = Vec::new();
    for tag in [
        T::SmmMass,
        T::ForDump,
        T::SmmBoundary,
        T::SmmStress,
        T::SmmGradu,
        T::SmmDensity,
        T::ConstitutiveLawId,
    ] {
        match model.synchronize(tag) {
            Ok(()) => errs.push(format!("element {tag}: no exchange took place")),
            Err(e) if e.to_string().contains("refused") => {}
            Err(e) => errs.push(format!("element {tag}: unexpected error: {e}")),
        }
    }
    if let Err(e) = model.synchronize(T::SmmForGradu) {
        errs.push(format!("element {}: communicates: {e}", T::SmmForGradu));
    }
    drop(probe);
    if nodal_state(model)? != before {
        errs.push("a refused exchange changed the nodal state".into());
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

/// `tag` gives every ghost element's internal `field` (`grad_u` or
/// `stress`, `dim × dim` per point) its owner's values: owners hold a
/// function of the element's barycentre (computed identically on every
/// rank from the copied coordinates), ghosts `−1` beforehand
/// (`Material::packData/unpackData`, `material_inline_impl.hh:186-218`).
fn ghost_field_takes_owner_values(
    comm: &Communicator,
    shared: &SharedMesh,
    model: &mut SolidMechanicsModel,
    tag: akantu_core::enums::SynchronizationTag,
    field: &str,
) -> Outcome {
    // barycentre sums of every element (both ghost types), per type
    let bary: std::collections::HashMap<(ElementType, GhostType), Vec<[Real; 3]>> = {
        let mesh = ok(shared.read(), "read")?;
        let x_g = mesh.nodes();
        let x = x_g.as_slice();
        let mut out = std::collections::HashMap::new();
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            for ty in mesh.element_types(akantu_core::TypeFilter::new().dim(3).ghost(ghost)) {
                let conn = ok(mesh.connectivity(ty, ghost), "connectivity")?;
                let nn = ty.nb_nodes_per_element();
                let v: Vec<[Real; 3]> = conn
                    .as_slice()
                    .chunks_exact(nn)
                    .map(|el| {
                        let mut b = [0.0; 3];
                        for &n in el {
                            for (d, bd) in b.iter_mut().enumerate() {
                                *bd += x[3 * n as usize + d];
                            }
                        }
                        b
                    })
                    .collect();
                out.insert((ty, ghost), v);
            }
        }
        out
    };
    let value = |ty: ElementType, ghost: GhostType, e: usize, q: usize, c: usize| {
        let b = bary[&(ty, ghost)][e];
        b[0] + 7.0 * b[1] + 49.0 * b[2] + 0.5 * q as Real + 0.0625 * c as Real
    };
    for li in 0..model.physics.materials.nb_laws() {
        let law = ok(model.physics.materials.law_mut(li), "law")?;
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            for ty in law.base().filter_types(ghost) {
                let filter = ok(law.base().element_filter.get(ty, ghost), "filter")?
                    .as_slice()
                    .to_vec();
                let base = law.base_mut();
                let g = match field {
                    "grad_u" => &mut base.grad_u,
                    _ => &mut base.stress,
                };
                let mut g = ok(g.get_mut(ty, ghost), field)?;
                let nq = g.len() / filter.len().max(1);
                let s = g.as_mut_slice();
                for (i, &e) in filter.iter().enumerate() {
                    for q in 0..nq {
                        for c in 0..9 {
                            s[(i * nq + q) * 9 + c] = if ghost == GhostType::Ghost {
                                -1.0
                            } else {
                                value(ty, ghost, e as usize, q, c)
                            };
                        }
                    }
                }
            }
        }
    }
    let r = element_tag_leaves_nodal_state(model, tag);
    agree(comm, r)?;
    let r = (|| -> Outcome {
        let mut checked = 0usize;
        for li in 0..model.physics.materials.nb_laws() {
            let law = ok(model.physics.materials.law(li), "law")?;
            for ty in law.base().filter_types(GhostType::Ghost) {
                let filter = ok(
                    law.base().element_filter.get(ty, GhostType::Ghost),
                    "filter",
                )?;
                let g = match field {
                    "grad_u" => &law.base().grad_u,
                    _ => &law.base().stress,
                };
                let g = ok(g.get(ty, GhostType::Ghost), field)?;
                let nq = g.len() / filter.len().max(1);
                for (i, &e) in filter.as_slice().iter().enumerate() {
                    for q in 0..nq {
                        for c in 0..9 {
                            let have = g.as_slice()[(i * nq + q) * 9 + c];
                            let want = value(ty, GhostType::Ghost, e as usize, q, c);
                            if have.to_bits() != want.to_bits() {
                                return Err(format!(
                                    "{tag}: ghost {ty:?} {e} point {q} {field}[{c}] = {have} \
                                     != owner's {want}"
                                ));
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
        if comm.size() > 1 && checked == 0 {
            return Err(format!("{tag}: no ghost {field} checked"));
        }
        Ok(())
    })();
    agree(comm, r)
}

/// Selector failing on the last rank only.
#[derive(Debug)]
pub struct FailOnRank(pub i32);

impl akantu_model::constitutive_law::ConstitutiveLawSelector for FailOnRank {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        _element: akantu_core::Element,
    ) -> Result<Option<usize>> {
        if ctx.mesh.communicator().rank() == self.0 {
            Err(akantu_core::Error::invalid(format!(
                "selector fails on rank {} only",
                self.0
            )))
        } else {
            Ok(Some(0))
        }
    }
}

/// Check the outcome of a collective failure that the last rank caused:
/// every rank failed, the last one with its own error, the others with
/// the agreement's "failed on another rank"; `fatal` = agreed fatal.
fn check_agreed_failure(
    comm: &Communicator,
    r: Result<()>,
    own: &str,
    fatal: bool,
    elapsed: std::time::Duration,
) -> Outcome {
    let last = comm.size() as i32 - 1;
    let e = match r {
        Ok(()) => return Err("the initialisation must fail on every rank".into()),
        Err(e) => e,
    };
    let msg = e.to_string();
    if matches!(e, akantu_core::Error::Fatal { .. }) != fatal {
        return Err(format!("fatal = {fatal} expected, got: {msg}"));
    }
    if let akantu_core::Error::Fatal { kind, .. } = &e
        && *kind != akantu_core::FatalKind::Agreed
    {
        return Err(format!("an agreed fatal error expected, got: {msg}"));
    }
    let want = if comm.rank() == last {
        own
    } else {
        "failed on another rank"
    };
    if !msg.contains(want) {
        return Err(format!("rank {}: '{want}' expected in: {msg}", comm.rank()));
    }
    // promptly: far below the transport timeout of the harness
    if elapsed > std::time::Duration::from_secs(5) {
        return Err(format!("returned after {elapsed:?}"));
    }
    // the communicator is still usable
    let mut x = [1u64];
    ok(
        comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
        "all_reduce after the failure",
    )?;
    if x[0] != comm.size() as u64 {
        return Err(format!("all_reduce after the failure: {}", x[0]));
    }
    Ok(())
}

/// A selector failing on one rank only during
/// `init_full` returns promptly on every rank with the agreed recoverable
/// error (no rank waits in the `_constitutive_law_id` exchange); the
/// model can then be initialised with a working selector.
pub fn pa01_rank_specific_selector_failure_is_agreed(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm, 2, &Part::Rcb)?;
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )?;
    model.set_material_selector(Box::new(FailOnRank(comm.size() as i32 - 1)));
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    let own = format!("selector fails on rank {} only", comm.size() - 1);
    check_agreed_failure(&comm, r, &own, false, t0.elapsed())?;
    // recoverable: no law holds an element yet; a working selector
    // initialises the model (every local and ghost element once)
    let held = |model: &SolidMechanicsModel, g: GhostType| -> usize {
        model
            .physics
            .materials
            .laws()
            .iter()
            .map(|l| {
                l.base()
                    .element_filter
                    .get(ElementType::Tetrahedron4, g)
                    .map_or(0, |f| f.len())
            })
            .sum()
    };
    if GhostType::ALL.into_iter().any(|g| held(&model, g) > 0) {
        return Err("a failed selection must leave the law filters empty".into());
    }
    model.set_material_selector(Box::new(akantu_model::constitutive_law::DefaultSelector(0)));
    ok(model.init_materials(), "init_materials after the failure")?;
    let mesh = ok(shared.read(), "read")?;
    for g in GhostType::ALL {
        let n = mesh.nb_element(ElementType::Tetrahedron4, g);
        let h = held(&model, g);
        if h != n {
            return Err(format!("{g:?}: laws hold {h} elements, mesh has {n}"));
        }
    }
    Ok(())
}

/// A law whose
/// `initMaterial` fails on one rank only (an orthotropic law with finite
/// deformation registered on the last rank) gives the agreed fatal error
/// on every rank instead of leaving the peers in later collectives.
pub fn pa01_rank_specific_init_material_failure_is_agreed(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm, 2, &Part::Rcb)?;
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared, None, "smm", parser),
        "model",
    )?;
    if comm.rank() == comm.size() as i32 - 1 {
        let i = ok(
            model.register_new_material("bad", "elastic_orthotropic", ""),
            "register",
        )?;
        ok(model.physics.materials.law_mut(i), "law")?
            .base_mut()
            .finite_deformation = true;
    }
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    check_agreed_failure(
        &comm,
        r,
        "finite deformation not possible in material orthotropic",
        true,
        t0.elapsed(),
    )
}

/// A failure inside the `_constitutive_law_id`
/// exchange (after the selection was committed) is agreed **fatal** on
/// every rank — the laws already hold their local elements, and a retry
/// would add them a second time. Scenario: the last rank
/// registers a second elastic law and selects it, so the ranks holding
/// ghosts of the last rank receive the out-of-range law index 1. The
/// communicator stays usable.
pub fn pa2_01_law_id_exchange_failure_is_agreed_fatal(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm, 2, &Part::Rcb)?;
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared, None, "smm", parser),
        "model",
    )?;
    let last = comm.size() as i32 - 1;
    if comm.rank() == last {
        ok(
            model.register_new_material("steel2", "elastic", ""),
            "register",
        )?;
        model.set_material_selector(Box::new(akantu_model::constitutive_law::DefaultSelector(1)));
    }
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    let elapsed = t0.elapsed();
    let e = match r {
        Ok(()) => return Err("the initialisation must fail on every rank".into()),
        Err(e) => e,
    };
    let msg = e.to_string();
    if e.fatal_kind() != Some(akantu_core::FatalKind::Agreed) {
        return Err(format!("an agreed fatal error expected, got: {msg}"));
    }
    let own = msg.contains("out of range");
    if !own && !msg.contains("failed on another rank") {
        return Err(format!("rank {}: unexpected error: {msg}", comm.rank()));
    }
    // the last rank's ghosts carry law 0 (valid on every rank)
    if comm.rank() == last && own {
        return Err(format!("the last rank must not fail itself: {msg}"));
    }
    if elapsed > std::time::Duration::from_secs(5) {
        return Err(format!("returned after {elapsed:?}"));
    }
    // the communicator is still usable; at least one rank failed itself
    let mut x = [u64::from(own)];
    ok(
        comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
        "all_reduce after the failure",
    )?;
    if x[0] == 0 {
        return Err("no rank reported the out-of-range law index".into());
    }
    Ok(())
}

/// A law-instantiation failure on one rank only (an
/// unknown parameter in the last rank's input) is agreed **fatal** on
/// every rank: the failing rank keeps a registered, partially parsed law,
/// so the model cannot be initialised again. Promptly, communicator
/// usable.
pub fn pa2_02_instantiation_failure_is_agreed_fatal(comm: Communicator) -> Outcome {
    let m = distributed_cube(&comm, 2, &Part::Rcb)?;
    let last = comm.size() as i32 - 1;
    let dat = if comm.rank() == last {
        format!(
            "{DAT}material elastic [\n  name = steel2\n  rho = 1\n  E = 1\n  bogus_param = 3\n  nu = 0.3\n]\n"
        )
    } else {
        DAT.to_string()
    };
    let parser = ok(Parser::parse_str(&dat, "material.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared, None, "smm", parser),
        "model",
    )?;
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    check_agreed_failure(&comm, r, "bogus_param", true, t0.elapsed())
}

/* ---------------------------------------------------------------------- */
/* CL-008 and MPI-034                                                      */
/* ---------------------------------------------------------------------- */

/// Key of an element shared by its owner and its ghost copies: the bits
/// of its barycentre (coordinates copied bit for bit by the distribution,
/// summed in the same connectivity order on every rank).
type ElementKey = [u64; 3];

fn element_key(mesh: &Mesh, e: akantu_core::Element) -> std::result::Result<ElementKey, String> {
    let b = ok(mesh.barycenter(e), "barycenter")?;
    Ok([b[0].to_bits(), b[1].to_bits(), b[2].to_bits()])
}

/// Every owner's `(key, values)` gathered on every rank (collective).
fn gather_owned(
    comm: &Communicator,
    mine: &[(ElementKey, Vec<u64>)],
    width: usize,
) -> std::result::Result<std::collections::HashMap<ElementKey, Vec<u64>>, String> {
    let mut flat: Vec<i64> = Vec::with_capacity(mine.len() * (3 + width));
    for (k, v) in mine {
        if v.len() != width {
            return Err(format!(
                "gather_owned: {} values, {width} expected",
                v.len()
            ));
        }
        flat.extend(k.iter().map(|&b| b as i64));
        flat.extend(v.iter().map(|&b| b as i64));
    }
    let all = ok(comm.all_gather_v(&flat), "all_gather_v")?;
    let mut out = std::collections::HashMap::new();
    for rank in all {
        for c in rank.chunks_exact(3 + width) {
            let key = [c[0] as u64, c[1] as u64, c[2] as u64];
            let vals = c[3..].iter().map(|&b| b as u64).collect();
            if out.insert(key, vals).is_some() {
                return Err(format!("element {key:x?} has two owners"));
            }
        }
    }
    Ok(out)
}

/// The internal `name` of law `li` per element of `ghost`, as
/// `(key, bit patterns of its nq · 9 values)`.
fn law_internal_by_element(
    model: &SolidMechanicsModel,
    mesh: &Mesh,
    li: usize,
    name: &str,
    ghost: GhostType,
) -> std::result::Result<Vec<(ElementKey, Vec<u64>)>, String> {
    let law = ok(model.physics.materials.law(li), "law")?;
    let b = law.base();
    let field = match name {
        "stress" => &b.stress,
        "grad_u" => &b.grad_u,
        _ => b
            .piola_kirchhoff_2
            .as_ref()
            .ok_or("piola_kirchhoff_2 is not registered")?,
    };
    let mut out = Vec::new();
    for ty in b.filter_types(ghost) {
        let filter = ok(b.element_filter.get(ty, ghost), "filter")?;
        let a = ok(field.get(ty, ghost), name)?;
        let per = a.len() * 9 / filter.len().max(1);
        let vals = a.as_slice();
        if vals.len() != per * filter.len() {
            return Err(format!("law {li} {name} {ty:?}: {} values", vals.len()));
        }
        for (i, &e) in filter.as_slice().iter().enumerate() {
            let el = ok(
                akantu_core::Element::try_new(ty, e as usize, ghost),
                "element",
            )?;
            let bits = vals[i * per..(i + 1) * per]
                .iter()
                .map(|x| x.to_bits())
                .collect();
            out.push((element_key(mesh, el)?, bits));
        }
    }
    Ok(out)
}

/// Overwrite the ghost values of internal `name` of law `li` with `−1`.
fn garbage_ghost_internal(model: &mut SolidMechanicsModel, li: usize, name: &str) -> Outcome {
    let law = ok(model.physics.materials.law_mut(li), "law")?;
    let types = law.base().filter_types(GhostType::Ghost);
    let b = law.base_mut();
    let field = match name {
        "stress" => &mut b.stress,
        "grad_u" => &mut b.grad_u,
        _ => b
            .piola_kirchhoff_2
            .as_mut()
            .ok_or("piola_kirchhoff_2 is not registered")?,
    };
    for ty in types {
        ok(field.get_mut(ty, GhostType::Ghost), name)?
            .as_mut_slice()
            .fill(-1.0);
    }
    Ok(())
}

/// Every ghost value of internal `name` of law `li` equals its owner's bit
/// for bit (`want_owner = true`), or is still the `−1` garbage
/// (`want_owner = false`: the tag does not exchange it). Returns the number
/// of ghost values checked (collective).
fn ghost_internal_check(
    comm: &Communicator,
    model: &SolidMechanicsModel,
    shared: &SharedMesh,
    li: usize,
    name: &str,
    want_owner: bool,
    what: &str,
) -> std::result::Result<usize, String> {
    let mesh = ok(shared.read(), "read")?;
    let mine = law_internal_by_element(model, &mesh, li, name, GhostType::NotGhost)?;
    let width = mine.first().map_or(0, |(_, v)| v.len());
    let mut w = [width as u64];
    ok(comm.all_reduce(&mut w, akantu_comm::ReduceOp::Max), "width")?;
    let width = w[0] as usize;
    let owners = gather_owned(comm, &mine, width)?;
    let ghosts = law_internal_by_element(model, &mesh, li, name, GhostType::Ghost)?;
    let r = (|| -> std::result::Result<usize, String> {
        let garbage = (-1.0 as Real).to_bits();
        let mut checked = 0;
        for (key, have) in &ghosts {
            if want_owner {
                let want = owners.get(key).ok_or_else(|| {
                    format!("{what}: ghost {key:x?} of law {li} has no owner on law {li}")
                })?;
                if have != want {
                    return Err(format!(
                        "{what}: law {li} ghost {key:x?} {name} differs from its owner's"
                    ));
                }
            } else if have.iter().any(|&b| b != garbage) {
                return Err(format!(
                    "{what}: law {li} ghost {key:x?} {name} was written (not exchanged by the tag)"
                ));
            }
            checked += have.len();
        }
        Ok(checked)
    })();
    let n = match &r {
        Ok(n) => *n,
        Err(_) => 0,
    };
    agree(comm, r.map(|_| ()))?;
    Ok(n)
}

/// CL-008 / MPI-034: a distributed explicit model with two
/// laws, law 1 in finite deformation, elements chosen by the
/// [`CheckerboardSelector`]:
///
/// 1. after `init_full` every ghost element carries its **owner's** law
///    (`_constitutive_law_id`: the owner's law index, appended to that
///    law's ghost filter; upstream never runs the selector on ghosts), and
///    both laws have ghosts somewhere;
/// 2. after 5 explicit steps (physical internals), ghost internals set to
///    `−1`, then `_smm_stress` (`splitByConstitutiveLaw`, then per law
///    `Material::packData`, `material_inline_impl.hh:174-217`): the
///    finite-deformation law's ghost `piola_kirchhoff_2`, `grad_u` and
///    `stress` and the small-strain law's ghost `stress` equal the owner's
///    bit for bit; the small-strain ghost `grad_u` is not exchanged by
///    `_smm_stress` (still `−1`);
/// 3. `_smm_gradu` then gives both laws' ghosts the owner's `grad_u`.
pub fn two_law_ghosts_take_owner_law_and_internals(comm: Communicator) -> Outcome {
    use akantu_core::enums::SynchronizationTag as T;
    let m = distributed_cube(&comm, 3, &Part::Rcb)?;
    let shared = SharedMesh::new(m);
    let mut model = Laws::TwoWithFiniteDeformation.model(&shared)?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init",
    )?;
    let fd: Vec<bool> = model
        .physics
        .materials
        .laws()
        .iter()
        .map(|l| l.base().finite_deformation)
        .collect();
    if fd != [false, true] {
        return Err(format!("laws' finite_deformation flags {fd:?}"));
    }
    // 1. ghost law = owner's law
    {
        let mesh = ok(shared.read(), "read")?;
        let mut mine = Vec::new();
        let mut ghosts = Vec::new();
        let mut ghosts_per_law = [0u64; 2];
        for ghost in GhostType::ALL {
            for e in mesh.elements(akantu_core::TypeFilter::new().dim(3).ghost(ghost)) {
                let li = ok(
                    model.physics.materials.law_index_map().element_ref(e),
                    "law index",
                )?[0];
                let key = element_key(&mesh, e)?;
                if ghost == GhostType::Ghost {
                    ghosts.push((key, li));
                    ghosts_per_law[usize::try_from(li)
                        .map_err(|_| format!("ghost {e} without law"))?
                        .min(1)] += 1;
                } else {
                    mine.push((key, vec![li as u64]));
                }
            }
        }
        let owners = gather_owned(&comm, &mine, 1)?;
        let r = (|| -> Outcome {
            for (key, li) in &ghosts {
                let want = owners
                    .get(key)
                    .ok_or_else(|| format!("ghost {key:x?} has no owner"))?;
                if want[0] != *li as u64 {
                    return Err(format!(
                        "ghost {key:x?} on law {li}, its owner on law {}",
                        want[0]
                    ));
                }
            }
            Ok(())
        })();
        agree(&comm, r)?;
        ok(
            comm.all_reduce(&mut ghosts_per_law, akantu_comm::ReduceOp::Sum),
            "ghosts",
        )?;
        if comm.size() > 1 && ghosts_per_law.contains(&0) {
            return Err(format!(
                "ghosts per law {ghosts_per_law:?}: both laws need ghosts"
            ));
        }
    }
    // 2. physical internals after a few explicit steps
    let (coords, nb) = {
        let mm = ok(shared.read(), "read")?;
        (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
    };
    {
        let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 0.0 {
                b[3 * n..3 * n + 3].fill(true);
            }
        }
        drop(b_g);
        let mut u = ok(model.displacement_mut(), "u")?;
        let u = u.as_mut_slice();
        for n in 0..nb {
            let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            u[3 * n] = 1e-2 * z * z * (1.0 + 0.3 * y);
            u[3 * n + 1] = 5e-3 * z * (0.5 - x);
        }
    }
    let dt = ok(model.stable_time_step(), "stable time step")?;
    ok(model.set_time_step(0.8 * dt, None), "time step")?;
    for s in 0..5 {
        ok(model.solve_step(None), &format!("step {s}"))?;
    }
    for (li, names) in [
        (0, &["stress", "grad_u"][..]),
        (1, &["stress", "grad_u", "piola_kirchhoff_2"][..]),
    ] {
        for &name in names {
            garbage_ghost_internal(&mut model, li, name)?;
        }
    }
    ok(model.synchronize(T::SmmStress), "synchronize _smm_stress")?;
    let mut checked = [0u64; 4];
    checked[0] +=
        ghost_internal_check(&comm, &model, &shared, 0, "stress", true, "_smm_stress")? as u64;
    ghost_internal_check(&comm, &model, &shared, 0, "grad_u", false, "_smm_stress")?;
    for name in ["piola_kirchhoff_2", "grad_u", "stress"] {
        checked[1] +=
            ghost_internal_check(&comm, &model, &shared, 1, name, true, "_smm_stress")? as u64;
    }
    // 3. _smm_gradu: both laws' ghost grad_u
    garbage_ghost_internal(&mut model, 1, "grad_u")?;
    ok(model.synchronize(T::SmmGradu), "synchronize _smm_gradu")?;
    checked[2] +=
        ghost_internal_check(&comm, &model, &shared, 0, "grad_u", true, "_smm_gradu")? as u64;
    checked[3] +=
        ghost_internal_check(&comm, &model, &shared, 1, "grad_u", true, "_smm_gradu")? as u64;
    ok(
        comm.all_reduce(&mut checked, akantu_comm::ReduceOp::Sum),
        "checked",
    )?;
    if comm.size() > 1 && checked.contains(&0) {
        return Err(format!(
            "ghost values checked per case {checked:?}: none in some case"
        ));
    }
    // the finite-deformation internals are not trivial (PK2 ≠ 0 somewhere)
    let mesh = ok(shared.read(), "read")?;
    let pk2 = law_internal_by_element(&model, &mesh, 1, "piola_kirchhoff_2", GhostType::NotGhost)?;
    let mut nonzero =
        [u64::from(pk2.iter().any(|(_, v)| {
            v.iter().any(|&b| Real::from_bits(b) != 0.0)
        }))];
    drop(mesh);
    ok(
        comm.all_reduce(&mut nonzero, akantu_comm::ReduceOp::Max),
        "nonzero",
    )?;
    if nonzero[0] == 0 {
        return Err("the finite-deformation law's PK2 is zero everywhere".into());
    }
    if comm.rank() == 0 {
        println!(
            "INFO two laws ranks={}: ghost values checked: small stress {}, fd PK2+grad_u+stress {}, \
             grad_u small {} / fd {}",
            comm.size(),
            checked[0],
            checked[1],
            checked[2],
            checked[3]
        );
    }
    Ok(())
}
