// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Explicit-dynamics
// cases of the benchmark suite; the elastic set-up follows
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc (as
// crates/akantu-solid/examples/explicit_step_timing.rs), the laws' inputs
// test/test_model/test_solid_mechanics_model/test_materials/*.dat.

//! Explicit central-difference cases: the elastic step per element type
//! (small strain and finite deformation), the other constitutive laws,
//! several laws in one mesh and non-local damage.

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real, Result};
use akantu_solid::SolidMechanicsModel;

use super::common::{self, SlabSelector};
use super::mesh::{BoxSpec, box_mesh};
use crate::suite::{Ctx, Outcome};

/// Initial condition of an explicit case.
#[derive(Clone, Copy, Debug)]
pub enum Load {
    /// A sine–Gaussian displacement pulse ([`common::apply_pulse`], free
    /// boundaries).
    Pulse,
    /// A uniaxial stretch ramp along x: the x displacement of the faces
    /// `x = 0` and `x = 1` blocked and moved every step, the initial
    /// velocity `v_x = ε̇ (x − ½)`, `ε̇` chosen so that the strain reaches
    /// the given value after the warm-up and timed steps (the laws are in
    /// their inelastic regime for most of the run).
    Ramp(Real),
}

/// An explicit case.
#[derive(Clone, Debug)]
pub struct Explicit {
    /// Element type.
    pub ty: ElementType,
    /// Cells per axis at sizes s, m, l.
    pub n: [usize; 3],
    /// Steps of the timed loop at sizes s, m, l.
    pub steps: [usize; 3],
    /// Material file.
    pub dat: String,
    /// Initial condition.
    pub load: Load,
    /// Law selector (several laws by slabs of x).
    pub slabs: Option<usize>,
    /// Time the non-local neighbourhood rebuild.
    pub non_local: bool,
}

/// Boundary motion of [`Load::Ramp`].
struct Ramp {
    dofs: Vec<usize>,
    du: Vec<Real>,
}

fn setup_ramp(m: &mut SolidMechanicsModel, strain: Real, total_steps: usize) -> Result<Ramp> {
    let x = common::coordinates(m)?;
    let d = m.spatial_dimension();
    let dt = m.time_step(None)?;
    let rate = strain / (total_steps as Real * dt);
    let tol = 1e-9;
    let mut dofs = Vec::new();
    let mut du = Vec::new();
    {
        let mut b = m.blocked_dofs_mut()?;
        let b = b.as_mut_slice();
        for (n, p) in x.chunks_exact(d).enumerate() {
            if p[0].abs() <= tol || (p[0] - 1.0).abs() <= tol {
                b[n * d] = true;
                dofs.push(n * d);
                du.push(rate * dt * (p[0] - 0.5));
            }
        }
    }
    let mut v = m.velocity_mut()?;
    for (vn, p) in v.as_mut_slice().chunks_exact_mut(d).zip(x.chunks_exact(d)) {
        vn[0] = rate * (p[0] - 0.5);
    }
    Ok(Ramp { dofs, du })
}

fn ramp_increment(m: &mut SolidMechanicsModel, r: &Ramp) -> Result<()> {
    let mut u = m.displacement_mut()?;
    let u = u.as_mut_slice();
    for (&k, &du) in r.dofs.iter().zip(&r.du) {
        u[k] += du;
    }
    Ok(())
}

