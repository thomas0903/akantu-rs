// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): a Rust transcription of the structure of
// examples/python/solid_mechanics_model/custom-material/bi-material.py
// (LocalElastic: registerParamReal E/nu, registerInternalReal factor and
// quad_coordinates, initMaterial calling super().initMaterial() and
// getModel().getFEEngine().computeIntegrationPointsCoordinates, factor 0.5
// below y = 0.5, computeStress scaled by factor) and
// python/py_parser.cc / py_constitutive_law.cc (runtime registration).

//! A law registered at run time with runtime
//! parameters and internal fields, whose callbacks read model data through
//! a phase-scoped context.

use std::sync::Arc;

use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::{
    InternalFieldMut, LawContext, LawPhase, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamValue, ParameterAccess};
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material, MaterialFactory,
};

const TRI: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

/// Model services of a toy 2D model: two triangles of the unit square,
/// one integration point per element (the centroid).
struct ToyModel {
    mesh: Mesh,
    displacement: Array<Real>,
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
        Self {
            mesh,
            displacement: Array::zeros(4, 2),
        }
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
        match name {
            "displacement" => Ok((&self.displacement).into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
}

/// Behaviour of the user's `LocalElastic` class.
#[derive(Debug, Default)]
struct LocalElastic {
    lambda: Real,
    mu: Real,
}

impl ForeignMaterialMethods for LocalElastic {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        let nu = law.state.params.get_real("nu")?;
        let e = law.state.params.get_real("E")?;
        self.lambda = nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu));
        self.mu = e / (2.0 * (1.0 + nu));
        law.base_init_material()?; // super().initMaterial()
        // model.getFEEngine().computeIntegrationPointsCoordinates(
        //     self.quad_coords, self.getElementFilter())
        let model = law.ctx.services;
        let filter = law
            .state
            .base
            .element_filter
            .get(TRI, NG)?
            .as_slice()
            .to_vec();
        let mut coords = Array::new(2);
        model.integration_point_coordinates(TRI, NG, &filter, &mut coords)?;
        law.state
            .internals
            .real_mut("quad_coordinates")?
            .get_mut(TRI, NG)?
            .copy_from(&coords)?;
        let mut factor = law.state.internals.real_mut("factor")?.get_mut(TRI, NG)?;
        for (f, c) in factor.as_mut_slice().iter_mut().zip(coords.tuples()) {
            *f = if c[1] < 0.5 { 0.5 } else { 1.0 };
        }
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
        let factor = internals.real("factor")?.get(ty, g)?;
        let grad_u = base.grad_u.get(ty, g)?.clone();
        let mut sigma = base.stress.get_mut(ty, g)?;
        for ((s, gu), f) in sigma
            .tuples_mut()
            .zip(grad_u.tuples())
            .zip(factor.as_slice())
        {
            let (exx, eyy, exy) = (gu[0], gu[3], 0.5 * (gu[1] + gu[2]));
            let tr = exx + eyy;
            s[0] = f * (self.lambda * tr + 2.0 * self.mu * exx);
            s[3] = f * (self.lambda * tr + 2.0 * self.mu * eyy);
            s[1] = f * 2.0 * self.mu * exy;
            s[2] = s[1];
        }
        Ok(())
    }

    fn push_wave_speed(&self, state: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(((self.lambda + 2.0 * self.mu) / state.base.rho).sqrt())
    }
}

/// `aka.registerCustomMaterials({"local_elastic": LocalElastic})`.
fn register_local_elastic(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "local_elastic",
        Arc::new(
            |spec: &LawSpec, model: &dyn ModelServices| -> Result<Box<dyn Material>> {
                assert_eq!(model.spatial_dimension(), spec.spatial_dimension);
                let mut m =
                    ForeignMaterial::new(spec, "local_elastic", Box::new(LocalElastic::default()))?;
                let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
                m.register_param_real("E", rp, "Youngs modulus")?;
                m.register_param_real("nu", rp, "Poisson ratio")?;
                m.register_internal_real("factor", 1)?;
                m.register_internal_real("quad_coordinates", 2)?;
                Ok(Box::new(m))
            },
        ),
    )
}

fn create(model: &ToyModel) -> Box<dyn Material> {
    let mut factory = MaterialFactory::default();
    register_local_elastic(&mut factory).unwrap();
    let spec = LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "fictive".into(),
    };
    let mut m = factory.create("local_elastic", &spec, model).unwrap();
    m.base_mut()
        .element_filter
        .as_map_mut()
        .get_or_insert_with(TRI, NG, || Array::new(1))
        .append(&Array::from_vec(vec![0, 1], 1).unwrap())
        .unwrap();
    m
}

