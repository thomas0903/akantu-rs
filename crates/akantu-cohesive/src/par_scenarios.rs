// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the scenario definitions of the oracle probe
// (oracle/probes/parallel_cohesive/parallel_cohesive_probe.cc), shared by the
// parallel cohesive tests (local world and real MPI) so that the port builds
// exactly the probe's meshes, partitions, material files and loading.

//! Scenarios of the parallel cohesive oracle probe.
//!
//! Meshes are generated: a Tri3 square `[-1, 1]²` with `n = 8` cells per
//! side (cell `(i, j)` split along the diagonal `(i, j)–(i+1, j+1)`) and a
//! Kuhn Tet4 cube `[0, 1]³` with `n = 4`. The partitions are barycentre
//! rules (the probe's `MeshPartitionMeshData` mapping): see [`part_2d`] and
//! [`part_3d`]. The material files, the loading and the step budgets are
//! the probe's (`specs`). Kept in the library (not in a test module) so
//! that the example pre-screening the parameters, the local-world tests and
//! the real-MPI test binary share one definition.

use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};

/// Kind of loading of a scenario (probe `Loading`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loading {
    /// `test_cohesive_parallel_intrinsic`: `v_x = rate · X`, `X = 1`
    /// blocked in x.
    IntrinsicVelocity,
    /// `test_cohesive_parallel_extrinsic` (axes swapped): `|X| > 0.99`
    /// blocked in x (2-D: `|y| > 0.99` in y), `v_x = rate · X`, every step
    /// `u_x += rate · dt · X` on `|X| > 0.99`.
    ExtrinsicPull,
    /// `test_cohesive_parallel_extrinsic_IG_TG`: biaxial, the four sides
    /// moved by `±rate · t`.
    Biaxial,
}

/// One probe scenario.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Scenario name (fixture `parallel_cohesive/<name>/np<P>/rank<r>`).
    pub name: &'static str,
    /// Spatial dimension.
    pub dim: usize,
    /// Extrinsic insertion.
    pub extrinsic: bool,
    /// Intergranular / transgranular selector.
    pub grain_selector: bool,
    /// Material file.
    pub dat: String,
    /// Loading.
    pub loading: Loading,
    /// Loading rate.
    pub rate: Real,
    /// `dt = factor · stable time step`.
    pub dt_factor: Real,
    /// Number of steps.
    pub nb_steps: usize,
    /// Steps after which the probe records the full state.
    pub checkpoints: Vec<usize>,
    /// Crack line (2-D) or plane (3-D) `x` position.
    pub crack_x: Real,
}

/// Step budgets and rates (probe constants).
pub const INTRINSIC_TRI_STEPS: usize = 100;
/// See [`INTRINSIC_TRI_STEPS`].
pub const INTRINSIC_TET_STEPS: usize = 60;
/// See [`INTRINSIC_TRI_STEPS`].
pub const EXTRINSIC_TRI_STEPS: usize = 80;
/// See [`INTRINSIC_TRI_STEPS`].
pub const EXTRINSIC_TET_STEPS: usize = 80;
/// See [`INTRINSIC_TRI_STEPS`].
pub const IGTG_STEPS: usize = 120;
/// Pull rate of the extrinsic scenarios.
pub const PULL_RATE: Real = 0.5;
/// Rate of the biaxial scenario.
pub const BIAXIAL_RATE: Real = 0.1;

fn model_section(bbox: &str, mats: &str) -> String {
    format!(
        "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = {bbox}\n  ]\n{mats}]\n"
    )
}

const INTRINSIC_MAT: &str = "  material elastic [\n    name = bulk\n    rho = 1e3\n    E = 1e3\n    nu = 0.1\n  ]\n  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n    beta = 1.5\n    G_c = 1\n    delta_0 = 0.1\n  ]\n";
const EXTRINSIC_MAT: &str = "  material elastic [\n    name = steel\n    rho = 1e3\n    E = 1e3\n    nu = 0.001\n  ]\n  material cohesive_linear [\n    name = cohesive\n    beta = 1\n    G_c = 100\n    sigma_c = 100\n  ]\n";
const IGTG_MAT: &str = "  material elastic [\n    name = steel\n    rho = 1e3\n    E = 1e3\n    nu = 0.0\n  ]\n  material cohesive_linear [\n    name = TG_cohesive\n    beta = 0\n    G_c = 10\n    sigma_c = 100\n  ]\n  material cohesive_linear [\n    name = IG_cohesive\n    beta = 0\n    G_c = 10\n    sigma_c = 20\n  ]\n";

