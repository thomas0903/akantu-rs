// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Adversarial coupler
// cases compared with the oracle probe oracle/probes/couplers_v80/ (its
// outputs in tests/data/v80_oracle/), kept as regression tests: the static
// cohesive cases run with the cohesive model's UD-034 compat (upstream's
// symmetric K storage), which resolves the static cohesive difference: with upstream's effective K
// (UD-772, contact stiffness withheld) every recorded value is bitwise.
// Exercises src/model/model_couplers/coupler_solid_cohesive_contact.cc and
// coupler_solid_phasefield.cc through the port's couplers.

mod common;

use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, ContactUpstreamCompat};
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_core::{GhostType, Real};
use akantu_couplers::{CouplerSolidCohesiveContact, CouplerSolidPhaseField};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldRef};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use akantu_solver::ModelSolverOptions;
use common::*;

const NG: GhostType = GhostType::NotGhost;

fn eigen(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

fn newton(nls: &dyn akantu_solver::NonLinearSolver) -> (i64, f64) {
    let it = match nls.get_param("nb_iterations").unwrap() {
        ParamValue::Int(i) => i,
        o => panic!("{o:?}"),
    };
    let err = match nls.get_param("error").unwrap() {
        ParamValue::Real(e) => e,
        o => panic!("{o:?}"),
    };
    (it, err)
}

/* ------------------------------------------------------------------ */
/* Static solid-cohesive-contact coupler (K composition)                */
/* ------------------------------------------------------------------ */

const COHESIVE_DAT: &str = include_str!("data/contact/material-cohesive.dat");

fn cohesive_static(extrinsic: bool, epsilon_n: Real) -> CouplerSolidCohesiveContact {
    let dat = COHESIVE_DAT.replace("epsilon_n = 0", &format!("epsilon_n = {epsilon_n:e}"));
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(&dat, "material-cohesive.dat").unwrap();
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    // upstream behaviour everywhere (UD-224: intrinsic cohesive_linear has
    // zero strength, the interface is held by the contact penalty only)
    c.solid.ext.compat = akantu_cohesive::material::CohesiveUpstreamCompat::upstream();
    // UD-034 compat (the root cause of the static difference): upstream keeps K
    // symmetric and drops the lower triangle of the unsymmetric tangent
    c.solid
        .set_symmetry_policy(akantu_linalg::SymmetryPolicy::UpstreamUpperSelection)
        .unwrap();
    c.init_full(&eigen(AnalysisMethod::Static), extrinsic)
        .unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "sides").unwrap();
    c
}

/// (nnz, Σ a, Σ |a|) of the matrix `K` of the coupler's DOF manager.
fn k_sums(c: &CouplerSolidCohesiveContact) -> (usize, Real, Real) {
    let k = c.solid.smm.solver.dof_manager.matrix("K").unwrap();
    let v = k.values();
    (
        v.len(),
        v.iter().sum::<Real>(),
        v.iter().map(|x| x.abs()).sum::<Real>(),
    )
}

/// The coupler's `assembleStiffnessMatrix()` (solid K reassembled, plus the
/// contact stiffness for static / implicit) and the solid stiffness alone,
/// both into the coupler's DOF manager.
fn coupled_and_solid_k(
    c: &mut CouplerSolidCohesiveContact,
) -> ((usize, Real, Real), (usize, Real, Real)) {
    use akantu_couplers::SolidContactCallback;
    use akantu_solver::SolverCallback;
    {
        let akantu_solid::SolidMechanicsModel {
            physics, solver, ..
        } = &mut c.solid.smm;
        let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
        let mut cb =
            SolidContactCallback::coupler(&mut cohesive, &mut c.contact, AnalysisMethod::Static);
        solver
            .dof_manager
            .collective("v80:K", |dm| {
                cb.assemble_matrix(dm, "K")?;
                dm.finalize_matrix("K")
            })
            .unwrap();
    }
    let coupled = k_sums(c);
    {
        let akantu_solid::SolidMechanicsModel {
            physics, solver, ..
        } = &mut c.solid.smm;
        let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
        solver
            .dof_manager
            .collective("v80:Ks", |dm| {
                use akantu_couplers::contact::SolidPart;
                cohesive.assemble_solid_stiffness(dm, true)?;
                dm.finalize_matrix("K")
            })
            .unwrap();
    }
    (coupled, k_sums(c))
}

