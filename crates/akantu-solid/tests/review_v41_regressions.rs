// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests:
// material assignment scope
// (upstream constitutive_laws_handler{.hh,_tmpl.hh},
// mesh_iterators.hh), shape validation of user-resizable arrays,
// the stiffness gradient with an eigenstrain (material.cc:262-312
// vs:128-155), initNewSolver's method order (model.cc:57-83),
// extrapolateInternal (material.cc:771-807), the per-element energy
// dispatch (material.cc:652-660), assembleResidual accumulation
// (solid_mechanics_model.cc:245-258) and node removal
// (dof_manager.cc:901-902; implemented by SMM-015).

mod model_common;

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;
use std::sync::Arc;

use akantu_core::enums::{AnalysisMethod, IntegrationSchemeType, NonLinearSolverType};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Real, Result};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, ElementDataSelector, LawContext, LawSpec, ModelServices,
    SelectorContext,
};
use akantu_parser::Parser;
use akantu_parser::param::ParameterAccess;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SmmUpstreamCompat, SolidMechanicsModel,
};
use akantu_solver::SolutionType;
use akantu_validation::fixtures_root;
use model_common::*;

const TRI: ElementType = ElementType::Triangle3;
const SEG: ElementType = ElementType::Segment2;
const NG: GhostType = GhostType::NotGhost;

/// Frozen upstream mesh `test/test_model/patch_tests/data/bar_<name>.msh`:
/// a 10 × 1 bar whose **main mesh** holds the boundary segments of the
/// physical group `BC` next to the triangles of `bulk`.
fn bar(name: &str) -> Mesh {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join(format!("bar_{name}.msh"));
    read_mesh(&f, 2)
}

const TWO: &str = "material elastic [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n\
                   material elastic [\n  name = other\n  rho = 2\n  E = 3\n  nu = 0.2\n]\n";

fn model(dat: &str) -> SolidMechanicsModel {
    SolidMechanicsModel::with_parser(
        SharedMesh::new(bar("triangle_3")),
        None,
        "smm",
        Parser::parse_str(dat, "m.dat").unwrap(),
    )
    .unwrap()
}

fn initialised(method: AnalysisMethod) -> SolidMechanicsModel {
    let mut m = model(TWO);
    m.init_full(&ModelOptions::new(method)).unwrap();
    m
}

fn nb_nodes(m: &SolidMechanicsModel) -> usize {
    m.physics.base.mesh.read().unwrap().nb_nodes()
}

fn block_x0(m: &mut SolidMechanicsModel) {
    let x = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let mut b = m.blocked_dofs_mut().unwrap();
    let b = b.as_mut_slice();
    for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
        if p[0].abs() < 1e-12 {
            b[2 * n..2 * n + 2].fill(true);
        }
    }
}

fn is_dimension_mismatch(r: &Result<impl std::fmt::Debug>) -> bool {
    matches!(r, Err(Error::DimensionMismatch { .. }))
}

/* ---------------------------------------------------------------------- */
/* `sm01_*`: materials only on elements of the mesh's spatial dimension    */
/* ---------------------------------------------------------------------- */

/// Selector that refuses boundary elements (upstream never presents them,
/// e.g. the reassign test's barycentre map of the volume elements).
#[derive(Debug)]
struct VolumeOnly;

impl ConstitutiveLawSelector for VolumeOnly {
    fn select(&self, _ctx: &SelectorContext<'_>, e: Element) -> Result<Option<usize>> {
        if e.ty().spatial_dimension() != 2 {
            return Err(Error::invalid(format!("selector called on {e}")));
        }
        Ok(Some(e.idx() % 2))
    }
}

#[test]
fn sm01_boundary_elements_get_no_material() {
    let mut m = model(TWO);
    assert!(m.physics.base.mesh.read().unwrap().nb_element(SEG, NG) > 0);
    m.set_material_selector(Box::new(VolumeOnly));
    // before the fix: the selector is asked about the 38 segments → error
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    // upstream `constitutive_law_index`: types of the mesh dimension only
    assert!(!m.material_by_element().exists(SEG, NG));
    assert!(!m.material_local_numbering().exists(SEG, NG));
    assert!(!m.material_by_element().exists(SEG, GhostType::Ghost));
    let nt = m.physics.base.mesh.read().unwrap().nb_element(TRI, NG);
    assert_eq!(m.material_by_element().get(TRI, NG).unwrap().len(), nt);
    let mut assigned = 0;
    for i in 0..m.nb_materials() {
        let law = m.material(i).unwrap();
        for (ty, _, a) in law.base().element_filter.as_map().iter_all() {
            assert_eq!(ty, TRI, "law {i}: filter block of {ty}");
            assigned += a.len();
        }
        for (name, f) in law.base().internals() {
            for (ty, _, _) in f.values().as_map().iter_all() {
                assert_eq!(ty, TRI, "law {i}: internal {name} has a {ty} block");
            }
        }
    }
    assert_eq!(assigned, nt);
    // upstream getMaterialByElement on a boundary element: no entry
    assert!(m.material_of(Element::new(SEG, 0, NG)).is_err());
    // the model works on the volume elements, the selector keeps being
    // asked about volume elements only
    m.reassign_material().unwrap();
    m.solve_step(None).unwrap();
    assert!(m.stable_time_step().unwrap() > 0.0);
    m.energy("kinetic").unwrap();
    m.energy("potential").unwrap();
}

/// The physical-name selector (`MeshDataMaterialSelector<std::string>`):
/// the boundary group `BC` has no law of that name, and upstream never
/// asks for it.
#[test]
fn sm01_physical_name_selector_ignores_boundary_groups() {
    let dat = "material elastic [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";
    let mut m = model(dat);
    m.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    assert!(!m.material_by_element().exists(SEG, NG));
    let law = m.material(0).unwrap();
    assert!(!law.base().element_filter.exists(SEG, NG));
    assert!(!law.base().grad_u.exists(SEG, NG));
}

/* ---------------------------------------------------------------------- */
/* `sm02_*`: no panic on user-resized arrays                               */
/* ---------------------------------------------------------------------- */