/// The probe's scenarios (same order and parameters).
#[must_use]
pub fn specs() -> Vec<Spec> {
    vec![
        Spec {
            name: "intrinsic_tri",
            dim: 2,
            extrinsic: false,
            grain_selector: false,
            dat: model_section("[[-0.26, -0.24], [-10, 10]]", INTRINSIC_MAT),
            loading: Loading::IntrinsicVelocity,
            rate: 0.2,
            dt_factor: 0.8,
            nb_steps: INTRINSIC_TRI_STEPS,
            checkpoints: vec![INTRINSIC_TRI_STEPS / 2, INTRINSIC_TRI_STEPS],
            crack_x: -0.25,
        },
        Spec {
            name: "intrinsic_tet",
            dim: 3,
            extrinsic: false,
            grain_selector: false,
            dat: model_section("[[0.49, 0.51], [-10, 10], [-10, 10]]", INTRINSIC_MAT),
            loading: Loading::IntrinsicVelocity,
            rate: 0.2,
            dt_factor: 0.8,
            nb_steps: INTRINSIC_TET_STEPS,
            checkpoints: vec![INTRINSIC_TET_STEPS / 2, INTRINSIC_TET_STEPS],
            crack_x: 0.5,
        },
        Spec {
            name: "extrinsic_tri",
            dim: 2,
            extrinsic: true,
            grain_selector: false,
            dat: model_section("[[-0.1, 0.1], [-10, 10]]", EXTRINSIC_MAT),
            loading: Loading::ExtrinsicPull,
            rate: PULL_RATE,
            dt_factor: 0.1,
            nb_steps: EXTRINSIC_TRI_STEPS,
            checkpoints: vec![EXTRINSIC_TRI_STEPS],
            crack_x: 0.0,
        },
        Spec {
            name: "extrinsic_tet",
            dim: 3,
            extrinsic: true,
            grain_selector: false,
            dat: model_section("[[0.45, 0.55], [-10, 10], [-10, 10]]", EXTRINSIC_MAT),
            loading: Loading::ExtrinsicPull,
            rate: PULL_RATE,
            dt_factor: 0.1,
            nb_steps: EXTRINSIC_TET_STEPS,
            checkpoints: vec![EXTRINSIC_TET_STEPS],
            crack_x: 0.5,
        },
        Spec {
            name: "extrinsic_igtg_tri",
            dim: 2,
            extrinsic: true,
            grain_selector: true,
            dat: model_section("[[-0.2, 0.2], [-10, 10]]", IGTG_MAT),
            loading: Loading::Biaxial,
            rate: BIAXIAL_RATE,
            dt_factor: 0.1,
            nb_steps: IGTG_STEPS,
            checkpoints: vec![IGTG_STEPS],
            crack_x: 0.0,
        },
    ]
}

/// The scenario named `name`.
///
/// # Errors
/// Unknown name.
pub fn spec(name: &str) -> Result<Spec> {
    specs()
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| akantu_core::Error::not_found("parallel cohesive scenario", name))
}

/// Tri3 square `[-1, 1]²`, `n` cells per side, appended to `mesh` (empty).
///
/// # Errors
/// Mesh errors.
pub fn square_tri3(mesh: &mut Mesh, n: usize) -> Result<()> {
    let h = 2.0 / n as Real;
    let id = |i: usize, j: usize| akantu_core::to_idx(j * (n + 1) + i);
    {
        let mut a = MeshAccessor::new(mesh);
        for j in 0..=n {
            for i in 0..=n {
                a.push_node(&[-1.0 + i as Real * h, -1.0 + j as Real * h])?;
            }
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                let (n0, n1, n2, n3) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                c.push_tuple(&[n0, n1, n2]);
                c.push_tuple(&[n0, n2, n3]);
            }
        }
    }
    mesh.compute_bounding_box()
}

/// Kuhn Tet4 cube `[0, 1]³`, `n` cells per side, appended to `mesh` (empty).
///
/// # Errors
/// Mesh errors.
pub fn cube_tet4(mesh: &mut Mesh, n: usize) -> Result<()> {
    let h = 1.0 / n as Real;
    let id = |i: usize, j: usize, k: usize| akantu_core::to_idx((k * (n + 1) + j) * (n + 1) + i);
    {
        let mut a = MeshAccessor::new(mesh);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    a.push_node(&[i as Real * h, j as Real * h, k as Real * h])?;
                }
            }
        }
        let mut c = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
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
                    for t in [
                        [0, 1, 2, 6],
                        [0, 2, 3, 6],
                        [0, 3, 7, 6],
                        [0, 7, 4, 6],
                        [0, 4, 5, 6],
                        [0, 5, 1, 6],
                    ] {
                        c.push_tuple(&[v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                    }
                }
            }
        }
    }
    mesh.compute_bounding_box()
}

/// The scenario's mesh on rank 0 of `mesh`'s communicator (empty mesh
/// elsewhere).
///
/// # Errors
/// Mesh errors.
pub fn build_mesh(spec: &Spec, mesh: &mut Mesh, full: bool) -> Result<()> {
    if !full {
        return Ok(());
    }
    if spec.dim == 2 {
        square_tri3(mesh, 8)
    } else {
        cube_tet4(mesh, 4)
    }
}

fn cell(x: Real, lo: Real, h: Real) -> i64 {
    // the probe's `Int(std::floor((x - lo) / h))`
    #[allow(clippy::cast_possible_truncation)] // small, finite cell index
    let c = ((x - lo) / h).floor() as i64;
    c
}

/// 2-D rule of the probe (crack `x = xc`, `n = 8` square): see the probe
/// header. `None` for an unsupported rank count.
#[must_use]
pub fn part_2d(b: &[Real], xc: Real, np: usize) -> Option<Idx> {
    if np == 1 {
        return Some(0);
    }
    let right = b[0] > xc;
    if np == 2 {
        return Some(Idx::from(right));
    }
    if np != 4 {
        return None;
    }
    let h = 0.25;
    let i = cell(b[0], -1.0, h);
    let j = cell(b[1], -1.0, h);
    #[allow(clippy::cast_possible_truncation)] // the probe's std::lround
    let ic = ((xc + 1.0) / h).round() as i64;
    if !right {
        return Some(if b[1] < 0.0 { 0 } else { 1 });
    }
    if i == ic && (j == 4 || j == 6) {
        return Some(0);
    }
    Some(if b[1] < 0.25 { 2 } else { 3 })
}

