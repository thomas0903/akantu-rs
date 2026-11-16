// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Cohesive cases of
// the benchmark suite; the problems follow
// test/test_model/test_solid_mechanics_model/test_cohesive/
// test_cohesive_parallel_intrinsic and _extrinsic (materials of the port's
// akantu-cohesive par_scenarios), the implicit cohesive probe fixtures
// (fixtures/model/cohesive/implicit_static_*), and the cohesive
// scalability benchmark (performance-testing-cohesive cohesive_extrinsic.cc,
// benchmarks/cohesive-perf/inputs/material.dat, as
// crates/akantu-cohesive/examples/cohesive_step_timing.rs).

//! Cohesive cases: intrinsic (explicit with the bilinear and exponential
//! laws, implicit static), extrinsic with the insertion check and no
//! insertion, extrinsic with insertions.

use std::time::Instant;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;

use super::common;
use super::mesh::{BoxSpec, box_mesh};
use crate::suite::{Ctx, Extra, Outcome};

/// Seed of the random cohesive strengths (one engine per process).
const SEED: i64 = 1;

/// A cohesive model on `mesh` from `dat`.
///
/// # Errors
/// Parser and model errors.
pub fn model(mesh: Mesh, dat: &str) -> Result<SolidMechanicsModelCohesive> {
    let parser = Parser::parse_str(dat, "material.dat")?;
    SolidMechanicsModelCohesive::with_parser(
        SharedMesh::new(mesh),
        None,
        akantu_cohesive::model::DEFAULT_ID,
        parser,
    )
}

/// Local elements of the mesh dimension of `kind`.
///
/// # Errors
/// Lock errors.
pub fn nb_elements(m: &SolidMechanicsModelCohesive, kind: ElementKind) -> Result<usize> {
    let mesh = m.smm.physics.base.mesh.read()?;
    let d = mesh.spatial_dimension();
    Ok(mesh
        .element_types(TypeFilter::new().dim(d).ghost(GhostType::NotGhost))
        .filter(|t| t.kind() == kind)
        .map(|t| mesh.nb_element(t, GhostType::NotGhost))
        .sum())
}

/// The model's energies (kinetic, potential, dissipated, reversible).
///
/// # Errors
/// Model errors.
fn energies(m: &mut SolidMechanicsModelCohesive) -> Result<()> {
    for id in ["kinetic", "potential", "dissipated", "reversible"] {
        std::hint::black_box(m.energy(id)?);
    }
    Ok(())
}

/// Stretch ramp along x for a cohesive model: the x displacement of every
/// node on `x = lo` or `x = hi` blocked and moved by `ε̇ Δt (x − x_mid)`
/// per step (nodes doubled by insertions included: the node set is re-read
/// when it grows).
#[derive(Debug)]
pub struct Ramp {
    lo: Real,
    hi: Real,
    rate: Real,
    dt: Real,
    nb_nodes: usize,
    dofs: Vec<usize>,
    du: Vec<Real>,
}

impl Ramp {
    /// A ramp of strain rate `rate` between `x = lo` and `x = hi`; sets the
    /// initial velocity `v_x = ε̇ (x − x_mid)`.
    ///
    /// # Errors
    /// Model errors.
    pub fn new(
        m: &mut SolidMechanicsModelCohesive,
        lo: Real,
        hi: Real,
        rate: Real,
    ) -> Result<Self> {
        let dt = m.smm.time_step(None)?;
        let mut r = Self {
            lo,
            hi,
            rate,
            dt,
            nb_nodes: 0,
            dofs: Vec::new(),
            du: Vec::new(),
        };
        r.refresh(m)?;
        let x = common::coordinates(&m.smm)?;
        let d = m.smm.spatial_dimension();
        let mid = 0.5 * (lo + hi);
        let mut v = m.smm.velocity_mut()?;
        for (vn, p) in v.as_mut_slice().chunks_exact_mut(d).zip(x.chunks_exact(d)) {
            vn[0] = rate * (p[0] - mid);
        }
        Ok(r)
    }

