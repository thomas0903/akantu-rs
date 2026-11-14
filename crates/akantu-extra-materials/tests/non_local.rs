// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative_non_local_serial.cc,
// material_non_local.dat, plate.msh} (first assertion block) and
// independent checks of the non-local variants
// (extra_packages/extra-materials/src/material_damage/*_non_local*).

//! The non-local extra laws (XMAT-002/006/008/011) on the solid model's
//! non-local manager. Upstream's package does
//! not compile at the reference commit: no oracle, the evidence is
//!
//! * the integration itself: the manager exists, registers the law's
//!   neighbourhood under the law's name with the declared variables, and
//!   averages them at every residual assembly;
//! * uniform fields: the manager's average equals the field to rounding
//!   (upstream's first assertion block, `Σ ‖∇u_nl − e_y⊗e_y‖ ≤ 1e-12`), and
//!   the non-local pass on an exactly uniform average reproduces the local
//!   law bit for bit;
//! * non-uniform fields: for every law, the manager's averages equal an
//!   independent brute-force average `Σ_j W(r_ij)·J_j·v_j / Σ_j W(r_ij)·J_j`
//!   computed here from the weight function `W(r) = (1 − r²/R²)²` (r ≤ R)
//!   and the quadrature points (rule XNL-1, `docs/NUMERICAL_VALIDATION.md`),
//!   and the law's non-local response is a function of the
//!   averaged variable, not of the local one;
//! * the declared rank reductions and the refusal of a distributed
//!   mesh.
//!
//! TEST-186's other blocks are in `upstream_damage_iterative_non_local.rs`.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_extra_materials::non_local::{
    MaterialBrittleNonLocal, MaterialDamageIterativeNonLocal,
    MaterialOrthotropicDamageIterativeNonLocal, MaterialVreePeerlingsNonLocal,
};
use akantu_extra_materials::{
    ExtraLawOptions, as_iterative_damage, register_extra_materials_with,
    register_extra_non_local_materials_with,
};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{InternalFieldRef, LawPhase};
use akantu_parser::Parser;
use akantu_solid::materials::non_local::MaterialNonLocal;
use akantu_solid::{Material as _, SolidMechanicsModel};
use common::{data, read_mesh};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;
const EPS: Real = Real::EPSILON;

fn model_opts(dat: &str, method: AnalysisMethod, options: ExtraLawOptions) -> SolidMechanicsModel {
    let mesh = read_mesh(&data("test_material_damage/plate.msh"), 2);
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register_extra_materials_with(&mut m.physics.factory, options).unwrap();
    register_extra_non_local_materials_with(&mut m.physics.factory, options).unwrap();
    m.init_full(&ModelOptions::new(method)).unwrap();
    m
}

fn model(dat: &str, method: AnalysisMethod) -> SolidMechanicsModel {
    model_opts(dat, method, ExtraLawOptions::default())
}

fn node_coords(m: &SolidMechanicsModel) -> Vec<Real> {
    m.physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/// `u = (0, y + 1)`: `∇u = e_y⊗e_y` (the upstream test's state after its
/// first solve: `top` at 2, `bottom` at 0 on the 2 × 2 plate); with
/// `extra_velocity`, `v = (0, 3y)`.
fn set_uniform(m: &mut SolidMechanicsModel, extra_velocity: bool) {
    set_field(
        m,
        |_, y| [0., y + 1.],
        extra_velocity.then_some(|_: Real, y: Real| [0., 3. * y]),
    );
}

/// Nodal displacement `u(x, y)` and, when given, velocity `v(x, y)`.
fn set_field(
    m: &mut SolidMechanicsModel,
    u: impl Fn(Real, Real) -> [Real; 2],
    v: Option<impl Fn(Real, Real) -> [Real; 2]>,
) {
    let x = node_coords(m);
    {
        let mut d = m.displacement_mut().unwrap();
        for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
            let val = u(p[0], p[1]);
            d.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&val);
        }
    }
    if let Some(v) = v {
        let mut d = m.velocity_mut().unwrap();
        for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
            let val = v(p[0], p[1]);
            d.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&val);
        }
    }
}

/// The real internal `name` of material 0 (block `T3`, not ghost).
fn internal(m: &SolidMechanicsModel, name: &str) -> Vec<Real> {
    let mut out = None;
    m.material(0).unwrap().for_each_internal(&mut |n, f| {
        if n == name
            && let InternalFieldRef::Real(f) = f
        {
            out = Some(f.get(T3, NG).unwrap().as_slice().to_vec());
        }
    });
    out.unwrap_or_else(|| panic!("no internal {name}"))
}

