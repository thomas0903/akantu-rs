// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the cohesive model. Ported behaviour:
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc (initConstitutiveLaws, checkCohesiveStress,
// computeNormals), materials/constitutive_laws/material_cohesive_linear.cc
// (checkInsertion), material_cohesive_bilinear.cc (onElementsAdded),
// src/common/aka_math.cc (compute_tangents), fragment_manager.cc.
//
// This file only uses API that existed before the fix, so every
// test could be run there: the `co0*` tests assert the corrected default and
// fail before the fix (the failure observed there is quoted in each doc
// comment); the `defaults_*` tests cover corrected defaults that
// already existed before the fix. Upstream-compat paths of the new switches are
// tested in `review_v48_compat.rs`.

mod common;

use akantu_cohesive::{FragmentManager, SolidMechanicsModelCohesive};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, ElementKind, ElementType, GhostType, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use akantu_parser::Parser;
use common::StrainIncrement;

const NG: GhostType = GhostType::NotGhost;

fn build(mesh: SharedMesh, dat: &str) -> SolidMechanicsModelCohesive {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    SolidMechanicsModelCohesive::with_parser(mesh, None, "solid_mechanics_model_cohesive", parser)
        .unwrap()
}

fn init(model: &mut SolidMechanicsModelCohesive, extrinsic: bool) {
    model
        .init_full(
            &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            extrinsic,
        )
        .unwrap();
}

const ELASTIC: &str = "  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
";

/// Prescribe `u = ε·X` (`eps` column-major) and compute the stresses.
fn uniform_strain(model: &mut SolidMechanicsModelCohesive, mesh: &SharedMesh, eps: &[Real]) {
    let dim = model.smm.spatial_dimension();
    let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
    let mut u = model.smm.displacement_mut().unwrap();
    for (x, ui) in nodes.chunks(dim).zip(u.as_mut_slice().chunks_mut(dim)) {
        for i in 0..dim {
            ui[i] = (0..dim).map(|k| eps[i + dim * k] * x[k]).sum();
        }
    }
    drop(u);
    model.assemble_internal_forces().unwrap();
}

fn nb_true(model: &SolidMechanicsModelCohesive, ty: ElementType) -> usize {
    model
        .ext
        .inserter
        .check_facets()
        .get(ty, NG)
        .map_or(0, |a| a.as_slice().iter().filter(|b| **b).count())
}

// -----------------------------------------------------------------

fn tet4_all_facets() -> (SharedMesh, SolidMechanicsModelCohesive) {
    let mesh = common::read_mesh(
        &common::cohesive_mesh("_cohesive_3d_6_tetrahedron_4.msh"),
        3,
    );
    let dat = format!(
        "model solid_mechanics_model_cohesive [\n{ELASTIC}  material cohesive_linear [
    name = coh
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
]
"
    );
    let mut model = build(mesh.clone(), &dat);
    init(&mut model, true);
    (mesh, model)
}

/// Hydrostatic strain giving the in-situ stress `σ·I` (E = 1e9, ν = 0.2).
fn hydrostatic(sigma: Real) -> Vec<Real> {
    let e = sigma * (1. - 2. * 0.2) / 1e9;
    vec![e, 0., 0., 0., e, 0., 0., 0., e]
}

/// Under hydrostatic tension `σ·I` every facet sees a pure normal
/// traction `σ n`, so its effective stress is `σ` whatever its orientation:
/// no facet may open below `σc`, every checkable facet opens above it. Before
/// the fix (upstream's first tangent `normalize(n1, −n0, n2)`) inclined
/// facets picked up `n2²·σ` of shear and opened at `σ = 0.95 σc`.
#[test]
fn co01_inclined_facets_open_at_sigma_c_under_hydrostatic_tension() {
    let (mesh, mut model) = tet4_all_facets();
    let checkable = nb_true(&model, ElementType::Triangle3);
    assert!(checkable > 50, "{checkable} checkable facets");
    uniform_strain(&mut model, &mesh, &hydrostatic(0.95e6));
    assert_eq!(
        model.check_cohesive_stress().unwrap(),
        0,
        "facets opened below sigma_c under pure normal traction"
    );
    uniform_strain(&mut model, &mesh, &hydrostatic(1.05e6));
    assert_eq!(model.check_cohesive_stress().unwrap(), checkable);
    akantu_mesh::utils::check_facet_mesh_consistency(&mesh.read().unwrap()).unwrap();
}

