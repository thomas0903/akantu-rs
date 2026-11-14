// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_local_material.cc, local_material_damage.hh,
// local_material_damage.cc, local_material_damage_inline_impl.hh,
// material.dat}.

//! Port of upstream `test_local_material` (TEST-135; SMM-031 runtime
//! registration, SMM-030 quadrature loops): the test's user law
//! `LocalMaterialDamage` is registered in the model's material factory
//! under `local_damage` (upstream `MaterialFactory::registerAllocator`),
//! the model is built from `material.dat` on `mesh_section_gap.msh`
//! (frozen, D-009), dumped, run for 1100 explicit steps with
//! `FixedValue(0, x)` on `Fixed` and `FromHigherDim(5e7 I)` on `Traction`,
//! dumped again, and the material is looked up by name and checked to be
//! the user law (upstream's `dynamic_cast`). Upstream asserts nothing else;
//! here every state recorded by the oracle (`fixtures/model/smm2/
//! local_material`: damage, stress and displacement after the first three
//! steps, final damage, stress, displacement and velocity) is compared
//! bitwise and both dumps byte for byte.
//!
//! The law is transcribed statement by statement, including two defects of
//! the test's material (UD-684, test code, reproduced): the stress is
//! `λ tr(∇u) I + μ ε` (not `2μ ε`), and the loop's `++dam` increments the
//! **damage value** (the loop variable is a reference since the
//! `zip`-iterator rewrite), so every point is fully damaged (stress zero)
//! from its second evaluation on.
//!
//! SMM-030 (quadrature-loop macros): upstream writes the loop with `zip`
//! over `make_view(gradu)`, `make_view(stress)` and the damage internal; the
//! Rust law iterates the same three per-point slices
//! (`chunks_exact(dim²)`), which is the port's equivalent of the macros.

mod model_common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use akantu_core::enums::SpatialDirection;
use akantu_core::tensor::MatrixX;
use akantu_core::{Element, ElementType, GhostType, Real, Result};
use akantu_io::DumperParaview;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromHigherDim};
use akantu_model::constitutive_law::{LawSpec, ModelServices};
use akantu_parser::param::ParameterAccess;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};
use model_common::*;

const G: GhostType = GhostType::NotGhost;

/// `LocalMaterialDamage` (local_material_damage.{hh,cc,_inline_impl.hh}).
/// `corrected` = the law as intended (not a port of the test): stress
/// `λ tr(∇u) I + 2 μ ε` and no `++dam` (used only by the independent
/// closed-form check; the oracle replays use the upstream law).
#[derive(Debug, Default)]
struct LocalMaterialDamage {
    corrected: bool,
    e: Real,
    nu: Real,
    lambda: Real,
    mu: Real,
    kpa: Real,
    yd: Real,
    sd: Real,
}

impl LocalMaterialDamage {
    /// `computeStressOnQuad(grad_u, sigma, dam)` (column-major `dim × dim`,
    /// dynamic Eigen expressions: sequential reductions).
    fn stress_on_quad(&self, dim: usize, g: &[Real], sigma: &mut [Real], dam: &mut Real) {
        // Real trace = grad_u.trace();
        let mut trace = g[0];
        for k in 1..dim {
            trace += g[k + k * dim];
        }
        // epsilon = (grad_u + grad_u^T) / 2.
        let mut eps = [0.0; 9];
        for j in 0..dim {
            for i in 0..dim {
                eps[i + j * dim] = (g[i + j * dim] + g[j + i * dim]) / 2.;
            }
        }
        // sigma = Identity * trace * lambda + mu * epsilon (UD-684: mu, not
        // 2 mu; the corrected variant uses 2 mu)
        let mu = if self.corrected {
            2. * self.mu
        } else {
            self.mu
        };
        for j in 0..dim {
            for i in 0..dim {
                let id = if i == j { 1.0 } else { 0.0 };
                sigma[i + j * dim] = id * trace * self.lambda + mu * eps[i + j * dim];
            }
        }
        // Y = sigma.doubleDot(epsilon) / 2.
        let mut dd = sigma[0] * eps[0];
        for k in 1..dim * dim {
            dd += sigma[k] * eps[k];
        }
        let y = dd / 2.;
        let fd = y - self.yd - self.sd * *dam;
        if fd > 0. {
            *dam = (y - self.yd) / self.sd;
        }
        // dam = std::min(dam, 1.)
        if 1. < *dam {
            *dam = 1.;
        }
        // sigma *= 1 - dam
        let f = 1. - *dam;
        for s in sigma.iter_mut().take(dim * dim) {
            *s *= f;
        }
    }
}