#[test]
fn sm02_short_velocity_kinetic_energy_is_an_error() {
    let mut m = initialised(AnalysisMethod::ExplicitLumpedMass);
    m.solve_step(None).unwrap();
    m.velocity_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 4], 2).unwrap());
    // before the fix: panic at energy.rs:167
    assert!(is_dimension_mismatch(&m.energy("kinetic")));
    assert!(is_dimension_mismatch(
        &m.kinetic_energy_of_element(Element::new(TRI, 0, NG))
    ));
}

#[test]
fn sm02_short_external_force_external_work_is_an_error() {
    let mut m = initialised(AnalysisMethod::ExplicitLumpedMass);
    m.solve_step(None).unwrap();
    m.external_force_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 4], 2).unwrap());
    // before the fix: panic at energy.rs:243
    assert!(is_dimension_mismatch(&m.energy("external work")));
}

#[test]
fn sm02_short_external_force_in_solve_step_is_an_error() {
    for method in [
        AnalysisMethod::ExplicitLumpedMass,
        AnalysisMethod::Static,
        AnalysisMethod::ImplicitDynamic,
    ] {
        let mut m = initialised(method);
        block_x0(&mut m);
        if method != AnalysisMethod::Static {
            let dt = 0.5 * m.stable_time_step().unwrap();
            m.set_time_step(dt, None).unwrap();
        }
        m.solve_step(None).unwrap();
        m.external_force_mut()
            .unwrap()
            .replace(Array::from_vec(vec![0.0; 4], 2).unwrap());
        // before the fix: panic at energy.rs:85 (external_work_increment inside
        // assemble_residual)
        let r = m.solve_step(None);
        assert!(is_dimension_mismatch(&r), "{method}: {r:?}");
    }
}

#[test]
fn sm02_wrong_component_count_is_an_error() {
    let mut m = initialised(AnalysisMethod::ExplicitLumpedMass);
    m.solve_step(None).unwrap();
    let nn = nb_nodes(&m);
    // same number of scalars, one component per tuple
    m.velocity_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 2 * nn], 1).unwrap());
    assert!(m.energy("kinetic").is_err());
    m.velocity_mut().unwrap().replace(Array::zeros(nn, 2));
    m.internal_force_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 2 * nn], 1).unwrap());
    assert!(is_dimension_mismatch(&m.energy("external work")));
    assert!(m.solve_step(None).is_err());
}

#[test]
fn sm02_consistent_kinetic_energy_short_velocity_is_an_error() {
    let mut m = initialised(AnalysisMethod::ExplicitConsistentMass);
    block_x0(&mut m);
    let dt = 0.5 * m.stable_time_step().unwrap();
    m.set_time_step(dt, None).unwrap();
    m.solve_step(None).unwrap();
    m.velocity_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 6], 2).unwrap());
    assert!(is_dimension_mismatch(&m.energy("kinetic")));
}

#[test]
fn sm02_resized_material_map_stable_time_step_is_an_error() {
    let mut m = initialised(AnalysisMethod::ExplicitLumpedMass);
    m.material_by_element_mut()
        .get_mut(TRI, NG)
        .unwrap()
        .try_resize(3, 0)
        .unwrap();
    // before the fix: panic in stable_time_step_local (`idx.as_slice()[e]`)
    assert!(is_dimension_mismatch(&m.stable_time_step()));
}

#[test]
fn sm02_short_displacement_assembly_is_an_error() {
    let mut m = initialised(AnalysisMethod::Static);
    m.displacement_mut()
        .unwrap()
        .replace(Array::from_vec(vec![0.0; 4], 2).unwrap());
    assert!(is_dimension_mismatch(&m.assemble_internal_forces()));
    assert!(is_dimension_mismatch(&m.assemble_stiffness_matrix(true)));
    assert!(is_dimension_mismatch(&m.assemble_residual()));
}

/* ---------------------------------------------------------------------- */
/* `sm03_*`: stiffness gradient with an eigenstrain                        */
/* ---------------------------------------------------------------------- */

/// Incremental elastic law with a tangent assembled in every Newton
/// iteration (base `hasStiffnessMatrixChanged = true`): the elastic
/// predictor of upstream's linear isotropic hardening,
/// `σ = σ_prev + C : sym(grad_u − previous_grad_u)`
/// (`material_linear_isotropic_hardening_inline_impl.hh:39-40`), plane
/// strain. A `previous_grad_u` saved from a stiffness assembly that ignored
/// `ε⁰` shifts every later increment by `ε⁰`.
#[derive(Debug, Default)]
struct IncrementalElastic {
    lambda: Real,
    mu: Real,
}

impl ForeignMaterialMethods for IncrementalElastic {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        let e = law.state.params.get_real("E")?;
        let nu = law.state.params.get_real("nu")?;
        self.lambda = nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu));
        self.mu = e / (2.0 * (1.0 + nu));
        law.base_init_material()
    }

    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        let base = &mut law.state.base;
        let gu = base.grad_u.get(ty, g)?.clone();
        let gu_prev = base.grad_u.previous(ty, g)?.clone();
        let s_prev = base.stress.previous(ty, g)?.clone();
        let mut s = base.stress.get_mut(ty, g)?;
        for (((s, gu), gp), sp) in s
            .tuples_mut()
            .zip(gu.tuples())
            .zip(gu_prev.tuples())
            .zip(s_prev.tuples())
        {
            let d = |k: usize| gu[k] - gp[k];
            let (exx, eyy, exy) = (d(0), d(3), 0.5 * (d(1) + d(2)));
            let tr = exx + eyy;
            s[0] = sp[0] + self.lambda * tr + 2.0 * self.mu * exx;
            s[3] = sp[3] + self.lambda * tr + 2.0 * self.mu * eyy;
            s[1] = sp[1] + 2.0 * self.mu * exy;
            s[2] = sp[2] + 2.0 * self.mu * exy;
        }
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _law: &mut ForeignLaw<'_, '_>,
        _ty: ElementType,
        _g: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let (l, m) = (self.lambda, self.mu);
        let c = [l + 2.0 * m, l, 0.0, l, l + 2.0 * m, 0.0, 0.0, 0.0, m];
        for t in tangent.as_mut_slice().as_chunks_mut::<9>().0 {
            t.copy_from_slice(&c);
        }
        Ok(())
    }

    fn push_wave_speed(&self, state: &ForeignLawState, _element: Element) -> Result<Real> {
        Ok(((self.lambda + 2.0 * self.mu) / state.base.rho).sqrt())
    }
}

