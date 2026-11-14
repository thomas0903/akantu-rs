// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared support of
// the tests: the upstream `TestSMMCFixture`
// (test/test_model/test_solid_mechanics_model/test_cohesive/
// test_cohesive_fixture.hh) and mesh/parser helpers.

#![allow(dead_code)] // shared by several test binaries

use std::path::PathBuf;
use std::sync::Mutex;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::DirichletFunctor;
use akantu_parser::Parser;
use akantu_validation::fixtures_root;

/// The global random engine is process-wide: tests with random fields run
/// one at a time.
pub static RNG_LOCK: Mutex<()> = Mutex::new(());

/// [`RNG_LOCK`], recovered when a previous test panicked while holding it.
pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    RNG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub const MATERIAL_0: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho = 1e3   # density
    E   = 1e9   # young's modulus
    nu  = 0.2 # poisson's ratio
  ]
  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
]
";

pub const MATERIAL_1: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho = 1e3   # density
    E   = 1e9   # young's modulus
    nu  = 0.2 # poisson's ratio
  ]
  material cohesive_bilinear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
    delta_0 = 1e-7
  ]
]
";

pub fn frozen(rel: &str) -> PathBuf {
    fixtures_root().join("meshes/gmsh-4.13.1").join(rel)
}

pub fn cohesive_mesh(name: &str) -> PathBuf {
    frozen(&format!(
        "test/test_model/test_solid_mechanics_model/test_cohesive/{name}"
    ))
}

pub fn read_mesh(path: &std::path::Path, dim: usize) -> SharedMesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh).unwrap();
    SharedMesh::new(mesh)
}

/// Upstream `StrainIncrement` functor: nodes with `|x(dir)| ≥ 1e-8` get
/// blocked and `u += ε_inc · x`.
pub struct StrainIncrement {
    pub strain: Vec<Real>, // column-major dim × dim
    pub dim: usize,
    pub dir: usize,
}

impl DirichletFunctor for StrainIncrement {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> akantu_core::Result<()> {
        if coord[self.dir].abs() < 1e-8 {
            return Ok(());
        }
        flags.iter_mut().for_each(|f| *f = true);
        let d = self.dim;
        // `primal += strain_inc * coord` (Matrix<Real> × VectorProxy: GEMV)
        for (i, p) in primal.iter_mut().enumerate().take(d) {
            let mut acc = 0.0;
            for (k, c) in coord.iter().enumerate().take(d) {
                acc += self.strain[i + d * k] * c;
            }
            *p += acc;
        }
        Ok(())
    }
}

/// Upstream `BC::Dirichlet::FlagOnly(axis)`.
pub struct FlagOnly(pub usize);

impl DirichletFunctor for FlagOnly {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        _primal: &mut [Real],
        _coord: &[Real],
    ) -> akantu_core::Result<()> {
        flags[self.0] = true;
        Ok(())
    }
}

pub fn real_param(model: &SolidMechanicsModelCohesive, mat: &str, p: &str) -> Real {
    let i = model.smm.material_index(mat).unwrap();
    let law = model.smm.material(i).unwrap();
    match law.get_param(p).unwrap() {
        akantu_parser::param::ParamValue::Real(v) => v,
        akantu_parser::param::ParamValue::Random(r) => r.base_value,
        other => panic!("{p}: {other:?}"),
    }
}

/// Upstream `TestSMMCFixture` state.
pub struct Fixture {
    pub mesh: SharedMesh,
    pub model: SolidMechanicsModelCohesive,
    pub dim: usize,
    pub cohesive_type: ElementType,
    pub surface: Real,
    pub group_size: usize,
    pub nb_steps: usize,
    pub extrinsic: bool,
    /// `getStableTimeStep()` of `createModel()` (the time step is 0.01 of it).
    pub stable_time_step: Real,
    /// [`Fixture::create_on`] on more than one rank: the RCB part of every
    /// element of the mesh dimension of the read mesh, in type order (rank 0;
    /// empty elsewhere and for [`Fixture::create`]).
    pub partition: Vec<Idx>,
}

impl Fixture {
    /// `createModel()`.
    pub fn create(
        mesh_file: &str,
        dim: usize,
        cohesive_type: ElementType,
        dat: &str,
        extrinsic: bool,
    ) -> Self {
        Self::create_with(mesh_file, dim, cohesive_type, dat, extrinsic, |_| {})
    }