    fn refresh(&mut self, m: &mut SolidMechanicsModelCohesive) -> Result<()> {
        let x = common::coordinates(&m.smm)?;
        let d = m.smm.spatial_dimension();
        let n = x.len() / d;
        if n == self.nb_nodes {
            return Ok(());
        }
        let tol = 1e-9 * (self.hi - self.lo);
        let mid = 0.5 * (self.lo + self.hi);
        self.dofs.clear();
        self.du.clear();
        let mut b = m.smm.blocked_dofs_mut()?;
        let b = b.as_mut_slice();
        for (i, p) in x.chunks_exact(d).enumerate() {
            if (p[0] - self.lo).abs() <= tol || (p[0] - self.hi).abs() <= tol {
                b[i * d] = true;
                self.dofs.push(i * d);
                self.du.push(self.rate * self.dt * (p[0] - mid));
            }
        }
        self.nb_nodes = n;
        Ok(())
    }

    /// Move the faces by one step.
    ///
    /// # Errors
    /// Model errors.
    pub fn increment(&mut self, m: &mut SolidMechanicsModelCohesive) -> Result<()> {
        self.refresh(m)?;
        let mut u = m.smm.displacement_mut()?;
        let u = u.as_mut_slice();
        for (&k, &du) in self.dofs.iter().zip(&self.du) {
            u[k] += du;
        }
        Ok(())
    }
}

fn record_counts(ctx: &mut Ctx<'_>, m: &SolidMechanicsModelCohesive) -> Result<()> {
    common::set_counts(ctx, &m.smm)?;
    ctx.extra(
        "n_cohesive",
        Extra::Int(nb_elements(m, ElementKind::Cohesive)? as i64),
    );
    Ok(())
}

/// The bulk law of the intrinsic parallel tests.
const INTRINSIC_BULK: &str =
    "  material elastic [\n    name = bulk\n    rho = 1e3\n    E = 1e3\n    nu = 0.1\n  ]\n";

/// Intrinsic explicit case: cohesive elements inserted at `init_full` on
/// every facet whose box test passes in the slab `0.4 ≤ x ≤ 0.6` of the
/// unit cube (tetrahedron_4), stretch ramp along x.
///
/// Phases: `mesh`, `init` (facet mesh, intrinsic insertion, laws, mass),
/// `stable_time_step`, `solve_step` (bulk and cohesive internal forces,
/// cohesive law with damage), `energies`.
///
/// # Errors
/// Model errors.
pub fn intrinsic(ctx: &mut Ctx<'_>, law: &str) -> Result<Outcome> {
    let n = ctx.pick(3, 20, 40);
    let steps = ctx.pick(4, 300, 300);
    let dat = format!(
        "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = \
         [[0.4, 0.6], [-1, 2], [-1, 2]]\n  ]\n{INTRINSIC_BULK}{law}]\n"
    );
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = model(mesh, &dat)?;
        m.init_full(
            &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            false,
        )?;
        Ok(m)
    })?;
    record_counts(ctx, &m)?;
    let mut dt = 0.0;
    ctx.repeat("stable_time_step", ctx.pick(1, 5, 5), |_| {
        dt = m.stable_time_step()?;
        Ok(())
    })?;
    m.set_time_step(0.8 * dt, None)?;
    let total = crate::suite::WARMUP_CALLS + steps;
    let rate = 5e-3 / (total as Real * 0.8 * dt);
    let mut ramp = Ramp::new(&mut m, 0.0, 1.0, rate)?;
    ctx.repeat_primary("solve_step", steps, |_| {
        ramp.increment(&mut m)?;
        m.solve_step(None)
    })?;
    ctx.repeat("energies", (steps / 5).max(1), |_| energies(&mut m))?;
    ctx.set_digest(&common::digest(&m.smm)?);
    Ok(Outcome::Done)
}

