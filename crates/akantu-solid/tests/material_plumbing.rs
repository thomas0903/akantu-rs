// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// constitutive-law plumbing ported from src/model/common/constitutive_laws/
// {constitutive_law*.hh, constitutive_laws_handler*.hh,
// constitutive_law_selector*.hh, internal_field*.hh,
// random_internal_field*.hh}, src/model/solid_mechanics/material*.{hh,cc}
// and src/io/parser/parameter_registry*: parameter registry semantics,
// creation from `.dat` sections, selection and assignment, element removal,
// history, matrix flags, computeAllStresses, energies, error paths; and the
// material part of test/test_model/test_solid_mechanics_model/
// test_materials/test_material_orthotropic.cc (orthotropic.dat).

mod common;

use akantu_core::enums::MatrixType;
use akantu_core::random::{
    IdxSlot, MinstdRand0, RandomDistributionType, RandomGenerator, RandomParameter,
};
use akantu_core::{Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real};
use akantu_mesh::data::MeshDataValues;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, ElementDataSelector, InternalFieldRef, LawPhase, LawSpec, SelectorChain,
};
use akantu_model::internal_field::AnyInternalField;
use akantu_parser::{ByteString, ParamValue, ParameterRegistry};
use akantu_solid::material::{LawOptions, MaterialFactory, register_builtin_materials};
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::{ForeignLaw, ForeignMaterial, ForeignMaterialMethods, Material};
use common::{NG, SimplexModel, materials_from_dat, materials_from_dat_with};

const TRI: ElementType = ElementType::Triangle3;

/// Unit square split into two triangles.
fn square() -> SimplexModel {
    SimplexModel::new(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.], vec![1., 1.]],
        &[vec![0, 1, 2], vec![1, 3, 2]],
    )
}

fn spec(dim: usize) -> LawSpec {
    LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: "m".into(),
    }
}

fn real(m: &dyn ParameterRegistry, n: &str) -> Real {
    m.get_param(n).unwrap().as_real().unwrap()
}

/// Upstream `ParameterRegistry`: access flags, defaults, `set` followed by
/// `updateInternalParameters`, input-file setters without it.
#[test]
fn parameters_follow_upstream_registry_semantics() {
    let model = square();
    let mut m = MaterialElastic::<2>::new(&spec(2), Some(&model.mesh)).unwrap();
    let names = m.param_names();
    for n in [
        "name",
        "rho",
        "finite_deformation",
        "inelastic_deformation",
        "eigen_grad_u",
        "Plane_Stress",
        "E",
        "nu",
        "alpha",
        "delta_T",
        "lambda",
        "mu",
        "kapa",
    ] {
        assert!(names.contains(&n), "{n} missing");
    }
    let m3 = MaterialElastic::<3>::new(&spec(3), None).unwrap();
    assert!(!m3.param_names().contains(&"Plane_Stress"), "2-D only");
    // upstream defaults
    assert_eq!(real(&m, "E"), 0.);
    assert_eq!(real(&m, "nu"), 0.5);
    assert_eq!(real(&m, "alpha"), 0.);
    assert_eq!(real(&m, "rho"), 0.);
    assert_eq!(real(&m, "delta_T"), 0.);
    assert_eq!(
        m.get_param("finite_deformation").unwrap(),
        ParamValue::Bool(false)
    );
    assert_eq!(
        m.get_param("name").unwrap(),
        ParamValue::ByteString(ByteString::default())
    );
    // access flags
    assert!(
        m.set_param("lambda", ParamValue::Real(1.)).is_err(),
        "readable only"
    );
    assert!(
        m.set_param("name", ParamValue::String("x".into())).is_err(),
        "parsable + readable"
    );
    assert!(m.get_param("eigen_grad_u").is_err(), "parsable only");
    assert!(m.get_param("inelastic_deformation").is_err(), "internal");
    assert!(m.set_param("bogus", ParamValue::Real(1.)).is_err());
    assert!(
        m.set_param("E", ParamValue::Bool(true)).is_err(),
        "kind checked"
    );
    // set → updateInternalParameters
    m.set_param("E", ParamValue::Int(2)).unwrap();
    m.set_param("nu", ParamValue::Real(0.25)).unwrap();
    let (e, nu) = (2.0, 0.25);
    assert_eq!(real(&m, "lambda"), nu * e / ((1. + nu) * (1. - 2. * nu)));
    assert_eq!(real(&m, "mu"), e / (2. * (1. + nu)));
    // setAuto (input) does not update; the parser updates at section end
    m.set_param_from_input("E", ParamValue::Real(4.)).unwrap();
    assert_eq!(real(&m, "mu"), e / (2. * (1. + nu)));
    m.update_internal_parameters().unwrap();
    assert_eq!(real(&m, "mu"), 4. / (2. * (1. + nu)));
    m.set_param("Plane_Stress", ParamValue::Bool(true)).unwrap();
    assert_eq!(real(&m, "lambda"), nu * 4. / ((1. + nu) * (1. - nu)));
    assert_eq!(
        real(&m, "kapa"),
        real(&m, "lambda") + 2. / 3. * real(&m, "mu")
    );
}