    /// [`Fixture::create`] with `configure` applied to the model before
    /// `init_full` (e.g. upstream-compat switches).
    pub fn create_with(
        mesh_file: &str,
        dim: usize,
        cohesive_type: ElementType,
        dat: &str,
        extrinsic: bool,
        configure: impl FnOnce(&mut SolidMechanicsModelCohesive),
    ) -> Self {
        let mesh = read_mesh(&cohesive_mesh(mesh_file), dim);
        let parser = Parser::parse_str(dat, "material.dat").unwrap();
        let mut model = SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            "solid_mechanics_model_cohesive",
            parser,
        )
        .unwrap();
        configure(&mut model);
        model
            .init_full(
                &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
                extrinsic,
            )
            .unwrap();
        let stable_time_step = model.stable_time_step().unwrap();
        let dt = stable_time_step * 0.01;
        model.set_time_step(dt, None).unwrap();
        let (surface, group_size) = if dim == 1 {
            (1.0, 1)
        } else {
            let facet_type = cohesive_type.facet_type(0).unwrap();
            let m = mesh.read().unwrap();
            let elems = m
                .groups()
                .element_group("insertion")
                .unwrap()
                .elements_of(facet_type, GhostType::NotGhost)
                .to_vec();
            let fe = model.smm.physics.base.fems.boundary("", &m).unwrap();
            let nq = fe
                .nb_integration_points(facet_type, GhostType::NotGhost)
                .unwrap();
            let ones = vec![1.0; nq * elems.len()];
            let s = fe
                .integrate_scalar(
                    akantu_core::array::ArrayView::new(&ones, 1).unwrap(),
                    facet_type,
                    GhostType::NotGhost,
                    Some(&elems),
                )
                .unwrap();
            (s, elems.len())
        };
        Self {
            mesh,
            model,
            dim,
            cohesive_type,
            surface,
            group_size,
            nb_steps: 1000,
            extrinsic,
            stable_time_step,
            partition: Vec::new(),
        }
    }

    /// `SetUp()` + `createModel()` under the suite's `PARALLEL_LEVEL 1 2`
    /// registration on `comm`: the mesh is read
    /// on rank 0 and distributed (recursive coordinate bisection; upstream's
    /// default partitioner is Scotch, its checks do not depend on the
    /// partition), its facets built with the corrected flip
    /// (`FacetFlipMode::RemoteOrder`, the default of the distributed
    /// cohesive tests); `surface` and `group_size` are the sums over the
    /// ranks of the not-ghost `insertion` facets (upstream's `allReduce`);
    /// 1-D: 1 and 1 without reduction, as upstream. A 1-D mesh on more than
    /// one rank has no facet mesh (UD-1141, refused as upstream: see
    /// [`distributed_1d_refusal`]), so callers skip those cases as upstream's
    /// test does.
    pub fn create_on(
        comm: &akantu_comm::Communicator,
        mesh_file: &str,
        dim: usize,
        cohesive_type: ElementType,
        dat: &str,
        extrinsic: bool,
    ) -> Self {
        use akantu_comm::ReduceOp;
        use akantu_mesh::facet_synchronizer::DistributedFacetsOptions;
        use akantu_mesh::partition::{MeshPartitioner, RcbPartitioner};
        use akantu_mesh::utils::{FacetFlipMode, init_mesh_facets_with};
        let mut m = Mesh::with_communicator(dim, "mesh", comm.clone()).unwrap();
        if comm.rank() == 0 {
            akantu_io::read_mesh(&mut m, &cohesive_mesh(mesh_file), MeshIoType::Gmsh).unwrap();
        }
        let mut partition = Vec::new();
        if comm.size() > 1 {
            m.compute_bounding_box().unwrap();
            if comm.rank() == 0 {
                // the partition distribute_with computes (deterministic),
                // recorded for the oracle comparison
                let p = RcbPartitioner.partition(&m, comm.size()).unwrap();
                let types: Vec<ElementType> = m
                    .element_types(akantu_core::TypeFilter::new().dim(dim))
                    .collect();
                for ty in types {
                    partition.extend_from_slice(p.partition(ty).unwrap());
                }
            }
            let mut p = RcbPartitioner;
            let _ = akantu_mesh::distribution::distribute_with(
                &mut m,
                Some(&mut p as &mut dyn MeshPartitioner),
                0,
            )
            .unwrap();
            let _ = init_mesh_facets_with(
                &mut m,
                DistributedFacetsOptions {
                    flip: FacetFlipMode::RemoteOrder,
                },
            )
            .unwrap();
        }
        let mesh = SharedMesh::new(m);
        let parser = Parser::parse_str(dat, "material.dat").unwrap();
        let mut model = SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            "solid_mechanics_model_cohesive",
            parser,
        )
        .unwrap();
        model
            .init_full(
                &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
                extrinsic,
            )
            .unwrap();
        let stable_time_step = model.stable_time_step().unwrap();
        let dt = stable_time_step * 0.01;
        model.set_time_step(dt, None).unwrap();
        let (surface, group_size) = if dim == 1 {
            (1.0, 1)
        } else {
            let facet_type = cohesive_type.facet_type(0).unwrap();
            let m = mesh.read().unwrap();
            let elems = m
                .groups()
                .element_group("insertion")
                .unwrap()
                .elements_of(facet_type, GhostType::NotGhost)
                .to_vec();
            let fe = model.smm.physics.base.fems.boundary("", &m).unwrap();
            let nq = fe
                .nb_integration_points(facet_type, GhostType::NotGhost)
                .unwrap();
            let ones = vec![1.0; nq * elems.len()];
            let s = fe
                .integrate_scalar(
                    akantu_core::array::ArrayView::new(&ones, 1).unwrap(),
                    facet_type,
                    GhostType::NotGhost,
                    Some(&elems),
                )
                .unwrap();
            let mut s = [s];
            comm.all_reduce(&mut s, ReduceOp::Sum).unwrap();
            let mut n = [elems.len() as u64];
            comm.all_reduce(&mut n, ReduceOp::Sum).unwrap();
            (s[0], n[0] as usize)
        };
        Self {
            mesh,
            model,
            dim,
            cohesive_type,
            surface,
            group_size,
            nb_steps: 1000,
            extrinsic,
            stable_time_step,
            partition,
        }
    }

    /// `setInitialCondition(strain)`.
    pub fn set_initial_condition(&mut self, strain: &[Real]) {
        let d = self.dim;
        let nodes = self.mesh.read().unwrap().nodes().as_slice().to_vec();
        let mut u = self.model.smm.displacement_mut().unwrap();
        for (x, ui) in nodes.chunks(d).zip(u.as_mut_slice().chunks_mut(d)) {
            for i in 0..d {
                let mut acc = 0.0;
                for k in 0..d {
                    acc += strain[i + d * k] * x[k];
                }
                ui[i] = acc;
            }
        }
    }

    /// `steps(strain)`: `nb_steps` × (BCs on `loading` and `fixed`,
    /// `checkCohesiveStress` if extrinsic, `solveStep`), with a per-step
    /// observer.
    pub fn steps(
        &mut self,
        strain: &[Real],
        mut observe: impl FnMut(usize, &mut SolidMechanicsModelCohesive, usize),
    ) {
        // upstream `StrainIncrement functor((1. / 300) * strain, …)`: the
        // reciprocal times each entry (not a division), as the oracle
        let functor = StrainIncrement {
            strain: strain.iter().map(|s| (1. / 300.) * s).collect(),
            dim: self.dim,
            dir: if self.dim == 1 { 0 } else { 1 },
        };
        for s in 0..self.nb_steps {
            self.model
                .smm
                .apply_dirichlet_bc(&functor, "loading")
                .unwrap();
            self.model
                .smm
                .apply_dirichlet_bc(&functor, "fixed")
                .unwrap();
            let n = if self.extrinsic {
                self.model.check_cohesive_stress().unwrap()
            } else {
                0
            };
            self.model.solve_step(None).unwrap();
            observe(s, &mut self.model, n);
        }
    }

    fn scaled(m: &[Real], f: Real) -> Vec<Real> {
        m.iter().map(|x| x * f).collect()
    }

    /// `testModeI()` up to the steps; returns the strain.
    pub fn mode_i_strain(&mut self) -> Vec<Real> {
        let speed = {
            let i = self.model.smm.material_index("body").unwrap();
            self.model
                .smm
                .material(i)
                .unwrap()
                .push_wave_speed(Element::NULL)
                .unwrap()
        };
        let dir = if self.dim == 1 { 0 } else { 1 };
        let length = {
            let m = self.mesh.read().unwrap();
            m.upper_bounds()[dir] - m.lower_bounds()[dir]
        };
        let dt = self.model.smm.time_step(None).unwrap();
        self.nb_steps = (length / speed / dt) as usize;
        let sigma_c = real_param(&self.model, "insertion", "sigma_c");
        let e = real_param(&self.model, "body", "E");
        let nu = real_param(&self.model, "body", "nu");
        let mut strain: Vec<Real> = match self.dim {
            1 => vec![1.],
            2 => Self::scaled(&[-nu, 0., 0., 1. - nu], 1. + nu),
            _ => vec![-nu, 0., 0., 0., 1., 0., 0., 0., -nu],
        };
        for s in &mut strain {
            *s *= sigma_c / e;
        }
        strain
    }

    /// `checkInsertion`'s count: the not-ghost cohesive elements summed
    /// over the ranks of the mesh's communicator (upstream `allReduce`).
    pub fn nb_cohesive_global(&self) -> usize {
        let m = self.mesh.read().unwrap();
        let mut n = [m.nb_element(self.cohesive_type, GhostType::NotGhost) as u64];
        m.communicator()
            .all_reduce(&mut n, akantu_comm::ReduceOp::Sum)
            .unwrap();
        n[0] as usize
    }

    pub fn nb_cohesive(&self) -> usize {
        self.mesh
            .read()
            .unwrap()
            .nb_element(self.cohesive_type, GhostType::NotGhost)
    }
}

