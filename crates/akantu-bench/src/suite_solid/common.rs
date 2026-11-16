// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared pieces of
// the solid-mechanics benchmark cases; the explicit set-up follows
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc as the
// port's crates/akantu-solid/examples/explicit_step_timing.rs does.

//! Model construction, initial conditions, digests and the step-splitting
//! interceptor shared by the solid cases.

use std::time::Instant;

use akantu_core::array::ArrayViewMut;
use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawContext, LawPhase, ModelServices, SelectorContext,
};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::callback::{Delegate, InterceptCallback};

use crate::suite::{Ctx, Digest};

/// The explicit example's elastic parameters (`docs/BENCHMARKS.md`).
pub const RHO: Real = 2500.0;
/// Young's modulus of the explicit example.
pub const YOUNG: Real = 70e9;
/// Poisson's ratio of the explicit example.
pub const NU: Real = 0.22;

/// The explicit example's material file with one elastic law.
#[must_use]
pub fn elastic_dat(fd: bool) -> String {
    format!(
        "material elastic [\n  name = body\n  rho = {RHO}\n  E = {YOUNG}\n  nu = {NU}\n  \
         finite_deformation = {fd}\n]\n"
    )
}

/// A solid mechanics model on `mesh` from the material file `dat`.
///
/// # Errors
/// Parser and model errors.
pub fn solid_model(mesh: Mesh, dat: &str) -> Result<SolidMechanicsModel> {
    let parser = Parser::parse_str(dat, "material.dat")?;
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)
}

/// `init_full` with `method` and default solver options.
///
/// # Errors
/// Model errors.
pub fn init(m: &mut SolidMechanicsModel, method: AnalysisMethod) -> Result<()> {
    m.init_full(&ModelOptions::new(method))
}

/// Node coordinates of the model's mesh (copied).
///
/// # Errors
/// Lock errors.
pub fn coordinates(m: &SolidMechanicsModel) -> Result<Vec<Real>> {
    Ok(m.physics.base.mesh.read()?.nodes().as_slice().to_vec())
}

/// A sine–Gaussian displacement pulse along x, centred in the x range
/// `[lo, hi]` of the box (not the local mesh's: ranks agree): `u_x = a sin(4π ξ) exp(−(ξ/0.2)²)`, `ξ = (x − x_mid)/L`,
/// `a = 1e-3 L`. The amplitude is the explicit timing harness's; its pulse
/// (the upstream example's formula) is narrower than one element of these
/// structured meshes and vanishes at the centre node, which leaves the
/// state almost zero, so this one is spread over about a fifth of the
/// length (the cost per element and step does not depend on it).
///
/// # Errors
/// Model errors.
pub fn apply_pulse(m: &mut SolidMechanicsModel, lo: Real, hi: Real) -> Result<()> {
    let x = coordinates(m)?;
    let d = m.spatial_dimension();
    let len = (hi - lo).max(Real::MIN_POSITIVE);
    let mid = 0.5 * (lo + hi);
    let amp = 1e-3 * len;
    let four_pi = 4.0 * std::f64::consts::PI;
    for (u, p) in m
        .displacement_mut()?
        .as_mut_slice()
        .chunks_exact_mut(d)
        .zip(x.chunks_exact(d))
    {
        let xi = (p[0] - mid) / len;
        u[0] = amp * (four_pi * xi).sin() * (-(xi / 0.2) * (xi / 0.2)).exp();
    }
    Ok(())
}

/// Element blocks of the model dimension owned by laws: (law, type, filter).
///
/// # Errors
/// Missing filters.
pub fn law_blocks(m: &SolidMechanicsModel) -> Result<Vec<(usize, ElementType, Vec<Idx>)>> {
    let dim = m.spatial_dimension();
    let mut out = Vec::new();
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        for ty in law.base().filter_types(GhostType::NotGhost) {
            if ty.spatial_dimension() != dim {
                continue;
            }
            let f = law
                .base()
                .element_filter
                .get(ty, GhostType::NotGhost)?
                .as_slice()
                .to_vec();
            if !f.is_empty() {
                out.push((i, ty, f));
            }
        }
    }
    Ok(out)
}

/// Elements, nodes and DOFs of a serial solid model.
///
/// # Errors
/// Lock errors.
pub fn counts(m: &SolidMechanicsModel) -> Result<(usize, usize, usize)> {
    let mesh = m.physics.base.mesh.read()?;
    let d = m.spatial_dimension();
    let ne: usize = mesh
        .element_types(TypeFilter::new().dim(d).ghost(GhostType::NotGhost))
        .filter(|t| t.kind() == ElementKind::Regular)
        .map(|t| mesh.nb_element(t, GhostType::NotGhost))
        .sum();
    Ok((ne, mesh.nb_nodes(), mesh.nb_nodes() * d))
}