/// The model's facet tangents form an orthonormal frame with the
/// facet normals at every facet quadrature point. Before the fix `n·t₁ = n₂²`.
#[test]
fn co01_model_facet_tangents_are_orthonormal_to_the_normals() {
    let (_, model) = tet4_all_facets();
    let t = model.ext.tangents.get(ElementType::Triangle3, NG).unwrap();
    let n = model
        .ext
        .facet_fe
        .normals_on_integration_points(ElementType::Triangle3, NG)
        .unwrap();
    let dot = |a: &[Real], b: &[Real]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut inclined = 0;
    for (nq, tq) in n.as_slice().chunks(3).zip(t.as_slice().chunks(6)) {
        let (t0, t1) = (&tq[..3], &tq[3..]);
        if nq[2].abs() > 0.1 {
            inclined += 1;
        }
        for (what, v) in [
            ("n·t0", dot(nq, t0)),
            ("n·t1", dot(nq, t1)),
            ("t0·t1", dot(t0, t1)),
            ("|t0|²−1", dot(t0, t0) - 1.),
            ("|t1|²−1", dot(t1, t1) - 1.),
        ] {
            assert!(v.abs() < 1e-14, "normal {nq:?}: {what} = {v:e}");
        }
    }
    assert!(inclined > 10, "the mesh must have inclined facets");
}

// -----------------------------------------------------------------

/// Facets take law `left` or `right` by the sign of `x − x_split` at their
/// first node, alternating in stripes of width `stripe` (anything else: the
/// next selector), so the two laws' cohesive elements interleave in the
/// mesh numbering.
#[derive(Debug)]
struct StripedFacets {
    left: usize,
    right: usize,
    stripe: Real,
}

impl ConstitutiveLawSelector for StripedFacets {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> Result<Option<usize>> {
        if el.kind() != ElementKind::Regular
            || el.ty().spatial_dimension() + 1 != ctx.mesh.spatial_dimension()
        {
            return Ok(None);
        }
        let Some(fm) = ctx.mesh.mesh_facets() else {
            return Ok(None);
        };
        let conn = fm.connectivities().element_ref(el)?;
        let dim = ctx.mesh.spatial_dimension();
        let nodes_g = ctx.mesh.nodes();
        let nodes = nodes_g.as_slice();
        let x = conn.iter().map(|&n| nodes[n as usize * dim]).sum::<Real>() / conn.len() as Real;
        let stripe = (x / self.stripe).floor() as i64;
        Ok(Some(if stripe % 2 == 0 {
            self.left
        } else {
            self.right
        }))
    }
}

const TWO_BILINEAR: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = steel
    rho = 1
    E = 6
    nu = 0.0
  ]
  material cohesive_bilinear [
    name = coh_a
    sigma_c = 1
    G_c = 0.25
    delta_0 = 0.01
    penalty = 10
  ]
  material cohesive_bilinear [
    name = coh_b
    sigma_c = 2
    G_c = 0.8
    delta_0 = 0.05
    penalty = 10
  ]
]
";