/// Pre-screen of the oracle case `v80/cohesive_static`: intrinsic cohesive
/// elements on `interface` (zero strength, UD-224 upstream behaviour:
/// the contact surface exists from the start and only the contact penalty
/// holds the interface), static compression (`loading` -inc, `fixed`
/// +inc), Newton at 100 iterations. Prints the Newton history, the number
/// of contact elements per step and the K composition.
#[test]
fn v80_cohesive_static_prescreen() {
    let steps: usize = std::env::var("V80_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let inc: Real = 1e-3 / 50.0;
    for compat in [
        ContactUpstreamCompat::all(),
        ContactUpstreamCompat::default(),
    ] {
        eprintln!(
            "--- contact compat all = {}",
            compat == ContactUpstreamCompat::all()
        );
        let mut c = cohesive_static(false, 1e3);
        c.contact.set_upstream_compat(compat);
        {
            let nls = c.solid.smm.solver.non_linear_solver_mut(None).unwrap();
            nls.set_param("max_iterations", ParamValue::Int(100))
                .unwrap();
        }
        for s in 0..steps {
            let up = IncrementValue {
                value: -inc,
                axis: SpatialDirection::Y,
            };
            let down = IncrementValue {
                value: inc,
                axis: SpatialDirection::Y,
            };
            c.apply_dirichlet_bc(&up, "loading").unwrap();
            c.apply_dirichlet_bc(&down, "fixed").unwrap();
            let r = c.solve_step(None);
            let (it, err) = newton(c.solid.smm.solver.non_linear_solver(None).unwrap());
            let pen = c
                .contact
                .gaps()
                .as_slice()
                .iter()
                .fold(0.0f64, |m, g| m.max(*g));
            eprintln!(
                "step {s}: {} it {it} err {err:e} contact elements {} max penetration {pen:e}",
                if r.is_ok() { "ok" } else { "FAILED" },
                c.contact.contact_elements().len()
            );
            if let Err(e) = r {
                eprintln!("  error: {e}");
                break;
            }
        }
        let (kc, ks) = coupled_and_solid_k(&mut c);
        eprintln!("K coupled (nnz, sum, sum|.|) = {kc:?}\nK solid   (nnz, sum, sum|.|) = {ks:?}");
    }
}

/* ------------------------------------------------------------------ */
/* UD-382 independent check: two identical laws vs one law              */
/* ------------------------------------------------------------------ */

const TWO_EQUAL: &str = r"
material phasefield no_split [
	 name = plate
	 rho = 1.
	 E = 210.0
	 nu = 0.3
	 Plane_Stress = false
]
material phasefield no_split [
	 name = rigid
	 rho = 1.
	 E = 210.0
	 nu = 0.3
	 Plane_Stress = false
]
phasefield quadratic no_split [
  name = plate
  E = 210.0
  nu = 0.3
  gc = 5e-3
  l0 = 0.1
]
phasefield quadratic no_split [
  name = rigid
  E = 210.0
  nu = 0.3
  gc = 5e-3
  l0 = 0.1
]
";

fn pf_data(name: &str) -> std::path::PathBuf {
    data_dir().join("test_phase_field_model").join(name)
}

fn pf_coupler(dat: &str, mat_sel: bool, pf_sel: bool) -> CouplerSolidPhaseField {
    let mesh = read_mesh(&pf_data("test_two_element.msh"), 2);
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    if mat_sel {
        c.solid
            .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    }
    c.solid.init_full(&eigen(AnalysisMethod::Static)).unwrap();
    if pf_sel {
        c.phase
            .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    }
    c.phase.init_full(&eigen(AnalysisMethod::Static)).unwrap();
    c
}

/// `inhomogeneous`: the top node at x = 0 moves by eps/4 only and the
/// middle nodes are free, so the two elements strain (and damage)
/// differently.
fn apply_displacement_with(c: &mut CouplerSolidPhaseField, eps: Real, inhomogeneous: bool) {
    let nodes = c.solid.physics.base.mesh.read().unwrap().nodes().clone();
    let mut u = c.solid.displacement().unwrap().as_slice().to_vec();
    let mut b = c.solid.blocked_dofs().unwrap().as_slice().to_vec();
    for (n, x) in nodes.as_slice().as_chunks::<2>().0.iter().enumerate() {
        if x[1] == -1. {
            u[2 * n] = 0.;
            u[2 * n + 1] = 0.;
            b[2 * n] = true;
            b[2 * n + 1] = true;
        } else if x[1] == 1. {
            u[2 * n] = 0.;
            u[2 * n + 1] = if inhomogeneous && x[0] == 0. {
                0.25 * eps
            } else {
                eps
            };
            b[2 * n] = true;
            b[2 * n + 1] = true;
        } else if !inhomogeneous {
            u[2 * n] = 0.;
            b[2 * n] = true;
        }
    }
    c.solid
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u);
    c.solid
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&b);
}

fn run_pf(
    c: &mut CouplerSolidPhaseField,
    steps: usize,
    inhomogeneous: bool,
) -> Vec<(Vec<Real>, Vec<Real>)> {
    let mut out = Vec::new();
    for s in 0..steps {
        apply_displacement_with(c, 2e-4 * s as Real, inhomogeneous);
        c.solve(None, None)
            .unwrap_or_else(|e| panic!("step {s}: {e}"));
        out.push((
            c.solid.displacement().unwrap().as_slice().to_vec(),
            c.phase.damage().unwrap().as_slice().to_vec(),
        ));
    }
    out
}