impl ForeignMaterialMethods for LocalMaterialDamage {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        law.base_init_material()?; // Material::initMaterial()
        let p = &law.state.params;
        self.e = p.get_real("E")?;
        self.nu = p.get_real("nu")?;
        self.yd = p.get_real("Yd")?;
        self.sd = p.get_real("Sd")?;
        let (e, nu) = (self.e, self.nu);
        self.lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        self.mu = e / (2. * (1. + nu));
        self.kpa = self.lambda + 2. / 3. * self.mu;
        Ok(())
    }

    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        let ForeignLawState {
            base, internals, ..
        } = &mut *law.state;
        let dim = base.spatial_dimension;
        let d2 = dim * dim;
        let grad_u = base.grad_u.get(ty, g)?.as_slice().to_vec();
        let mut sigma_g = base.stress.get_mut(ty, g)?;
        let sigma = sigma_g.as_mut_slice();
        let mut damage_g = internals.real_mut("damage")?.get_mut(ty, g)?;
        let damage = damage_g.as_mut_slice();
        for ((gu, s), dam) in grad_u
            .chunks_exact(d2)
            .zip(sigma.chunks_exact_mut(d2))
            .zip(damage.iter_mut())
        {
            self.stress_on_quad(dim, gu, s, dam);
            // upstream `++dam;` on the damage reference (UD-684)
            if !self.corrected {
                *dam += 1.;
            }
        }
        Ok(())
    }

    /// `getCelerity(element)` = `sqrt(E / rho)` (overridden by the test
    /// law; the port's stable time step goes through the push wave speed).
    fn push_wave_speed(&self, state: &ForeignLawState, _: Element) -> Result<Real> {
        Ok((self.e / state.base.rho).sqrt())
    }
}

/// `MaterialFactory::getInstance().registerAllocator("local_damage", ...)`.
fn register_local_damage(model: &mut SolidMechanicsModel) {
    register_law(model, "local_damage", false);
}