/// `true` when `e` is UD-1141's refusal of a distributed 1-D facet mesh.
fn is_ud1141_refusal(e: &akantu_core::Error) -> bool {
    matches!(e, akantu_core::Error::Unsupported(m) if m.contains("UD-1141"))
}

/// Every rank of `comm` holds the same `text` (two all-reduces of a digest).
fn same_on_every_rank(comm: &akantu_comm::Communicator, text: &str) -> Result<bool, String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    let d = h.finish();
    let mut v = [d, !d];
    comm.all_reduce(&mut v, akantu_comm::ReduceOp::Max)
        .map_err(|e| format!("all_reduce: {e}"))?;
    Ok(v[0] == d && v[1] == !d)
}

/// UD-1141 (category C): upstream's `TestSMMCFixture`
/// mesh `_cohesive_1d_2_segment_2.msh` read on rank 0 and distributed over
/// `comm` (RCB). On more than one rank the facet mesh
/// (`init_mesh_facets_with`) and the cohesive model's constructor, for an
/// intrinsic (`material_1.dat`) and an extrinsic (`material_0.dat`) model,
/// are refused with UD-1141's `Error::Unsupported`, the same message on
/// every rank, and leave the mesh unchanged (no facet mesh, same revision);
/// afterwards the communicator still reduces and the mesh still runs a
/// solid mechanics model (explicit steps). On one rank (a mesh "distributed"
/// on a single process, UD-011) the facet mesh and both cohesive models are
/// built, as upstream. Returns a one-line summary.
pub fn distributed_1d_refusal(comm: &akantu_comm::Communicator) -> Result<String, String> {
    use akantu_mesh::facet_synchronizer::DistributedFacetsOptions;
    use akantu_mesh::partition::{MeshPartitioner, RcbPartitioner};
    use akantu_mesh::utils::{FacetFlipMode, init_mesh_facets_with};
    use akantu_solid::SolidMechanicsModel;
    let e2s = |e: akantu_core::Error| e.to_string();
    let size = comm.size();
    let mut m = Mesh::with_communicator(1, "mesh", comm.clone()).map_err(e2s)?;
    if comm.rank() == 0 {
        akantu_io::read_mesh(
            &mut m,
            &cohesive_mesh("_cohesive_1d_2_segment_2.msh"),
            MeshIoType::Gmsh,
        )
        .map_err(e2s)?;
    }
    m.compute_bounding_box().map_err(e2s)?;
    let mut p = RcbPartitioner;
    let _ = akantu_mesh::distribution::distribute_with(
        &mut m,
        Some(&mut p as &mut dyn MeshPartitioner),
        0,
    )
    .map_err(e2s)?;
    let revision = m.revision();
    let facets = init_mesh_facets_with(
        &mut m,
        DistributedFacetsOptions {
            flip: FacetFlipMode::RemoteOrder,
        },
    );
    let mut messages = Vec::new();
    match (size, facets) {
        (1, Ok(_)) if m.mesh_facets().is_some() => {}
        (1, other) => return Err(format!("one rank: facet mesh not built: {other:?}")),
        (_, Err(e)) if is_ud1141_refusal(&e) => messages.push(e.to_string()),
        (_, other) => {
            return Err(format!(
                "{size} ranks: init_mesh_facets_with not refused with UD-1141: {other:?}"
            ));
        }
    }
    if size > 1 && (m.mesh_facets().is_some() || m.revision() != revision) {
        return Err("the refused facet mesh changed the mesh".into());
    }
    let mesh = SharedMesh::new(m);
    for (dat, extrinsic) in [(MATERIAL_1, false), (MATERIAL_0, true)] {
        let parser = Parser::parse_str(dat, "material.dat").map_err(e2s)?;
        let model = SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            "solid_mechanics_model_cohesive",
            parser,
        );
        match (size, model) {
            (1, Ok(mut model)) => {
                model
                    .init_full(
                        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
                        extrinsic,
                    )
                    .map_err(|e| format!("one rank, extrinsic {extrinsic}: init_full: {e}"))?;
            }
            (1, Err(e)) => return Err(format!("one rank, extrinsic {extrinsic}: {e}")),
            (_, Err(e)) if is_ud1141_refusal(&e) => messages.push(e.to_string()),
            (_, Err(e)) => return Err(format!("extrinsic {extrinsic}: another error: {e}")),
            (_, Ok(_)) => return Err(format!("extrinsic {extrinsic}: the model was built")),
        }
        if size > 1 {
            let g = mesh.read().map_err(e2s)?;
            if g.mesh_facets().is_some() || g.revision() != revision {
                return Err(format!(
                    "extrinsic {extrinsic}: the refusal changed the mesh"
                ));
            }
        }
    }
    let joined = messages.join("\n");
    if !same_on_every_rank(comm, &joined)? {
        return Err(format!("the refusals differ between the ranks: {joined}"));
    }
    let mut one = [1u64];
    comm.all_reduce(&mut one, akantu_comm::ReduceOp::Sum)
        .map_err(|e| format!("communicator after the refusal: {e}"))?;
    if one[0] != size as u64 {
        return Err(format!("communicator after the refusal: sum {}", one[0]));
    }
    if size > 1 {
        // the mesh after the refusal: a solid mechanics model, explicit steps
        let parser = Parser::parse_str(
            "material elastic [\n  name = body\n  rho = 1e3\n  E = 1e9\n  nu = 0.2\n]\n",
            "material.dat",
        )
        .map_err(e2s)?;
        let mut smm = SolidMechanicsModel::with_parser(mesh.clone(), None, "smm", parser)
            .map_err(|e| format!("solid model after the refusal: {e}"))?;
        smm.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .map_err(|e| format!("solid model after the refusal: init_full: {e}"))?;
        let dt = smm.stable_time_step().map_err(e2s)? * 0.5;
        smm.set_time_step(dt, None).map_err(e2s)?;
        {
            let mut v = smm.velocity_mut().map_err(e2s)?;
            v.as_mut_slice().fill(1e-3);
        }
        for _ in 0..3 {
            smm.solve_step(None)
                .map_err(|e| format!("solid model after the refusal: solve_step: {e}"))?;
        }
        let ek = smm.energy("kinetic").map_err(e2s)?;
        if !(ek.is_finite() && ek > 0.) {
            return Err(format!("solid model after the refusal: E_kin {ek}"));
        }
    }
    Ok(if size > 1 {
        format!(
            "{size} ranks: facet mesh and both cohesive models refused (UD-1141), same message \
             on every rank, mesh and communicator usable afterwards"
        )
    } else {
        "one rank: facet mesh and both cohesive models built".into()
    })
}