/// One stress cycle of material 0 with an **exactly** uniform average: the
/// local pass (`computeAllStresses`), average := local value (what the
/// manager computes up to rounding), then the non-local pass — on a model
/// whose manager has not run yet (a second non-local pass on the same local
/// pass would apply the damage twice).
fn emulate_exact_cycle(m: &mut SolidMechanicsModel) {
    m.with_material(0, LawPhase::Stress, |law, ctx| {
        law.compute_all_stresses(ctx, NG)?;
        let any = law.as_any_mut().unwrap();
        if let Some(l) = any.downcast_mut::<MaterialDamageIterativeNonLocal<2>>() {
            let g = l.local.base().grad_u.get(T3, NG)?.clone();
            l.grad_u_nl.get_mut(T3, NG)?.copy_from(&g)?;
            l.compute_non_local_stresses(ctx, NG)
        } else if let Some(l) = any.downcast_mut::<MaterialOrthotropicDamageIterativeNonLocal<2>>()
        {
            let g = l.local.base().grad_u.get(T3, NG)?.clone();
            l.grad_u_nl.get_mut(T3, NG)?.copy_from(&g)?;
            l.compute_non_local_stresses(ctx, NG)
        } else if let Some(l) = any.downcast_mut::<MaterialBrittleNonLocal<2>>() {
            let s = l.sigma_max.get(T3, NG)?.clone();
            l.sigma_max_nl.get_mut(T3, NG)?.copy_from(&s)?;
            l.compute_non_local_stresses(ctx, NG)
        } else if let Some(l) = any.downcast_mut::<MaterialVreePeerlingsNonLocal<2>>() {
            let a = l.local.equi_strain.get(T3, NG)?.clone();
            let b = l.local.equi_strain_rate.get(T3, NG)?.clone();
            l.equi_strain_nl.get_mut(T3, NG)?.copy_from(&a)?;
            l.equi_strain_rate_nl.get_mut(T3, NG)?.copy_from(&b)?;
            l.compute_non_local_stresses(ctx, NG)
        } else {
            panic!("not a non-local extra law")
        }
    })
    .unwrap();
}