/// Register the law under `name` (`corrected`: see [`LocalMaterialDamage`]).
fn register_law(model: &mut SolidMechanicsModel, name: &'static str, corrected: bool) {
    model
        .physics
        .factory
        .register(
            name,
            Arc::new(
                move |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut m = ForeignMaterial::new(
                        spec,
                        name,
                        Box::new(LocalMaterialDamage {
                            corrected,
                            ..LocalMaterialDamage::default()
                        }),
                    )?;
                    // upstream: E, nu `_pat_parsable` (the law reads its C++
                    // members); a runtime law reads its values through the
                    // registry, which hands them out only when readable
                    let parsable = ParameterAccess::PARSABLE | ParameterAccess::READABLE;
                    let parsmod = ParameterAccess::PARSMOD;
                    m.register_param_real("E", parsable, "Young's modulus")?;
                    m.register_param_real("nu", parsable, "Poisson's ratio")?;
                    m.register_param_real("Yd", parsmod, "")?;
                    m.register_param_real("Sd", parsmod, "")?;
                    m.register_internal_real("damage", 1)?;
                    Ok(Box::new(m))
                },
            ),
        )
        .unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("upstream_local_material")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn compare_tree(f: &akantu_validation::Fixture, dir: &Path) -> usize {
    let want: Vec<String> = param(f, "dump_files")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let mut bytes = 0;
    for rel in &want {
        let w = std::fs::read(f.input_path(&format!("dump:{rel}")).unwrap()).unwrap();
        let g = std::fs::read(dir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(g == w, "{rel}: differs from the oracle");
        bytes += w.len();
    }
    bytes
}

#[test]
fn test_local_material() {
    let f = case("smm2/local_material");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register_local_damage(&mut model);
    model.init_full(&ModelOptions::default()).unwrap();

    let out = scratch("local_material");
    model.set_directory(&out.join("paraview")).unwrap();
    model
        .dumpers
        .dumpable
        .dumper_as::<DumperParaview>("solid_mechanics_model")
        .unwrap()
        .set_compat_layout(true); // upstream file layout (UD-110/113/114)
    model.add_dump_field("damage").unwrap();
    model.add_dump_field("strain").unwrap();
    model.add_dump_field("stress").unwrap();
    model.add_dump_field_vector("displacement").unwrap();
    model.add_dump_field_vector("external_force").unwrap();
    model.add_dump_field_vector("internal_force").unwrap();
    model.dump(None, None).unwrap();

    let time_step = model.stable_time_step().unwrap();
    model.set_time_step(time_step / 2.5, None).unwrap();
    let mut n = assert_bits(&f, "time_step", &[time_step, time_step / 2.5]);
    model
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "Fixed",
        )
        .unwrap();
    let mut s = vec![0.0; 4];
    s[0] = 5e7;
    s[3] = 5e7;
    model
        .apply_neumann_bc(
            &FromHigherDim::new(MatrixX::from_col_major(2, 2, s).unwrap()),
            "Traction",
        )
        .unwrap();

    let ty = ElementType::Triangle6;
    let mat = model.material_index("concrete").unwrap();
    let state = |m: &SolidMechanicsModel| {
        let law = m.material(mat).unwrap();
        let mut damage = Vec::new();
        law.for_each_internal(&mut |name, field| {
            if name == "damage"
                && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = field
            {
                damage = f.get(ty, G).unwrap().as_slice().to_vec();
            }
        });
        let stress = law.base().stress.get(ty, G).unwrap().as_slice().to_vec();
        (damage, stress)
    };
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    assert_eq!(steps, 1100);
    for k in 0..steps {
        model.solve_step(None).unwrap();
        if k < 3 {
            let (d, st) = state(&model);
            n += assert_bits(&f, &format!("step_{}/damage", k + 1), &d);
            n += assert_bits(&f, &format!("step_{}/stress", k + 1), &st);
            n += assert_bits(
                &f,
                &format!("step_{}/displacement", k + 1),
                model.displacement().unwrap().as_slice(),
            );
        }
    }
    model.dump(None, None).unwrap();
    let (d, st) = state(&model);
    n += assert_bits(&f, "damage", &d);
    n += assert_bits(&f, "stress", &st);
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(&f, "velocity", model.velocity().unwrap().as_slice());
    // upstream: dynamic_cast<LocalMaterialDamage &>(model.getMaterial("concrete"))
    assert_eq!(
        model.material_by_name("concrete").unwrap().law_type(),
        "local_damage"
    );
    assert_eq!(param(&f, "law_type").as_str(), Some("local_damage"));
    // UD-684: every point fully damaged after its second evaluation
    assert!(d.iter().all(|&x| x == 2.0), "damage {:?}", &d[..4]);
    assert!(st.iter().all(|&x| x == 0.0));
    let bytes = compare_tree(&f, &out);
    eprintln!("test_local_material: {n} values bitwise, {bytes} dump bytes identical");
}

/// The probe's initial displacement of `local_material_live` (same
/// operation order): `u_x = 8e-3 x² + 5e-3 y²`, `u_y = -3e-3 x y`.
fn set_initial_displacement(model: &mut SolidMechanicsModel) {
    let x: Vec<Real> = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let mut u_g = model.displacement_mut().unwrap();
    let u = u_g.as_mut_slice();
    for (n, un) in u.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let (px, py) = (x[2 * n], x[2 * n + 1]);
        un[0] = 8e-3 * px * px + 5e-3 * py * py;
        un[1] = -3e-3 * px * py;
    }
}

/// `(grad_u, damage, stress)` of the law `mat` for `_triangle_6`.
fn law_state(m: &SolidMechanicsModel, mat: usize) -> (Vec<Real>, Vec<Real>, Vec<Real>) {
    let ty = ElementType::Triangle6;
    let law = m.material(mat).unwrap();
    let mut damage = Vec::new();
    law.for_each_internal(&mut |name, field| {
        if name == "damage"
            && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = field
        {
            damage = f.get(ty, G).unwrap().as_slice().to_vec();
        }
    });
    let grad_u = law.base().grad_u.get(ty, G).unwrap().as_slice().to_vec();
    let stress = law.base().stress.get(ty, G).unwrap().as_slice().to_vec();
    (grad_u, damage, stress)
}