fn incremental_model(criterion: &str, compat: bool) -> SolidMechanicsModel {
    incremental_model_with(criterion, compat, None)
}

/// [`incremental_model`] with an explicit non-linear solver type.
fn incremental_model_with(
    criterion: &str,
    compat: bool,
    nls: Option<NonLinearSolverType>,
) -> SolidMechanicsModel {
    let dat = "material incremental_elastic [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";
    let mut m = model(dat);
    m.physics
        .factory
        .register(
            "incremental_elastic",
            Arc::new(
                |spec: &LawSpec, _model: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut law = ForeignMaterial::new(
                        spec,
                        "incremental_elastic",
                        Box::new(IncrementalElastic::default()),
                    )?;
                    let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
                    law.register_param_real("E", rp, "Young's modulus")?;
                    law.register_param_real("nu", rp, "Poisson ratio")?;
                    Ok(Box::new(law))
                },
            ),
        )
        .unwrap();
    m.physics.upstream_compat = SmmUpstreamCompat {
        stiffness_grad_u_ignores_eigen_grad_u: compat,
        ..SmmUpstreamCompat::default()
    };
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.non_linear_solver_type = nls;
    m.init_full(&options).unwrap();
    set_newton(&mut m, 20, 1e-12, criterion);
    block_x0(&mut m);
    let eig = MatrixX::from_col_major(2, 2, vec![0.01, 0.002, 0.002, -0.003]).unwrap();
    m.apply_eigen_grad_u(&eig, "bulk", NG).unwrap();
    m
}