/// 3-D rule of the probe (crack plane `x = 0.5`, `n = 4` cube). `None` for
/// an unsupported rank count.
#[must_use]
pub fn part_3d(b: &[Real], np: usize) -> Option<Idx> {
    if np == 1 {
        return Some(0);
    }
    let right = b[0] > 0.5;
    if np == 2 {
        return Some(Idx::from(right));
    }
    if np != 4 {
        return None;
    }
    let h = 0.25;
    let (i, j, k) = (cell(b[0], 0.0, h), cell(b[1], 0.0, h), cell(b[2], 0.0, h));
    if !right {
        return Some(if b[1] < 0.5 { 0 } else { 1 });
    }
    if i == 2 && j == 2 && (k == 0 || k == 2) {
        return Some(0);
    }
    Some(if b[2] < 0.5 { 2 } else { 3 })
}

/// Linearised parts of the scenario's mesh (elements of the mesh dimension,
/// types in enum order): the probe's `partition/linearized`.
///
/// # Errors
/// Mesh errors, an unsupported rank count.
pub fn partition(spec: &Spec, mesh: &Mesh, np: usize) -> Result<Vec<Idx>> {
    let dim = spec.dim;
    let mut out = Vec::new();
    let mut b = vec![0.0; dim];
    for ty in mesh.element_types(akantu_core::TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: GhostType::NotGhost,
        kind: None,
    }) {
        for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
            mesh.barycenter_into(
                akantu_core::Element::try_new(ty, e, GhostType::NotGhost)?,
                &mut b,
            )?;
            let p = if dim == 2 {
                part_2d(&b, spec.crack_x, np)
            } else {
                part_3d(&b, np)
            };
            out.push(p.ok_or_else(|| {
                akantu_core::Error::invalid(format!(
                    "the parallel cohesive scenarios define partitions for 1, 2 and 4 ranks, not {np}"
                ))
            })?);
        }
    }
    Ok(out)
}

/// Coordinate in `[-1, 1]` of the loading axis.
#[must_use]
pub fn loading_coordinate(spec: &Spec, x: Real) -> Real {
    if spec.dim == 2 { x } else { 2.0 * x - 1.0 }
}

/// The probe's `GrainSelector` (`test_cohesive_parallel_extrinsic_IG_TG`'s
/// `MultiGrainMaterialSelector` with the grain of an element = side of
/// `x = 0` of its barycentre): a facet between two grains takes `ig`, a
/// facet inside a grain or on the boundary `tg`; every other element defers
/// to the rest of the chain (the default cohesive selection).
#[derive(Clone, Debug)]
pub struct GrainSelector {
    /// Index of the transgranular law.
    pub tg: usize,
    /// Index of the intergranular law.
    pub ig: usize,
}

impl akantu_model::constitutive_law::ConstitutiveLawSelector for GrainSelector {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        element: akantu_core::Element,
    ) -> Result<Option<usize>> {
        let dim = ctx.mesh.spatial_dimension();
        if element.ty().spatial_dimension() + 1 != dim
            || element.kind() != akantu_core::ElementKind::Regular
        {
            return Ok(None);
        }
        let Some(fm) = ctx.mesh.mesh_facets() else {
            return Ok(None);
        };
        let row = &fm
            .data()
            .element_lists(akantu_mesh::utils::ELEMENT_TO_SUBELEMENT)?
            .element(element)?[0];
        let el1 = row.first().copied().unwrap_or(akantu_core::Element::NULL);
        let el2 = row.get(1).copied().unwrap_or(akantu_core::Element::NULL);
        if el2.is_null() {
            return Ok(Some(self.tg));
        }
        let mut b = vec![0.0; dim];
        let mut grain = |e: akantu_core::Element| -> Result<bool> {
            ctx.mesh.barycenter_into(e, &mut b)?;
            Ok(b[0] < 0.0)
        };
        let g1 = grain(el1)?;
        let g2 = grain(el2)?;
        Ok(Some(if g1 == g2 { self.tg } else { self.ig }))
    }
}

/// Per-step record of a run ([`run`]).
#[derive(Clone, Debug, Default)]
pub struct StepRecord {
    /// `check_cohesive_stress` result (extrinsic; 0 otherwise).
    pub inserted: usize,
    /// Energies after the solve: kinetic, potential, dissipated,
    /// reversible, cohesive contact.
    pub energies: [Real; 5],
}

/// Hooks of [`run`] (all optional).
pub trait RunObserver {
    /// After the initialisation, the time step and the boundary
    /// conditions (the probe's `init/` records).
    ///
    /// # Errors
    /// The observer's.
    fn after_init(&mut self, _model: &mut crate::SolidMechanicsModelCohesive) -> Result<()> {
        Ok(())
    }
    /// After `check_cohesive_stress` of `step` (extrinsic only).
    ///
    /// # Errors
    /// The observer's.
    fn after_check(
        &mut self,
        _model: &mut crate::SolidMechanicsModelCohesive,
        _step: usize,
        _inserted: usize,
    ) -> Result<()> {
        Ok(())
    }
    /// After the solve of `step`.
    ///
    /// # Errors
    /// The observer's.
    fn after_step(
        &mut self,
        _model: &mut crate::SolidMechanicsModelCohesive,
        _step: usize,
        _record: &StepRecord,
    ) -> Result<()> {
        Ok(())
    }
}

/// No hooks.
#[derive(Debug, Default)]
pub struct NoObserver;
impl RunObserver for NoObserver {}