/// TEST-135 variant with live stresses (oracle case
/// `fixtures/model/smm2/local_material_live`): the same user law, run and
/// boundary conditions, but a non-uniform initial displacement makes the
/// law's first evaluation non-zero. Every recorded value of the 5 steps is
/// compared bitwise (grad_u, damage, stress, internal force, displacement,
/// velocity), so the law's stress arithmetic is exercised, including the
/// test material's defects (UD-684): the stress `λ tr(∇u) I + μ ε`
/// (correcting it to `2 μ ε` changes the first step's stresses) and
/// `++dam` (every point fully damaged from its second evaluation on).
#[test]
fn test_local_material_live_stresses() {
    let f = case("smm2/local_material_live");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register_local_damage(&mut model);
    model.init_full(&ModelOptions::default()).unwrap();
    set_initial_displacement(&mut model);
    let time_step = model.stable_time_step().unwrap();
    model.set_time_step(time_step / 2.5, None).unwrap();
    let mut n = assert_bits(&f, "time_step", &[time_step, time_step / 2.5]);
    model
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "Fixed",
        )
        .unwrap();
    let mut s = vec![0.0; 4];
    s[0] = 5e7;
    s[3] = 5e7;
    model
        .apply_neumann_bc(
            &FromHigherDim::new(MatrixX::from_col_major(2, 2, s).unwrap()),
            "Traction",
        )
        .unwrap();
    n += assert_bits(
        &f,
        "initial_displacement",
        model.displacement().unwrap().as_slice(),
    );
    let mat = model.material_index("concrete").unwrap();
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    assert_eq!(steps, 5);
    let mut first = None;
    for k in 1..=steps {
        model.solve_step(None).unwrap();
        let (g, d, st) = law_state(&model, mat);
        n += assert_bits(&f, &format!("step_{k}/grad_u"), &g);
        n += assert_bits(&f, &format!("step_{k}/damage"), &d);
        n += assert_bits(&f, &format!("step_{k}/stress"), &st);
        n += assert_bits(
            &f,
            &format!("step_{k}/internal_force"),
            model.internal_force().unwrap().as_slice(),
        );
        n += assert_bits(
            &f,
            &format!("step_{k}/displacement"),
            model.displacement().unwrap().as_slice(),
        );
        n += assert_bits(
            &f,
            &format!("step_{k}/velocity"),
            model.velocity().unwrap().as_slice(),
        );
        if k == 1 {
            first = Some((d, st));
        }
    }
    // the first evaluation covers the law's three regimes: elastic
    // (damage 0, then ++dam = 1), damaging (0 < d < 1, stored d + 1) and
    // fully damaged (min(d, 1) = 1, stored 2); its stresses are non-zero
    let (d1, st1) = first.unwrap();
    let elastic = d1.iter().filter(|&&x| x == 1.0).count();
    let damaging = d1.iter().filter(|&&x| x > 1.0 && x < 2.0).count();
    let broken = d1.iter().filter(|&&x| x == 2.0).count();
    let live = st1.iter().filter(|&&x| x != 0.0).count();
    assert!(
        elastic > 0 && damaging > 0 && broken > 0,
        "regimes: {elastic} elastic, {damaging} damaging, {broken} fully damaged"
    );
    assert!(
        live > st1.len() / 4,
        "{live} of {} stresses non-zero",
        st1.len()
    );
    eprintln!(
        "test_local_material_live_stresses: {n} values bitwise; step 1: {elastic} elastic, \
         {damaging} damaging, {broken} fully damaged points, {live} non-zero stresses"
    );
}

/// Tolerance of the corrected-law closed-form check, relative to the
/// largest stress magnitude of the point: the law evaluates
/// `λ tr(ε) I + 2 μ ε` with `λ`, `μ` from `E`, `ν`; the closed form below
/// uses `E/(1+ν) (ε + ν/(1−2ν) tr(ε) I)` — the same quantity rounded
/// differently (a handful of roundings each, so a few ulps).
/// Recorded with its reason (measured 4.44e-16;
/// docs/NUMERICAL_VALIDATION.md).
const CLOSED_FORM_TOL: Real = 1e-14;

/// Closed-form small-strain stress of `(E, ν)` for a column-major `2 × 2`
/// displacement gradient.
fn closed_form(e: Real, nu: Real, g: &[Real]) -> [Real; 4] {
    let eps = [g[0], 0.5 * (g[1] + g[2]), 0.5 * (g[1] + g[2]), g[3]];
    let tr = eps[0] + eps[3];
    let c = e / (1. + nu);
    let l = nu / (1. - 2. * nu) * tr;
    [c * (eps[0] + l), c * eps[1], c * eps[2], c * (eps[3] + l)]
}