fn max_rel(a: &[(Vec<Real>, Vec<Real>)], b: &[(Vec<Real>, Vec<Real>)]) -> (Real, usize) {
    let mut worst: Real = 0.0;
    let mut differing = 0;
    for ((ua, da), (ub, db)) in a.iter().zip(b) {
        let scale = ua
            .iter()
            .chain(da)
            .fold(0.0f64, |m, x| m.max(x.abs()))
            .max(1e-300);
        let d = ua
            .iter()
            .zip(ub)
            .chain(da.iter().zip(db))
            .fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        if d != 0.0 {
            differing += 1;
        }
        worst = worst.max(d / scale);
    }
    (worst, differing)
}

/// Two materials / phase fields with identical parameters but different
/// names (plate on element 0, rigid on element 1) must give the same
/// trajectory as one material / phase field covering both elements, to the
/// Newton tolerance. The corrected default (UD-382 filtered transfer,
/// UD-377 filtered interpolation) must reproduce it; upstream compat must
/// not when the two elements damage differently.
fn ud382_case(inhomogeneous: bool) -> (Real, Real, Option<(usize, String)>, Real) {
    let steps = 150;
    let mut one = pf_coupler(TWO_EQUAL, false, false);
    assert_eq!(one.solid.nb_materials(), 2);
    let r_one = run_pf(&mut one, steps, inhomogeneous);
    let last = &r_one.last().unwrap().1;
    let d_end = last.iter().fold(0.0f64, |m, x| m.max(*x));
    let d_min = last.iter().fold(1.0f64, |m, x| m.min(*x));
    eprintln!(
        "[inhomogeneous={inhomogeneous}] one-law run: nodal damage at the end in [{d_min}, {d_end}]"
    );
    let mut two = pf_coupler(TWO_EQUAL, true, true);
    let r_two = run_pf(&mut two, steps, inhomogeneous);
    let (w, n) = max_rel(&r_one, &r_two);
    eprintln!(
        "[inhomogeneous={inhomogeneous}] corrected two-law vs one-law: max rel diff {w:e}, steps differing {n}"
    );
    let mut up = pf_coupler(TWO_EQUAL, true, true);
    up.set_upstream_compat(true);
    let mut r_up = Vec::new();
    let mut failed = None;
    for s in 0..steps {
        apply_displacement_with(&mut up, 2e-4 * s as Real, inhomogeneous);
        if let Err(e) = up.solve(None, None) {
            failed = Some((s, e.to_string()));
            break;
        }
        r_up.push((
            up.solid.displacement().unwrap().as_slice().to_vec(),
            up.phase.damage().unwrap().as_slice().to_vec(),
        ));
    }
    let (wu, nu) = max_rel(&r_one[..r_up.len()], &r_up);
    eprintln!(
        "[inhomogeneous={inhomogeneous}] upstream-compat two-law vs one-law: {} steps run, max rel diff {wu:e}, steps differing {nu}, failure {failed:?}",
        r_up.len()
    );
    (w, wu, failed, d_end - d_min)
}

#[test]
fn v80_ud382_two_identical_laws_equal_one_law() {
    // homogeneous strain: both elements damage identically, so upstream's
    // copy of element 0's damage into element 1 is harmless
    let (w, wu, _, _) = ud382_case(false);
    assert!(w <= 1e-8 && wu <= 1e-8, "homogeneous: {w:e} {wu:e}");
    // inhomogeneous: the corrected default reproduces the one-law run,
    // upstream compat does not
    let (w, wu, failed, spread) = ud382_case(true);
    assert!(
        spread > 1e-3,
        "the elements must damage differently ({spread:e})"
    );
    assert!(
        w <= 1e-8,
        "corrected two-law run differs from the one-law run: {w:e}"
    );
    assert!(
        wu > 1e-3 || failed.is_some(),
        "compat should differ: {wu:e}"
    );
}