/// Run an explicit case.
///
/// Phases: `mesh`, `init` (`init_full`: FE engines, shape functions,
/// laws, lumped mass), `stable_time_step`, `solve_step` with its
/// sub-phases (`predictor`, `residual` = internal forces and the residual
/// assembly, `corrector`, `step_hooks`, `step_rest` = the lumped solve and
/// the solver's bookkeeping), `internal_forces`
/// (`assemble_internal_forces`), `stresses` (every law's
/// `compute_all_stresses`), `energies` (kinetic and potential),
/// `nonlocal_rebuild` (non-local cases).
///
/// # Errors
/// Model errors.
pub fn run(ctx: &mut Ctx<'_>, c: &Explicit) -> Result<Outcome> {
    let n = ctx.pick(c.n[0], c.n[1], c.n[2]);
    let steps = ctx.pick(c.steps[0], c.steps[1], c.steps[2]);
    let spec = BoxSpec::unit(c.ty, n);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init", || {
        let mut m = common::solid_model(mesh, &c.dat)?;
        if let Some(nb) = c.slabs {
            m.set_material_selector(Box::new(SlabSelector {
                lo: 0.0,
                hi: 1.0,
                nb,
            }));
        }
        common::init(&mut m, AnalysisMethod::ExplicitLumpedMass)?;
        Ok(m)
    })?;
    common::set_counts(ctx, &m)?;
    let small = ctx.pick(1, 5, 5);
    let mut dt = 0.0;
    ctx.repeat("stable_time_step", small, |_| {
        dt = m.stable_time_step()?;
        Ok(())
    })?;
    m.set_time_step(0.8 * dt, None)?;
    let ramp = match c.load {
        Load::Pulse => {
            common::apply_pulse(&mut m, 0.0, 1.0)?;
            None
        }
        Load::Ramp(strain) => Some(setup_ramp(
            &mut m,
            strain,
            crate::suite::WARMUP_CALLS + steps,
        )?),
    };
    common::timed_steps(ctx, &mut m, steps, |m| match &ramp {
        Some(r) => ramp_increment(m, r),
        None => Ok(()),
    })?;
    let k = (steps / 5).max(1);
    ctx.repeat("internal_forces", k, |_| m.assemble_internal_forces())?;
    ctx.repeat("stresses", k, |_| common::compute_all_stresses(&mut m))?;
    let has_potential = m.energy("potential").is_ok();
    ctx.repeat("energies", k, |_| {
        std::hint::black_box(m.kinetic_energy()?);
        if has_potential {
            std::hint::black_box(m.energy("potential")?);
        }
        Ok(())
    })?;
    if c.non_local {
        ctx.repeat("nonlocal_rebuild", ctx.pick(1, 2, 2), |_| {
            m.reinitialize_non_local()
        })?;
    }
    ctx.set_digest(&common::digest(&m)?);
    Ok(Outcome::Done)
}

fn law(keyword: &str, params: &str) -> String {
    format!(
        "material {keyword} [\n  name = body\n  rho = {}\n{params}]\n",
        common::RHO
    )
}

fn elastic_params(extra: &str) -> String {
    format!("  E = {}\n  nu = {}\n{extra}", common::YOUNG, common::NU)
}

/// Linear isotropic hardening plasticity (yield at ε ≈ 7e-4).
#[must_use]
pub fn plastic_dat() -> String {
    law(
        "plastic_linear_isotropic_hardening",
        &elastic_params("  h = 5e9\n  sigma_y = 50e6\n"),
    )
}

/// Marigo damage (`Y = ½ ε:C:ε` above `Yd` from ε ≈ 5e-4).
#[must_use]
pub fn marigo_dat() -> String {
    law("marigo", &elastic_params("  Yd = 1e4\n  Sd = 1e5\n"))
}

/// Mazars damage (the upstream test's parameters, threshold `K0 = 1e-4`).
#[must_use]
pub fn mazars_dat() -> String {
    law(
        "mazars",
        &elastic_params(
            "  K0 = 1e-4\n  At = 1.15\n  Bt = 10000\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.0\n",
        ),
    )
}

/// The explicit elastic cases (four types, small strain and finite
/// deformation).
pub mod elastic {
    use super::*;

    macro_rules! elastic_case {
        ($fn:ident, $ty:expr, $n:expr, $steps:expr, $fd:expr) => {
            /// See [`super::run`].
            ///
            /// # Errors
            /// Model errors.
            pub fn $fn(ctx: &mut Ctx<'_>) -> Result<Outcome> {
                super::run(
                    ctx,
                    &Explicit {
                        ty: $ty,
                        n: $n,
                        steps: $steps,
                        dat: common::elastic_dat($fd),
                        load: Load::Pulse,
                        slabs: None,
                        non_local: false,
                    },
                )
            }
        };
    }

    elastic_case!(
        tet4_ss,
        ElementType::Tetrahedron4,
        [3, 30, 60],
        [4, 200, 200],
        false
    );
    elastic_case!(
        tet4_fd,
        ElementType::Tetrahedron4,
        [3, 30, 60],
        [4, 150, 150],
        true
    );
    elastic_case!(
        tet10_ss,
        ElementType::Tetrahedron10,
        [2, 26, 52],
        [4, 50, 50],
        false
    );
    elastic_case!(
        tet10_fd,
        ElementType::Tetrahedron10,
        [2, 26, 52],
        [4, 40, 40],
        true
    );
    elastic_case!(
        hex8_ss,
        ElementType::Hexahedron8,
        [3, 48, 96],
        [4, 50, 50],
        false
    );
    elastic_case!(
        hex8_fd,
        ElementType::Hexahedron8,
        [3, 48, 96],
        [4, 30, 30],
        true
    );
    elastic_case!(
        tri3_ss,
        ElementType::Triangle3,
        [6, 300, 850],
        [4, 300, 300],
        false
    );
    elastic_case!(
        tri3_fd,
        ElementType::Triangle3,
        [6, 300, 850],
        [4, 200, 200],
        true
    );
}

/// The other constitutive laws on tetrahedra under the stretch ramp.
pub mod laws {
    use super::*;