/// The user-law path with the law as intended (the
/// corrected law is exercised): the same `ForeignMaterial` registration
/// with `2 μ ε` and no `++dam`, compared with closed forms at every
/// quadrature point over three explicit steps (the stresses stay live at
/// every evaluation, unlike the upstream test law):
/// - `Yd` huge (no damage): `σ = λ tr(ε) I + 2 μ ε`;
/// - the test's `Yd`, `Sd`: `Y = σ₀:ε / 2`, `d = min((Y − Yd)/Sd, 1)` where
///   `Y > Yd + Sd d_prev` (else unchanged), `σ = (1 − d) σ₀`.
#[test]
fn corrected_local_material_law_matches_the_closed_form() {
    let f = case("smm2/local_material");
    for (yd, label) in [(1e30, "elastic"), (1e5, "damaging")] {
        let dat = format!(
            "material local_damage_corrected [\n name = concrete\n rho = 3000\n E = 40e9\n \
             nu = 0.3\n Yd = {yd:e}\n Sd = 2.748e6\n]\n"
        );
        let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
        let parser = akantu_parser::Parser::parse_str(&dat, "material.dat").unwrap();
        let mut model = SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        )
        .unwrap();
        register_law(&mut model, "local_damage_corrected", true);
        model.init_full(&ModelOptions::default()).unwrap();
        set_initial_displacement(&mut model);
        let dt = model.stable_time_step().unwrap() / 2.5;
        model.set_time_step(dt, None).unwrap();
        let mat = model.material_index("concrete").unwrap();
        let (e, nu, sd) = (40e9, 0.3, 2.748e6);
        let mut prev_d: Option<Vec<Real>> = None;
        let mut checked = 0;
        let mut damaged = 0;
        let mut worst = 0.0_f64;
        for step in 1..=3 {
            model.solve_step(None).unwrap();
            let (g, d, st) = law_state(&model, mat);
            let d_prev = prev_d.clone().unwrap_or_else(|| vec![0.0; d.len()]);
            for (q, ((gq, sq), (&dq, &dp))) in g
                .as_chunks::<4>()
                .0
                .iter()
                .zip(st.as_chunks::<4>().0)
                .zip(d.iter().zip(&d_prev))
                .enumerate()
            {
                let s0 = closed_form(e, nu, gq);
                let eps = [gq[0], 0.5 * (gq[1] + gq[2]), 0.5 * (gq[1] + gq[2]), gq[3]];
                let y = (s0[0] * eps[0] + s0[1] * eps[1] + s0[2] * eps[2] + s0[3] * eps[3]) / 2.;
                let fd = y - yd - sd * dp;
                if fd.abs() <= 1e-9 * y.abs().max(yd) {
                    continue; // branch decided by rounding: not checked
                }
                let want_d = if fd > 0. {
                    ((y - yd) / sd).min(1.)
                } else {
                    dp.min(1.)
                };
                let scale = s0.iter().fold(0.0_f64, |m, v| m.max(v.abs())).max(1.0);
                assert!(
                    (dq - want_d).abs() <= CLOSED_FORM_TOL * want_d.abs().max(1.0),
                    "{label} step {step} point {q}: damage {dq} vs closed form {want_d}"
                );
                worst = worst.max((dq - want_d).abs() / want_d.abs().max(1.0));
                for (a, b) in sq.iter().zip(&s0) {
                    let want = (1. - want_d) * b;
                    worst = worst.max((a - want).abs() / scale);
                    assert!(
                        (a - want).abs() <= CLOSED_FORM_TOL * scale,
                        "{label} step {step} point {q}: stress {a:e} vs closed form {want:e}"
                    );
                }
                damaged += usize::from(want_d > 0.);
                checked += 1;
            }
            assert!(
                st.iter().any(|&x| x != 0.0),
                "{label} step {step}: live stresses"
            );
            prev_d = Some(d);
        }
        if label == "elastic" {
            assert_eq!(damaged, 0);
        } else {
            assert!(damaged > 0, "the damaging run reaches the damage branch");
        }
        assert!(checked > 0);
        eprintln!(
            "corrected law ({label}): {checked} point evaluations checked, {damaged} damaged, \
             max relative deviation {worst:.2e}"
        );
    }
}