/// Intrinsic, bilinear law (the parallel intrinsic test's).
///
/// # Errors
/// Model errors.
pub fn intrinsic_bilinear(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    intrinsic(
        ctx,
        "  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n    beta = 1.5\n    \
         G_c = 1\n    delta_0 = 0.1\n  ]\n",
    )
}

/// Intrinsic, exponential law with the exponential contact penalty.
///
/// # Errors
/// Model errors.
pub fn intrinsic_exponential(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    intrinsic(
        ctx,
        "  material cohesive_exponential [\n    name = cohesive\n    sigma_c = 1\n    beta = 1\n    \
         delta_c = 0.1\n    exponential_penalty = true\n    contact_tangent = 1.0\n  ]\n",
    )
}

/// Implicit static intrinsic case (the bilinear implicit cohesive probe's
/// set-up, fixtures/model/cohesive/implicit_static_bilinear_triangle_6, on
/// a structured mesh): triangle_6 on `[0, 1] × [0, 2]`, cohesive elements
/// on the line `y = 1`, the top edge pulled in increments of `3/200`,
/// Newton–Raphson on the residual (threshold `1e-5`), Eigen's SparseLU
/// translation.
///
/// Phases: `mesh`, `init`, `solve_step` (Newton iterations with the bulk
/// and cohesive tangents, factorisations and solves). Extra field:
/// `newton_iterations`.
///
/// # Errors
/// Model and solver errors.
pub fn intrinsic_implicit(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(3, 30, 60);
    let steps = ctx.pick(3, 30, 30);
    let h = 1.0 / n as Real;
    let dat = format!(
        "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = \
         [[-10, 10], [{}, {}]]\n  ]\n  material elastic [\n    name = steel\n    rho = 1\n    \
         E = 6\n    nu = 0.0\n  ]\n  material cohesive_bilinear [\n    name = cohesive\n    \
         sigma_c = 1\n    G_c = 0.25\n    delta_0 = 0.01\n    penalty = 10\n  ]\n]\n",
        1.0 - 0.25 * h,
        1.0 + 0.25 * h
    );
    let spec = BoxSpec {
        ty: ElementType::Triangle6,
        n: [n, 2 * n, 1],
        lo: [0.0; 3],
        hi: [1.0, 2.0, 0.0],
    };
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = model(mesh, &dat)?;
        let opts =
            ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
                non_linear_solver_type: Some(NonLinearSolverType::Auto),
                sparse_solver_type: Some(SparseSolverType::Eigen),
                ..ModelSolverOptions::default()
            });
        m.init_full(&opts, false)?;
        Ok(m)
    })?;
    record_counts(ctx, &m)?;
    {
        let nls = m.smm.solver.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(100))?;
        nls.set_param("threshold", ParamValue::Real(1e-5))?;
        nls.set_param("convergence_type", ParamValue::Enum("residual".into()))?;
    }
    let x = common::coordinates(&m.smm)?;
    let tol = 1e-9;
    let mut top = Vec::new();
    {
        let mut b = m.smm.blocked_dofs_mut()?;
        let b = b.as_mut_slice();
        for (i, p) in x.as_chunks::<2>().0.iter().enumerate() {
            if p[1].abs() < tol {
                b[2 * i + 1] = true;
            }
            if p[0].abs() < tol && p[1] < 1.1 {
                b[2 * i] = true;
            }
            if (p[0] - 1.).abs() < tol && (p[1] - 1.).abs() < tol {
                b[2 * i] = true;
            }
            if (p[1] - 2.).abs() < tol {
                b[2 * i + 1] = true;
                top.push(2 * i + 1);
            }
        }
    }
    let increment = 3. / 200.;
    let mut iterations = 0usize;
    ctx.repeat_primary("solve_step", steps, |timed| {
        {
            let mut u = m.smm.displacement_mut()?;
            let u = u.as_mut_slice();
            for &k in &top {
                u[k] += increment;
            }
        }
        m.solve_step(None)?;
        if timed {
            iterations += m.smm.solver.non_linear_solver(None)?.nb_iterations();
        }
        Ok(())
    })?;
    ctx.extra("newton_iterations", Extra::Int(iterations as i64));
    ctx.set_digest(&common::digest(&m.smm)?);
    Ok(Outcome::Done)
}