/// Two intrinsic `cohesive_bilinear` laws whose elements interleave
/// in the mesh numbering. Each law rescales exactly its own elements:
/// `δc,eff = 2 G_c/σc` and `σc,eff = σc·δc/(δc − δ0)`. Before the fix `init_full`
/// failed: "cohesive_bilinear: new element (mesh index used as law index,
/// upstream)" (out of range).
#[test]
fn co02_two_bilinear_laws_rescale_only_their_own_elements() {
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_6_quadrangle_8.msh"), 2);
    let width = {
        let m = mesh.read().unwrap();
        m.upper_bounds()[0] - m.lower_bounds()[0]
    };
    let mut model = build(mesh.clone(), TWO_BILINEAR);
    model.set_material_selector(Box::new(StripedFacets {
        left: 1,
        right: 2,
        stripe: width / 5.,
    }));
    init(&mut model, false);
    let ct = ElementType::Cohesive2d6;
    let total = mesh.read().unwrap().nb_element(ct, NG);
    let mut seen = 0;
    for (name, sigma_c, g_c, delta_0) in [("coh_a", 1.0, 0.25, 0.01), ("coh_b", 2.0, 0.8, 0.05)] {
        let mat = model.cohesive_material(name).unwrap();
        let n = akantu_solid::Material::base(mat)
            .element_filter
            .get(ct, NG)
            .unwrap()
            .len();
        assert!(n > 0, "{name}: no element");
        seen += n;
        let d: Real = 2. * g_c / sigma_c;
        let s = sigma_c * (d / (d - delta_0));
        let sig_g = mat.array("sigma_c_eff", ct, NG).unwrap();
        let sig = sig_g.as_slice();
        let del_g = mat.array("delta_c_eff", ct, NG).unwrap();
        let del = del_g.as_slice();
        let nq = sig.len() / n;
        assert_eq!(sig.len(), n * nq);
        for (k, (a, b)) in sig.iter().zip(del).enumerate() {
            assert_eq!(*b, d, "{name}: delta_c_eff[{k}]");
            assert_eq!(*a, s, "{name}: sigma_c_eff[{k}]");
        }
    }
    assert_eq!(seen, total);
}

// -----------------------------------------------------------------

/// Bulk elements take law `bulk`.
#[derive(Debug)]
struct BulkTo(usize);

impl ConstitutiveLawSelector for BulkTo {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> Result<Option<usize>> {
        Ok((el.kind() == ElementKind::Regular
            && el.ty().spatial_dimension() == ctx.mesh.spatial_dimension())
        .then_some(self.0))
    }
}

/// `TestSMMCFixture` mode-I loading of an extrinsic model (`init_factor`
/// 1 − 1e-5, `inc_factor` 2e-2): `steps` × (BCs, check, solve); returns the
/// number of inserted elements per step.
fn mode_i_steps(
    model: &mut SolidMechanicsModelCohesive,
    mesh: &SharedMesh,
    steps: usize,
    mut before_check: impl FnMut(&mut SolidMechanicsModelCohesive),
) -> Vec<usize> {
    let dt = model.stable_time_step().unwrap() * 0.01;
    model.set_time_step(dt, None).unwrap();
    let (sigma_c, e, nu) = (1e6, 1e9, 0.2);
    let strain: Vec<Real> = [-nu * (1. + nu), 0., 0., (1. - nu) * (1. + nu)]
        .iter()
        .map(|s| s * sigma_c / e)
        .collect();
    let init: Vec<Real> = strain.iter().map(|s| (1. - 1e-5) * s).collect();
    let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
    {
        let mut u = model.smm.displacement_mut().unwrap();
        for (x, ui) in nodes.chunks(2).zip(u.as_mut_slice().chunks_mut(2)) {
            for i in 0..2 {
                ui[i] = init[i] * x[0] + init[i + 2] * x[1];
            }
        }
    }
    let functor = StrainIncrement {
        strain: strain.iter().map(|s| 1. / 300. * (2e-2 * s)).collect(),
        dim: 2,
        dir: 1,
    };
    let mut inserted = Vec::new();
    for _ in 0..steps {
        model.smm.apply_dirichlet_bc(&functor, "loading").unwrap();
        model.smm.apply_dirichlet_bc(&functor, "fixed").unwrap();
        before_check(model);
        inserted.push(model.check_cohesive_stress().unwrap());
        model.solve_step(None).unwrap();
    }
    inserted
}

const COHESIVE_FIRST: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
]
";

/// The cohesive law listed first (material 0) still receives the
/// facets of the insertion surface and extrinsic insertion happens (as with
/// the law listed second, step 24 of the upstream mode-I scenario). Before
/// the fix the facet filter stayed empty and nothing was ever inserted.
#[test]
fn co03_cohesive_law_listed_first_inserts() {
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
    let mut model = build(mesh.clone(), COHESIVE_FIRST);
    model.set_material_selector(Box::new(BulkTo(1)));
    init(&mut model, true);
    let mat = model.cohesive_material("insertion").unwrap();
    let nb_facets = mat
        .core
        .facet_filter
        .get(ElementType::Segment2, NG)
        .map_or(0, |a| a.len());
    assert!(nb_facets > 0, "the cohesive law received no facet");
    let inserted: usize = mode_i_steps(&mut model, &mesh, 30, |_| {}).iter().sum();
    assert_eq!(
        inserted, 5,
        "insertions of the mode-I scenario (oracle: 3 + 2)"
    );
}