/// `delta_T` is an internal-field parameter: setting it resets every value
/// of the internal (upstream `setDefaultValue`) and is the value of new
/// entries.
#[test]
fn delta_t_parameter_sets_the_internal_field() {
    let model = square();
    let mut h = materials_from_dat(
        &model,
        b"material elastic [\n name = s\n E = 1\n delta_T = 3\n]\n",
    )
    .unwrap();
    let law = h.law_mut(0).unwrap();
    let dt = |law: &dyn Material| {
        let mut v = Vec::new();
        law.for_each_internal(&mut |n, f| {
            if let (InternalFieldRef::Real(f), "delta_T") = (f, n) {
                v = f.get(TRI, NG).unwrap().as_slice().to_vec();
            }
        });
        v
    };
    assert_eq!(dt(law), vec![3., 3.]);
    law.set_param("delta_T", ParamValue::Real(-2.)).unwrap();
    assert_eq!(dt(law), vec![-2., -2.]);
    assert_eq!(real(law, "delta_T"), -2.);
}

/// Upstream `registerNewConstitutiveLaw(section)` / `instantiateConstitutiveLaws`.
#[test]
fn material_creation_from_dat_sections() {
    let model = square();
    let dat = b"E0 = 2e11\nmaterial elastic [\n name = steel\n E = E0\n nu = 0.3\n rho = 7800\n]\n\
material elastic_orthotropic [\n name = wood\n E1 = 1e10\n E2 = 5e9\n nu12 = 0.2\n G12 = 1e9\n]\n";
    let h = materials_from_dat(&model, dat).unwrap();
    assert_eq!(h.nb_laws(), 2);
    assert_eq!(
        h.law(0).unwrap().law_id(),
        "solid_mechanics_model:0:elastic"
    );
    assert_eq!(
        h.law(1).unwrap().law_id(),
        "solid_mechanics_model:1:elastic_orthotropic"
    );
    assert_eq!(h.law_index_of("steel").unwrap(), 0);
    assert_eq!(h.law_index_of("wood").unwrap(), 1);
    assert!(h.law_index_of("iron").is_err());
    assert_eq!(real(h.law(0).unwrap(), "E"), 2e11);
    assert_eq!(h.law(1).unwrap().law_type(), "elastic_orthotropic");
    // default selector: every element to law 0
    assert_eq!(h.law_of(Element::new(TRI, 1, NG)).unwrap().name(), "steel");
    // errors: unknown type, missing name, duplicate name, unknown parameter
    assert!(matches!(
        materials_from_dat(&model, b"material plastique [\n name = x\n]\n"),
        Err(Error::NotFound { .. })
    ));
    assert!(materials_from_dat(&model, b"material elastic [\n E = 1\n]\n").is_err());
    assert!(
        materials_from_dat(
            &model,
            b"material elastic [\n name = a\n]\nmaterial elastic [\n name = a\n]\n"
        )
        .is_err()
    );
    assert!(materials_from_dat(&model, b"material elastic [\n name = a\n bogus = 1\n]\n").is_err());
    // permissive parser: unknown parameters are warnings
    let mut f = MaterialFactory::default();
    register_builtin_materials(&mut f).unwrap();
    let p = akantu_parser::Parser::parse_bytes(
        b"material elastic opt [\n name = a\n bogus = 1\n]\n",
        "p.dat",
        akantu_parser::AnonymousIds::Local(&mut 0),
    )
    .unwrap();
    let mut h = akantu_model::ConstitutiveLawsHandler::<dyn Material>::new(
        "model",
        &model.mesh,
        2,
        akantu_parser::ParserType::Material,
    )
    .unwrap();
    h.instantiate_laws(&f, &p.root, &model, true).unwrap();
    assert_eq!(h.law(0).unwrap().law_id(), "model:0:elastic");
}