    const TET_N: [usize; 3] = [3, 26, 52];

    fn ramp_case(ctx: &mut Ctx<'_>, dat: String, strain: Real, steps: usize) -> Result<Outcome> {
        super::run(
            ctx,
            &Explicit {
                ty: ElementType::Tetrahedron4,
                n: TET_N,
                steps: [4, steps, steps],
                dat,
                load: Load::Ramp(strain),
                slabs: None,
                non_local: false,
            },
        )
    }

    /// Plasticity with linear isotropic hardening.
    ///
    /// # Errors
    /// Model errors.
    pub fn plastic(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        ramp_case(ctx, plastic_dat(), 3e-3, 120)
    }

    /// Marigo damage.
    ///
    /// # Errors
    /// Model errors.
    pub fn marigo(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        ramp_case(ctx, marigo_dat(), 3e-3, 120)
    }

    /// Mazars damage.
    ///
    /// # Errors
    /// Model errors.
    pub fn mazars(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        ramp_case(ctx, mazars_dat(), 1e-3, 120)
    }

    /// Standard linear solid (deviatoric viscoelasticity).
    ///
    /// # Errors
    /// Model errors.
    pub fn sls(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let dat = law(
            "sls_deviatoric",
            &elastic_params("  Eta = 1e3\n  Ev = 20e9\n"),
        );
        ramp_case(ctx, dat, 1e-3, 120)
    }

    /// Generalised Maxwell viscoelasticity (two branches).
    ///
    /// # Errors
    /// Model errors.
    pub fn maxwell(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let dat = law(
            "viscoelastic_maxwell",
            &format!(
                "  Einf = {}\n  nu = {}\n  Eta = [1e3, 1e4]\n  Ev = [20e9, 10e9]\n",
                common::YOUNG,
                common::NU
            ),
        );
        ramp_case(ctx, dat, 1e-3, 120)
    }

    /// Neo-Hookean hyperelasticity (finite deformation).
    ///
    /// # Errors
    /// Model errors.
    pub fn neohookean(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        ramp_case(ctx, law("neohookean", &elastic_params("")), 1e-2, 100)
    }

    /// Orthotropic elasticity (rotated axes).
    ///
    /// # Errors
    /// Model errors.
    pub fn orthotropic(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let dat = law(
            "elastic_orthotropic",
            "  n1 = [0.6, 0.8, 0]\n  n2 = [-0.8, 0.6, 0]\n  n3 = [0, 0, 1]\n  E1 = 140e9\n  \
             E2 = 10e9\n  E3 = 10e9\n  nu12 = 0.3\n  nu13 = 0.3\n  nu23 = 0.4\n  G12 = 5e9\n  \
             G13 = 5e9\n  G23 = 3.5e9\n",
        );
        ramp_case(ctx, dat, 1e-3, 150)
    }

    /// Isotropic elasticity with a thermal strain (`alpha`, `delta_T`).
    ///
    /// # Errors
    /// Model errors.
    pub fn thermal(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let dat = law(
            "elastic",
            &elastic_params("  alpha = 1e-5\n  delta_T = 50\n"),
        );
        ramp_case(ctx, dat, 1e-3, 150)
    }

    /// Three laws in one mesh (elastic, plastic, Mazars) by slabs of x:
    /// the material selector and the per-law element filters.
    ///
    /// # Errors
    /// Model errors.
    pub fn multi_material(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let dat = format!(
            "{}{}{}",
            common::elastic_dat(false),
            plastic_dat().replace("name = body", "name = plastic"),
            mazars_dat().replace("name = body", "name = mazars")
        );
        super::run(
            ctx,
            &Explicit {
                ty: ElementType::Tetrahedron4,
                n: TET_N,
                steps: [4, 120, 120],
                dat,
                load: Load::Ramp(3e-3),
                slabs: Some(3),
                non_local: false,
            },
        )
    }

    /// Non-local Mazars damage (`base_wf`, radius 2 cells): the
    /// neighbourhood build in `init`, its rebuild, and the per-step
    /// averaging inside the step.
    ///
    /// # Errors
    /// Model errors.
    pub fn mazars_non_local(ctx: &mut Ctx<'_>) -> Result<Outcome> {
        let n = ctx.pick(4, 20, 40);
        let radius = 2.0 / n as Real;
        let dat = format!(
            "{}non_local body base_wf [\n  radius = {radius}\n]\n",
            mazars_dat().replace("material mazars [", "material mazars_non_local [")
        );
        super::run(
            ctx,
            &Explicit {
                ty: ElementType::Tetrahedron4,
                n: [4, 20, 40],
                steps: [4, 60, 60],
                dat,
                load: Load::Ramp(1e-3),
                slabs: None,
                non_local: true,
            },
        )
    }
}