fn stress(m: &SolidMechanicsModel) -> Vec<Real> {
    m.material(0)
        .unwrap()
        .base()
        .stress
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

const LOCAL_DI: &str = "material damage_iterative [\n name = test_material\n rho = 2.7e-9\n E = 1\n nu = 0.\n Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.99\n]\n";

/// The four keywords, their local counterparts, parameters and the analysis
/// method their local pass needs (brittle and Vree–Peerlings read the
/// velocity).
const LAWS: [(&str, &str, &str, AnalysisMethod); 4] = [
    (
        "damage_iterative_non_local",
        "damage_iterative",
        " E = 1\n nu = 0.2\n Sc = 10\n",
        AnalysisMethod::Static,
    ),
    (
        "brittle_non_local",
        "brittle",
        " E = 1e9\n nu = 0.2\n",
        AnalysisMethod::ExplicitLumpedMass,
    ),
    (
        "vreepeerlings_non_local",
        "vreepeerlings",
        " E = 1e9\n nu = 0.2\n Kapaoi = 0.1\n Kapac = 2\n Crate = 0.1\n",
        AnalysisMethod::ExplicitLumpedMass,
    ),
    (
        "damage_orthotropic_iterative_non_local",
        "orthotropic_damage_iterative",
        " E = 1\n nu = 0.2\n Sc = 10\n",
        AnalysisMethod::Static,
    ),
];

fn dat(kw: &str, params: &str, radius: Real) -> String {
    format!(
        "material {kw} [\n name = m\n rho = 1\n{params}]\n\
         non_local m base_wf [\n radius = {radius}\n]\n"
    )
}

// ------------------------------------------------------------ integration

/// Every wrapper is a
/// `MaterialNonLocal` the solid model discovers; the model creates its
/// manager at `init_full`, registers the neighbourhood named after the law
/// (with the radius of its `non_local` section) and the law's declared
/// variables, inserts every quadrature point of the law, and averages at
/// every residual assembly — no attach step, no refusal.
#[test]
fn the_four_wrappers_are_driven_by_the_solid_model_manager() {
    let expected: [&[(&str, &str, usize)]; 4] = [
        &[("grad_u", "grad_u non local", 4)],
        &[("Sigma max", "Sigma_max non local", 1)],
        &[
            ("equi-strain", "equi-strain_non_local", 1),
            ("equi-strain-rate", "equi-strain-rate_non_local", 1),
        ],
        &[("grad_u", "grad_u non local", 4)],
    ];
    for ((kw, _, params, method), vars) in LAWS.into_iter().zip(expected) {
        let mut m = model(&dat(kw, params, 0.5), method);
        let nb_el = m.physics.base.mesh.read().unwrap().nb_element(T3, NG);
        {
            let law = m.material(0).unwrap();
            assert_eq!(law.law_type(), kw);
            let nl = law.as_non_local().expect("a MaterialNonLocal");
            assert_eq!(nl.neighborhood_name(), "m");
            assert_eq!(nl.weight_function_id(), "m");
            let declared: Vec<_> = nl
                .non_local_variables()
                .into_iter()
                .map(|v| (v.local, v.non_local, v.nb_component))
                .collect();
            let want: Vec<_> = vars
                .iter()
                .map(|&(l, n, c)| (l.to_owned(), n.to_owned(), c))
                .collect();
            assert_eq!(declared, want, "{kw}");
        }
        let manager = m.non_local_manager().expect("a non-local manager");
        let n = manager.neighborhood("m").unwrap();
        assert_eq!(n.radius(), 0.5, "{kw}");
        assert_eq!(n.points().len(), nb_el, "{kw}: every quadrature point");
        let decl = manager.declared_variables("m").unwrap();
        for &(l, nl, c) in vars {
            let v = &decl[nl];
            assert_eq!((v.local.as_str(), v.nb_component), (l, c), "{kw}");
        }
        assert_eq!(manager.nb_stress_calls(), 0);
        set_uniform(&mut m, method != AnalysisMethod::Static);
        m.assemble_internal_forces().unwrap();
        let manager = m.non_local_manager().unwrap();
        assert_eq!(manager.nb_stress_calls(), 1, "{kw}: one averaging cycle");
        for &(_, nl, c) in vars {
            assert_eq!(manager.averages(nl).unwrap().len(), nb_el * c, "{kw}");
        }
    }
}

// ---------------------------------------------------------- uniform fields

/// Upstream `test_material_damage_iterative_non_local_serial`, first block,
/// through the real manager: the averaged `∇u` of the uniform field is
/// `e_y⊗e_y` (`Σ ‖∇u_nl − e_y⊗e_y‖ ≤ 1e-12`, upstream's criterion); with an
/// exactly uniform average the non-local law then matches the local
/// `damage_iterative` bit for bit (stress, equivalent stress, maximum).
#[test]
fn damage_iterative_non_local_uniform_field_matches_local() {
    let dat = std::fs::read_to_string(data("test_material_damage/material_non_local.dat")).unwrap();
    let mut nl = model(&dat, AnalysisMethod::Static);
    set_uniform(&mut nl, false);
    nl.assemble_internal_forces().unwrap();
    let mut diff = 0.;
    for g in internal(&nl, "grad_u non local").as_chunks::<4>().0 {
        let d = [g[0], g[1], g[2], g[3] - 1.];
        diff += d.iter().map(|x| x * x).sum::<Real>().sqrt();
    }
    assert!(
        diff <= 10e-13,
        "Error in the non-local grad_u computation: {diff}"
    );
    eprintln!("uniform field: Σ ‖∇u_nl − e_y⊗e_y‖ = {diff:e}");

    let mut nl = model(&dat, AnalysisMethod::Static);
    set_uniform(&mut nl, false);
    emulate_exact_cycle(&mut nl);
    let mut local = model(LOCAL_DI, AnalysisMethod::Static);
    set_uniform(&mut local, false);
    local.assemble_internal_forces().unwrap();
    assert_eq!(stress(&nl), stress(&local));
    assert_eq!(
        internal(&nl, "equivalent_stress"),
        internal(&local, "equivalent_stress")
    );
    assert!(
        internal(&nl, "equivalent_stress")
            .iter()
            .all(|&e| akantu_core::math::are_float_equal(e, 0.1, 1e-13))
    );
    assert_eq!(
        as_iterative_damage(nl.material(0).unwrap())
            .unwrap()
            .norm_max_equivalent_stress(),
        as_iterative_damage(local.material(0).unwrap())
            .unwrap()
            .norm_max_equivalent_stress()
    );
}

/// The other variants: the manager's average of a uniform field is the
/// brute force (XNL-1) and the field itself (XMAT-T1), and with an exactly
/// uniform average the non-local law equals its local law bit for bit
/// (stress and damage after the non-local pass).
#[test]
fn other_non_local_variants_uniform_field_match_local() {
    for (nl_kw, local_kw, params, method) in &LAWS[1..] {
        let params = if *nl_kw == "damage_orthotropic_iterative_non_local" {
            " E = 1\n nu = 0\n Sc = 10\n"
        } else {
            params
        };
        let mut nl = model(&dat(nl_kw, params, 0.5), *method);
        set_uniform(&mut nl, *method != AnalysisMethod::Static);
        nl.assemble_internal_forces().unwrap();
        let vars: &[(&str, &str)] = match *nl_kw {
            "brittle_non_local" => &[("Sigma max", "Sigma_max non local")],
            "vreepeerlings_non_local" => &[
                ("equi-strain", "equi-strain_non_local"),
                ("equi-strain-rate", "equi-strain-rate_non_local"),
            ],
            _ => &[("grad_u", "grad_u non local")],
        };
        for (l, a) in vars {
            let nc = if *l == "grad_u" { 4 } else { 1 };
            // the brute force (XNL-1), and the average is the field (closed
            // form of a uniform field, rule XMAT-T1)
            let level = check_brute_force(&nl, l, a, nc, 0.5);
            let scale = internal(&nl, l)
                .iter()
                .fold(0., |m: Real, x| m.max(x.abs()));
            assert!(
                scale > 0. && close(level.max_departure, 0., scale),
                "{nl_kw} {a}"
            );
        }
        let mut nl = model(&dat(nl_kw, params, 0.5), *method);
        set_uniform(&mut nl, *method != AnalysisMethod::Static);
        emulate_exact_cycle(&mut nl);
        let dat_local = format!("material {local_kw} [\n name = m\n rho = 1\n{params}]\n");
        let mut local = model(&dat_local, *method);
        set_uniform(&mut local, *method != AnalysisMethod::Static);
        local.assemble_internal_forces().unwrap();
        assert_eq!(stress(&nl), stress(&local), "{nl_kw}");
        // the uniform loading damages brittle and Vree–Peerlings points
        if *nl_kw != "damage_orthotropic_iterative_non_local" {
            let (dn, dl) = (internal(&nl, "damage"), internal(&local, "damage"));
            assert_eq!(dn, dl, "{nl_kw}");
            assert!(dl.iter().any(|&x| x > 0.), "{nl_kw}: undamaged");
        }
    }
}

// ------------------------------------------------- non-uniform: brute force

/// Inputs of the brute-force average: quadrature-point coordinates and
/// jacobians (`J = w_q·det(∂x/∂ξ)`) of the law's elements, in filter order.
fn quadrature(m: &SolidMechanicsModel) -> (Vec<[Real; 2]>, Vec<Real>) {
    let mesh = m.physics.base.mesh.read().unwrap();
    let fe = m.physics.base.fems.get("").unwrap();
    let filter = m
        .material(0)
        .unwrap()
        .base()
        .element_filter
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let nb = mesh.nb_element(T3, NG);
    assert_eq!(filter.len(), nb, "one law on the whole plate");
    let mut xq = Array::new(2);
    fe.integration_points_coordinates(&mesh, &mut xq, T3, NG, None)
        .unwrap();
    let jac = fe.jacobians(T3, NG).unwrap();
    assert_eq!(
        fe.nb_integration_points(T3, NG).unwrap(),
        1,
        "T3: one point"
    );
    let x: Vec<[Real; 2]> = filter
        .iter()
        .map(|&e| {
            let t = xq.tuple(e as usize);
            [t[0], t[1]]
        })
        .collect();
    let j: Vec<Real> = filter.iter().map(|&e| jac.as_slice()[e as usize]).collect();
    // The FE engine's data checked against the node
    // coordinates, so that the reference is independent of the engine too:
    // for one-point T3, x_q is the centroid and J = w_q·det(∂x/∂ξ) the
    // element's area (w_q = 1/2, det = twice the area). Both sides are a
    // few roundings of the same coordinates, hence 8 ulp of the plate's
    // largest coordinate / of the area.
    let nodes = mesh.nodes();
    let nodes = nodes.as_slice();
    let conn = mesh.connectivity(T3, NG).unwrap();
    let conn = conn.as_slice();
    let scale = nodes.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    for (k, &e) in filter.iter().enumerate() {
        let e = usize::try_from(e).unwrap();
        let p: Vec<[Real; 2]> = conn[3 * e..3 * e + 3]
            .iter()
            .map(|&n| {
                let n = usize::try_from(n).unwrap();
                [nodes[2 * n], nodes[2 * n + 1]]
            })
            .collect();
        for d in 0..2 {
            let c = (p[0][d] + p[1][d] + p[2][d]) / 3.;
            assert!(
                (x[k][d] - c).abs() <= 8. * Real::EPSILON * scale,
                "element {e}: x_q[{d}] = {} vs centroid {c}",
                x[k][d]
            );
        }
        let area = 0.5
            * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]))
                .abs();
        assert!(
            (j[k] - area).abs() <= 8. * Real::EPSILON * area,
            "element {e}: J = {} vs area {area}",
            j[k]
        );
    }
    (x, j)
}