/// The cohesive scalability benchmark's laws (finite-deformation elastic
/// bulk, extrinsic `cohesive_linear`), every facet checked; `sigma_c` may
/// carry a random part.
#[must_use]
pub fn benchmark_dat(sigma_c: &str) -> String {
    format!(
        "model solid_mechanics_model_cohesive [\n  material elastic [\n    name = body\n    \
         rho = 2500\n    E = 70e9\n    nu = 0.22\n    finite_deformation = true\n  ]\n  \
         material cohesive_linear [\n    name = insertion\n    beta = 3\n    G_c = 7.6\n    \
         sigma_c = {sigma_c}\n    delta_c = 0.217e-6\n  ]\n  cohesive_inserter [\n    \
         bounding_box = [[-1., 2.], [-1., 2.], [-1., 2.]]\n  ]\n]\n"
    )
}

/// Extrinsic case without insertion (`cohesive_step_timing`'s set-up on a
/// generated cube): every interior facet checked, `Δt = 0.1 Δt_crit`, the
/// velocity ramp `v_z = ε̇ z` reaching 30 % of `σ_c` at the end of the run.
///
/// Phases: `mesh`, `init` (facet mesh, extrinsic set-up), `check_insertion`
/// (`check_cohesive_stress`: stress interpolation to the facets and the
/// insertion criterion), `solve_step`.
///
/// # Errors
/// Model errors; an insertion (the case measures the pre-insertion cost).
pub fn extrinsic_check(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(3, 24, 48);
    let steps = ctx.pick(3, 100, 100);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = model(mesh, &benchmark_dat("70e6"))?;
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)?;
        Ok(m)
    })?;
    record_counts(ctx, &m)?;
    let dt = 0.1 * m.stable_time_step()?;
    m.set_time_step(dt, None)?;
    let total = crate::suite::WARMUP_CALLS + steps;
    let rate = 0.3 * 70e6 / 70e9 / (total as Real * dt);
    let x = common::coordinates(&m.smm)?;
    {
        let mut v = m.smm.velocity_mut()?;
        for (vn, p) in v
            .as_mut_slice()
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(x.as_chunks::<3>().0)
        {
            vn[2] = rate * p[2];
        }
    }
    let mut inserted = 0;
    ctx.repeat("check_insertion", steps, |_| {
        inserted += m.check_cohesive_stress()?;
        Ok(())
    })?;
    ctx.repeat_primary("solve_step", steps, |_| m.solve_step(None))?;
    inserted += m.check_cohesive_stress()?;
    if inserted != 0 {
        return Err(Error::invalid(format!(
            "extrinsic_check: {inserted} cohesive elements inserted"
        )));
    }
    ctx.set_digest(&common::digest(&m.smm)?);
    Ok(Outcome::Done)
}

/// One step of an extrinsic run: insertion check, then the solve; returns
/// (inserted, check seconds, solve seconds).
fn extrinsic_step(
    m: &mut SolidMechanicsModelCohesive,
    ramp: &mut Ramp,
) -> Result<(usize, f64, f64)> {
    ramp.increment(m)?;
    let t0 = Instant::now();
    let k = m.check_cohesive_stress()?;
    let t1 = Instant::now();
    m.solve_step(None)?;
    Ok((k, (t1 - t0).as_secs_f64(), t1.elapsed().as_secs_f64()))
}

/// Configuration of the extrinsic insertion run (serial and parallel).
#[derive(Clone, Copy, Debug)]
pub struct InsertionRun {
    /// Cells per axis at sizes s, m, l.
    pub n: [usize; 3],
    /// Steps before the timed loop.
    pub preload: [usize; 3],
    /// Imposed strain at the end of the preload (`1e-3` is the lowest
    /// strength in a static stretch; the dynamic overshoot breaks facets
    /// earlier).
    pub preload_strain: Real,
    /// Timed steps.
    pub steps: [usize; 3],
}