// -----------------------------------------------------------------

fn material_0() -> String {
    common::MATERIAL_0.to_owned()
}

fn law_lengths(model: &SolidMechanicsModelCohesive) -> (usize, [usize; 4]) {
    let mat = model.cohesive_material("insertion").unwrap();
    let ct = ElementType::Cohesive2d4;
    let filter = akantu_solid::Material::base(mat)
        .element_filter
        .get(ct, NG)
        .map_or(0, |a| a.len());
    let len = |n: &str| mat.array(n, ct, NG).map_or(0, |a| a.len());
    let prev = mat.core.tractions.previous(ct, NG).map_or(0, |a| a.len());
    (
        filter,
        [
            len("sigma_c_eff"),
            len("delta_c_eff"),
            len("insertion_stress"),
            prev,
        ],
    )
}

/// An insertion pass that fails before commit (here: a live read
/// guard on the mesh) leaves no pending parameters and no insertion flags,
/// and the run continues exactly as one without the failures. Before the fix
/// the failed pass at step 24 left 6 σc,eff entries (3 facets × 2 points)
/// for 0 elements, which the next pass reused.
#[test]
fn co05_failed_insertion_pass_restores_the_laws_arrays() {
    let run = |inject: bool| {
        let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
        let mut model = build(mesh.clone(), &material_0());
        init(&mut model, true);
        let mut failures = 0;
        let mesh_h = mesh.clone();
        let inserted = mode_i_steps(&mut model, &mesh, 30, |model| {
            if !inject {
                return;
            }
            let r = {
                let _reader = mesh_h.read().unwrap();
                model.check_cohesive_stress()
            };
            match r {
                Err(_) => {
                    failures += 1;
                    let (filter, lens) = law_lengths(model);
                    let n = filter
                        * akantu_cohesive::material::nb_quad_cohesive(ElementType::Cohesive2d4)
                            .unwrap();
                    assert_eq!(lens, [n, n, n, n], "stale parameters ({filter} elements)");
                    assert!(
                        model
                            .ext
                            .inserter
                            .insertion_facets()
                            .as_map()
                            .iter_all()
                            .all(|(_, _, a)| !a.as_slice().contains(&true)),
                        "stale insertion flags"
                    );
                }
                Ok(n) => assert_eq!(n, 0, "a pass with a live reader cannot insert"),
            }
        });
        let u = model.smm.displacement().unwrap().as_slice().to_vec();
        let mat = model.cohesive_material("insertion").unwrap();
        let s = mat
            .array("sigma_c_eff", ElementType::Cohesive2d4, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        (inserted, u, s, failures)
    };
    let (ins_a, u_a, s_a, failures) = run(true);
    let (ins_b, u_b, s_b, _) = run(false);
    assert!(failures >= 2, "{failures} injected failures");
    assert_eq!(ins_a, ins_b);
    assert_eq!(s_a, s_b);
    assert!(
        u_a.iter()
            .zip(&u_b)
            .all(|(a, b)| akantu_validation::exact_eq(*a, *b)) // D-046
    );
}

// -----------------------------------------------------------------

/// `add_facet` after initialisation is refused, and a facet filter
/// grown behind the law's back makes the insertion check fail with an error,
/// not a panic. Before the fix `add_facet` succeeded and `check_cohesive_stress`
/// panicked ("range end index … out of range").
#[test]
fn co06_late_add_facet_is_an_error_not_a_panic() {
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
    let mut model = build(mesh.clone(), &material_0());
    init(&mut model, true);
    let facet = Element::new(ElementType::Segment2, 0, NG);
    let r = model
        .cohesive_material_mut("insertion")
        .unwrap()
        .add_facet(facet);
    assert!(r.is_err(), "add_facet after init_full must be refused");
    // a filter extended directly (public field) is an error at the check
    {
        let mat = model.cohesive_material_mut("insertion").unwrap();
        let f = mat
            .core
            .facet_filter
            .get_mut(ElementType::Segment2, NG)
            .unwrap();
        f.try_push_tuple(&[0]).unwrap();
        f.try_push_tuple(&[1]).unwrap();
    }
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        model.check_cohesive_stress()
    }));
    assert!(r.expect("check_cohesive_stress panicked").is_err());
}