/// The weight function of the `base_wf` sections, written from its
/// definition (upstream `BaseWeightFunction`: `(1 − r²/R²)²` for `r ≤ R`).
fn w(r: Real, radius: Real) -> Real {
    if r <= radius {
        let a = 1. - (r * r) / (radius * radius);
        a * a
    } else {
        0.
    }
}

/// Result of a brute-force comparison: the largest `|avg − bf| / (ε·S)`
/// (`S = Σ W·J·|v| / Σ W·J` per point and component), the neighbour counts,
/// and the largest `|avg − v|` (the field is not uniform at that scale).
struct Level {
    max_units: Real,
    min_neighbours: usize,
    max_neighbours: usize,
    max_departure: Real,
}

/// Rule XNL-1: the manager's average of `local` into `averaged` (`nc`
/// components per point) equals the brute-force
/// `Σ_j W(r_ij)·J_j·v_j / Σ_j W(r_ij)·J_j` to `2(n_i + 2)·ε·S_i`.
fn check_brute_force(
    m: &SolidMechanicsModel,
    local: &str,
    averaged: &str,
    nc: usize,
    radius: Real,
) -> Level {
    let (x, jac) = quadrature(m);
    let v = internal(m, local);
    let avg = internal(m, averaged);
    let n = x.len();
    assert_eq!((v.len(), avg.len()), (n * nc, n * nc), "{averaged}");
    let mut level = Level {
        max_units: 0.,
        min_neighbours: usize::MAX,
        max_neighbours: 0,
        max_departure: 0.,
    };
    for i in 0..n {
        let mut num = vec![0.; nc];
        let mut abs_num = vec![0.; nc];
        let mut den = 0.;
        let mut nb = 0;
        for j in 0..n {
            let dx = x[i][0] - x[j][0];
            let dy = x[i][1] - x[j][1];
            let r = (dx * dx + dy * dy).sqrt();
            let wj = w(r, radius);
            if r <= radius {
                nb += 1;
            }
            for c in 0..nc {
                num[c] += wj * jac[j] * v[j * nc + c];
                abs_num[c] += wj * jac[j] * v[j * nc + c].abs();
            }
            den += wj * jac[j];
        }
        level.min_neighbours = level.min_neighbours.min(nb);
        level.max_neighbours = level.max_neighbours.max(nb);
        for c in 0..nc {
            let bf = num[c] / den;
            let s = abs_num[c] / den;
            let got = avg[i * nc + c];
            let diff = (got - bf).abs();
            assert!(
                diff <= 2. * (nb as Real + 2.) * EPS * s,
                "{averaged}[{i}][{c}]: manager {got:e} vs brute force {bf:e} ({nb} neighbours, S = {s:e})"
            );
            if s > 0. {
                level.max_units = level.max_units.max(diff / (EPS * s));
            }
            level.max_departure = level.max_departure.max((got - v[i * nc + c]).abs());
        }
    }
    eprintln!(
        "{averaged}: max |avg − bf| = {:.3} ε·S, neighbours {}…{} of {n}, max |avg − v| = {:e}",
        level.max_units, level.min_neighbours, level.max_neighbours, level.max_departure
    );
    level
}