/// The serial insertion run's configuration.
pub const INSERTION: InsertionRun = InsertionRun {
    n: [3, 16, 32],
    preload: [4, 60, 60],
    preload_strain: 1e-3,
    steps: [3, 12, 12],
};

/// Run the extrinsic insertion case on a mesh (serial or distributed):
/// the benchmark's laws with `sigma_c = 70e6 uniform [0, 14e6]` (seeded),
/// `Δt = 0.1 Δt_crit`, a stretch ramp along x that reaches the lowest
/// strength (`ε = 1e-3`) at the end of the preload, so that facets break
/// progressively during the timed steps.
///
/// Phases: `init`, `preload` (steps with checks), `step` (check and solve)
/// split into `check_insertion` (stress interpolation, criterion, the
/// insertion itself: facet doubling, mesh events, model updates) and
/// `solve_step`. Extra fields: `inserted` (timed steps), `inserted_preload`
/// (preload and warm-up), `inserted_per_step`.
///
/// # Errors
/// Model errors.
pub fn insertion_run(
    ctx: &mut Ctx<'_>,
    cfg: &InsertionRun,
    mesh: Mesh,
) -> Result<SolidMechanicsModelCohesive> {
    let preload = ctx.pick(cfg.preload[0], cfg.preload[1], cfg.preload[2]);
    let steps = ctx.pick(cfg.steps[0], cfg.steps[1], cfg.steps[2]);
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(SEED);
    let mut m = ctx.once("init", || {
        let mut m = model(mesh, &benchmark_dat("70e6 uniform [0, 14e6]"))?;
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)?;
        Ok(m)
    })?;
    let mut dt = [m.stable_time_step()?];
    ctx.comm().all_reduce(&mut dt, akantu_comm::ReduceOp::Min)?;
    let dt = 0.1 * dt[0];
    m.set_time_step(dt, None)?;
    let rate = cfg.preload_strain / (preload as Real * dt);
    let mut ramp = Ramp::new(&mut m, 0.0, 1.0, rate)?;
    let mut inserted_preload = 0;
    let mut trace: Vec<String> = Vec::new();
    ctx.once("preload", || {
        for _ in 0..preload {
            let k = extrinsic_step(&mut m, &mut ramp)?.0;
            inserted_preload += k;
            trace.push(k.to_string());
        }
        Ok(())
    })?;
    let (mut inserted, mut check_s, mut solve_s) = (0, 0.0, 0.0);
    let calls = ctx.repeat_primary("step", steps, |timed| {
        let (k, c, s) = extrinsic_step(&mut m, &mut ramp)?;
        trace.push(k.to_string());
        if timed {
            inserted += k;
            check_s += c;
            solve_s += s;
        } else {
            inserted_preload += k;
        }
        Ok(())
    })?;
    ctx.record("check_insertion", calls, check_s);
    ctx.record("solve_step", calls, solve_s);
    ctx.extra("inserted", Extra::Int(inserted as i64));
    ctx.extra("inserted_preload", Extra::Int(inserted_preload as i64));
    // insertions per step: preload, warm-up, timed steps
    ctx.extra("inserted_per_step", Extra::Text(trace.join(" ")));
    Ok(m)
}

/// Extrinsic case with insertions (serial).
///
/// # Errors
/// Model errors.
pub fn extrinsic_insertion(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    let n = ctx.pick(INSERTION.n[0], INSERTION.n[1], INSERTION.n[2]);
    let spec = BoxSpec::unit(ElementType::Tetrahedron4, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let m = insertion_run(ctx, &INSERTION, mesh)?;
    record_counts(ctx, &m)?;
    ctx.set_digest(&common::digest(&m.smm)?);
    Ok(Outcome::Done)
}