// ------------------------------------------------------ (defaults)

const QUADRATIC: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
    delta_c = 1e-4
    max_quad_stress_insertion = MAXQ
  ]
]
";

/// Fragments (a), UD-040 default: a facet with two quadrature points and
/// different strengths `[s0, s1]` opens when the facet stress exceeds their
/// mean, and its cohesive points get `δc,eff = s_q/σ_q·δc` from their own
/// strengths; same with `max_quad_stress_insertion` (maximum point stress
/// against the mean strength; a uniform stress here).
#[test]
fn defaults_ud040_mean_strength_of_a_two_point_facet() {
    for maxq in ["false", "true"] {
        let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_6_triangle_6.msh"), 2);
        let mut model = build(mesh.clone(), &QUADRATIC.replace("MAXQ", maxq));
        init(&mut model, true);
        let ft = ElementType::Segment3;
        let stress = 0.5e6;
        // two checkable facets of the law: mean strength below / above the
        // stress (every other facet keeps σc = 1e6)
        let check = model
            .ext
            .inserter
            .check_facets()
            .get(ft, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        {
            let mat = model.cohesive_material_mut("insertion").unwrap();
            let filter = mat
                .core
                .facet_filter
                .get(ft, NG)
                .unwrap()
                .as_slice()
                .to_vec();
            let ks: Vec<usize> = filter
                .iter()
                .enumerate()
                .filter(|(_, f)| check[**f as usize])
                .map(|(k, _)| k)
                .take(2)
                .collect();
            assert_eq!(ks.len(), 2);
            let mut s = mat.core.sigma_c.values_mut().get_mut(ft, NG).unwrap();
            assert_eq!(s.len(), 2 * filter.len(), "two strengths per facet");
            let s = s.as_mut_slice();
            s[2 * ks[0]..2 * ks[0] + 2].copy_from_slice(&[0.3e6, 0.6e6]);
            s[2 * ks[1]..2 * ks[1] + 2].copy_from_slice(&[0.3e6, 0.8e6]);
        }
        // in-plane hydrostatic plane strain: σ·n = σ n on every facet
        let (e, nu): (Real, Real) = (1e9, 0.2);
        let lambda = e * nu / ((1. + nu) * (1. - 2. * nu));
        let mu = e / (2. * (1. + nu));
        let eps = stress / (2. * (lambda + mu));
        uniform_strain(&mut model, &mesh, &[eps, 0., 0., eps]);
        assert_eq!(
            model.check_cohesive_stress().unwrap(),
            1,
            "max_quad = {maxq}"
        );
        let mat = model.cohesive_material("insertion").unwrap();
        let ct = ElementType::Cohesive2d6;
        let sig_g = mat.array("sigma_c_eff", ct, NG).unwrap();
        let sig = sig_g.as_slice();
        let del_g = mat.array("delta_c_eff", ct, NG).unwrap();
        let del = del_g.as_slice();
        assert_eq!(sig.len(), 2);
        for (q, s_q) in [0.3e6, 0.6e6].into_iter().enumerate() {
            assert!((sig[q] - stress).abs() < 1e-6 * stress, "σ_q = {}", sig[q]);
            assert_eq!(del[q], s_q / sig[q] * 1e-4, "δc,eff[{q}]");
        }
    }
}

/// A strip of `n` unit-height quads of width `a` along x.
fn strip(n: usize, a: Real, b: Real) -> SharedMesh {
    let mut m = Mesh::new(2, "strip").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for i in 0..=n {
            acc.push_node(&[a * i as Real, 0.]).unwrap();
            acc.push_node(&[a * i as Real, b]).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Quadrangle4, NG);
        for i in 0..n {
            let k = 2 * i as akantu_core::Idx;
            c.try_push_tuple(&[k, k + 2, k + 3, k + 1]).unwrap();
        }
    }
    SharedMesh::new(m)
}