/// A smooth non-uniform displacement on the plate: piecewise-constant,
/// element-wise different gradients on the T3 mesh.
fn wavy(x: Real, y: Real) -> [Real; 2] {
    [
        1e-3 * (0.7 * x * x + 0.4 * x * y - 0.2 * y),
        1e-3 * (0.9 * y * y - 0.5 * x * x + 0.3 * x),
    ]
}

/// Plane-strain (2-D default) elastic stress of `∇u` (`σ_th = 0`).
fn elastic_stress(g: &[Real], e: Real, nu: Real) -> [Real; 3] {
    let lambda = e * nu / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let (exx, eyy, exy) = (g[0], g[3], 0.5 * (g[1] + g[2]));
    let tr = exx + eyy;
    [
        lambda * tr + 2. * mu * exx,
        lambda * tr + 2. * mu * eyy,
        2. * mu * exy,
    ]
}

/// Largest principal value of a symmetric 2 × 2 `[sxx, syy, sxy]`.
fn max_principal(s: [Real; 3]) -> Real {
    let m = 0.5 * (s[0] + s[1]);
    let r = (0.25 * (s[0] - s[1]) * (s[0] - s[1]) + s[2] * s[2]).sqrt();
    m + r
}

/// Rule XMAT-T1: relative `1e-12` for closed forms.
fn close(a: Real, b: Real, scale: Real) -> bool {
    (a - b).abs() <= 1e-12 * scale
}