/// Record the counts of a serial model.
///
/// # Errors
/// Lock errors.
pub fn set_counts(ctx: &mut Ctx<'_>, m: &SolidMechanicsModel) -> Result<()> {
    let (e, n, d) = counts(m)?;
    ctx.set_counts(e, n, d);
    Ok(())
}

/// Digest of a serial solid model: `u`, `v`, `a`, `f_int`, then every
/// law's stress per element type (law order, type order).
///
/// # Errors
/// Missing arrays.
pub fn digest(m: &SolidMechanicsModel) -> Result<Digest> {
    let mut h = Digest::new();
    h.reals(m.displacement()?.as_slice());
    if let Ok(v) = m.velocity() {
        h.reals(v.as_slice());
    }
    if let Ok(a) = m.acceleration() {
        h.reals(a.as_slice());
    }
    h.reals(m.internal_force()?.as_slice());
    for law in m.physics.materials.laws() {
        for ty in law.base().filter_types(GhostType::NotGhost) {
            if let Ok(s) = law.base().stress.get(ty, GhostType::NotGhost) {
                h.reals(s.as_slice());
            }
        }
    }
    Ok(h)
}

/// Sub-phases of a solver step, timed through the model's intercepting
/// callback (`solve_step_intercepted`, which delegates every call to the
/// model: the step computes what `solve_step` computes).
#[derive(Debug, Default)]
pub struct StepSplit {
    /// Accumulate only while set (timed calls).
    pub on: bool,
    /// Seconds per sub-phase: predictor, residual, matrix, lumped matrix,
    /// corrector, before/after-step hooks.
    pub seconds: [f64; 6],
    /// Calls per sub-phase.
    pub calls: [usize; 6],
}

/// Names of [`StepSplit`]'s sub-phases.
pub const SPLIT_NAMES: [&str; 6] = [
    "predictor",
    "residual",
    "matrix",
    "lumped_matrix",
    "corrector",
    "step_hooks",
];

impl StepSplit {
    fn time(&mut self, k: usize, f: impl FnOnce() -> Result<()>) -> Result<()> {
        if !self.on {
            return f();
        }
        let t = Instant::now();
        let r = f();
        self.seconds[k] += t.elapsed().as_secs_f64();
        self.calls[k] += 1;
        r
    }

    /// Record the sub-phases that were called, and the remainder of the
    /// step (`step_rest`: the solver's own work, e.g. the linear solve)
    /// against the step total `step_seconds`.
    pub fn record(&self, ctx: &mut Ctx<'_>, step_calls: usize, step_seconds: f64) {
        let mut sum = 0.0;
        for (k, name) in SPLIT_NAMES.iter().enumerate() {
            if self.calls[k] > 0 {
                ctx.record(name, self.calls[k], self.seconds[k]);
                sum += self.seconds[k];
            }
        }
        ctx.record("step_rest", step_calls, (step_seconds - sum).max(0.0));
    }
}

impl InterceptCallback for StepSplit {
    fn assemble_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        self.time(2, || model.assemble_matrix(matrix_id))
    }
    fn assemble_lumped_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        self.time(3, || model.assemble_lumped_matrix(matrix_id))
    }
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        self.time(1, || model.assemble_residual())
    }
    fn predictor(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        self.time(0, || model.predictor())
    }
    fn corrector(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        self.time(4, || model.corrector())
    }
    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        self.time(5, || model.before_solve_step())
    }
    fn after_solve_step(&mut self, model: &mut Delegate<'_>, converged: bool) -> Result<()> {
        self.time(5, || model.after_solve_step(converged))
    }
}

/// Time `steps` solver steps as the loop phase `solve_step` (primary),
/// split into the [`StepSplit`] sub-phases; `before` runs before every
/// step, untimed (boundary increments).
///
/// # Errors
/// Model errors.
pub fn timed_steps(
    ctx: &mut Ctx<'_>,
    m: &mut SolidMechanicsModel,
    steps: usize,
    mut before: impl FnMut(&mut SolidMechanicsModel) -> Result<()>,
) -> Result<()> {
    let mut split = StepSplit::default();
    let mut step_seconds = 0.0;
    let calls = ctx.repeat_primary("solve_step", steps, |timed| {
        before(m)?;
        split.on = timed;
        let t = Instant::now();
        m.solve_step_intercepted(&mut split, None)?;
        if timed {
            step_seconds += t.elapsed().as_secs_f64();
        }
        Ok(())
    })?;
    split.record(ctx, calls, step_seconds);
    Ok(())
}