/// Upstream-like run of the scenario on `mesh` (distributed or serial),
/// exactly as the probe's `run_case`: model, selector, `init_full`, time
/// step, boundary conditions and initial velocity, then the steps (extrinsic:
/// boundary increment, `check_cohesive_stress`, solve; intrinsic: solve) with
/// the five energies after every solve. Returns the model, the time steps
/// `[stable, dt]` and the per-step records.
///
/// # Errors
/// Model, solver or observer errors.
pub fn run(
    spec: &Spec,
    mesh: akantu_mesh::SharedMesh,
    observer: &mut dyn RunObserver,
) -> Result<(
    crate::SolidMechanicsModelCohesive,
    [Real; 2],
    Vec<StepRecord>,
)> {
    use akantu_core::enums::AnalysisMethod;
    let parser = akantu_parser::Parser::parse_str(&spec.dat, "material.dat")?;
    let mut model = crate::SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        crate::model::DEFAULT_ID,
        parser,
    )?;
    if spec.grain_selector {
        // law indices in creation order: steel 0, TG 1, IG 2 (the probe
        // resolves them by name in its selector)
        model.set_material_selector(Box::new(GrainSelector { tg: 1, ig: 2 }));
    }
    model.init_full(
        &akantu_model::ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        spec.extrinsic,
    )?;
    let stable = model.stable_time_step()?;
    let dt = stable * spec.dt_factor;
    model.set_time_step(dt, None)?;
    let (coords, nb, dim) = {
        let m = mesh.read()?;
        (
            m.nodes().as_slice().to_vec(),
            m.nb_nodes(),
            m.spatial_dimension(),
        )
    };
    {
        let eps = Real::EPSILON;
        let mut blocked = model.smm.blocked_dofs_mut()?;
        let b = blocked.as_mut_slice();
        for n in 0..nb {
            let xx = loading_coordinate(spec, coords[dim * n]);
            match spec.loading {
                Loading::IntrinsicVelocity => {
                    if (xx - 1.0).abs() < eps {
                        b[dim * n] = true;
                    }
                }
                Loading::ExtrinsicPull => {
                    if xx.abs() > 0.99 {
                        b[dim * n] = true;
                    }
                    if dim == 2 && coords[dim * n + 1].abs() > 0.99 {
                        b[dim * n + 1] = true;
                    }
                }
                Loading::Biaxial => {
                    if coords[dim * n + 1].abs() > 0.99 {
                        b[dim * n + 1] = true;
                    }
                    if coords[dim * n].abs() > 0.99 {
                        b[dim * n] = true;
                    }
                }
            }
        }
        drop(blocked);
        let mut vel = model.smm.velocity_mut()?;
        let v = vel.as_mut_slice();
        for n in 0..nb {
            let xx = loading_coordinate(spec, coords[dim * n]);
            match spec.loading {
                Loading::IntrinsicVelocity | Loading::ExtrinsicPull => {
                    v[dim * n] = spec.rate * xx;
                }
                Loading::Biaxial => {
                    v[dim * n + 1] = spec.rate * coords[dim * n + 1];
                    v[dim * n] = spec.rate * coords[dim * n];
                }
            }
        }
    }
    observer.after_init(&mut model)?;
    let mut records = Vec::with_capacity(spec.nb_steps);
    let mut dispy: Real = 0.0;
    let vi = spec.rate;
    for step in 1..=spec.nb_steps {
        let mut inserted = 0;
        if spec.extrinsic {
            // the node set grows with the insertions: re-read it
            let (coords, nb) = {
                let m = mesh.read()?;
                (m.nodes().as_slice().to_vec(), m.nb_nodes())
            };
            if spec.loading == Loading::Biaxial {
                dispy += vi * dt;
            }
            {
                let mut disp = model.smm.displacement_mut()?;
                let u = disp.as_mut_slice();
                for n in 0..nb {
                    let (x, y) = (coords[dim * n], coords[dim * n + 1]);
                    match spec.loading {
                        Loading::ExtrinsicPull => {
                            let xx = loading_coordinate(spec, x);
                            if xx.abs() > 0.99 {
                                u[dim * n] += spec.rate * dt * xx;
                            }
                        }
                        Loading::Biaxial => {
                            if y > 0.99 {
                                u[dim * n + 1] = dispy;
                            }
                            if y < -0.99 {
                                u[dim * n + 1] = -dispy;
                            }
                            if x > 0.99 {
                                u[dim * n] = dispy;
                            }
                            if x < -0.99 {
                                u[dim * n] = -dispy;
                            }
                        }
                        Loading::IntrinsicVelocity => {}
                    }
                }
            }
            if spec.loading == Loading::Biaxial {
                let mut vel = model.smm.velocity_mut()?;
                let v = vel.as_mut_slice();
                for n in 0..nb {
                    let (x, y) = (coords[dim * n], coords[dim * n + 1]);
                    if y > 0.99 {
                        v[dim * n + 1] = vi;
                    }
                    if y < -0.99 {
                        v[dim * n + 1] = -vi;
                    }
                    if x > 0.99 {
                        v[dim * n] = vi;
                    }
                    if x < -0.99 {
                        v[dim * n] = -vi;
                    }
                }
            }
            inserted = model.check_cohesive_stress()?;
            observer.after_check(&mut model, step, inserted)?;
        }
        model.solve_step(None)?;
        let mut energies = [0.0; 5];
        for (e, id) in energies.iter_mut().zip([
            "kinetic",
            "potential",
            "dissipated",
            "reversible",
            "cohesive contact",
        ]) {
            *e = model.energy(id)?;
        }
        let rec = StepRecord { inserted, energies };
        observer.after_step(&mut model, step, &rec)?;
        records.push(rec);
    }
    Ok((model, [stable, dt], records))
}