#[test]
fn builtin_registration_names() {
    let mut f = MaterialFactory::default();
    register_builtin_materials(&mut f).unwrap();
    // every keyword implemented so far
    // (the `damage_non_local` keywords with the default feature), each
    // registered exactly once (sorted, as the factory lists)
    let mut expected = vec![
        "anisotropic_damage",
        "elastic",
        "elastic_anisotropic",
        "elastic_orthotropic",
        "marigo",
        "mazars",
        "neohookean",
        "phasefield",
        "plastic_drucker_prager",
        "plastic_linear_isotropic_hardening",
        "plastic_mazars",
        "sls_deviatoric",
        "viscoelastic_maxwell",
    ];
    if cfg!(feature = "damage-non-local") {
        expected.extend([
            "marigo_non_local",
            "mazars_non_local",
            "plastic_mazars_non_local",
        ]);
        expected.sort_unstable();
    }
    assert_eq!(f.names().collect::<Vec<_>>(), expected);
    assert!(
        register_builtin_materials(&mut f).is_err(),
        "duplicate registration"
    );
}

/// `MeshDataMaterialSelector<std::string>("physical_names")` with the
/// default selector as fallback, element assignment and local numbering.
#[test]
fn selector_by_physical_names_and_assignment() {
    let mut model = square();
    let mut names = ElementTypeMapArray::<String>::new("physical_names");
    names
        .as_map_mut()
        .insert(
            TRI,
            NG,
            Array::from_vec(vec!["soft".to_owned(), "stiff".to_owned()], 1).unwrap(),
        )
        .unwrap();
    model
        .mesh
        .data_mut()
        .insert_elemental("physical_names", MeshDataValues::String(names));
    let p = akantu_parser::Parser::parse_bytes(
        b"material elastic [\n name = stiff\n E = 2e11\n]\nmaterial elastic [\n name = soft\n E = 1e9\n]\n",
        "s.dat",
        akantu_parser::AnonymousIds::Local(&mut 0),
    )
    .unwrap();
    let mut h = akantu_model::ConstitutiveLawsHandler::<dyn Material>::new(
        "solid_mechanics_model",
        &model.mesh,
        2,
        akantu_parser::ParserType::Material,
    )
    .unwrap();
    h.selector = SelectorChain::with_fallback_to_default(Box::new(ElementDataSelector::new(
        "physical_names",
    )));
    h.instantiate_laws(&common::factory(), &p.root, &model, false)
        .unwrap();
    h.assign_laws_to_elements(&model.mesh, None).unwrap();
    assert_eq!(h.law_index_map().get(TRI, NG).unwrap().as_slice(), &[1, 0]);
    assert_eq!(
        h.local_numbering_map().get(TRI, NG).unwrap().as_slice(),
        &[0, 0]
    );
    assert_eq!(
        h.law(0)
            .unwrap()
            .element_filter()
            .get(TRI, NG)
            .unwrap()
            .as_slice(),
        &[1]
    );
    assert_eq!(
        h.law(1)
            .unwrap()
            .element_filter()
            .get(TRI, NG)
            .unwrap()
            .as_slice(),
        &[0]
    );
    assert_eq!(h.law_of(Element::new(TRI, 0, NG)).unwrap().name(), "soft");
    let ctx = model.ctx(LawPhase::Init);
    for law in h.laws_mut() {
        law.init_material(&ctx).unwrap();
        assert_eq!(law.base().stress.get(TRI, NG).unwrap().len(), 1);
        assert_eq!(
            law.base().grad_u.previous(TRI, NG).unwrap().len(),
            1,
            "history"
        );
    }
}

