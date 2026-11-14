// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_damage_materials.cc (typed gtest ComputeStress for
// MaterialMazars<1,2,3> against the Python reference py_mazars.py; the
// DISABLED EnergyDensity / ComputeTangentModuli / ComputeCelerity are covered
// by inelastic_properties.rs), test_material_mazars.cc (material-point part)
// and the intent of py_mazars.py.

//! Same parameters, strain histories (1001 values `2e-6·i` of `ε_xx`, with
//! `ε_yy = ε_zz = −ν ε_xx` in 2-D plane stress / 3-D) and tolerances
//! (`|σ_xx − σ_ref| < 1e-5`, `|d − d_ref| < 1e-10`) as upstream. Upstream
//! calls `computeStressOnQuad` with a damage variable that persists across
//! the strain values and no history: the port evaluates the law repeatedly
//! without `beforeSolveStep`, which is the same thing. `py_mazars.py` is
//! transcribed below ([`PyMazars`]).

mod common;
mod inelastic_common;

use akantu_core::Real;
use akantu_solid::material::LawOptions;
use akantu_validation::reduce::NanMinMax;
use inelastic_common::Point;

/// Upstream `FriendMaterial<MaterialMazars<dim>>::setParams` as a `.dat`.
fn mazars_dat(dim: usize) -> String {
    format!(
        "material mazars [\n  name = concrete\n  K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  \
         Bc = 1391.3\n  beta = 1.\n  E = 25e9\n  nu = 0.2\n{}]\n",
        if dim == 2 {
            "  Plane_Stress = true\n"
        } else {
            ""
        }
    )
}

/// Transcription of `py_mazars.py` (`Mazars.compute_step`): uniaxial
/// traction damage `d_t` only.
struct PyMazars {
    k0: Real,
    at: Real,
    bt: Real,
    e: Real,
}

impl PyMazars {
    fn compute_step(&self, eps: Real, dam: Real) -> (Real, Real) {
        let mut dam_t = 0.;
        let sigma = self.e * eps;
        if eps > self.k0 {
            dam_t =
                1. - self.k0 * (1. - self.at) / eps - self.at * (-self.bt * (eps - self.k0)).exp();
        }
        let dam = dam.max(dam_t).min(1.);
        ((1. - dam) * sigma, dam)
    }
}

fn compute_stress(dim: usize) {
    let mut p = Point::new(dim, &mazars_dat(dim), LawOptions::default());
    let reference = PyMazars {
        k0: 1e-4,
        at: 1.0,
        bt: 5e3,
        e: 25e9,
    };
    let nu = if dim == 1 { 0. } else { 0.2 };
    let mut dam_ref = 0.;
    let mut max_ds: Real = 0.;
    let mut max_dd: Real = 0.;
    for i in 0..1001 {
        let eps = 2e-6 * f64::from(i);
        let mut strain = vec![0.0; dim * dim];
        strain[0] = eps;
        for d in 1..dim {
            strain[d * dim + d] = -nu * eps;
        }
        p.eval(&strain).unwrap();
        let sigma = p.stress()[0];
        let dam = p.scalar("damage");
        let (sigma_ref, d_ref) = reference.compute_step(eps, dam_ref);
        dam_ref = d_ref;
        assert!(
            (sigma - sigma_ref).abs() < 1e-5,
            "{dim}D step {i}: sigma {sigma} vs {sigma_ref}"
        );
        assert!(
            (dam - dam_ref).abs() < 1e-10,
            "{dim}D step {i}: d {dam} vs {dam_ref}"
        );
        max_ds = max_ds.nan_max((sigma - sigma_ref).abs());
        max_dd = max_dd.nan_max((dam - dam_ref).abs());
    }
    assert!(dam_ref > 0.9, "the history must reach strong damage");
    eprintln!("mazars {dim}D: max |dσ| = {max_ds:e}, max |dd| = {max_dd:e}");
}

#[test]
fn test_damage_materials_mazars_1d_compute_stress() {
    compute_stress(1);
}

#[test]
fn test_damage_materials_mazars_2d_compute_stress() {
    compute_stress(2);
}

#[test]
fn test_damage_materials_mazars_3d_compute_stress() {
    compute_stress(3);
}

/// Material-point intent of `test_material_mazars.cc` (upstream: one
/// `_hexahedron_8` driven explicitly; the program does not compile, UD-067):
/// uniaxial compression to `ε_xx = −0.003`, unloading to 0, compression to
/// `−0.004`; "after this second compression step softening should have
/// started": `σ_xx(−0.004) ≥ σ_xx(−0.003)` (smaller magnitude). The
/// lateral strains of the uniaxial stress state are those of the undamaged
/// material (`−ν ε_xx`); the parameters are `material_mazars.dat`'s.
#[test]
fn test_material_mazars_compression_softening() {
    let dat = "material mazars [\n  name = concrete\n  rho = 3000\n  E = 32e9\n  nu = 0.2\n  \
               K0 = 9.375e-5\n  At = 1.15\n  Bt = 10000\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.00\n]\n";
    let mut p = Point::new(3, dat, LawOptions::default());
    let nu = 0.2;
    let strain = |e: Real| vec![e, 0., 0., 0., -nu * e, 0., 0., 0., -nu * e];
    let n: u32 = 300;
    let mut path = Vec::new();
    path.extend((1..=n).map(|i| -0.003 * f64::from(i) / f64::from(n)));
    path.extend((1..=n).map(|i| -0.003 + 0.003 * f64::from(i) / f64::from(n)));
    path.extend((1..=n).map(|i| -0.004 * f64::from(i) / f64::from(n)));
    let mut sigma_first = 0.;
    let mut d_prev: Real = 0.;
    for (k, &e) in path.iter().enumerate() {
        p.step(&strain(e)).unwrap();
        let d = p.scalar("damage");
        assert!(
            d >= d_prev && (0. ..=1.).contains(&d),
            "damage monotone and bounded"
        );
        d_prev = d;
        if k + 1 == n as usize {
            sigma_first = p.stress()[0];
        }
    }
    let sigma_second = p.stress()[0];
    assert!(sigma_first < 0.);
    assert!(
        sigma_second >= sigma_first,
        "softening expected: σ(−0.004) = {sigma_second} vs σ(−0.003) = {sigma_first}"
    );
    // compression damage was driven by the lateral extension only
    assert!(d_prev > 0.);
}