// ---------------------------------------------------------------------------
// Scenarios (probe version 2): facets-only insertion,
// clustering and fragments on a distributed mesh.
// ---------------------------------------------------------------------------

/// Explicit steps before the facets-only insertion (`facets_only_*`).
pub const FACETS_ONLY_PRE_STEPS: usize = 10;
/// Explicit steps after the facets-only insertion (`facets_only_*`).
pub const FACETS_ONLY_POST_STEPS: usize = 20;

/// The facets-only scenarios of the probe: `facets_only_tri` (Tri3
/// square, the 2-D rule with `xc = 0`), `facets_only_tet` (Kuhn cube, the
/// 3-D rule) and `facets_only_shared_tri` (as `facets_only_tri`, crack on
/// the rank boundary): the extrinsic material and loading of
/// `extrinsic_tri` / `extrinsic_tet` (pull in x, no stress check),
/// [`FACETS_ONLY_PRE_STEPS`] steps, then on every rank the **not-ghost**
/// facets of the scenario's crack ([`facets_only_flagged`]; ghost facets
/// get their flags from their owners) are doubled by
/// `insert_facets_only` (upstream `insertElements(true)`), then
/// [`FACETS_ONLY_POST_STEPS`] steps. The cracks of the first two keep
/// every doubled node on one rank (other ranks double only the ghost
/// copies of the cracked facets); the third doubles nodes shared by two
/// ranks (refused, UD-1043; upstream asserts). The adversarial cases
/// `facets_only_pg_tri` (a crack ending on the rank boundary) and
/// `facets_only_bnd_tri` (one rank-boundary facet doubled without any
/// node), both Tri3 as `facets_only_tri` (probe v3).
#[must_use]
pub fn facets_only_specs() -> Vec<Spec> {
    let n = FACETS_ONLY_PRE_STEPS + FACETS_ONLY_POST_STEPS;
    let tri = |name: &'static str| Spec {
        name,
        dim: 2,
        extrinsic: true,
        grain_selector: false,
        dat: model_section("[[-0.1, 0.1], [-10, 10]]", EXTRINSIC_MAT),
        loading: Loading::ExtrinsicPull,
        rate: PULL_RATE,
        dt_factor: 0.1,
        nb_steps: n,
        checkpoints: vec![n],
        crack_x: 0.0,
    };
    vec![
        tri("facets_only_tri"),
        Spec {
            name: "facets_only_tet",
            dim: 3,
            extrinsic: true,
            grain_selector: false,
            dat: model_section("[[0.45, 0.55], [-10, 10], [-10, 10]]", EXTRINSIC_MAT),
            loading: Loading::ExtrinsicPull,
            rate: PULL_RATE,
            dt_factor: 0.1,
            nb_steps: n,
            checkpoints: vec![n],
            crack_x: 0.5,
        },
        tri("facets_only_shared_tri"),
        // adversarial cases (probe v3)
        tri("facets_only_pg_tri"),
        tri("facets_only_bnd_tri"),
    ]
}

/// `true` for a facet (its node coordinates, `dim` per node) of the crack
/// of the facets-only scenario `name`:
/// * `facets_only_tri`: every node on `x = −0.5` with `y ≤ 0`, or on
///   `y = −0.5` with `x ≤ −0.5` (a T inside rank 0 at 2 and 4 ranks; its
///   top facet is a ghost facet of rank 1 at 4 ranks);
/// * `facets_only_tet`: every node on `x = 0.25` with `y ≤ 0.5`, or on
///   `z = 0.5` with `x ≤ 0.25` and `y ≤ 0.5`;
/// * `facets_only_shared_tri`: every node on `x = 0` (the rank boundary at
///   2 ranks), or on `y = 0` with `x ≤ 0`;
/// * `facets_only_pg_tri`: every node on `y = −0.5` with `x ≤ 0` (the
///   crack ends on the rank boundary `x = 0`; its tip node is not doubled,
///   only rank 0 doubles nodes, the other ranks double only the ghost
///   copies of the cracked facets);
/// * `facets_only_bnd_tri`: every node on `x = 0` with `−0.5 ≤ y ≤ −0.25`
///   (one facet on the rank boundary: doubled without any node, the
///   facet-stress schemes are pruned, no node is added on any rank).
///
/// Exact comparisons: the generated grids put these nodes exactly on the
/// planes.
#[must_use]
#[allow(clippy::float_cmp)] // exact grid coordinates (see the doc)
pub fn facets_only_flagged(name: &str, coords: &[Real], dim: usize) -> bool {
    let nodes: Vec<&[Real]> = coords.chunks_exact(dim).collect();
    let all = |f: &dyn Fn(&[Real]) -> bool| nodes.iter().all(|x| f(x));
    match name {
        "facets_only_tri" => {
            all(&|x| x[0] == -0.5 && x[1] <= 0.0) || all(&|x| x[1] == -0.5 && x[0] <= -0.5)
        }
        "facets_only_tet" => {
            all(&|x| x[0] == 0.25 && x[1] <= 0.5)
                || all(&|x| x[2] == 0.5 && x[0] <= 0.25 && x[1] <= 0.5)
        }
        "facets_only_shared_tri" => all(&|x| x[0] == 0.0) || all(&|x| x[1] == 0.0 && x[0] <= 0.0),
        "facets_only_pg_tri" => all(&|x| x[1] == -0.5 && x[0] <= 0.0),
        "facets_only_bnd_tri" => all(&|x| x[0] == 0.0 && x[1] >= -0.5 && x[1] <= -0.25),
        _ => false,
    }
}