/// Strain transfer with different selectors: the solid has
/// one material on both elements (default selector), the phase field two
/// laws by physical names. Each law must receive the symmetric gradient of
/// its own element.
#[test]
fn v80_strain_transfer_with_different_selectors() {
    let mut c = pf_coupler(TWO_EQUAL, false, true);
    // u = (x^2 y, x - y^3): non-uniform, skew gradient
    let nodes = c.solid.physics.base.mesh.read().unwrap().nodes().clone();
    {
        let mut u = c.solid.displacement_mut().unwrap();
        for (uv, x) in u
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(nodes.as_slice().as_chunks::<2>().0)
        {
            uv[0] = x[0] * x[0] * x[1];
            uv[1] = x[0] - x[1] * x[1] * x[1];
        }
    }
    c.solid.assemble_internal_forces().unwrap();
    c.compute_strain_on_quad_points(NG).unwrap();
    // grad_u of material 0 (elements 0 and 1 in mesh order)
    let m = c.solid.material(0).unwrap();
    let mut gu = Vec::new();
    m.for_each_internal(&mut |n, f| {
        if n == "grad_u"
            && let InternalFieldRef::Real(f) = f
        {
            gu.extend_from_slice(
                f.get(akantu_core::ElementType::Quadrangle4, NG)
                    .unwrap()
                    .as_slice(),
            );
        }
    });
    assert_eq!(gu.len(), 2 * 4 * 4);
    for l in 0..2 {
        let law = c.phase.phase_field(l).unwrap();
        let filt = law
            .base()
            .element_filter
            .get(akantu_core::ElementType::Quadrangle4, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        let strain = law
            .base()
            .strain
            .get(akantu_core::ElementType::Quadrangle4, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        assert_eq!(filt.len(), 1);
        let e = filt[0] as usize;
        let g = &gu[e * 16..(e + 1) * 16];
        for q in 0..4 {
            let gq = &g[q * 4..(q + 1) * 4];
            let sq = &strain[q * 4..(q + 1) * 4];
            assert_eq!(sq[0], gq[0]);
            assert_eq!(sq[3], gq[3]);
            assert_eq!(sq[1], (gq[1] + gq[2]) / 2.);
            assert_eq!(sq[2], sq[1]);
        }
        eprintln!(
            "law {l} ({}) element {e}: strain = sym(grad_u) of its element",
            law.name()
        );
    }
}

/* ------------------------------------------------------------------ */
/* Oracle comparisons (outputs of v80_probe, tests/data/v80_oracle/)        */
/* ------------------------------------------------------------------ */

struct OracleOut {
    info: Vec<(String, String)>,
    rec: std::collections::HashMap<String, Vec<f64>>,
}

fn oracle(name: &str) -> OracleOut {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/v80_oracle")
        .join(name);
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut info = Vec::new();
    let mut rec = std::collections::HashMap::new();
    for line in text.lines() {
        let mut it = line.splitn(3, ' ');
        match (it.next(), it.next(), it.next()) {
            (Some("I"), Some(k), Some(v)) => info.push((k.to_owned(), v.to_owned())),
            (Some("R"), Some(k), Some(v)) => {
                let mut w = v.split(' ');
                let n: usize = w.next().unwrap().parse().unwrap();
                let vals: Vec<f64> = w
                    .map(|h| f64::from_bits(u64::from_str_radix(h, 16).unwrap()))
                    .collect();
                assert_eq!(vals.len(), n, "{k}");
                rec.insert(k.to_owned(), vals);
            }
            _ => {}
        }
    }
    OracleOut { info, rec }
}

/// Upstream's static `CouplerSolidCohesiveContact`: the contact stiffness
/// goes into the contact model's own DOF manager, so the solved `K` is the
/// solid's alone. This wrapper reproduces that on the port's callback.
struct WithoutContactK<'a, 'b> {
    inner: &'b mut akantu_couplers::SolidContactCallback<'a>,
}

impl akantu_solver::SolverCallback for WithoutContactK<'_, '_> {
    fn matrix_type(&self, id: &str) -> akantu_core::enums::MatrixType {
        self.inner.matrix_type(id)
    }
    fn assemble_matrix(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
        id: &str,
    ) -> akantu_core::Result<()> {
        if id == "K" {
            self.inner.solid.assemble_solid_stiffness(dofs, true)
        } else {
            self.inner.assemble_matrix(dofs, id)
        }
    }
    fn assemble_lumped_matrix(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
        id: &str,
    ) -> akantu_core::Result<()> {
        self.inner.assemble_lumped_matrix(dofs, id)
    }
    fn assemble_residual(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
    ) -> akantu_core::Result<()> {
        self.inner.assemble_residual(dofs)
    }
    fn assemble_residual_part(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
        part: &str,
    ) -> akantu_core::Result<()> {
        self.inner.assemble_residual_part(dofs, part)
    }
    fn can_split_residual(&self) -> bool {
        self.inner.can_split_residual()
    }
    fn predictor(&mut self, dofs: &mut akantu_solver::DofManager) -> akantu_core::Result<()> {
        self.inner.predictor(dofs)
    }
    fn corrector(&mut self, dofs: &mut akantu_solver::DofManager) -> akantu_core::Result<()> {
        self.inner.corrector(dofs)
    }
    fn before_solve_step(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
    ) -> akantu_core::Result<()> {
        self.inner.before_solve_step(dofs)
    }
    fn after_solve_step(
        &mut self,
        dofs: &mut akantu_solver::DofManager,
        c: bool,
    ) -> akantu_core::Result<()> {
        self.inner.after_solve_step(dofs, c)
    }
}

/// Static solid-cohesive-contact coupler against the oracle (`v80_probe cohesive_static`): the port's
/// coupler (contact stiffness in the solved K) does not reproduce
/// upstream; the same coupler with the contact stiffness withheld (what
/// upstream's separate contact DOF manager amounts to) does, bitwise.
#[test]
fn v80_cp01_cohesive_static_vs_oracle() {
    use akantu_couplers::SolidContactCallback;
    use akantu_couplers::contact::SolidPart;
    use akantu_solver::SolverCallback;
    let o = oracle("cohesive_static.run1.txt");
    let inc: Real = 1e-3 / 50.0;
    for withhold in [false, true] {
        let mut c = cohesive_static(false, 1e3);
        {
            let nls = c.solid.smm.solver.non_linear_solver_mut(None).unwrap();
            nls.set_param("max_iterations", ParamValue::Int(100))
                .unwrap();
        }
        let mut first_mismatch: Option<String> = None;
        let mut steps_bitwise = 0;
        for s in 0..10usize {
            let Some((_, line)) = o.info.iter().find(|(k, _)| k == &format!("step_{s}")) else {
                break;
            };
            let up = IncrementValue {
                value: -inc,
                axis: SpatialDirection::Y,
            };
            let down = IncrementValue {
                value: inc,
                axis: SpatialDirection::Y,
            };
            c.apply_dirichlet_bc(&up, "loading").unwrap();
            c.apply_dirichlet_bc(&down, "fixed").unwrap();
            let r = if withhold {
                let contact = &mut c.contact;
                c.solid.smm.solve_step_with(None, |physics, solver, id| {
                    let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
                    let mut inner = SolidContactCallback::coupler(
                        &mut cohesive,
                        contact,
                        AnalysisMethod::Static,
                    );
                    let mut cb = WithoutContactK { inner: &mut inner };
                    solver.solve_step(&mut cb, id)
                })
            } else {
                c.solve_step(None)
            };
            let (it, err) = newton(c.solid.smm.solver.non_linear_solver(None).unwrap());
            let status = if r.is_ok() { "ok" } else { "not_converged" };
            let got = format!(
                "{status} it {it} contact_elements {}",
                c.contact.contact_elements().len()
            );
            let e_err = &o.rec[&format!("step_{s}/error")];
            let e_u = &o.rec[&format!("step_{s}/displacement")];
            let u = c.solid.smm.displacement().unwrap().as_slice().to_vec();
            let pen = c
                .contact
                .gaps()
                .as_slice()
                .iter()
                .fold(0.0f64, |m, g| m.max(*g));
            let same = &got == line
                && same_bits(err, e_err[0])
                && same_bits(pen, o.rec[&format!("step_{s}/max_penetration")][0])
                && count_bits(&u, e_u) == e_u.len();
            if same {
                steps_bitwise += 1;
            } else if first_mismatch.is_none() {
                first_mismatch = Some(format!(
                    "step {s}: port '{got}' err {err:e} pen {pen:e} / oracle '{line}' err {:e}; displacement bitwise {}/{}",
                    e_err[0],
                    count_bits(&u, e_u),
                    e_u.len()
                ));
            }
            if r.is_err() {
                break;
            }
        }
        eprintln!(
            "withhold contact K = {withhold}: {steps_bitwise} oracle steps reproduced bitwise; first mismatch: {first_mismatch:?}"
        );
        // K composition after the run, as the probe: coupler assembly vs solid alone
        let (kc, ks) = coupled_and_solid_k(&mut c);
        eprintln!(
            "port K coupled {kc:?} / solid {ks:?}; oracle coupler {:?} / solid {:?} / contact DOF manager {:?}",
            o.rec["K/coupler_after_coupler_assembly"],
            o.rec["K/coupler_after_solid_only_assembly"],
            o.rec["K/contact_dof_manager_after_coupler_assembly"]
        );
        // the port's coupler assembly (whatever the run variant) adds the
        // contact stiffness to the solved K; upstream's does not
        assert!(
            kc.2 > ks.2,
            "port: the solved K carries the contact stiffness"
        );
        let _ = SolidPart::physics(&akantu_cohesive::model::CohesiveCallback::new(
            &mut c.solid.smm.physics,
        ));
        // The port's coupler differs from upstream (UD-772). With the
        // contact stiffness withheld (upstream's effective K) and the UD-034
        // compat it reproduces every step bitwise.
        if withhold {
            assert!(
                first_mismatch.is_none() && steps_bitwise == 5,
                "withheld contact K: {steps_bitwise} steps bitwise, first mismatch {first_mismatch:?}"
            );
        } else {
            assert!(
                first_mismatch.is_some(),
                "the port's coupler was expected to differ from upstream"
            );
        }
        let _ = <SolidContactCallback as SolverCallback>::can_split_residual;
    }
    assert_eq!(
        o.rec["K/coupler_after_coupler_assembly"], o.rec["K/coupler_after_solid_only_assembly"],
        "upstream: the coupler's K carries no contact stiffness"
    );
}

/// The probe's `pf_selector_mismatch` (solid: one material on both
/// elements; phase field: two laws by physical names; inhomogeneous load)
/// replayed through the port in upstream compat (bitwise expected) and with
/// the corrected defaults (law `damage` storage differs by design; the
/// nodal fields are compared).
#[test]
fn v80_pf_selector_mismatch_vs_oracle() {
    use akantu_solid::material::LawOptions;
    const DAT: &str = include_str!("data/test_phase_field_model/material_multiple.dat");
    let o = oracle("pf_selector_mismatch.run1.txt");
    for upstream in [true, false] {
        let mesh = read_mesh(&pf_data("test_two_element.msh"), 2);
        let parser = Parser::parse_str(DAT, "material_multiple.dat").unwrap();
        let mut c = CouplerSolidPhaseField::with_parser(
            SharedMesh::new(mesh),
            None,
            "coupler_solid_phasefield",
            parser,
        )
        .unwrap();
        if upstream {
            c.set_upstream_compat(true);
            c.solid.physics.law_options = Some(LawOptions::upstream());
        }
        c.solid.init_full(&eigen(AnalysisMethod::Static)).unwrap();
        c.phase
            .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
        c.phase.init_full(&eigen(AnalysisMethod::Static)).unwrap();
        let mut compared = 0usize;
        let mut mismatch: Vec<String> = Vec::new();
        for s in 0..60usize {
            let Some((_, line)) = o.info.iter().find(|(k, _)| k == &format!("step_{s}")) else {
                break;
            };
            apply_displacement_with(&mut c, 2e-4 * s as Real, true);
            let r = c.solve(None, None);
            let si = newton(c.solid.solver.non_linear_solver(None).unwrap()).0;
            let pi = newton(c.phase.solver.non_linear_solver(None).unwrap()).0;
            let got = format!(
                "{} solid_it {si} phase_it {pi}",
                if r.is_ok() { "ok" } else { "not_converged" }
            );
            if &got != line {
                mismatch.push(format!("step {s}: '{got}' vs '{line}'"));
            }
            let mut cmp = |key: String, v: Vec<f64>| {
                let e = &o.rec[&key];
                if v.len() != e.len() || count_bits(&v, e) != e.len() {
                    mismatch.push(format!(
                        "{key}: {}/{} bitwise (len {} vs {})",
                        count_bits(&v, e),
                        e.len(),
                        v.len(),
                        e.len()
                    ));
                }
                compared += e.len();
            };
            cmp(
                format!("step_{s}/displacement"),
                c.solid.displacement().unwrap().as_slice().to_vec(),
            );
            cmp(
                format!("step_{s}/damage"),
                c.phase.damage().unwrap().as_slice().to_vec(),
            );
            for l in 0..2 {
                let st = c
                    .phase
                    .phase_field(l)
                    .unwrap()
                    .base()
                    .strain
                    .get(akantu_core::ElementType::Quadrangle4, NG)
                    .unwrap()
                    .as_slice()
                    .to_vec();
                cmp(format!("step_{s}/law/{l}/strain"), st);
            }
            let m = c.solid.material(0).unwrap();
            for what in ["damage", "stress"] {
                let mut v = Vec::new();
                m.for_each_internal(&mut |n, f| {
                    if n == what
                        && let InternalFieldRef::Real(f) = f
                    {
                        v.extend_from_slice(
                            f.get(akantu_core::ElementType::Quadrangle4, NG)
                                .unwrap()
                                .as_slice(),
                        );
                    }
                });
                cmp(format!("step_{s}/material/0/{what}"), v);
            }
            if r.is_err() {
                break;
            }
        }
        eprintln!(
            "upstream compat = {upstream}: {compared} values compared, {} mismatches; first: {:?}",
            mismatch.len(),
            mismatch.first()
        );
        if upstream {
            assert!(mismatch.is_empty());
        }
    }
}

/// Dense K from triplets (0-based); symmetric storage (one triangle) is
/// mirrored.
fn dense_from(irn: &[usize], jcn: &[usize], a: &[f64], n: usize) -> Vec<f64> {
    let set: std::collections::HashSet<(usize, usize)> =
        irn.iter().copied().zip(jcn.iter().copied()).collect();
    let one_triangle = !set.iter().any(|&(i, j)| i != j && set.contains(&(j, i)));
    let mut d = vec![0.0; n * n];
    for ((&i, &j), &v) in irn.iter().zip(jcn).zip(a) {
        d[i * n + j] += v;
        if one_triangle && i != j {
            d[j * n + i] += v;
        }
    }
    d
}

fn port_dense_k(c: &CouplerSolidCohesiveContact) -> (usize, Vec<f64>) {
    let k = c.solid.smm.solver.dof_manager.matrix("K").unwrap();
    let n = usize::try_from(k.size()).unwrap();
    let irn: Vec<usize> = k
        .irn_one_based()
        .into_iter()
        .map(|i| usize::try_from(i - 1).unwrap())
        .collect();
    let jcn: Vec<usize> = k
        .jcn_one_based()
        .into_iter()
        .map(|i| usize::try_from(i - 1).unwrap())
        .collect();
    (n, dense_from(&irn, &jcn, k.values(), n))
}

/// The coupler's K before the first solve (upstream: solid only, no contact
/// element yet) and after one solve, entry by entry against the oracle
/// (`v80_probe cohesive_static_k0`).
#[test]
fn v80_cohesive_static_k0_vs_oracle() {
    use akantu_couplers::SolidContactCallback;
    use akantu_solver::SolverCallback;
    let o = oracle("cohesive_static_k0.run1.txt");
    let inc: Real = 1e-3 / 50.0;
    let mut c = cohesive_static(false, 1e3);
    // upstream's configuration: UD-772 contact K withheld,
    // UD-374 per-iterate history (with the UD-034 compat of the helper)
    let compat = akantu_couplers::SolidContactCompat::all();
    c.set_upstream_compat(compat);
    {
        let nls = c.solid.smm.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(100))
            .unwrap();
    }
    let up = IncrementValue {
        value: -inc,
        axis: SpatialDirection::Y,
    };
    let down = IncrementValue {
        value: inc,
        axis: SpatialDirection::Y,
    };
    c.apply_dirichlet_bc(&up, "loading").unwrap();
    c.apply_dirichlet_bc(&down, "fixed").unwrap();
    let assemble = |c: &mut CouplerSolidCohesiveContact| {
        let akantu_solid::SolidMechanicsModel {
            physics, solver, ..
        } = &mut c.solid.smm;
        let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
        let mut cb =
            SolidContactCallback::coupler(&mut cohesive, &mut c.contact, AnalysisMethod::Static)
                .with_compat(compat);
        solver
            .dof_manager
            .collective("v80:K", |dm| {
                cb.assemble_matrix(dm, "K")?;
                dm.finalize_matrix("K")
            })
            .unwrap();
    };
    for (stage, solve) in [("K0", false), ("K1", true)] {
        if solve {
            c.solve_step(None).unwrap();
        }
        assemble(&mut c);
        let (n, kp) = port_dense_k(&c);
        let to_idx = |v: &Vec<f64>| v.iter().map(|x| *x as usize).collect::<Vec<usize>>();
        let ko = dense_from(
            &to_idx(&o.rec[&format!("{stage}/irn")]),
            &to_idx(&o.rec[&format!("{stage}/jcn")]),
            &o.rec[&format!("{stage}/a")],
            n,
        );
        let mut nd = 0;
        let mut worst = (0.0f64, 0usize, 0.0f64, 0.0f64);
        for i in 0..n * n {
            if !same_bits(kp[i], ko[i]) {
                nd += 1;
                let d = (kp[i] - ko[i]).abs();
                if d > worst.0 {
                    worst = (d, i, kp[i], ko[i]);
                }
            }
        }
        let u = c.solid.smm.displacement().unwrap().as_slice().to_vec();
        let eu = &o.rec[&format!("{stage}/displacement")];
        eprintln!(
            "{stage}: n = {n}, dense entries differing {nd}/{}; worst |dK| = {:e} at ({}, {}): port {:e} oracle {:e}; displacement bitwise {}/{}",
            n * n,
            worst.0,
            worst.1 / n,
            worst.1 % n,
            worst.2,
            worst.3,
            count_bits(&u, eu),
            eu.len()
        );
        assert_eq!(nd, 0, "{stage}: K differs from the oracle's");
        assert_eq!(count_bits(&u, eu), eu.len(), "{stage}: displacement");
    }
}

/// The cohesive model alone (no coupler, no contact) on the
/// `cohesive_static` setup, against the oracle (`v80_probe
/// cohesive_static_solo`): separates the cohesive model's static
/// intrinsic path from the coupled path.
#[test]
fn v80_cohesive_static_solo_vs_oracle() {
    let o = oracle("cohesive_static_solo.run1.txt");
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(COHESIVE_DAT, "material-cohesive.dat").unwrap();
    let mut solid = akantu_cohesive::SolidMechanicsModelCohesive::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: solid.default_cohesive_selector(),
    };
    solid.set_material_selector(Box::new(sel));
    solid.ext.compat = akantu_cohesive::material::CohesiveUpstreamCompat::upstream();
    solid
        .init_full(&eigen(AnalysisMethod::Static), false)
        .unwrap();
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    solid.smm.apply_dirichlet_bc(&fix, "sides").unwrap();
    solid
        .smm
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .set_param("max_iterations", ParamValue::Int(100))
        .unwrap();
    let inc: Real = 1e-3 / 50.0;
    for s in 0..4usize {
        let Some((_, line)) = o.info.iter().find(|(k, _)| k == &format!("step_{s}")) else {
            break;
        };
        let up = IncrementValue {
            value: -inc,
            axis: SpatialDirection::Y,
        };
        let down = IncrementValue {
            value: inc,
            axis: SpatialDirection::Y,
        };
        solid.smm.apply_dirichlet_bc(&up, "loading").unwrap();
        solid.smm.apply_dirichlet_bc(&down, "fixed").unwrap();
        let r = solid.solve_step(None);
        let (it, err) = newton(solid.smm.solver.non_linear_solver(None).unwrap());
        let got = format!("{} it {it}", if r.is_ok() { "ok" } else { "not_converged" });
        let u = solid.smm.displacement().unwrap().as_slice().to_vec();
        let eu = &o.rec[&format!("step_{s}/displacement")];
        eprintln!(
            "solo step {s}: port '{got}' err {err:e} / oracle '{line}' err {:e}; displacement bitwise {}/{}",
            o.rec[&format!("step_{s}/error")][0],
            count_bits(&u, eu),
            eu.len()
        );
        if r.is_err() {
            break;
        }
    }
}