#[test]
fn runtime_law_registers_parameters_and_internals() {
    let model = ToyModel::new();
    let mut m = create(&model);
    // `.dat` / initFull({"fictive": {"type": "local_elastic", "rho": 1, "E": 1, "nu": 0.3}})
    for (k, v) in [("rho", 1.0), ("E", 1.0), ("nu", 0.3)] {
        m.set_param(k, ParamValue::Real(v)).unwrap();
    }
    assert_eq!(m.parameter_names(), ["name", "rho", "E", "nu"]);
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &model,
    };
    m.init_material(&ctx).unwrap();

    let mut names = vec![];
    m.for_each_internal(&mut |n, _| names.push(n.to_owned()));
    assert_eq!(
        names,
        [
            "stress",
            // every upstream `Material` registers `eigen_grad_u`
            "eigen_grad_u",
            "grad_u",
            "potential_energy",
            "factor",
            "quad_coordinates"
        ]
    );

    // Uniaxial strain εxx = 0.01 in both elements.
    for gu in m.base_mut().grad_u.get_mut(TRI, NG).unwrap().tuples_mut() {
        gu.copy_from_slice(&[0.01, 0.0, 0.0, 0.0]);
    }
    let ctx = LawContext {
        phase: LawPhase::Stress,
        services: &model,
    };
    m.compute_stress(&ctx, TRI, NG).unwrap();
    let (nu, e) = (0.3, 1.0);
    let (l, mu) = (
        nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu)),
        e / (2.0 * (1.0 + nu)),
    );
    let sxx: Vec<f64> = m
        .base()
        .stress
        .get(TRI, NG)
        .unwrap()
        .tuples()
        .map(|s| s[0])
        .collect();
    // Element 0 (centroid y = 1/3 < 0.5) is softened by factor 0.5.
    assert_eq!(sxx, [0.5 * (l + 2.0 * mu) * 0.01, (l + 2.0 * mu) * 0.01]);
    assert_eq!(
        m.push_wave_speed(Element::new(TRI, 0, NG)).unwrap(),
        (l + 2.0 * mu).sqrt()
    );
}

#[test]
fn runtime_internals_follow_the_history_and_resize_contract() {
    let model = ToyModel::new();
    let mut m = create(&model);
    for (k, v) in [("rho", 1.0), ("E", 1.0), ("nu", 0.3)] {
        m.set_param(k, ParamValue::Real(v)).unwrap();
    }
    m.init_material(&LawContext {
        phase: LawPhase::Init,
        services: &model,
    })
    .unwrap();
    m.for_each_internal_mut(&mut |n, f| {
        if let (true, InternalFieldMut::Real(f)) = (n == "factor", f) {
            f.initialize_history();
        }
        Ok(())
    })
    .unwrap();
    m.save_previous_state().unwrap();
    m.for_each_internal_mut(&mut |n, f| {
        if let (true, InternalFieldMut::Real(f)) = (n == "factor", f) {
            f.get_mut(TRI, NG)?.fill(9.0);
        }
        Ok(())
    })
    .unwrap();
    m.restore_previous_state().unwrap();
    let mut factor = vec![];
    m.for_each_internal(&mut |n, f| {
        if let (true, akantu_model::constitutive_law::InternalFieldRef::Real(f)) =
            (n == "factor", f)
        {
            factor = f.get(TRI, NG).unwrap().as_slice().to_vec();
        }
    });
    assert_eq!(
        factor,
        [0.5, 1.0],
        "runtime internal restored like a typed member"
    );
    // Element addition (e.g. after insertion): every internal grows.
    m.base_mut()
        .element_filter
        .get_mut(TRI, NG)
        .unwrap()
        .push_tuple(&[1]);
    m.init_material(&LawContext {
        phase: LawPhase::Init,
        services: &model,
    })
    .unwrap();
    let mut sizes = vec![];
    m.for_each_internal(&mut |n, f| {
        if let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f {
            sizes.push((n.to_owned(), f.get(TRI, NG).unwrap().len()));
        }
    });
    assert!(sizes.iter().all(|(_, s)| *s == 3), "{sizes:?}");
}

#[test]
fn unset_runtime_parameter_is_an_error_not_garbage() {
    let model = ToyModel::new();
    let mut m = create(&model);
    m.set_param("rho", ParamValue::Real(1.0)).unwrap();
    let err = m
        .init_material(&LawContext {
            phase: LawPhase::Init,
            services: &model,
        })
        .unwrap_err();
    assert!(
        err.to_string().contains("nu") || err.to_string().contains('E'),
        "{err}"
    );
}