/// Flag the **not-ghost** facets of the facets-only crack
/// ([`facets_only_flagged`]) in the model's inserter; returns how many.
///
/// # Errors
/// A mesh without facet mesh.
pub fn flag_facets_only(
    spec: &Spec,
    model: &mut crate::SolidMechanicsModelCohesive,
) -> Result<usize> {
    let mesh_h = model.smm.physics.base.mesh.clone();
    let m = mesh_h.read()?;
    let fm = m
        .mesh_facets()
        .ok_or_else(|| akantu_core::Error::Mesh("no facet mesh".into()))?;
    let dim = spec.dim;
    let nodes = fm.nodes();
    let x = nodes.as_slice();
    let flags = model.ext.inserter.insertion_facets_mut();
    let mut count = 0;
    for ty in fm.element_types(akantu_core::TypeFilter {
        spatial_dimension: Some(dim - 1),
        ghost_type: GhostType::NotGhost,
        kind: Some(akantu_core::ElementKind::Regular),
    }) {
        let conn = fm.connectivity(ty, GhostType::NotGhost)?;
        let nn = conn.nb_component();
        let f = flags.get_mut(ty, GhostType::NotGhost)?;
        for (e, row) in conn.as_slice().chunks_exact(nn).enumerate() {
            let mut c = Vec::with_capacity(nn * dim);
            for &n in row {
                let n = akantu_core::try_to_usize(n)?;
                c.extend_from_slice(&x[n * dim..(n + 1) * dim]);
            }
            if facets_only_flagged(spec.name, &c, dim) {
                f.as_mut_slice()[e] = true;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Set the boundary conditions and the initial velocity of `spec`'s
/// loading (the probe's set-up after the time step).
fn apply_loading_setup(
    spec: &Spec,
    model: &mut crate::SolidMechanicsModelCohesive,
    coords: &[Real],
    nb: usize,
    dim: usize,
) -> Result<()> {
    let eps = Real::EPSILON;
    let mut blocked = model.smm.blocked_dofs_mut()?;
    let b = blocked.as_mut_slice();
    for n in 0..nb {
        let xx = loading_coordinate(spec, coords[dim * n]);
        match spec.loading {
            Loading::IntrinsicVelocity => {
                if (xx - 1.0).abs() < eps {
                    b[dim * n] = true;
                }
            }
            Loading::ExtrinsicPull => {
                if xx.abs() > 0.99 {
                    b[dim * n] = true;
                }
                if dim == 2 && coords[dim * n + 1].abs() > 0.99 {
                    b[dim * n + 1] = true;
                }
            }
            Loading::Biaxial => {
                if coords[dim * n + 1].abs() > 0.99 {
                    b[dim * n + 1] = true;
                }
                if coords[dim * n].abs() > 0.99 {
                    b[dim * n] = true;
                }
            }
        }
    }
    drop(blocked);
    let mut vel = model.smm.velocity_mut()?;
    let v = vel.as_mut_slice();
    for n in 0..nb {
        let xx = loading_coordinate(spec, coords[dim * n]);
        match spec.loading {
            Loading::IntrinsicVelocity | Loading::ExtrinsicPull => {
                v[dim * n] = spec.rate * xx;
            }
            Loading::Biaxial => {
                v[dim * n + 1] = spec.rate * coords[dim * n + 1];
                v[dim * n] = spec.rate * coords[dim * n];
            }
        }
    }
    Ok(())
}

/// Hooks of [`run_facets_only`] in addition to [`RunObserver`]'s.
pub trait FacetsOnlyObserver: RunObserver {
    /// After `insert_facets_only` (after step [`FACETS_ONLY_PRE_STEPS`]).
    ///
    /// # Errors
    /// The observer's.
    fn after_facets_only(
        &mut self,
        _model: &mut crate::SolidMechanicsModelCohesive,
        _flagged: usize,
        _result: &crate::inserter::InsertionResult,
    ) -> Result<()> {
        Ok(())
    }
}

impl FacetsOnlyObserver for NoObserver {}

/// The probe's facets-only run of `spec` (see [`facets_only_specs`]):
/// model, `init_full` (extrinsic), time step, loading set-up, then
/// [`FACETS_ONLY_PRE_STEPS`] steps (boundary increment, solve; no stress
/// check), the flags and `insert_facets_only`, then
/// [`FACETS_ONLY_POST_STEPS`] steps. Energies after every solve.
///
/// # Errors
/// Model, solver, insertion or observer errors.
pub fn run_facets_only(
    spec: &Spec,
    mesh: akantu_mesh::SharedMesh,
    observer: &mut dyn FacetsOnlyObserver,
) -> Result<(
    crate::SolidMechanicsModelCohesive,
    [Real; 2],
    Vec<StepRecord>,
)> {
    use akantu_core::enums::AnalysisMethod;
    let parser = akantu_parser::Parser::parse_str(&spec.dat, "material.dat")?;
    let mut model = crate::SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        crate::model::DEFAULT_ID,
        parser,
    )?;
    model.init_full(
        &akantu_model::ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        true,
    )?;
    let stable = model.stable_time_step()?;
    let dt = stable * spec.dt_factor;
    model.set_time_step(dt, None)?;
    let dim = spec.dim;
    {
        let (coords, nb) = {
            let m = mesh.read()?;
            (m.nodes().as_slice().to_vec(), m.nb_nodes())
        };
        apply_loading_setup(spec, &mut model, &coords, nb, dim)?;
    }
    observer.after_init(&mut model)?;
    let mut records = Vec::with_capacity(spec.nb_steps);
    for step in 1..=spec.nb_steps {
        let (coords, nb) = {
            let m = mesh.read()?;
            (m.nodes().as_slice().to_vec(), m.nb_nodes())
        };
        {
            let mut disp = model.smm.displacement_mut()?;
            let u = disp.as_mut_slice();
            for n in 0..nb {
                let xx = loading_coordinate(spec, coords[dim * n]);
                if xx.abs() > 0.99 {
                    u[dim * n] += spec.rate * dt * xx;
                }
            }
        }
        model.solve_step(None)?;
        let mut energies = [0.0; 5];
        for (e, id) in energies.iter_mut().zip([
            "kinetic",
            "potential",
            "dissipated",
            "reversible",
            "cohesive contact",
        ]) {
            *e = model.energy(id)?;
        }
        let rec = StepRecord {
            inserted: 0,
            energies,
        };
        observer.after_step(&mut model, step, &rec)?;
        records.push(rec);
        if step == FACETS_ONLY_PRE_STEPS {
            let flagged = flag_facets_only(spec, &mut model)?;
            let r = model.insert_facets_only()?;
            observer.after_facets_only(&mut model, flagged, &r)?;
        }
    }
    Ok((model, [stable, dt], records))
}

/// Cell `(i, j)` (`h = 0.25`) of a point of the `n = 8` square.
fn square_cell(b: &[Real]) -> (i64, i64) {
    (cell(b[0], -1.0, 0.25), cell(b[1], -1.0, 0.25))
}

/// The clustering filter of the probe's `clusters_tri` scenario on the
/// barycentre of an element of the `n = 8` square: the cells of row
/// `j = 3` (`y ∈ [−0.25, 0]`) and, above it, of the columns `i = 2` and
/// `i = 5` are left out, which leaves four clusters (the bottom band and
/// three columns), several of them over three or four ranks with the 2-D
/// rule (`xc = 0`).
#[must_use]
pub fn clusters_filter(b: &[Real]) -> bool {
    let (i, j) = square_cell(b);
    !(j == 3 || (j >= 4 && (i == 2 || i == 5)))
}

/// Crack lines of the probe's `fragments_tri` scenario (the check facets
/// are the facets with every node on one of them).
pub const FRAGMENT_LINES: [Real; 3] = [-0.5, 0.0, 0.5];
/// Opening imposed on a crack line to break it (5 `δc` of the material).
pub const FRAGMENT_OPENING: Real = 0.01;

/// Material file of `fragments_tri`: elastic bulk and a `cohesive_linear`
/// law with `σc = 1`, `G_c = 1e-3` (`δc = 2e-3`), every facet checkable
/// before the check-facet restriction.
#[must_use]
pub fn fragments_dat() -> String {
    model_section(
        "[[-10, 10], [-10, 10]]",
        "  material elastic [\n    name = bulk\n    rho = 1e3\n    E = 1e3\n    nu = 0.1\n  ]\n  material cohesive_linear [\n    name = cohesive\n    beta = 1\n    G_c = 1e-3\n    sigma_c = 1\n  ]\n",
    )
}

/// The `fragments_tri` scenario as a [`Spec`] (Tri3 square, the 2-D rule
/// with `xc = 0`); the run is [`run_fragments`].
#[must_use]
pub fn fragments_spec() -> Spec {
    Spec {
        name: "fragments_tri",
        dim: 2,
        extrinsic: true,
        grain_selector: false,
        dat: fragments_dat(),
        loading: Loading::ExtrinsicPull,
        rate: 0.0,
        dt_factor: 0.1,
        nb_steps: 0,
        checkpoints: Vec::new(),
        crack_x: 0.0,
    }
}

/// The displacement stages of `fragments_tri` after the insertion: the
/// nodes of every element (local and ghost) whose barycentre `x` is below
/// (`true`) or above (`false`) the limit move by the amount in x (each
/// node once; upstream test_cohesive_parallel_buildfragments'
/// `displaceElements`). Stage 1 is the undisplaced state.
pub const FRAGMENT_STAGES: [(Real, bool, Real); 3] = [
    (-0.5, true, -FRAGMENT_OPENING),
    (0.5, false, FRAGMENT_OPENING),
    (0.0, true, -FRAGMENT_OPENING),
];

/// Keep only the check facets with every node on one of
/// [`FRAGMENT_LINES`] (both ghost types).
///
/// # Errors
/// A mesh without facet mesh.
pub fn restrict_fragment_check_facets(
    model: &mut crate::SolidMechanicsModelCohesive,
) -> Result<()> {
    let mesh_h = model.smm.physics.base.mesh.clone();
    let m = mesh_h.read()?;
    let fm = m
        .mesh_facets()
        .ok_or_else(|| akantu_core::Error::Mesh("no facet mesh".into()))?;
    let dim = m.spatial_dimension();
    let nodes = fm.nodes();
    let x = nodes.as_slice();
    let check = model.ext.inserter.check_facets_mut();
    for ghost in GhostType::ALL {
        for ty in fm.element_types(akantu_core::TypeFilter {
            spatial_dimension: Some(dim - 1),
            ghost_type: ghost,
            kind: Some(akantu_core::ElementKind::Regular),
        }) {
            let conn = fm.connectivity(ty, ghost)?;
            let nn = conn.nb_component();
            let Ok(f) = check.get_mut(ty, ghost) else {
                continue;
            };
            for (e, row) in conn.as_slice().chunks_exact(nn).enumerate() {
                let xs: Vec<Real> = row
                    .iter()
                    .map(|&n| akantu_core::try_to_usize(n).map(|n| x[n * dim]))
                    .collect::<Result<_>>()?;
                #[allow(clippy::float_cmp)] // exact grid coordinates
                let on_line = FRAGMENT_LINES.iter().any(|&l| xs.iter().all(|&xi| xi == l));
                if !on_line && let Some(s) = f.as_mut_slice().get_mut(e) {
                    *s = false;
                }
            }
        }
    }
    Ok(())
}

/// Hooks of [`run_fragments`].
pub trait FragmentsObserver {
    /// After the insertion (`check_cohesive_stress`).
    ///
    /// # Errors
    /// The observer's.
    fn after_insertion(
        &mut self,
        _model: &mut crate::SolidMechanicsModelCohesive,
        _inserted: usize,
    ) -> Result<()> {
        Ok(())
    }
    /// After `compute_all_data` of stage `stage` (1-based).
    ///
    /// # Errors
    /// The observer's.
    fn after_stage(
        &mut self,
        _model: &mut crate::SolidMechanicsModelCohesive,
        _fm: &crate::FragmentManager,
        _stage: usize,
    ) -> Result<()> {
        Ok(())
    }
}

impl FragmentsObserver for NoObserver {}

/// Move in x, by `amount`, every node of the elements of dimension `dim`
/// (local and ghost) whose barycentre `x` is below (`below`) or above the
/// `limit`, each node once (upstream `displaceElements`).
///
/// # Errors
/// Mesh errors.
pub fn displace_elements(
    model: &mut crate::SolidMechanicsModelCohesive,
    limit: Real,
    below: bool,
    amount: Real,
) -> Result<()> {
    let mesh_h = model.smm.physics.base.mesh.clone();
    let m = mesh_h.read()?;
    let dim = m.spatial_dimension();
    let mut displaced = vec![false; m.nb_nodes()];
    let mut disp = model.smm.displacement_mut()?;
    let u = disp.as_mut_slice();
    let mut b = vec![0.0; dim];
    for ghost in GhostType::ALL {
        for ty in m.element_types(akantu_core::TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: ghost,
            kind: Some(akantu_core::ElementKind::Regular),
        }) {
            let conn = m.connectivity(ty, ghost)?;
            let nn = conn.nb_component();
            for (e, row) in conn.as_slice().chunks_exact(nn).enumerate() {
                m.barycenter_into(akantu_core::Element::try_new(ty, e, ghost)?, &mut b)?;
                let hit = if below { b[0] < limit } else { b[0] > limit };
                if !hit {
                    continue;
                }
                for &n in row {
                    let n = akantu_core::try_to_usize(n)?;
                    if !displaced[n] {
                        u[n * dim] += amount;
                        displaced[n] = true;
                    }
                }
            }
        }
    }
    Ok(())
}

/// The probe's `fragments_tri` run: model, `init_full` (extrinsic),
/// the check facets restricted to [`FRAGMENT_LINES`], `u_x = 0.1 x`,
/// `v = (x, 0.5 y)`, internal forces, `check_cohesive_stress` (every
/// check facet opens), then a [`crate::FragmentManager`] and four stages
/// of `compute_all_data(1)`: undisplaced, then after each of
/// [`FRAGMENT_STAGES`] (displacement, internal forces). Returns the model
/// and the number inserted (global).
///
/// # Errors
/// Model, insertion, fragment or observer errors.
pub fn run_fragments(
    mesh: akantu_mesh::SharedMesh,
    observer: &mut dyn FragmentsObserver,
) -> Result<(crate::SolidMechanicsModelCohesive, usize)> {
    use akantu_core::enums::AnalysisMethod;
    let parser = akantu_parser::Parser::parse_str(&fragments_dat(), "material.dat")?;
    let mut model = crate::SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        crate::model::DEFAULT_ID,
        parser,
    )?;
    model.init_full(
        &akantu_model::ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        true,
    )?;
    restrict_fragment_check_facets(&mut model)?;
    {
        let (coords, nb, dim) = {
            let m = mesh.read()?;
            (
                m.nodes().as_slice().to_vec(),
                m.nb_nodes(),
                m.spatial_dimension(),
            )
        };
        let mut disp = model.smm.displacement_mut()?;
        let u = disp.as_mut_slice();
        for n in 0..nb {
            u[dim * n] = coords[dim * n] * 0.1;
        }
        drop(disp);
        let mut vel = model.smm.velocity_mut()?;
        let v = vel.as_mut_slice();
        for n in 0..nb {
            v[dim * n] = coords[dim * n];
            v[dim * n + 1] = 0.5 * coords[dim * n + 1];
        }
    }
    model.assemble_internal_forces()?;
    let inserted = model.check_cohesive_stress()?;
    observer.after_insertion(&mut model, inserted)?;
    let mut fm = crate::FragmentManager::new(&model)?;
    fm.compute_all_data(&mut model, 1.0)?;
    observer.after_stage(&mut model, &fm, 1)?;
    for (k, &(limit, below, amount)) in FRAGMENT_STAGES.iter().enumerate() {
        displace_elements(&mut model, limit, below, amount)?;
        model.assemble_internal_forces()?;
        fm.compute_all_data(&mut model, 1.0)?;
        observer.after_stage(&mut model, &fm, k + 2)?;
    }
    Ok((model, inserted))
}