/// Energy accounting under `CouplerSolidContact` (static Hertz, 12 steps)
/// against the oracle (`v80_probe coupler_energies`): the solid's
/// accumulated "external work new" (not accumulated under the coupler, as
/// upstream), "external work" and "potential", and the active set.
#[test]
fn v80_coupler_energies_vs_oracle() {
    use akantu_contact::PhysicalSurfaceSelector;
    use akantu_couplers::CouplerSolidContact;
    const HERTZ_DAT: &str = include_str!("data/contact/hertz_material.dat");
    let o = oracle("coupler_energies.run1.txt");
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
        2,
    );
    let parser = Parser::parse_str(HERTZ_DAT, "material.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let mut c = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(&eigen(AnalysisMethod::Static)).unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    let fix = |axis| FixedValue { value: 0.0, axis };
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::Y), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "loading")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "symmetry")
        .unwrap();
    let mut n = 0;
    for s in 0..12 {
        let inc = IncrementValue {
            value: -1e-4,
            axis: SpatialDirection::Y,
        };
        c.apply_dirichlet_bc(&inc, "loading").unwrap();
        c.solve_step(None).unwrap();
        let got = [
            c.solid.energy("external work new").unwrap(),
            c.solid.energy("external work").unwrap(),
            c.solid.energy("potential").unwrap(),
            c.contact.contact_elements().len() as f64,
        ];
        let e = &o.rec[&format!("step_{s}/energies")];
        for (i, (g, x)) in got.iter().zip(e).enumerate() {
            assert!(
                same_bits(*g, *x),
                "step {s} entry {i}: port {g:e} oracle {x:e}"
            );
        }
        n += 4;
        if s == 11 {
            eprintln!(
                "step 11: external work new {:e}, external work {:e}, potential {:e}, contact elements {}",
                got[0], got[1], got[2], got[3]
            );
        }
    }
    eprintln!("{n} values bitwise");
}