/// `damage_iterative_non_local` and `damage_orthotropic_iterative_non_local`
/// on a non-uniform field (radius 0.6 on the 2 × 2 plate: partial
/// neighbourhoods): `grad_u non local` equals the brute-force average
/// (XNL-1), and the normalised equivalent stress of every point is the
/// largest principal stress of the **averaged** gradient over `Sc` (closed
/// form, rule XMAT-T1), which differs from that of the local gradient by
/// far more than the rule's bound; the maximum is the largest point value.
#[test]
fn iterative_non_local_laws_average_grad_u_like_a_brute_force_sum() {
    let radius = 0.6;
    for (kw, params) in [
        ("damage_iterative_non_local", LAWS[0].2),
        ("damage_orthotropic_iterative_non_local", LAWS[3].2),
    ] {
        let mut m = model(&dat(kw, params, radius), AnalysisMethod::Static);
        set_field(&mut m, wavy, None::<fn(Real, Real) -> [Real; 2]>);
        m.assemble_internal_forces().unwrap();
        let level = check_brute_force(&m, "grad_u", "grad_u non local", 4, radius);
        assert!(level.min_neighbours < level.max_neighbours);
        assert!(level.max_departure > 1e-5, "the field is not uniform");
        let (g, gnl) = (internal(&m, "grad_u"), internal(&m, "grad_u non local"));
        let eq = internal(&m, "equivalent_stress");
        let (e, nu, sc) = (1., 0.2, 10.);
        let scale = 1e-3;
        let mut max_local_gap: Real = 0.;
        let mut max_closed_form: Real = 0.;
        for (q, &eqq) in eq.iter().enumerate() {
            let want = max_principal(elastic_stress(&gnl[4 * q..4 * q + 4], e, nu)) / sc;
            assert!(
                close(eqq, want, scale / sc),
                "{kw} q{q}: {eqq:e} vs {want:e}"
            );
            max_closed_form = max_closed_form.max((eqq - want).abs() / (scale / sc));
            let local = max_principal(elastic_stress(&g[4 * q..4 * q + 4], e, nu)) / sc;
            max_local_gap = max_local_gap.max((local - eqq).abs());
        }
        eprintln!(
            "{kw}: equivalent stress vs closed form of the average: {max_closed_form:e} \
             (relative to the scale); local vs averaged: {:e}",
            max_local_gap / (scale / sc)
        );
        assert!(
            max_local_gap > 1e3 * 1e-12 * scale / sc,
            "{kw}: the local gradient gives the same equivalent stress"
        );
        let max = eq.iter().copied().fold(Real::NEG_INFINITY, Real::max);
        assert_eq!(
            as_iterative_damage(m.material(0).unwrap())
                .unwrap()
                .norm_max_equivalent_stress(),
            max,
            "{kw}"
        );
    }
}