/// Two static steps under the same eigenstrain: the second step must not
/// move (zero strain increment), whatever the convergence criterion.
fn two_steps(criterion: &str, compat: bool) -> (Vec<Real>, Vec<Real>, Vec<Real>) {
    let mut m = incremental_model(criterion, compat);
    m.solve_step(None).unwrap();
    let u1 = m.displacement().unwrap().as_slice().to_vec();
    m.solve_step(None).unwrap();
    let u2 = m.displacement().unwrap().as_slice().to_vec();
    m.assemble_internal_forces().unwrap();
    let s = m
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(TRI, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    (u1, u2, s)
}

fn max_rel(a: &[Real], b: &[Real]) -> Real {
    let scale = b
        .iter()
        .fold(0.0_f64, |m, x| m.nan_max(x.abs()))
        .max(1e-300);
    a.iter()
        .zip(b)
        .fold(0.0_f64, |m, (x, y)| m.nan_max((x - y).abs()))
        / scale
}

/// Guard (passes upstream too, i.e. in both modes): with the full
/// Newton–Raphson solver, the converged state and the next step's increment
/// do not depend on the criterion. Under `solution`, a converged step ends
/// with a residual assembly (`non_linear_solver_newton_raphson.cc:206-210`);
/// under `residual`, the in-loop residual follows every in-loop `J`: both
/// restore `grad_u = ∇u − ε⁰` before `previous_grad_u` is saved, so the
/// "increment off by ε⁰" of UD-208 does not occur on these paths (it does on
/// the modified-Newton early return:
/// `sm2_01_modified_newton_residual_early_return_shifts_the_next_step_in_compat`).
#[test]
fn sm03_converged_state_independent_of_the_criterion() {
    for compat in [false, true] {
        let (u1_sol, u2_sol, s_sol) = two_steps("solution", compat);
        let (u1_res, u2_res, s_res) = two_steps("residual", compat);
        assert!(
            u1_sol.iter().any(|x| x.abs() > 1e-4),
            "eigenstrain moves the bar"
        );
        // linear problem: the criteria agree up to round-off at every step
        assert!(max_rel(&u1_sol, &u1_res) < 1e-10, "compat = {compat}");
        assert!(
            max_rel(&u2_sol, &u2_res) < 1e-10,
            "compat = {compat}: step 2 depends on the criterion"
        );
        assert!(max_rel(&s_sol, &s_res) < 1e-10, "compat = {compat}");
        // the increment of step 2 is zero: the state does not move
        assert!(
            max_rel(&u2_sol, &u1_sol) < 1e-10,
            "compat = {compat}: step 2 moved"
        );
        assert!(max_rel(&u2_res, &u1_res) < 1e-10, "compat = {compat}");
    }
}

/// `∇u − ε⁰` of the current displacement, as `computeAllStresses` stores it.
fn mechanical_grad_u(m: &SolidMechanicsModel) -> Vec<Real> {
    let mesh = m.physics.base.mesh.read().unwrap();
    let fe = m.physics.base.fems.get("").unwrap();
    let mut gu = Array::new(4);
    fe.gradient_on_integration_points(
        &mesh,
        m.displacement().unwrap().view(),
        &mut gu,
        2,
        TRI,
        NG,
        None,
    )
    .unwrap();
    let law = m.material(0).unwrap();
    let eigen = law.base().eigen_grad_u.get(TRI, NG).unwrap();
    gu.as_slice()
        .iter()
        .zip(eigen.as_slice())
        .map(|(g, e)| g - e)
        .collect()
}

/// Modified Newton–Raphson under the `residual`
/// criterion assembles `K` before the initial convergence test
/// (`non_linear_solver_newton_raphson.cc:117-123`) and returns at that test
/// when the step is already converged (`:130-134`, no residual assembly
/// afterwards). With a law that reassembles `K` (base flag `true`), upstream
/// leaves `grad_u = ∇u` there (UD-208), so the next step's
/// `previous_grad_u` is off by `ε⁰` and the incremental law integrates a
/// spurious increment: the bar ratchets every other step (compat mode,
/// 9.97e-2 after step 3, 1.99e-1 after step 5).
/// Default mode: `grad_u = ∇u − ε⁰` after every step and no motion.
#[test]
fn sm2_01_modified_newton_residual_early_return_shifts_the_next_step_in_compat() {
    for compat in [false, true] {
        let mut m = incremental_model_with(
            "residual",
            compat,
            Some(NonLinearSolverType::NewtonRaphsonModified),
        );
        set_newton(&mut m, 20, 1e-10, "residual");
        let mut drift = Vec::new();
        let mut iterations = Vec::new();
        let mut u1 = Vec::new();
        for step in 0..5 {
            m.solve_step(None).unwrap();
            iterations.push(m.solver.non_linear_solver(None).unwrap().nb_iterations());
            let u = m.displacement().unwrap().as_slice().to_vec();
            if step == 0 {
                u1.clone_from(&u);
                assert!(
                    u.iter().any(|x| x.abs() > 1e-3),
                    "eigenstrain moves the bar"
                );
            }
            drift.push(
                u.iter()
                    .zip(&u1)
                    .fold(0.0_f64, |a, (x, y)| a.nan_max((x - y).abs())),
            );
            let gu = m
                .material(0)
                .unwrap()
                .base()
                .grad_u
                .get(TRI, NG)
                .unwrap()
                .as_slice()
                .to_vec();
            let want = mechanical_grad_u(&m);
            let eigen_xx = 0.01;
            let off: Real = gu
                .iter()
                .zip(&want)
                .fold(0.0_f64, |a, (g, w)| a.nan_max((g - w).abs()));
            if !compat {
                assert!(
                    off <= 1e-15,
                    "default, step {}: grad_u − (∇u − ε⁰) = {off:e}",
                    step + 1
                );
            } else if iterations[step] == 0 {
                // initial-test return right after the K assembly: ∇u kept
                assert!(
                    (off - eigen_xx).abs() < 1e-12,
                    "compat, step {}: |grad_u − (∇u − ε⁰)| = {off:e}, expected max |ε⁰|",
                    step + 1
                );
            }
        }
        eprintln!("material scope, compat = {compat}: iterations {iterations:?}, drift {drift:?}");
        if compat {
            // ratchets every other step (s2 returns early, s3 moves, …)
            assert_eq!(iterations[1], 0, "compat iterations {iterations:?}");
            assert!(drift[2] > 5e-2, "compat drift {drift:?}");
            assert!(drift[4] > 1.5e-1, "compat drift {drift:?}");
            assert!(drift[4] > 1.5 * drift[2], "compat drift {drift:?}");
        } else {
            for (s, d) in drift.iter().enumerate() {
                assert!(
                    *d <= 1e-12,
                    "default, step {}: drift {d:e} ({drift:?})",
                    s + 1
                );
            }
        }
    }
}

/// Nonlinear elastic law whose tangent depends on the strain:
/// `σ = λ tr(ε) I + 2μ ε + κ tr(ε)² I` (plane strain), consistent tangent
/// `C + 2κ tr(ε) I⊗I` evaluated from the stored `grad_u` — as the tangents
/// of upstream's plastic laws, which rebuild the trial state from `grad_u`
/// in `computeTangentModuli` (`material_linear_isotropic_hardening.cc`,
/// `material_drucker_prager_inline_impl.hh`).
#[derive(Debug, Default)]
struct NonlinearElastic {
    lambda: Real,
    mu: Real,
    kappa: Real,
}

impl ForeignMaterialMethods for NonlinearElastic {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        let e = law.state.params.get_real("E")?;
        let nu = law.state.params.get_real("nu")?;
        self.lambda = nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu));
        self.mu = e / (2.0 * (1.0 + nu));
        self.kappa = law.state.params.get_real("kappa")?;
        law.base_init_material()
    }

    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        let base = &mut law.state.base;
        let gu = base.grad_u.get(ty, g)?.clone();
        let mut s = base.stress.get_mut(ty, g)?;
        for (s, gu) in s.tuples_mut().zip(gu.tuples()) {
            let (exx, eyy, exy) = (gu[0], gu[3], 0.5 * (gu[1] + gu[2]));
            let tr = exx + eyy;
            let iso = self.lambda * tr + self.kappa * tr * tr;
            s[0] = iso + 2.0 * self.mu * exx;
            s[3] = iso + 2.0 * self.mu * eyy;
            s[1] = 2.0 * self.mu * exy;
            s[2] = s[1];
        }
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        g: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let gu = law.state.base.grad_u.get(ty, g)?;
        let (l, m) = (self.lambda, self.mu);
        for (t, gu) in tangent
            .as_mut_slice()
            .as_chunks_mut::<9>()
            .0
            .iter_mut()
            .zip(gu.tuples())
        {
            let k = l + 2.0 * self.kappa * (gu[0] + gu[3]);
            t.copy_from_slice(&[k + 2.0 * m, k, 0.0, k, k + 2.0 * m, 0.0, 0.0, 0.0, m]);
        }
        Ok(())
    }

    fn push_wave_speed(&self, state: &ForeignLawState, _element: Element) -> Result<Real> {
        Ok(((self.lambda + 2.0 * self.mu) / state.base.rho).sqrt())
    }
}

fn nonlinear_model(compat: bool) -> SolidMechanicsModel {
    let dat = "material nonlinear_elastic [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n  kappa = 5\n]\n";
    let mut m = model(dat);
    m.physics
        .factory
        .register(
            "nonlinear_elastic",
            Arc::new(
                |spec: &LawSpec, _model: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut law = ForeignMaterial::new(
                        spec,
                        "nonlinear_elastic",
                        Box::new(NonlinearElastic::default()),
                    )?;
                    let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
                    law.register_param_real("E", rp, "Young's modulus")?;
                    law.register_param_real("nu", rp, "Poisson ratio")?;
                    law.register_param_real("kappa", rp, "quadratic volumetric modulus")?;
                    Ok(Box::new(law))
                },
            ),
        )
        .unwrap();
    m.physics.upstream_compat = SmmUpstreamCompat {
        stiffness_grad_u_ignores_eigen_grad_u: compat,
        ..SmmUpstreamCompat::default()
    };
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    set_newton(&mut m, 50, 1e-12, "residual");
    block_x0(&mut m);
    let eig = MatrixX::from_col_major(2, 2, vec![0.02, 0.0, 0.0, 0.01]).unwrap();
    m.apply_eigen_grad_u(&eig, "bulk", NG).unwrap();
    m
}