/// First Newton iterations of the `cohesive_static` increment, one by one
/// (`max_iterations = k`, fresh coupler each), against the oracle
/// (`v80_probe cohesive_static_iters`), with the port's coupler and with
/// the contact stiffness withheld (upstream's effective K).
#[test]
fn v80_cohesive_static_iterations_vs_oracle() {
    use akantu_couplers::SolidContactCallback;
    let o = oracle("cohesive_static_iters.run1.txt");
    let inc: Real = 1e-3 / 50.0;
    for withhold in [false, true] {
        for k in 1..=4i64 {
            let mut c = cohesive_static(false, 1e3);
            c.solid
                .smm
                .solver
                .non_linear_solver_mut(None)
                .unwrap()
                .set_param("max_iterations", ParamValue::Int(k))
                .unwrap();
            let up = IncrementValue {
                value: -inc,
                axis: SpatialDirection::Y,
            };
            let down = IncrementValue {
                value: inc,
                axis: SpatialDirection::Y,
            };
            c.apply_dirichlet_bc(&up, "loading").unwrap();
            c.apply_dirichlet_bc(&down, "fixed").unwrap();
            let _ = if withhold {
                let contact = &mut c.contact;
                c.solid.smm.solve_step_with(None, |physics, solver, id| {
                    let mut cohesive = akantu_cohesive::model::CohesiveCallback::new(physics);
                    let mut inner = SolidContactCallback::coupler(
                        &mut cohesive,
                        contact,
                        AnalysisMethod::Static,
                    );
                    let mut cb = WithoutContactK { inner: &mut inner };
                    solver.solve_step(&mut cb, id)
                })
            } else {
                c.solve_step(None)
            };
            let err = newton(c.solid.smm.solver.non_linear_solver(None).unwrap()).1;
            let pre = format!("k{k}");
            let err_same = same_bits(err, o.rec[&format!("{pre}/error")][0]);
            let mut all_bitwise = err_same;
            let mut line = format!("withhold={withhold} k={k}: err {err_same}");
            for (what, got) in [
                (
                    "displacement",
                    c.solid.smm.displacement().unwrap().as_slice().to_vec(),
                ),
                (
                    "contact_force",
                    c.contact.internal_force().as_slice().to_vec(),
                ),
                ("gaps", c.contact.gaps().as_slice().to_vec()),
                ("normals", c.contact.normals().as_slice().to_vec()),
                ("areas", c.contact.nodal_area().as_slice().to_vec()),
                (
                    "solid_internal_force",
                    c.solid.smm.internal_force().unwrap().as_slice().to_vec(),
                ),
            ] {
                let e = &o.rec[&format!("{pre}/{what}")];
                all_bitwise &= count_bits(&got, e) == e.len();
                line += &format!(" {what} {}/{}", count_bits(&got, e), e.len());
            }
            let ce = o
                .info
                .iter()
                .find(|(kk, _)| kk == &format!("{pre}_contact_elements"))
                .unwrap();
            line += &format!(
                " contact elements {} vs {}",
                c.contact.contact_elements().len(),
                ce.1
            );
            eprintln!("{line}");
            // Resolved (UD-034 compat): upstream's effective K gives
            // every recorded value of every capped iteration bitwise
            if withhold {
                assert!(all_bitwise, "{line}");
            }
        }
    }
}