/// Upstream `ConstitutiveLaw::onElementsRemoved`: filter renumbered, removed
/// elements dropped, internals (and their history) compacted; the handler
/// records the new law-local numbering.
#[test]
fn element_removal_compacts_filter_and_internals() {
    let model = square();
    let mut h = materials_from_dat(&model, b"material elastic [\n name = s\n E = 1\n]\n").unwrap();
    let law = h.law_mut(0).unwrap();
    for (i, v) in law
        .base_mut()
        .stress
        .get_mut(TRI, NG)
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = i as Real;
    }
    // mesh element 0 removed, element 1 becomes 0
    let mut num = ElementTypeMapArray::<Idx>::new("new numbering");
    num.as_map_mut()
        .insert(TRI, NG, Array::from_vec(vec![-1, 0], 1).unwrap())
        .unwrap();
    let kept = law
        .on_elements_removed(&[Element::new(TRI, 0, NG)], &num, &|_, _| 1)
        .unwrap();
    assert_eq!(kept, vec![(Element::new(TRI, 0, NG), 0)]);
    assert_eq!(law.element_filter().get(TRI, NG).unwrap().as_slice(), &[0]);
    assert_eq!(
        law.base().stress.get(TRI, NG).unwrap().as_slice(),
        &[4., 5., 6., 7.]
    );
    assert_eq!(law.base().stress.previous(TRI, NG).unwrap().len(), 1);
    h.record_renumbered(0, &kept).unwrap();
    assert_eq!(
        h.local_numbering_map().get(TRI, NG).unwrap().as_slice()[0],
        0
    );
}

/// `beforeSolveStep` saves, `afterSolveStep(false)` restores,
/// `afterSolveStep(true)` keeps (upstream `Material` hooks).
#[test]
fn history_and_solve_step_hooks() {
    let model = square();
    let mut h = materials_from_dat(
        &model,
        b"material elastic [\n name = s\n E = 1e3\n nu = 0.2\n]\n",
    )
    .unwrap();
    let law = h.law_mut(0).unwrap();
    let ctx = model.ctx(LawPhase::BeforeSolveStep);
    let set_grad = |law: &mut dyn Material, v: Real| {
        law.base_mut()
            .grad_u
            .get_mut(TRI, NG)
            .unwrap()
            .as_mut_slice()
            .fill(v);
        law.compute_stress(&model.ctx(LawPhase::Stress), TRI, NG)
            .unwrap();
    };
    set_grad(law, 1e-3);
    let s1 = law.base().stress.get(TRI, NG).unwrap().as_slice().to_vec();
    law.before_solve_step(&ctx).unwrap();
    assert_eq!(
        law.base().stress.previous(TRI, NG).unwrap().as_slice(),
        &s1[..]
    );
    set_grad(law, 2e-3);
    law.after_solve_step(&model.ctx(LawPhase::AfterSolveStep), false)
        .unwrap();
    assert_eq!(
        law.base().stress.get(TRI, NG).unwrap().as_slice(),
        &s1[..],
        "restored"
    );
    set_grad(law, 2e-3);
    let s2 = law.base().stress.get(TRI, NG).unwrap().as_slice().to_vec();
    law.after_solve_step(&model.ctx(LawPhase::AfterSolveStep), true)
        .unwrap();
    assert_eq!(
        law.base().stress.get(TRI, NG).unwrap().as_slice(),
        &s2[..],
        "kept"
    );
}