/// `K v` with the assembled stiffness (symmetric storage: one triangle).
fn k_times(m: &SolidMechanicsModel, v: &[Real]) -> Vec<Real> {
    let k = m.dof_manager().matrix("K").unwrap();
    let eq = &m
        .dof_manager()
        .family("displacement")
        .unwrap()
        .local_equation_numbers;
    let mut dof_of = vec![0usize; eq.len()];
    for (d, &e) in eq.iter().enumerate() {
        dof_of[e as usize] = d;
    }
    let sym = k.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    let (r, c, a) = k.coo();
    let mut out = vec![0.0; v.len()];
    for ((&i, &j), &x) in r.iter().zip(c).zip(a) {
        let (i, j) = (dof_of[i as usize], dof_of[j as usize]);
        out[i] += x * v[j];
        if sym && i != j {
            out[j] += x * v[i];
        }
    }
    out
}

/// Central difference of the internal forces, `−(f(u + h v) − f(u − h v)) / 2h`
/// (`f_int = −∫ Bᵀσ`).
fn fd_tangent(m: &mut SolidMechanicsModel, v: &[Real], h: Real) -> Vec<Real> {
    let u0 = m.displacement().unwrap().as_slice().to_vec();
    let mut f = Vec::new();
    for s in [h, -h] {
        for ((u, u0), v) in m
            .displacement_mut()
            .unwrap()
            .as_mut_slice()
            .iter_mut()
            .zip(&u0)
            .zip(v)
        {
            *u = u0 + s * v;
        }
        m.assemble_internal_forces().unwrap();
        f.push(m.internal_force().unwrap().as_slice().to_vec());
    }
    m.displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    m.assemble_internal_forces().unwrap();
    f[0].iter()
        .zip(&f[1])
        .map(|(p, q)| -(p - q) / (2.0 * h))
        .collect()
}

/// The stiffness assembled with an eigenstrain is the derivative of the
/// internal forces (tangent evaluated at the mechanical strain `∇u − ε⁰`);
/// upstream evaluates it at `∇u` (UD-208), off by `2κ tr(ε⁰)`.
#[test]
fn sm03_tangent_is_evaluated_at_the_mechanical_strain() {
    let mut rel = Vec::new();
    let mut iterations = Vec::new();
    for compat in [false, true] {
        let mut m = nonlinear_model(compat);
        m.solve_step(None).unwrap();
        iterations.push(m.solver.non_linear_solver(None).unwrap().nb_iterations());
        m.assemble_stiffness_matrix(true).unwrap();
        let n = m.displacement().unwrap().as_slice().len();
        let v: Vec<Real> = (0..n).map(|k| (0.37 * k as Real).sin()).collect();
        let kv = k_times(&m, &v);
        let fd = fd_tangent(&mut m, &v, 1e-6);
        rel.push(max_rel(&kv, &fd));
    }
    eprintln!(
        "sm03: |K v − FD| / |FD| default {:e}, upstream compat {:e}; NR iterations {iterations:?}",
        rel[0], rel[1]
    );
    // before the fix (= compat): the tangent misses −ε⁰
    assert!(rel[0] < 1e-6, "default tangent inconsistent: {:e}", rel[0]);
    assert!(rel[1] > 1e-2, "compat tangent should be off: {:e}", rel[1]);
    // the wrong tangent costs Newton iterations
    assert!(iterations[0] < iterations[1], "iterations {iterations:?}");
}

/// Direct check of the stored gradient after a stiffness assembly.
#[test]
fn sm03_stiffness_assembly_stores_grad_u_minus_eigen_grad_u() {
    for compat in [false, true] {
        let mut m = incremental_model("solution", compat);
        m.solve_step(None).unwrap();
        m.assemble_internal_forces().unwrap();
        let after_stress = m
            .material(0)
            .unwrap()
            .base()
            .grad_u
            .get(TRI, NG)
            .unwrap()
            .clone();
        m.assemble_stiffness_matrix(true).unwrap();
        let law = m.material(0).unwrap();
        let after_k = law.base().grad_u.get(TRI, NG).unwrap();
        let eigen = law.base().eigen_grad_u.get(TRI, NG).unwrap();
        for ((k, s), e) in after_k
            .as_slice()
            .iter()
            .zip(after_stress.as_slice())
            .zip(eigen.as_slice())
        {
            if compat {
                // upstream: ∇u = (∇u − ε⁰) + ε⁰ up to one rounding
                assert!((k - (s + e)).abs() <= 1e-15 * (1.0 + k.abs()));
            } else {
                assert_eq!(k.to_bits(), s.to_bits());
            }
        }
    }
}

/* ---------------------------------------------------------------------- */
/* `sm04_*`: initNewSolver sets the method before the default options       */
/* ---------------------------------------------------------------------- */

#[test]
fn sm04_init_new_solver_uses_the_new_method_options() {
    let mut m = initialised(AnalysisMethod::Static);
    m.init_new_solver(AnalysisMethod::ExplicitConsistentMass)
        .unwrap();
    assert_eq!(m.analysis_method(), AnalysisMethod::ExplicitConsistentMass);
    let tss = m.solver.solver(Some("explicit")).unwrap();
    // upstream (UD-212): trapezoidal rule 2 with a displacement solution,
    // the options of the previous method `_static` → implicit Newmark
    assert_eq!(
        tss.integration_scheme("displacement").unwrap().kind(),
        IntegrationSchemeType::CentralDifference
    );
    assert_eq!(
        tss.solution_type("displacement"),
        Some(SolutionType::Acceleration)
    );
    // and the solver is the default one now
    assert!(
        m.solver
            .solver(None)
            .unwrap()
            .integration_scheme("displacement")
            .is_ok()
    );
}

/* ---------------------------------------------------------------------- */
/* `sm05_*`: extrapolateInternal                                           */
/* ---------------------------------------------------------------------- */

