// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the contract of python/py_material.cc's
// `PyMaterial<MaterialElastic<dim>>` trampoline (methods a subclass does not
// define fall back to src/model/solid_mechanics/materials/material_elastic.cc)
// as a Rust test of `akantu_solid::DerivedMaterial` (D-078,
// PY-077), on the toy model structure of runtime_registered_law.rs.

//! A runtime law derived from the built-in elastic law: without overrides
//! it is the elastic law (parameters, internals, stress, tangent, wave
//! speeds, bit for bit); an override replaces one method only; runtime
//! parameters and internals come after the built-in ones.

use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, LawContext, LawPhase, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamValue, ParameterAccess};
use akantu_solid::materials::elastic::new_elastic_base;
use akantu_solid::{
    DerivedLaw, DerivedLawState, DerivedMaterial, DerivedMaterialMethods, Material, RuntimeLawState,
};

const TRI: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

struct ToyModel {
    mesh: Mesh,
}

impl ToyModel {
    fn new() -> Self {
        let mut mesh = Mesh::new(2, "square").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(TRI, NG);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[1, 3, 2]);
        }
        Self { mesh }
    }
}

impl ModelServices for ToyModel {
    fn spatial_dimension(&self) -> usize {
        2
    }
    fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
        1
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        g: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        *out = Array::new(2);
        for &e in filter {
            let b = self.mesh.barycenter(Element::new(ty, e, g))?;
            out.push_tuple(&b);
        }
        Ok(())
    }
    fn nodal_field(&self, name: &str) -> Result<akantu_core::array_shared::ArrayRef<'_, Real>> {
        Err(Error::not_found("nodal field", name))
    }
}

fn spec() -> LawSpec {
    LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "bar".into(),
    }
}

fn with_filter(m: &mut dyn Material) {
    m.base_mut()
        .element_filter
        .as_map_mut()
        .get_or_insert_with(TRI, NG, || Array::new(1))
        .append(&Array::from_vec(vec![0, 1], 1).unwrap())
        .unwrap();
}

fn set_common(m: &mut dyn Material) {
    for (k, v) in [("rho", 2.0), ("E", 3.0), ("nu", 0.25)] {
        m.set_param(k, ParamValue::Real(v)).unwrap();
    }
}

/// No override: every default is the base law's.
#[derive(Debug)]
struct NoOverride;
impl DerivedMaterialMethods for NoOverride {}

/// Doubles the stress of the base after computing it, halves the wave speed.
#[derive(Debug)]
struct Scaled;
impl DerivedMaterialMethods for Scaled {
    fn compute_stress(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        law.state.law.compute_stress(law.ctx, ty, ghost)?;
        for s in law
            .state
            .law
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .as_mut_slice()
        {
            *s *= 2.0;
        }
        Ok(())
    }
    fn push_wave_speed(&self, state: &DerivedLawState, element: Element) -> Result<Real> {
        Ok(state.law.push_wave_speed(element)? * 0.5)
    }
}

fn run(m: &mut dyn Material, model: &ToyModel) -> (Vec<Real>, Vec<Real>, Real, Real) {
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: model,
    };
    m.init_material(&ctx).unwrap();
    for gu in m.base_mut().grad_u.get_mut(TRI, NG).unwrap().tuples_mut() {
        gu.copy_from_slice(&[0.01, 0.002, -0.003, 0.004]);
    }
    let ctx = LawContext {
        phase: LawPhase::Stress,
        services: model,
    };
    m.compute_stress(&ctx, TRI, NG).unwrap();
    let stress = m.base().stress.get(TRI, NG).unwrap().as_slice().to_vec();
    let mut tangent = Array::zeros(2, 9);
    m.compute_tangent_moduli(&ctx, TRI, NG, &mut tangent)
        .unwrap();
    let el = Element::new(TRI, 0, NG);
    (
        stress,
        tangent.as_slice().to_vec(),
        m.push_wave_speed(el).unwrap(),
        m.shear_wave_speed(el).unwrap(),
    )
}

#[test]
fn a_derived_law_without_overrides_is_the_base_law() {
    let model = ToyModel::new();
    let mut base = new_elastic_base(&spec(), &model).unwrap();
    with_filter(base.as_mut());
    set_common(base.as_mut());
    let mut derived = DerivedMaterial::new(
        new_elastic_base(&spec(), &model).unwrap(),
        "py_elastic",
        Box::new(NoOverride),
    );
    with_filter(&mut derived);
    set_common(&mut derived);
    assert_eq!(derived.law_type(), "py_elastic");
    assert_eq!(
        derived.get_param("lambda").unwrap(),
        base.get_param("lambda").unwrap()
    );
    let a = run(base.as_mut(), &model);
    let b = run(&mut derived, &model);
    assert_eq!(a, b);
    assert!(a.0.iter().any(|&s| s != 0.0));
    let (mut na, mut nb) = (vec![], vec![]);
    base.for_each_internal(&mut |n, _| na.push(n.to_owned()));
    derived.for_each_internal(&mut |n, _| nb.push(n.to_owned()));
    assert_eq!(na, nb);
}

#[test]
fn an_override_replaces_one_method_and_runtime_members_follow_the_base() {
    let model = ToyModel::new();
    let mut base = new_elastic_base(&spec(), &model).unwrap();
    with_filter(base.as_mut());
    set_common(base.as_mut());
    let mut derived = DerivedMaterial::new(
        new_elastic_base(&spec(), &model).unwrap(),
        "py_elastic",
        Box::new(Scaled),
    );
    with_filter(&mut derived);
    derived
        .register_param_real("factor", ParameterAccess::PARSMOD, "a runtime parameter")
        .unwrap();
    assert!(
        derived
            .register_param_real("E", ParameterAccess::PARSMOD, "taken")
            .is_err()
    );
    derived.register_internal_real("my_field", 2).unwrap();
    set_common(&mut derived);
    derived.set_param("factor", ParamValue::Real(0.5)).unwrap();
    assert_eq!(derived.get_param("factor").unwrap(), ParamValue::Real(0.5));
    let a = run(base.as_mut(), &model);
    let b = run(&mut derived, &model);
    let doubled: Vec<Real> = a.0.iter().map(|s| s * 2.0).collect();
    assert_eq!(b.0, doubled);
    assert_eq!(b.1, a.1, "the tangent is the base's");
    assert_eq!(b.2, a.2 * 0.5);
    assert_eq!(b.3, a.3, "the shear wave speed is the base's");
    // the runtime internal is sized by the base initialisation
    let mut names = vec![];
    derived.for_each_internal(&mut |n, _| names.push(n.to_owned()));
    assert_eq!(names.last().map(String::as_str), Some("my_field"));
    let f = derived.state.internals.real_mut("my_field").unwrap();
    assert_eq!(f.get(TRI, NG).unwrap().len(), 2);
    // the state as a callback sees it: the base answers of super-calls and
    // the elastic law's own internals
    let st: &dyn RuntimeLawState = &derived.state;
    assert_eq!(
        st.base_push_wave_speed(Element::new(TRI, 0, NG)).unwrap(),
        a.2
    );
    let own: Vec<&str> = st.own_real_internals().iter().map(|(n, _)| *n).collect();
    assert_eq!(
        own,
        [
            "third_axis_deformation",
            "delta_T",
            "sigma_th",
            "epsilon_th"
        ]
    );
}