/// `hasMatrixChanged` / `getMatrixType` and the stiffness flags; the
/// orthotropic update resets the flag by default and does not upstream
/// (UD-105, `LawOptions::orthotropic_parent_update`).
#[test]
fn stiffness_matrix_flags() {
    let model = square();
    let mut h = materials_from_dat(
        &model,
        b"material elastic [\n name = s\n E = 1\n]\nmaterial elastic_orthotropic [\n name = o\n E1 = 2\n E2 = 1\n G12 = 1\n]\n",
    )
    .unwrap();
    let ctx = model.ctx(LawPhase::Tangent);
    for law in h.laws_mut() {
        let nq = law.base().grad_u.get(TRI, NG).unwrap().len();
        let mut t = Array::zeros(nq, 9);
        assert!(law.has_matrix_changed("K"));
        assert_eq!(law.matrix_type("K"), MatrixType::Symmetric);
        assert_eq!(law.matrix_type("M"), MatrixType::Symmetric);
        assert_eq!(law.matrix_type("X"), MatrixType::NotDefined);
        assert!(!law.has_matrix_changed("M"));
        law.compute_tangent_moduli(&ctx, TRI, NG, &mut t).unwrap();
        assert!(!law.has_matrix_changed("K"));
    }
    h.law_mut(0)
        .unwrap()
        .set_param("E", ParamValue::Real(3.))
        .unwrap();
    assert!(h.law(0).unwrap().has_matrix_changed("K"));
    let o = h.law_mut(1).unwrap();
    o.set_param("E1", ParamValue::Real(3.)).unwrap();
    assert!(o.has_matrix_changed("K"), "UD-105: flag reset by default");
    // upstream: the orthotropic update leaves the flag
    let nq = o.base().grad_u.get(TRI, NG).unwrap().len();
    o.compute_tangent_moduli(&ctx, TRI, NG, &mut Array::zeros(nq, 9))
        .unwrap();
    assert!(!o.has_matrix_changed("K"));
    o.options_mut().unwrap().orthotropic_parent_update = false;
    o.set_param("E1", ParamValue::Real(4.)).unwrap();
    assert!(
        !o.has_matrix_changed("K"),
        "upstream: flag not reset by the orthotropic update"
    );
    // wrong tangent block size
    let mut bad = Array::zeros(1, 9);
    assert!(o.compute_tangent_moduli(&ctx, TRI, NG, &mut bad).is_err());
    let mut bad = Array::zeros(0, 4);
    assert!(o.compute_tangent_moduli(&ctx, TRI, NG, &mut bad).is_err());
}

/// Upstream `computeAllStresses`: `grad_u = ∇u − eigen_grad_u`, then the law.
/// `eigen_grad_u` (a parsed matrix) is copied into the not-ghost internal;
/// the orthotropic law applies it by default and ignores it upstream
/// (UD-105).
#[test]
fn compute_all_stresses_subtracts_eigen_grad_u() {
    let mut model = square();
    model.gradient = Some(vec![1e-3, 2e-4, -1e-4, 3e-4]);
    let mut h = materials_from_dat(
        &model,
        b"material elastic [\n name = s\n E = 1e3\n nu = 0.25\n eigen_grad_u = [[1e-4, 0], [5e-5, -1e-4]]\n]\n",
    )
    .unwrap();
    let law = h.law_mut(0).unwrap();
    let eig: Vec<Real> = law
        .base()
        .eigen_grad_u
        .get(TRI, NG)
        .unwrap()
        .tuple(0)
        .to_vec();
    // column-major M(r, c) at r + 2c: [[1e-4, 0], [5e-5, -1e-4]]
    assert_eq!(eig, vec![1e-4, 5e-5, 0., -1e-4]);
    law.compute_all_stresses(&model.ctx(LawPhase::Stress), NG)
        .unwrap();
    let g = law.base().grad_u.get(TRI, NG).unwrap().tuple(1).to_vec();
    let want: Vec<Real> = [1e-3, 2e-4, -1e-4, 3e-4]
        .iter()
        .zip(&eig)
        .map(|(a, b)| a - b)
        .collect();
    assert_eq!(g, want);
    let s = law.base().stress.get(TRI, NG).unwrap().tuple(1).to_vec();
    let mut expected = [[0.0; 2]; 2];
    let m = MaterialElastic::<2>::new(&spec(2), None);
    let mut m = m.unwrap();
    for (n, v) in [("E", 1e3), ("nu", 0.25)] {
        m.set_param(n, ParamValue::Real(v)).unwrap();
    }
    m.stress_on_quad(
        akantu_core::tensor::as_matrix(&want).unwrap(),
        0.,
        &mut expected,
    );
    assert_eq!(s, expected.as_flattened());
    // orthotropic: applied by default (UD-105), dropped upstream
    let dat = b"material elastic_orthotropic [\n name = o\n E1 = 2\n E2 = 1\n G12 = 1\n eigen_grad_u = [[1, 0], [0, 1]]\n]\n";
    let h = materials_from_dat(&model, dat).unwrap();
    for k in 0..2 {
        assert_eq!(
            h.law(0)
                .unwrap()
                .base()
                .eigen_grad_u
                .get(TRI, NG)
                .unwrap()
                .tuple(k),
            &[1., 0., 0., 1.]
        );
    }
    let h = materials_from_dat_with(&model, dat, Some(LawOptions::upstream())).unwrap();
    assert!(
        h.law(0)
            .unwrap()
            .base()
            .eigen_grad_u
            .get(TRI, NG)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&v| v == 0.)
    );
}