#[test]
fn sm05_extrapolate_internal() {
    let mut m = initialised(AnalysisMethod::Static);
    let el = Element::new(TRI, 0, NG);
    let mat = akantu_core::decode_idx(m.material_by_element().element_ref(el).unwrap()[0])
        .unwrap()
        .unwrap();
    let local = m.material_local_numbering().element(el).unwrap()[0] as usize;
    // non-internal id: zero-filled (the earlier code left `out` untouched)
    let mut out = vec![42.0; 8];
    m.extrapolate_internal(mat, "no_such_internal", el, &mut out)
        .unwrap();
    assert_eq!(out, vec![0.0; 8]);
    // all-zero internal: point 0 (zeros), every column written
    let mut out = vec![42.0; 8];
    m.extrapolate_internal(mat, "stress", el, &mut out).unwrap();
    assert_eq!(out, vec![0.0; 8]);
    // first quadrature point with a positive norm, copied into each column
    // (triangle_3: one point per element; write a recognisable stress)
    let tensor = [1.0, 2.0, 3.0, 4.0];
    m.material_mut(mat)
        .unwrap()
        .base_mut()
        .stress
        .get_mut(TRI, NG)
        .unwrap()
        .as_mut_slice()[local * 4..local * 4 + 4]
        .copy_from_slice(&tensor);
    let mut out = vec![0.0; 12];
    m.extrapolate_internal(mat, "stress", el, &mut out).unwrap();
    for col in out.as_chunks::<4>().0 {
        assert_eq!(col, &tensor);
    }
    // an output that is not a whole number of columns
    let mut out = vec![0.0; 6];
    assert!(is_dimension_mismatch(
        &m.extrapolate_internal(mat, "stress", el, &mut out)
    ));
}

/// Several quadrature points: the first one with a positive norm.
#[test]
fn sm05_extrapolate_internal_first_non_zero_point() {
    let dat = "material elastic [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(bar("triangle_6")),
        None,
        "smm",
        Parser::parse_str(dat, "m.dat").unwrap(),
    )
    .unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let ty = ElementType::Triangle6;
    let el = Element::new(ty, 2, NG);
    let local = m.material_local_numbering().element(el).unwrap()[0] as usize;
    let nq = m
        .physics
        .base
        .fems
        .get("")
        .unwrap()
        .nb_integration_points(ty, NG)
        .unwrap();
    assert!(nq > 2);
    {
        let mut s = m
            .material_mut(0)
            .unwrap()
            .base_mut()
            .stress
            .get_mut(ty, NG)
            .unwrap();
        let s = s.as_mut_slice();
        let base = local * nq * 4;
        s[base + 4..base + 8].copy_from_slice(&[0.0, 0.0, 0.0, 0.0]);
        s[base + 8..base + 12].copy_from_slice(&[5.0, 0.0, 0.0, -1.0]);
    }
    let mut out = vec![0.0; 8];
    m.extrapolate_internal(0, "stress", el, &mut out).unwrap();
    assert_eq!(out, vec![5.0, 0.0, 0.0, -1.0, 5.0, 0.0, 0.0, -1.0]);
}

/* ---------------------------------------------------------------------- */
/* `sm07_*`: per-element energies dispatch through the material            */
/* ---------------------------------------------------------------------- */

/// Elastic-like law reporting a per-element `dissipated` energy of 1 per
/// element (stand-in for the viscoelastic laws' override).
#[derive(Debug)]
struct WithDissipation(ForeignMaterial);

impl akantu_model::constitutive_law::ConstitutiveLaw for WithDissipation {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn law_id(&self) -> &str {
        self.0.law_id()
    }
    fn law_type(&self) -> &str {
        self.0.law_type()
    }
    fn element_filter(&self) -> &akantu_core::ElementTypeMapArray<akantu_core::Idx> {
        akantu_model::constitutive_law::ConstitutiveLaw::element_filter(&self.0)
    }
    fn element_filter_mut(&mut self) -> &mut akantu_core::ElementTypeMapArray<akantu_core::Idx> {
        akantu_model::constitutive_law::ConstitutiveLaw::element_filter_mut(&mut self.0)
    }
    fn parameter_names(&self) -> Vec<String> {
        self.0.parameter_names()
    }
    fn for_each_internal(
        &self,
        f: &mut dyn FnMut(&str, akantu_model::constitutive_law::InternalFieldRef<'_>),
    ) {
        self.0.for_each_internal(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, akantu_model::constitutive_law::InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.0.for_each_internal_mut(f)
    }
}

impl akantu_parser::ParameterRegistry for WithDissipation {
    fn get_param(&self, name: &str) -> Result<akantu_parser::ParamValue> {
        self.0.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: akantu_parser::ParamValue) -> Result<()> {
        self.0.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.0.param_names()
    }
    fn param_info(&self, name: &str) -> Option<akantu_parser::param::ParamInfo> {
        self.0.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: akantu_parser::ParamValue) -> Result<()> {
        self.0.set_param_from_input(name, value)
    }
}

impl akantu_parser::Parsable for WithDissipation {
    fn section_type(&self) -> akantu_parser::ParserType {
        self.0.section_type()
    }
    fn parsable_id(&self) -> &str {
        self.0.parsable_id()
    }
}

impl Material for WithDissipation {
    fn base(&self) -> &akantu_solid::MaterialBase {
        self.0.base()
    }
    fn base_mut(&mut self) -> &mut akantu_solid::MaterialBase {
        self.0.base_mut()
    }
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.0.init_material(ctx)
    }
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        self.0.compute_stress(ctx, ty, g)
    }
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        g: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.0.compute_tangent_moduli(ctx, ty, g, tangent)
    }
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.0.compute_potential_energy(ctx, ty)
    }
    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.0.push_wave_speed(element)
    }
    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.0.shear_wave_speed(element)
    }
    fn energy_of_element(
        &mut self,
        _ctx: &LawContext<'_>,
        energy_id: &str,
        _element: Element,
    ) -> Result<Real> {
        Ok(if energy_id == "dissipated" { 1.0 } else { 0.0 })
    }
}

#[test]
fn sm07_element_energy_dispatches_through_the_material() {
    let dat = "material dissipating [\n  name = bulk\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";
    let mut m = model(dat);
    m.physics
        .factory
        .register(
            "dissipating",
            Arc::new(
                |spec: &LawSpec, _model: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut law = ForeignMaterial::new(
                        spec,
                        "dissipating",
                        Box::new(IncrementalElastic::default()),
                    )?;
                    let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
                    law.register_param_real("E", rp, "Young's modulus")?;
                    law.register_param_real("nu", rp, "Poisson ratio")?;
                    Ok(Box::new(WithDissipation(law)))
                },
            ),
        )
        .unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    // before the fix: hard-coded "potential" dispatch → 0
    let e = m
        .energy_of_element("dissipated", Element::new(TRI, 3, NG))
        .unwrap();
    assert_eq!(e, 1.0);
    let nt = m.physics.base.mesh.read().unwrap().nb_element(TRI, NG);
    assert_eq!(m.energy_of_group("dissipated", "bulk").unwrap(), nt as Real);
}