/// Read-only services for timing every law's `compute_all_stresses`
/// outside the model (the model's own services type is crate-private): the
/// mesh, the FE engines and the displacement.
struct Services<'a> {
    mesh: &'a Mesh,
    fems: &'a akantu_model::model::FeEngineRegistry,
    displacement: &'a Array<Real>,
    dim: usize,
    time_step: Real,
}

impl ModelServices for Services<'_> {
    fn spatial_dimension(&self) -> usize {
        self.dim
    }
    fn mesh(&self) -> &Mesh {
        self.mesh
    }
    fn time_step(&self) -> Real {
        self.time_step
    }
    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> usize {
        self.fems
            .get("")
            .and_then(|fe| fe.nb_integration_points(ty, ghost))
            .unwrap_or(0)
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        self.fems.get("")?.interpolate_on_integration_points(
            self.mesh,
            self.mesh.nodes().view(),
            out,
            self.mesh.spatial_dimension(),
            ty,
            ghost,
            Some(filter),
        )
    }
    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        match name {
            "displacement" => Ok(self.displacement.into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        self.fems.get(name)
    }
    fn gradient_on_integration_points_view(
        &self,
        field: &str,
        out: ArrayViewMut<'_, Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let u = self.nodal_field(field)?;
        self.fe_engine("")?.gradient_on_integration_points_view(
            self.mesh,
            u.view(),
            out,
            nb_component,
            ty,
            ghost,
            Some(filter),
        )
    }
}

/// Every law's `compute_all_stresses` on the current displacement (the
/// stress part of the internal forces: gradient and constitutive law).
///
/// # Errors
/// Law errors.
pub fn compute_all_stresses(m: &mut SolidMechanicsModel) -> Result<()> {
    let dim = m.spatial_dimension();
    let time_step = m.law_time_step();
    let SolidMechanicsModel {
        physics, solver, ..
    } = m;
    let mesh_h = physics.base.mesh.clone();
    let mesh = mesh_h.read()?;
    let u = solver.dof_manager.dofs("displacement")?;
    let services = Services {
        mesh: &mesh,
        fems: &physics.base.fems,
        displacement: &u,
        dim,
        time_step,
    };
    let ctx = LawContext {
        phase: LawPhase::Stress,
        services: &services,
    };
    for law in physics.materials.laws_mut() {
        law.compute_all_stresses(&ctx, GhostType::NotGhost)?;
    }
    Ok(())
}

/// Law index by the x coordinate of the element's barycentre: law `k` for
/// `x` in the `k`-th of `nb` equal slabs of `[lo, hi]` (several materials
/// in one mesh, each with its own element filter).
#[derive(Clone, Copy, Debug)]
pub struct SlabSelector {
    /// Lower x bound.
    pub lo: Real,
    /// Upper x bound.
    pub hi: Real,
    /// Number of slabs (= laws).
    pub nb: usize,
}

impl ConstitutiveLawSelector for SlabSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let mesh = ctx.mesh;
        let d = mesh.spatial_dimension();
        let conn = mesh.connectivity(element.ty(), element.ghost_type())?;
        let nn = conn.nb_component();
        let row = &conn.as_slice()[element.idx() * nn..(element.idx() + 1) * nn];
        let x = mesh.nodes();
        let xs = x.as_slice();
        let mut bx = 0.0;
        for &n in row {
            bx += xs[usize::try_from(n).map_err(|_| Error::invalid("negative node"))? * d];
        }
        bx /= nn as Real;
        let t = ((bx - self.lo) / (self.hi - self.lo) * self.nb as Real).floor();
        Ok(Some((t.max(0.0) as usize).min(self.nb - 1)))
    }
}

/// The block nodes on the faces `x = lo` / `x = hi` (tolerance relative to
/// the length) of a coordinate array.
#[must_use]
pub fn face_nodes(x: &[Real], d: usize, axis: usize, value: Real, tol: Real) -> Vec<usize> {
    x.chunks_exact(d)
        .enumerate()
        .filter(|(_, p)| (p[axis] - value).abs() <= tol)
        .map(|(i, _)| i)
        .collect()
}