/// UD-032 (category A): the per-element potential energy integrates the
/// element's energy density (upstream integrates an uninitialised vector);
/// the element energies add up to the total.
#[test]
fn per_element_potential_energy() {
    let model = square();
    let mut h = materials_from_dat(
        &model,
        b"material elastic [\n name = s\n E = 1e3\n nu = 0.25\n]\n",
    )
    .unwrap();
    let law = h.law_mut(0).unwrap();
    let grads = [[1e-3, 2e-4, 2e-4, -5e-4], [3e-4, 0., 0., 1e-4]];
    for (k, g) in grads.iter().enumerate() {
        law.base_mut()
            .grad_u
            .get_mut(TRI, NG)
            .unwrap()
            .tuple_mut(k)
            .copy_from_slice(g);
    }
    let ctx = model.ctx(LawPhase::Energy);
    law.compute_stress(&ctx, TRI, NG).unwrap();
    let total = law.energy_of(&ctx, "potential").unwrap();
    let e0 = law
        .potential_energy_of_element(&ctx, Element::new(TRI, 0, NG))
        .unwrap();
    let e1 = law
        .potential_energy_of_element(&ctx, Element::new(TRI, 1, NG))
        .unwrap();
    assert!(e0 > 0. && e1 > 0.);
    assert!((e0 + e1 - total).abs() <= 4. * Real::EPSILON * total);
    assert_eq!(law.energy_of(&ctx, "dissipated").unwrap(), 0.);
}