/* ---------------------------------------------------------------------- */
/* `sm08_*`: assembleResidual accumulates                                  */
/* ---------------------------------------------------------------------- */

#[test]
fn sm08_assemble_residual_accumulates() {
    let mut m = initialised(AnalysisMethod::Static);
    {
        let x = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        let mut u_g = m.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for (u, x) in u.iter_mut().zip(&x) {
            *u = 1e-3 * x;
        }
        drop(u_g);
        let mut f = m.external_force_mut().unwrap();
        let f = f.as_mut_slice();
        f[1] = 0.25;
    }
    m.dof_manager_mut().zero_residual();
    m.assemble_residual().unwrap();
    let once = m.dof_manager().residual().values().to_vec();
    assert!(once.iter().any(|&r| r != 0.0));
    m.assemble_residual().unwrap();
    let twice = m.dof_manager().residual().values().to_vec();
    // upstream: `r += f_ext + f_int` (the earlier code zeroed first: twice == once)
    for (t, o) in twice.iter().zip(&once) {
        assert_eq!(t.to_bits(), (o + o).to_bits());
    }
}

/* ---------------------------------------------------------------------- */
/* node removal renumbers the DOFs (SMM-015)                               */
/* ---------------------------------------------------------------------- */

/// The numbering of `purify` through the model's own mutation entry point
/// (a non-identity renumbering of the bar: gmsh's node order is not the
/// order of first appearance).
fn purified(m: &mut SolidMechanicsModel) -> Vec<akantu_core::Idx> {
    use akantu_mesh::{MeshEvent, MeshPart};
    let c = m.mutate_mesh(|tx| tx.purify(MeshPart::Main), None).unwrap();
    c.events
        .iter()
        .find_map(|(_, e)| match e {
            MeshEvent::NodesRemoved { new_numbering, .. } => Some(new_numbering.clone()),
            _ => None,
        })
        .unwrap()
}

/// UD-209 → SMM-015: the earlier code compacted the DOF arrays
/// without renumbering the equations and left `current_position`
/// uncompacted; the removal was then refused until SMM-015. Now the DOF
/// manager renumbers (equal to a fresh registration on the renumbered
/// mesh), every nodal array follows its node — the DOF family's and the
/// model's, `current_position` included (refreshed: the removal bumps the
/// displacement release, as upstream's `onNodesRemoved`) — and the model
/// solves on.
#[test]
fn sm09_node_removal_renumbers_and_compacts_every_array() {
    let mut m = initialised(AnalysisMethod::Static);
    block_x0(&mut m);
    let nn = nb_nodes(&m);
    for (k, v) in m
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = 1e-3 * k as Real;
    }
    for (k, v) in m
        .external_force_mut()
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = 2e-3 * k as Real;
    }
    let u0 = m.displacement().unwrap().as_slice().to_vec();
    let f0 = m.external_force().unwrap().as_slice().to_vec();
    let b0 = m.blocked_dofs().unwrap().as_slice().to_vec();
    let x0 = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let numbering = purified(&mut m);
    assert_eq!(numbering.len(), nn);
    assert!(numbering.iter().enumerate().any(|(i, &n)| n as usize != i));
    let moved = |old: &[Real]| {
        let mut out = vec![0.0; old.len()];
        for (i, &n) in numbering.iter().enumerate() {
            let n = n as usize;
            out[2 * n..2 * n + 2].copy_from_slice(&old[2 * i..2 * i + 2]);
        }
        out
    };
    assert_eq!(m.displacement().unwrap().as_slice(), &moved(&u0)[..]);
    assert_eq!(m.external_force().unwrap().as_slice(), &moved(&f0)[..]);
    let mut b = vec![false; b0.len()];
    for (i, &n) in numbering.iter().enumerate() {
        let n = n as usize;
        b[2 * n..2 * n + 2].copy_from_slice(&b0[2 * i..2 * i + 2]);
    }
    assert_eq!(m.blocked_dofs().unwrap().as_slice(), &b[..]);
    // current position = X + u at the new indices (before the fix: not compacted)
    let want: Vec<Real> = moved(&x0)
        .iter()
        .zip(moved(&u0))
        .map(|(x, u)| x + u)
        .collect();
    assert_eq!(m.current_position().unwrap().as_slice(), &want[..]);
    // the numbering equals a fresh registration on the renumbered mesh
    let mut fresh = akantu_solver::DofManager::new("fresh", akantu_comm::Communicator::serial());
    fresh
        .register_nodal_dofs(&m.physics.base.mesh.read().unwrap(), "displacement", 2)
        .unwrap();
    assert_eq!(m.dof_manager().layout(), fresh.layout());
    assert_eq!(
        m.dof_manager()
            .local_equation_numbers("displacement")
            .unwrap(),
        fresh.local_equation_numbers("displacement").unwrap()
    );
    m.solve_step(None).unwrap();
    assert!(
        m.displacement()
            .unwrap()
            .as_slice()
            .iter()
            .all(|u| u.is_finite())
    );
}

/// A node appended and removed again in one mutation: the addition names
/// a node that does not exist in the final mesh (the DOF manager's
/// composition refusal, `akantu_solver` node-removal docs).
fn add_then_remove(
    tx: &mut akantu_mesh::MeshTransaction<'_>,
    nn: usize,
    origin: &str,
) -> Result<()> {
    use akantu_mesh::{MeshEvent, MeshPart};
    let n = tx.push_node(MeshPart::Main, &[0.5, 0.5])?;
    tx.record(
        MeshPart::Main,
        MeshEvent::NodesAdded {
            nodes: vec![n],
            old_nodes: None,
            origin: origin.into(),
            extension: None,
        },
    );
    let mut numbering: Vec<akantu_core::Idx> = (0..nn as akantu_core::Idx).collect();
    numbering.push(-1);
    tx.remove_nodes(MeshPart::Main, &numbering)
}