/// A runtime law reads its
/// `material` section through `Parsable::parse_section` — runtime `Real`
/// parameters evaluate expressions with variables of the enclosing scope,
/// and the upstream `std::string name` keeps every byte of the raw value
/// (not UTF-8, trailing white space included), as
/// `ParameterTyped<std::string>::setAuto`.
#[test]
fn runtime_law_parses_its_material_section() {
    use akantu_parser::{
        AnonymousIds, ByteString, ParameterRegistry, Parsable, Parser, ParserType,
    };

    let spec = LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "fictive".into(),
    };
    let mut m =
        ForeignMaterial::new(&spec, "local_elastic", Box::new(LocalElastic::default())).unwrap();
    let rp = ParameterAccess::READABLE | ParameterAccess::PARSMOD;
    m.register_param_real("E", rp, "Youngs modulus").unwrap();
    m.register_param_real("nu", rp, "Poisson ratio").unwrap();
    assert_eq!(
        m.get_param("name").unwrap(),
        ParamValue::ByteString(ByteString::from("fictive"))
    );

    let input = b"E0 = 1e9\nmaterial local_elastic [\n  name = st\xe9el \xff \n  rho = 7800\n  E = 2*E0\n  nu = 0.3\n]\n";
    let p = Parser::parse_bytes(input, "m.dat", AnonymousIds::Local(&mut 12)).unwrap();
    let section = &p.root.sub_sections[0];
    assert_eq!(section.section_type, ParserType::Material);
    m.parse_section(section, false).unwrap();
    assert_eq!(
        m.get_param("name").unwrap(),
        ParamValue::ByteString(ByteString::from(&b"st\xe9el \xff "[..]))
    );
    assert_eq!(m.get_param("rho").unwrap(), ParamValue::Real(7800.0));
    assert_eq!(m.get_param("E").unwrap(), ParamValue::Real(2e9));
    assert_eq!(m.get_param("nu").unwrap(), ParamValue::Real(0.3));

    // Unknown names: an error, a warning in permissive mode (upstream
    // `parseParam`); a section of another type is rejected.
    let p = Parser::parse_bytes(
        b"material local_elastic [\n  E = 1\n  bogus = 2\n]\nsolver s [\n]\n",
        "u.dat",
        AnonymousIds::Local(&mut 12),
    )
    .unwrap();
    let material = p
        .root
        .sub_sections
        .iter()
        .find(|s| s.section_type == ParserType::Material)
        .unwrap();
    assert!(m.parse_section(material, false).is_err());
    m.parse_section(material, true).unwrap();
    assert_eq!(m.get_param("E").unwrap(), ParamValue::Real(1.0));
    let other = p
        .root
        .sub_sections
        .iter()
        .find(|s| s.section_type != ParserType::Material)
        .unwrap();
    assert!(m.parse_section(other, true).is_err());
}

/// A runtime law answering `hasStiffnessMatrixChanged` itself
/// (upstream `PYBIND11_OVERRIDE(bool, _Material,
/// hasStiffnessMatrixChanged)`): the answer decides `hasMatrixChanged("K")`;
/// an error makes the law answer `true` and is handed to the model once by
/// `take_deferred_error`.
#[derive(Debug)]
struct Answering(Arc<std::sync::Mutex<Result<bool>>>);

impl ForeignMaterialMethods for Answering {
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
    fn has_stiffness_matrix_changed(
        &self,
        _: &ForeignLawState,
        _: &LawContext<'_>,
    ) -> Result<bool> {
        match &*self.0.lock().unwrap() {
            Ok(b) => Ok(*b),
            Err(e) => Err(Error::invalid(e.to_string())),
        }
    }
}

#[test]
fn runtime_law_answers_has_stiffness_matrix_changed() {
    let model = ToyModel::new();
    let ctx = LawContext {
        phase: LawPhase::Tangent,
        services: &model,
    };
    let spec = LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "answering".into(),
    };
    let answer = Arc::new(std::sync::Mutex::new(Ok(false)));
    let law =
        ForeignMaterial::new(&spec, "answering", Box::new(Answering(Arc::clone(&answer)))).unwrap();
    assert!(!law.has_matrix_changed_in(&ctx, "K"), "the law's answer");
    law.take_deferred_error().unwrap();
    *answer.lock().unwrap() = Ok(true);
    assert!(law.has_matrix_changed_in(&ctx, "K"));
    *answer.lock().unwrap() = Err(Error::invalid("raised by the law"));
    assert!(
        law.has_matrix_changed_in(&ctx, "K"),
        "an error answers true"
    );
    let e = law.take_deferred_error().unwrap_err();
    assert!(e.to_string().contains("raised by the law"), "{e}");
    law.take_deferred_error().unwrap(); // handed over once
    // the state-free question keeps the base answer
    *answer.lock().unwrap() = Ok(false);
    assert!(law.has_stiffness_matrix_changed());
}