/// Upstream errors are errors; upstream quirks kept.
#[test]
fn invalid_inputs_and_quirks() {
    let m1 = common::SimplexModel::new(1, &[vec![0.], vec![1.]], &[vec![0, 1]]);
    let m2 = square();
    let m3 = common::SimplexModel::new(
        3,
        &[
            vec![0., 0., 0.],
            vec![1., 0., 0.],
            vec![0., 1., 0.],
            vec![0., 0., 1.],
        ],
        &[vec![0, 1, 2, 3]],
    );
    // orthotropic in 1-D: error at the end of the parsed section
    assert!(
        materials_from_dat(
            &m1,
            b"material elastic_orthotropic [\n name = o\n E1 = 1\n]\n"
        )
        .is_err()
    );
    // parallel axes (2-D), left-handed axes (3-D)
    assert!(
        materials_from_dat(
            &m2,
            b"material elastic_anisotropic [\n name = a\n C11 = 1\n n1 = [1, 1]\n n2 = [2, 2]\n]\n"
        )
        .is_err()
    );
    assert!(
        materials_from_dat(
            &m3,
            b"material elastic_anisotropic [\n name = a\n C11 = 1\n n3 = [0, 0, -1]\n]\n"
        )
        .is_err()
    );
    // upstream accepts left-handed 2-D axes (only |n1 x n2| = 1 is checked)
    assert!(
        materials_from_dat(
            &m2,
            b"material elastic_anisotropic [\n name = a\n C11 = 1\n n1 = [0, 1]\n n2 = [1, 0]\n]\n"
        )
        .is_ok()
    );
    // wave speeds
    let h = materials_from_dat(
        &m2,
        b"material elastic_anisotropic [\n name = a\n C11 = 1\n rho = 1\n]\n",
    )
    .unwrap();
    let e = Element::new(TRI, 0, NG);
    assert!(matches!(
        h.law(0).unwrap().push_wave_speed(e),
        Err(Error::NotImplemented(_))
    ));
    assert!(matches!(
        h.law(0).unwrap().shear_wave_speed(e),
        Err(Error::NotImplemented(_))
    ));
    let h = materials_from_dat(
        &m1,
        b"material elastic [\n name = s\n E = 4\n nu = 0.3\n rho = 1\n]\n",
    )
    .unwrap();
    assert!(
        h.law(0)
            .unwrap()
            .shear_wave_speed(Element::new(ElementType::Segment2, 0, NG))
            .is_err()
    );
    assert_eq!(
        real(h.law(0).unwrap(), "nu"),
        0.,
        "nu forced to 0 in 1-D by initMaterial"
    );
    assert_eq!(
        h.law(0)
            .unwrap()
            .celerity(Element::new(ElementType::Segment2, 0, NG))
            .unwrap(),
        2.
    );
    // dimension mismatch at construction
    assert!(MaterialElastic::<2>::new(&spec(3), None).is_err());
    // Cij of the orthotropic law are overwritten by the engineering constants
    let h = materials_from_dat(
        &m2,
        b"material elastic_orthotropic [\n name = o\n C11 = 99\n E1 = 2\n E2 = 1\n G12 = 1\n]\n",
    )
    .unwrap();
    assert_eq!(real(h.law(0).unwrap(), "C11"), 2.);
    // a NaN stiffness (E1 = 0) does not make the update fail (upstream
    // ignores Eigen's status); the upstream celerity is NaN
    let mut h = materials_from_dat(
        &m2,
        b"material elastic_orthotropic [\n name = o\n E2 = 1\n G12 = 1\n]\n",
    )
    .unwrap();
    let o = h.law_mut(0).unwrap();
    *o.options_mut().unwrap() = LawOptions::upstream();
    assert!(o.celerity(e).unwrap().is_nan());
}