/// SMM-015: the capability check
/// (`check_mesh_event`, mutation stage 3b) is where a node event the model
/// cannot follow is refused **before** the mesh commit (recoverable: the
/// mesh is rolled back, the model untouched, the domain usable). Since
/// SMM-015 node removal is accepted; what the check still refuses is a node
/// **addition followed by a removal** renumbering the added node in the
/// same mutation (the DOF manager would read the final mesh at indices that
/// no longer exist). The model's own `token` is paired with the model by
/// moving it out for the duration of the call (a placeholder token of a
/// scratch domain keeps the field initialised).
#[test]
fn sm2_02_node_events_the_model_cannot_follow_are_refused_before_commit() {
    use akantu_core::enums::EventPriority;
    use akantu_mesh::{MeshEventHandler, MeshPart};
    let mut m = initialised(AnalysisMethod::Static);
    let shared = m.physics.base.mesh.clone();
    let u_before = m.displacement().unwrap().as_slice().to_vec();
    let (nodes_before, revision) = {
        let g = shared.read().unwrap();
        (g.nodes().as_slice().to_vec(), shared.revision().unwrap())
    };
    let scratch = SharedMesh::new(Mesh::new(2, "scratch").unwrap());
    let with_model_token =
        |m: &mut SolidMechanicsModel,
         name: &str,
         change: &dyn Fn(&mut akantu_mesh::MeshTransaction<'_>) -> Result<()>| {
            let placeholder = scratch
                .register_consumer(name, EventPriority::MODEL, MeshPart::Main)
                .unwrap();
            let token = std::mem::replace(&mut m.token, placeholder);
            let r = shared.mutate(
                &mut [(&token, m as &mut dyn MeshEventHandler)],
                |tx| change(tx),
                None,
            );
            m.token = token;
            r
        };
    let nn = nb_nodes(&m);
    let r = with_model_token(&mut m, "placeholder", &|tx| {
        add_then_remove(tx, nn, "sm2_02")
    });
    let Err(e) = &r else {
        panic!("expected the composition refusal, got {r:?}");
    };
    assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("record the removal first"), "{e}");
    // nothing committed: the mesh, its revision and the model arrays are
    // unchanged
    {
        let g = shared.read().unwrap();
        assert_eq!(g.nodes().as_slice(), &nodes_before[..]);
    }
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(m.displacement().unwrap().as_slice(), &u_before[..]);
    // the domain is usable: `purify` (a dispatched non-identity
    // renumbering) commits through the same path, the model solves
    let again = with_model_token(&mut m, "placeholder2", &|tx| tx.purify(MeshPart::Main));
    assert!(again.is_ok(), "{again:?}");
    m.solve_step(None).unwrap();
}

/// [`akantu_solid::model::SmmEventParts`] — the handler
/// a coordinator (the cohesive model, a coupler) passes to
/// `SharedMesh::mutate` with the model's token — follows a node removal as
/// the model itself does (SMM-015; refused before) and forwards the DOF
/// manager's composition refusal.
#[test]
fn fu01_smm_event_parts_follow_node_removal() {
    use akantu_mesh::{MeshEvent, MeshPart};
    let mut m = initialised(AnalysisMethod::Static);
    let shared = m.physics.base.mesh.clone();
    let revision = shared.revision().unwrap();
    let nn = nb_nodes(&m);
    let refused = {
        let (token, mut parts) = m.split_for_mutation();
        shared.mutate(
            &mut [(token, &mut parts)],
            |tx| add_then_remove(tx, nn, "fu01"),
            None,
        )
    };
    assert!(matches!(refused, Err(Error::Unsupported(_))), "{refused:?}");
    assert_eq!(shared.revision().unwrap(), revision);
    let u_before: Vec<Real> = (0..2 * nn).map(|k| k as Real * 1e-4).collect();
    m.displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u_before);
    let committed = {
        let (token, mut parts) = m.split_for_mutation();
        shared
            .mutate(
                &mut [(token, &mut parts)],
                |tx| tx.purify(MeshPart::Main),
                None,
            )
            .unwrap()
    };
    assert_eq!(committed.revision, revision + 1);
    let numbering = committed
        .events
        .iter()
        .find_map(|(_, e)| match e {
            MeshEvent::NodesRemoved { new_numbering, .. } => Some(new_numbering.clone()),
            _ => None,
        })
        .unwrap();
    let u = m.displacement().unwrap().as_slice().to_vec();
    for (i, &n) in numbering.iter().enumerate() {
        let n = n as usize;
        assert_eq!(u[2 * n..2 * n + 2], u_before[2 * i..2 * i + 2]);
    }
    m.solve_step(None).unwrap();
}

/// The solid model dispatches
/// mesh events to its DOF manager (level 20) and must therefore forward the
/// DOF manager's capability check too. The DOF manager refuses
/// `MeshIsDistributed` when its DOFs were numbered on the undistributed mesh
/// (upstream `onMeshIsDistributed`, `test_dof_mesh_distribute`); the model,
/// its event parts and the manager report the same refusal.
#[test]
fn fu01_solid_model_check_forwards_to_its_dof_manager() {
    use akantu_mesh::{MeshAccessor, MeshEvent, MeshEventHandler};
    let mut m = initialised(AnalysisMethod::Static);
    let mut distributed = bar("triangle_3");
    MeshAccessor::new(&mut distributed).set_distributed();
    let ev = MeshEvent::MeshIsDistributed {
        origin: "fu01".into(),
    };
    let direct = m
        .solver
        .dof_manager
        .check_mesh_event(&distributed, &ev)
        .map_err(|e| e.to_string());
    assert!(direct.is_err(), "{direct:?}");
    let via_model =
        MeshEventHandler::check_mesh_event(&m, &distributed, &ev).map_err(|e| e.to_string());
    assert_eq!(via_model, direct);
    let via_parts = {
        let (_, parts) = m.split_for_mutation();
        parts
            .check_mesh_event(&distributed, &ev)
            .map_err(|e| e.to_string())
    };
    assert_eq!(via_parts, direct);
    // the undistributed mesh is accepted by all three
    let plain = bar("triangle_3");
    assert!(MeshEventHandler::check_mesh_event(&m, &plain, &ev).is_ok());
}
