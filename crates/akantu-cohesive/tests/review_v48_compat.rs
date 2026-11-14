// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The upstream-compat
// switches added (`CohesiveUpstreamCompat`, UD-217…UD-220)
// reproduce upstream's behaviour: src/common/aka_math.cc (compute_tangents),
// solid_mechanics_model_cohesive.cc (initConstitutiveLaws),
// material_cohesive_bilinear.cc (onElementsAdded). The corrected defaults
// are tested in `review_v48_regressions.rs`; the UD-220 tangent in the unit
// tests of `laws::linear`.

mod common;

use akantu_cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, ElementKind, ElementType, GhostType, Real, Result};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use akantu_parser::Parser;

const NG: GhostType = GhostType::NotGhost;

fn build(mesh: SharedMesh, dat: &str) -> SolidMechanicsModelCohesive {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    SolidMechanicsModelCohesive::with_parser(mesh, None, "solid_mechanics_model_cohesive", parser)
        .unwrap()
}

fn init(model: &mut SolidMechanicsModelCohesive, extrinsic: bool) -> Result<()> {
    model.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        extrinsic,
    )
}

const TET_DAT: &str = "model solid_mechanics_model_cohesive [
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_linear [
    name = coh
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
]
";

/// UD-217 compat: upstream's tangents `t0 = normalize(n1, −n0, n2)`, and
/// inclined facets open at `σ = 0.95 σc` of hydrostatic tension (shear
/// `n2²·σ` picked up from the normal traction).
#[test]
fn ud217_compat_reproduces_upstream_tangents_and_early_insertion() {
    let mesh = common::read_mesh(
        &common::cohesive_mesh("_cohesive_3d_6_tetrahedron_4.msh"),
        3,
    );
    let mut model = build(mesh.clone(), TET_DAT);
    model.ext.compat.tangent_rotation_2d_in_3d = true;
    init(&mut model, true).unwrap();
    let t = model.ext.tangents.get(ElementType::Triangle3, NG).unwrap();
    let n = model
        .ext
        .facet_fe
        .normals_on_integration_points(ElementType::Triangle3, NG)
        .unwrap();
    for (nq, tq) in n.as_slice().chunks(3).zip(t.as_slice().chunks(6)) {
        // `normalize()` of the run-time vector (n1, −n0, n2): sequential sum
        let z = (nq[1] * nq[1] + nq[0] * nq[0]) + nq[2] * nq[2];
        let s = z.sqrt();
        assert_eq!(&tq[..3], &[nq[1] / s, -nq[0] / s, nq[2] / s]);
    }
    let e = 0.95e6 * (1. - 2. * 0.2) / 1e9;
    let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
    {
        let mut u_g = model.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for (ui, x) in u.iter_mut().zip(&nodes) {
            *ui = e * x;
        }
    }
    model.assemble_internal_forces().unwrap();
    let n = model.check_cohesive_stress().unwrap();
    assert!(n > 0, "upstream opens inclined facets below sigma_c");
}

#[derive(Debug)]
struct StripedFacets {
    stripe: Real,
}