// intrinsic insertion for the fragment topology only (the damage is set by
// hand): `cohesive_bilinear`, since the intrinsic `cohesive_linear` (no
// strength) is refused by default (UD-224)
const STRIP_DAT: &str = "model solid_mechanics_model_cohesive [
  material elastic [
    name = body
    rho = 3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_bilinear [
    name = coh
    beta = 1
    G_c = 10
    sigma_c = 1e6
    delta_0 = 1e-7
  ]
]
";

/// Fragments (b), inertia pairing default: 12 fully broken rectangles `a × b`;
/// every fragment has mass `ρab`, its own centre and principal moments
/// `m·max(a², b²)/12 ≥ m·min(a², b²)/12` (upstream pairs groups in name order
/// with centres in index order, wrong from 11 fragments on).
#[test]
fn defaults_inertia_moments_of_twelve_fragments() {
    let (n, a, b, rho) = (12usize, 1.0, 0.5, 3.0);
    let mesh = strip(n, a, b);
    let mut model = build(mesh.clone(), STRIP_DAT);
    model
        .ext
        .inserter
        .set_limit(0, 0.5, a * n as Real - 0.5)
        .unwrap();
    init(&mut model, false);
    let ct = ElementType::Cohesive2d4;
    assert_eq!(mesh.read().unwrap().nb_element(ct, NG), n - 1);
    model
        .cohesive_material_mut("coh")
        .unwrap()
        .core
        .damage
        .values_mut()
        .get_mut(ct, NG)
        .unwrap()
        .as_mut_slice()
        .fill(1.0);
    let mut fm = FragmentManager::new(&model).unwrap();
    fm.compute_all_data(&mut model, 1.0).unwrap();
    assert_eq!(fm.nb_fragment(), n);
    let m = rho * a * b;
    let (big, small) = (m * a * a / 12., m * b * b / 12.);
    for k in 0..n {
        assert!((fm.mass[2 * k] - m).abs() < 1e-12 * m, "mass {k}");
        let x = fm.mass_center[2 * k];
        let el = fm.fragments[k]
            .iter()
            .find(|e| e.kind() == ElementKind::Regular)
            .unwrap();
        let x_th = a * (el.idx() as Real + 0.5);
        assert!((x - x_th).abs() < 1e-12, "centre {k}: {x} vs {x_th}");
        let i = &fm.inertia_moments[2 * k..2 * k + 2];
        assert!((i[0] - big).abs() < 1e-12 * big, "fragment {k}: {i:?}");
        assert!((i[1] - small).abs() < 1e-12 * big, "fragment {k}: {i:?}");
    }
}

/// Fragments (c), UD-036 default: a fragment made of one quadrangle and two
/// triangles (no cohesive element) has the mass and centre of the whole
/// `2 × 1` rectangle (upstream resets the accumulator per element type).
#[test]
fn defaults_fragment_spanning_two_element_types() {
    let mut m = Mesh::new(2, "mixed").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for p in [[0., 0.], [1., 0.], [2., 0.], [2., 1.], [1., 1.], [0., 1.]] {
            acc.push_node(&p).unwrap();
        }
        acc.connectivity_mut(ElementType::Quadrangle4, NG)
            .try_push_tuple(&[0, 1, 4, 5])
            .unwrap();
        let mut t = acc.connectivity_mut(ElementType::Triangle3, NG);
        t.try_push_tuple(&[1, 2, 3]).unwrap();
        t.try_push_tuple(&[1, 3, 4]).unwrap();
    }
    let mesh = SharedMesh::new(m);
    let mut model = build(mesh.clone(), STRIP_DAT);
    // no facet in the insertion box: no cohesive element
    model.ext.inserter.set_limit(0, 100., 101.).unwrap();
    init(&mut model, false);
    let mut fm = FragmentManager::new(&model).unwrap();
    fm.compute_all_data(&mut model, 1.0).unwrap();
    assert_eq!(fm.nb_fragment(), 1);
    assert!((fm.mass[0] - 6.0).abs() < 1e-12, "mass {}", fm.mass[0]);
    assert!((fm.mass_center[0] - 1.0).abs() < 1e-12);
    assert!((fm.mass_center[1] - 0.5).abs() < 1e-12);
}