/// `brittle_non_local` on a non-uniform field: `Sigma max` (the local
/// largest principal stress) is averaged into `Sigma_max non local` as the
/// brute force (XNL-1); a point breaks (`d = 1`, stress `0`) exactly where
/// the **averaged** value exceeds the fracture stress, and that set differs
/// from the one of the local values (so the criterion is not local).
#[test]
fn brittle_non_local_averages_sigma_max_like_a_brute_force_sum() {
    let radius = 0.6;
    let (e, nu) = (1e9, 0.2);
    // S_0 between the smallest and the largest averaged σ_eq (set below)
    let probe = {
        let mut m = model(
            &dat("brittle_non_local", LAWS[1].2, radius),
            AnalysisMethod::ExplicitLumpedMass,
        );
        set_field(&mut m, wavy, Some(|_: Real, _: Real| [0., 0.]));
        m.assemble_internal_forces().unwrap();
        internal(&m, "Sigma_max non local")
    };
    let (lo, hi) = probe
        .iter()
        .fold((Real::INFINITY, Real::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    // an integer value: read back exactly by the input parser
    let s0 = (0.5 * (lo + hi)).round();
    let params = format!(" E = {e:e}\n nu = {nu}\n S_0 = {s0}\n");
    let mut m = model(
        &dat("brittle_non_local", &params, radius),
        AnalysisMethod::ExplicitLumpedMass,
    );
    set_field(&mut m, wavy, Some(|_: Real, _: Real| [0., 0.]));
    m.assemble_internal_forces().unwrap();
    let level = check_brute_force(&m, "Sigma max", "Sigma_max non local", 1, radius);
    assert!(level.max_departure > 1e-3 * (hi - lo));
    let (smax, snl) = (
        internal(&m, "Sigma max"),
        internal(&m, "Sigma_max non local"),
    );
    let frac = internal(&m, "Sigma_fracture");
    let dam = internal(&m, "damage");
    let sigma = stress(&m);
    let g = internal(&m, "grad_u");
    let mut differs = false;
    let mut max_closed_form: Real = 0.;
    for q in 0..dam.len() {
        assert_eq!(frac[q], s0, "zero strain rate: S_0");
        let broken = snl[q] > frac[q];
        assert_eq!(dam[q], if broken { 1. } else { 0. }, "q{q}");
        let el = elastic_stress(&g[4 * q..4 * q + 4], e, nu);
        let s = &sigma[4 * q..4 * q + 4];
        if broken {
            assert!(
                s.iter().all(|&v| v == 0.),
                "q{q}: broken point keeps stress"
            );
        } else {
            assert!(
                close(s[0], el[0], 1e6) && close(s[3], el[1], 1e6) && close(s[1], el[2], 1e6),
                "q{q}"
            );
            for (a, b) in [(s[0], el[0]), (s[3], el[1]), (s[1], el[2])] {
                max_closed_form = max_closed_form.max((a - b).abs() / 1e6);
            }
        }
        differs |= broken != (smax[q] > frac[q]);
    }
    eprintln!(
        "brittle_non_local: intact stress vs closed form: {max_closed_form:e} (relative to 1e6)"
    );
    assert!(dam.contains(&1.) && dam.contains(&0.));
    assert!(
        differs,
        "the averaged criterion breaks the same points as the local one"
    );
}

/// `vreepeerlings_non_local` on non-uniform displacement and velocity
/// fields: both averaged variables (`equi-strain`, `equi-strain-rate`)
/// equal the brute force (XNL-1); the damage of every point is the law's
/// point update applied to the **averaged** values (bit for bit), which
/// differs from the update of the local values somewhere.
#[test]
fn vreepeerlings_non_local_averages_both_strains_like_a_brute_force_sum() {
    let radius = 0.6;
    let vel = |x: Real, y: Real| [2e-1 * (x * y + 0.3 * y), 2e-1 * (0.5 * x * x - y)];
    let probe = {
        let mut m = model(
            &dat("vreepeerlings_non_local", LAWS[2].2, radius),
            AnalysisMethod::ExplicitLumpedMass,
        );
        set_field(&mut m, wavy, Some(vel));
        m.assemble_internal_forces().unwrap();
        internal(&m, "equi-strain_non_local")
    };
    let (lo, hi) = probe
        .iter()
        .fold((Real::INFINITY, Real::NEG_INFINITY), |(a, b), &x| {
            (a.min(x), b.max(x))
        });
    // damage starts at Kapaoi: put it inside the averaged range
    let kapaoi = 0.5 * (lo + hi);
    let params = format!(
        " E = 1e9\n nu = 0.2\n Kapaoi = {kapaoi:e}\n Kapac = {:e}\n Crate = 1e-6\n",
        10. * hi
    );
    let mut m = model(
        &dat("vreepeerlings_non_local", &params, radius),
        AnalysisMethod::ExplicitLumpedMass,
    );
    set_field(&mut m, wavy, Some(vel));
    m.assemble_internal_forces().unwrap();
    let l1 = check_brute_force(&m, "equi-strain", "equi-strain_non_local", 1, radius);
    let l2 = check_brute_force(
        &m,
        "equi-strain-rate",
        "equi-strain-rate_non_local",
        1,
        radius,
    );
    assert!(l1.max_departure > 1e-3 * (hi - lo) && l2.max_departure > 0.);
    let es = internal(&m, "equi-strain");
    let esr = internal(&m, "equi-strain-rate");
    let es_nl = internal(&m, "equi-strain_non_local");
    let esr_nl = internal(&m, "equi-strain-rate_non_local");
    let dam = internal(&m, "damage");
    let kapa = internal(&m, "Kapa");
    let law = m
        .material(0)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialVreePeerlingsNonLocal<2>>()
        .unwrap();
    let mut differs = false;
    for q in 0..dam.len() {
        let (d, ..) = law
            .local
            .damage_on_quad(0., es_nl[q], esr_nl[q], kapa[q], (0., 0., 0.));
        assert_eq!(dam[q].to_bits(), d.to_bits(), "q{q}");
        let (dl, ..) = law
            .local
            .damage_on_quad(0., es[q], esr[q], kapa[q], (0., 0., 0.));
        differs |= dl != d;
    }
    assert!(dam.iter().any(|&d| d > 0.) && dam.contains(&0.));
    assert!(differs, "the averaged update damages as the local one");
}

// --------------------------------------------------- declared reductions

/// Records the declared reductions; replaces every value with `value`.
struct RecordingReducer {
    nb_ranks: usize,
    value: Real,
    ops: Vec<akantu_comm::ReduceOp>,
}

impl akantu_model::constitutive_law::LawReducer for RecordingReducer {
    fn nb_ranks(&self) -> usize {
        self.nb_ranks
    }
    fn all_reduce_real(
        &mut self,
        values: &mut [Real],
        op: akantu_comm::ReduceOp,
    ) -> akantu_core::Result<()> {
        self.ops.push(op);
        values.fill(self.value);
        Ok(())
    }
}

/// The declared reduction of material 0 at 1 and 2 ranks: (operations,
/// maximum after) for each.
fn reductions(m: &mut SolidMechanicsModel) -> [(Vec<akantu_comm::ReduceOp>, Real); 2] {
    m.with_material(0, LawPhase::Stress, |law, _| {
        let mut out = Vec::new();
        for nb_ranks in [1, 2] {
            let mut r = RecordingReducer {
                nb_ranks,
                value: 7.25,
                ops: Vec::new(),
            };
            law.as_non_local_mut()
                .unwrap()
                .reduce_after_non_local_stresses(NG, &mut r)?;
            let max = as_iterative_damage(&*law)
                .unwrap()
                .norm_max_equivalent_stress();
            out.push((r.ops, max));
        }
        Ok([out.remove(0), out.remove(0)])
    })
    .unwrap()
}

/// The closing `allReduce(_max)` of upstream's
/// `MaterialDamageIterativeNonLocal::computeNonLocalStresses` is a declared
/// reduction (`reduce_after_non_local_stresses`), never a raw communicator
/// call: one `Max` reduction of the maximum on more than one rank, none on
/// one rank; the non-local pass itself does not communicate.
#[test]
fn damage_iterative_non_local_declares_its_maximum_reduction() {
    let dat = std::fs::read_to_string(data("test_material_damage/material_non_local.dat")).unwrap();
    let mut nl = model(&dat, AnalysisMethod::Static);
    set_uniform(&mut nl, false);
    nl.assemble_internal_forces().unwrap();
    let before = as_iterative_damage(nl.material(0).unwrap())
        .unwrap()
        .norm_max_equivalent_stress();
    let [(one, after_one), (two, after_two)] = reductions(&mut nl);
    assert!(one.is_empty(), "a reduction on one rank: {one:?}");
    assert_eq!(after_one, before);
    assert_eq!(two, vec![akantu_comm::ReduceOp::Max]);
    assert_eq!(after_two, 7.25);
}

/// UD-1005 (category B): upstream's orthotropic variant never all-reduces
/// the maximum of its non-local pass. Default: one declared `Max`
/// reduction on more than one rank (none on one rank); compat
/// (`orthotropic_non_local_max_all_reduced = false`): none.
#[test]
fn orthotropic_non_local_declares_its_maximum_reduction_ud1005() {
    for corrected in [true, false] {
        let options = ExtraLawOptions {
            orthotropic_non_local_max_all_reduced: corrected,
            ..ExtraLawOptions::default()
        };
        let mut m = model_opts(
            &dat("damage_orthotropic_iterative_non_local", LAWS[3].2, 0.5),
            AnalysisMethod::Static,
            options,
        );
        set_field(&mut m, wavy, None::<fn(Real, Real) -> [Real; 2]>);
        m.assemble_internal_forces().unwrap();
        let before = as_iterative_damage(m.material(0).unwrap())
            .unwrap()
            .norm_max_equivalent_stress();
        assert!(before > 0.);
        let [(one, after_one), (two, after_two)] = reductions(&mut m);
        assert!(one.is_empty());
        assert_eq!(after_one, before);
        if corrected {
            assert_eq!(two, vec![akantu_comm::ReduceOp::Max]);
            assert_eq!(after_two, 7.25);
        } else {
            assert!(two.is_empty(), "upstream: no reduction");
            assert_eq!(after_two, before);
        }
    }
}

/// `brittle_non_local` and `vreepeerlings_non_local` declare no reduction
/// (upstream: none).
#[test]
fn brittle_and_vreepeerlings_non_local_declare_no_reduction() {
    for (kw, _, params, method) in &LAWS[1..3] {
        let mut m = model(&dat(kw, params, 0.5), *method);
        set_uniform(&mut m, true);
        m.assemble_internal_forces().unwrap();
        m.with_material(0, LawPhase::Stress, |law, _| {
            let mut r = RecordingReducer {
                nb_ranks: 4,
                value: 0.,
                ops: Vec::new(),
            };
            law.as_non_local_mut()
                .unwrap()
                .reduce_after_non_local_stresses(NG, &mut r)?;
            assert!(r.ops.is_empty(), "{kw}: {:?}", r.ops);
            Ok(())
        })
        .unwrap();
    }
}

// ------------------------------------------------------ distributed meshes
//
// The laws on distributed meshes (TEST-187): tests/non_local_parallel.rs.