/// Material part of upstream `test_material_orthotropic` (the model run is
/// in the model tests): `orthotropic.dat` (verbatim parameters; 3-component axes
/// truncated in 2-D) — `test_mat_2` is `test_mat_1` with the axes rotated
/// by 90°: its stiffness swaps the 1 and 2 directions.
#[test]
fn test_material_orthotropic_materials() {
    let dat = b"material elastic_orthotropic [\n\
         name = test_mat_1\n\
         rho  = 3.8328782866680342684   # density in g/mol/A^2\n\
         E1   = 10 * 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         E2   = 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         nu12 = 0.33333333333333333333  # poisson's ratio\n\
         G12  = 3.6702824005625512938   # shear modulus in kcal/mol/A^3 \n\
]\n\
\n\
material elastic_orthotropic [\n\
         name = test_mat_2\n\
\t n1 = [0, 1, 0]\n\
\t n2 = [-1, 0, 0]\n\
         rho  = 3.8328782866680342684   # density in g/mol/A^2\n\
         E1   = 10 * 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         E2   = 9.7874197348334701168   # young's in kcal/mol/A^3\n\
         nu12 = 0.33333333333333333333  # poisson's ratio\n\
         G12  = 3.6702824005625512938   # shear modulus in kcal/mol/A^3 \n\
]\n";
    let model = square();
    let h = materials_from_dat(&model, dat).unwrap();
    assert_eq!(
        h.law(1).unwrap().get_param("n1").unwrap(),
        ParamValue::VectorReal(vec![0., 1.])
    );
    // one handler per material, so that each law owns the elements
    let text = std::str::from_utf8(dat).unwrap();
    let split = text.find("\n\nmaterial").unwrap();
    let c = |part: &str| {
        let mut hm = materials_from_dat(&model, part.as_bytes()).unwrap();
        let mut t = Array::zeros(2, 9);
        hm.law_mut(0)
            .unwrap()
            .compute_tangent_moduli(&model.ctx(LawPhase::Tangent), TRI, NG, &mut t)
            .unwrap();
        t.tuple(0).to_vec()
    };
    let (c1, c2) = (c(&text[..split]), c(&text[split..]));
    // column-major 3 × 3: swap of directions 1 and 2
    let at = |c: &[Real], i: usize, j: usize| c[i + 3 * j];
    let tol = 1e-12 * at(&c1, 0, 0);
    assert!((at(&c2, 0, 0) - at(&c1, 1, 1)).abs() <= tol);
    assert!((at(&c2, 1, 1) - at(&c1, 0, 0)).abs() <= tol);
    assert!((at(&c2, 0, 1) - at(&c1, 0, 1)).abs() <= tol);
    assert!((at(&c2, 2, 2) - at(&c1, 2, 2)).abs() <= tol);
    assert!(at(&c2, 0, 2).abs() <= tol && at(&c2, 1, 2).abs() <= tol);
    // a usable celerity for the stable time step (both conventions)
    for law in h.laws() {
        let k = law.celerity(Element::new(TRI, 0, NG)).unwrap();
        assert!(k.is_finite() && k > 0.);
    }
}

/// D-006 through a law: random internals are resized in upstream
/// register-id order (`std::map`: `Sd` before `Yd`, whatever the
/// registration order), each block drawing from the global engine.
#[test]
fn random_internals_draw_in_register_id_order() {
    #[derive(Debug)]
    struct Noop;
    impl ForeignMaterialMethods for Noop {
        fn compute_stress(
            &mut self,
            _: &mut ForeignLaw<'_, '_>,
            _: ElementType,
            _: GhostType,
        ) -> akantu_core::Result<()> {
            Ok(())
        }
        fn push_wave_speed(
            &self,
            _: &akantu_solid::ForeignLawState,
            _: Element,
        ) -> akantu_core::Result<Real> {
            Ok(0.)
        }
    }
    let model = square();
    let mut m = ForeignMaterial::new(&spec(2), "random_law", Box::new(Noop)).unwrap();
    let yd = RandomParameter::make(50., RandomDistributionType::Uniform, 0., 10.);
    let sd = RandomParameter::make(5e3, RandomDistributionType::Weibull, 100., 3.);
    m.state
        .internals
        .register(
            "Yd",
            AnyInternalField::Real(InternalField::new_random("Yd", 1, yd.clone()).unwrap()),
        )
        .unwrap();
    m.state
        .internals
        .register(
            "Sd",
            AnyInternalField::Real(InternalField::new_random("Sd", 1, sd.clone()).unwrap()),
        )
        .unwrap();
    m.add_element(Element::new(TRI, 0, NG)).unwrap();
    m.add_element(Element::new(TRI, 1, NG)).unwrap();
    RandomGenerator::<IdxSlot>::seed(4242);
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    let mut e = MinstdRand0::with_seed(4242);
    let (mut sd, mut yd) = (sd, yd);
    let mut sd_v = [0.0; 2];
    sd.set_values_with(&mut e, &mut sd_v);
    let mut yd_v = [0.0; 2];
    yd.set_values_with(&mut e, &mut yd_v);
    assert_eq!(
        m.state
            .internals
            .real("Sd")
            .unwrap()
            .get(TRI, NG)
            .unwrap()
            .as_slice(),
        &sd_v
    );
    assert_eq!(
        m.state
            .internals
            .real("Yd")
            .unwrap()
            .get(TRI, NG)
            .unwrap()
            .as_slice(),
        &yd_v
    );
}