impl ConstitutiveLawSelector for StripedFacets {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> Result<Option<usize>> {
        if el.kind() != ElementKind::Regular
            || el.ty().spatial_dimension() + 1 != ctx.mesh.spatial_dimension()
        {
            return Ok(None);
        }
        let fm = ctx.mesh.mesh_facets().unwrap();
        let conn = fm.connectivities().element_ref(el)?;
        let nodes_g = ctx.mesh.nodes();
        let nodes = nodes_g.as_slice();
        let x = conn.iter().map(|&n| nodes[n as usize * 2]).sum::<Real>() / conn.len() as Real;
        Ok(Some(if ((x / self.stripe).floor() as i64) % 2 == 0 {
            1
        } else {
            2
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
  ]
  material cohesive_bilinear [
    name = coh_b
    sigma_c = 2
    G_c = 0.8
    delta_0 = 0.05
  ]
]
";

/// UD-218 compat: upstream indexes every new element of the event by its
/// mesh index in each bilinear law; past a law's arrays (undefined behaviour
/// upstream) the port refuses with an out-of-range error.
#[test]
fn ud218_compat_mesh_indices_are_refused_out_of_range() {
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_6_quadrangle_8.msh"), 2);
    let width = {
        let m = mesh.read().unwrap();
        m.upper_bounds()[0] - m.lower_bounds()[0]
    };
    let mut model = build(mesh, TWO_BILINEAR);
    model.ext.compat.bilinear_mesh_indices = true;
    model.set_material_selector(Box::new(StripedFacets { stripe: width / 5. }));
    let err = init(&mut model, false).unwrap_err();
    assert!(
        format!("{err}").contains("mesh index used as law index"),
        "{err}"
    );
}

#[derive(Debug)]
struct BulkTo(usize);

impl ConstitutiveLawSelector for BulkTo {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> Result<Option<usize>> {
        Ok((el.kind() == ElementKind::Regular
            && el.ty().spatial_dimension() == ctx.mesh.spatial_dimension())
        .then_some(self.0))
    }
}

/// UD-219 compat: a cohesive law that is material 0 receives no facet
/// (upstream `if (not mat_index) return;`).
#[test]
fn ud219_compat_cohesive_material_zero_gets_no_facet() {
    let dat = "model solid_mechanics_model_cohesive [
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
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
    let mut model = build(mesh, dat);
    model.ext.compat.skip_material_index_0 = true;
    model.set_material_selector(Box::new(BulkTo(1)));
    init(&mut model, true).unwrap();
    let mat = model.cohesive_material("insertion").unwrap();
    assert_eq!(
        mat.core
            .facet_filter
            .get(ElementType::Segment2, NG)
            .map_or(0, |a| a.len()),
        0
    );
}

/// The model's set is merged (field-wise OR) into every cohesive law at
/// initialisation; `upstream()` sets every switch.
#[test]
fn model_compat_is_merged_into_the_laws() {
    let all = CohesiveUpstreamCompat::upstream();
    assert_eq!(CohesiveUpstreamCompat::default().merged(all), all);
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
    let mut model = build(mesh, common::MATERIAL_0);
    model.ext.compat.tangent_base_sigma_c = true;
    model.ext.compat.delta_max_without_history = true;
    init(&mut model, true).unwrap();
    let c = model.cohesive_material("insertion").unwrap().core.compat;
    assert!(c.tangent_base_sigma_c && c.delta_max_without_history);
    assert!(!c.sigma_c_zipped_per_quad && !c.bilinear_mesh_indices);
}

/// Every switch of `c` with its name. The destructuring names every field
/// without `..`: a new switch does not compile here until it is listed.
fn named_switches(c: CohesiveUpstreamCompat) -> Vec<(&'static str, bool)> {
    let CohesiveUpstreamCompat {
        sigma_c_zipped_per_quad,
        delta_max_without_history,
        tangent_rotation_2d_in_3d,
        skip_material_index_0,
        bilinear_mesh_indices,
        tangent_base_sigma_c,
        intrinsic_linear_zero_strength,
        delta_max_per_iterate,
        tangent_penetration_sign,
        tangent_contact_normal_stiffness,
        uncoupled_tangent,
        maxima_committed_at_step_entry,
        exponential_tangent,
        zones_unnamed_neighbour_error,
        friction_residual_sliding_abs,
        friction_upstream_implicit,
        tangent_failed_secant,
        doubled_node_keeps_external_force,
    } = c;
    vec![
        ("sigma_c_zipped_per_quad", sigma_c_zipped_per_quad),
        ("delta_max_without_history", delta_max_without_history),
        ("tangent_rotation_2d_in_3d", tangent_rotation_2d_in_3d),
        ("skip_material_index_0", skip_material_index_0),
        ("bilinear_mesh_indices", bilinear_mesh_indices),
        ("tangent_base_sigma_c", tangent_base_sigma_c),
        (
            "intrinsic_linear_zero_strength",
            intrinsic_linear_zero_strength,
        ),
        ("delta_max_per_iterate", delta_max_per_iterate),
        ("tangent_penetration_sign", tangent_penetration_sign),
        (
            "tangent_contact_normal_stiffness",
            tangent_contact_normal_stiffness,
        ),
        ("uncoupled_tangent", uncoupled_tangent),
        (
            "maxima_committed_at_step_entry",
            maxima_committed_at_step_entry,
        ),
        ("exponential_tangent", exponential_tangent),
        (
            "zones_unnamed_neighbour_error",
            zones_unnamed_neighbour_error,
        ),
        (
            "friction_residual_sliding_abs",
            friction_residual_sliding_abs,
        ),
        ("friction_upstream_implicit", friction_upstream_implicit),
        ("tangent_failed_secant", tangent_failed_secant),
        (
            "doubled_node_keeps_external_force",
            doubled_node_keeps_external_force,
        ),
    ]
}

/// `CohesiveUpstreamCompat::upstream()` turns on every
/// switch — it is what Python's `setUpstreamCompat(True)` (and its
/// `getUpstreamCompat()`) and the benchmark's upstream mode use — and the
/// default turns on none; `merged` keeps every switch of either side.
/// Before, a switch left out of `upstream()` went unnoticed (mutant R2:
/// `friction_residual_sliding_abs: false`).
#[test]
fn upstream_turns_on_every_cohesive_switch() {
    let up = CohesiveUpstreamCompat::upstream();
    let off: Vec<&str> = named_switches(up)
        .into_iter()
        .filter_map(|(name, on)| (!on).then_some(name))
        .collect();
    assert!(off.is_empty(), "upstream() leaves {off:?} off");
    let on: Vec<&str> = named_switches(CohesiveUpstreamCompat::default())
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
    assert!(on.is_empty(), "the default turns {on:?} on");
    let d = CohesiveUpstreamCompat::default();
    assert_eq!(up.merged(d), up);
    assert_eq!(d.merged(up), up);
}
