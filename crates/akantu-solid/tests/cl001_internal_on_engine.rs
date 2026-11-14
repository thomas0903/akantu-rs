// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test of the per-internal FE engine and element filter ported from
// Akantu (EPFL/LSMS) src/model/common/constitutive_laws/constitutive_law.hh:74-86
// and constitutive_law_tmpl.hh:46-72 (registerInternal(id, nb_component,
// fe_engine_id[, element_filter])), src/model/common/constitutive_laws/
// internal_field_tmpl.hh (InternalField::resize: nb_quad from the field's fem,
// blocks of the field's element filter).

//! CL-001: a user law registers internals on a second
//! FE engine of the model (here an engine of integration order 2: three
//! points per triangle, against one for the default engine) and on its own
//! element filter (of another size than the law's).
//! `initMaterial` sizes each internal on its own support; the
//! law-support-only resize paths refuse such internals instead of sizing
//! them wrongly, before any field changes; a compaction that would
//! have to move such an internal is refused, an identity one
//! leaves them as they are; an unknown engine is an error.

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::{IntegrationOrders, ShapeFamily};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, LawContext, LawPhase, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamValue, ParameterRegistry};
use akantu_solid::material::internal_blocks;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
};

const TRI: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

/// Two triangles of the unit square; the default engine has one
/// integration point per triangle, the engine `"high"` three (order 2).
struct TwoEngines {
    mesh: Mesh,
    displacement: Array<Real>,
    high: FeEngine,
}

impl TwoEngines {
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
        let mut high = FeEngine::new(
            &mesh,
            Some(2),
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "square:fem:high",
        )
        .unwrap();
        high.set_integration_orders(IntegrationOrders {
            per_type: vec![(TRI, 2)],
            mass: None,
        })
        .unwrap();
        high.init_shape_functions(&mesh, NG).unwrap();
        Self {
            mesh,
            displacement: Array::zeros(4, 2),
            high,
        }
    }
}

impl ModelServices for TwoEngines {
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
        _: ElementType,
        _: GhostType,
        _: &[Idx],
        _: &mut Array<Real>,
    ) -> Result<()> {
        Err(Error::NotImplemented("not used"))
    }
    fn nodal_field(&self, name: &str) -> Result<akantu_core::array_shared::ArrayRef<'_, Real>> {
        match name {
            "displacement" => Ok((&self.displacement).into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        match name {
            "high" => Ok(&self.high),
            _ => Err(Error::not_found("FE engine", name)),
        }
    }
}

#[derive(Debug)]
struct Inert;

impl ForeignMaterialMethods for Inert {
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        Ok(())
    }
    fn compute_tangent_moduli(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
        _: &mut Array<Real>,
    ) -> Result<()> {
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
}

fn subset(elements: &[Idx]) -> ElementTypeMapArray<Idx> {
    let mut f = ElementTypeMapArray::new("subset");
    f.as_map_mut()
        .insert(TRI, NG, Array::from_vec(elements.to_vec(), 1).unwrap())
        .unwrap();
    f
}

/// A user law on elements `filter` with two plain internals (`first`
/// sorts before the others: a per-field refusal inside the name-ordered
/// resize loop would resize it before refusing), one on the engine
/// `engine` (law filter) and one on `engine` with its own filter `{0, 1}`.
fn law(filter: &[Idx], engine: &str) -> ForeignMaterial {
    let spec = LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "user".into(),
    };
    let mut m = ForeignMaterial::new(&spec, "user_law", Box::new(Inert)).unwrap();
    m.register_internal_real("plain", 1).unwrap();
    m.register_internal_real_on("on_engine", 2, engine, None)
        .unwrap();
    m.register_internal_real_on("on_engine_subset", 1, engine, Some(subset(&[0, 1])))
        .unwrap();
    m.register_internal_real("first", 1).unwrap();
    m.set_param("rho", ParamValue::Real(1.0)).unwrap();
    for &e in filter {
        m.add_element(Element::new(TRI, e, NG)).unwrap();
    }
    m
}

fn lengths(m: &ForeignMaterial) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (n, f) in m.state.internals.iter() {
        let len = match f {
            akantu_model::internal_field::AnyInternalField::Real(f) => {
                f.get(TRI, NG).map_or(0, |a| a.len())
            }
            _ => unreachable!(),
        };
        out.push((n.to_owned(), len));
    }
    out
}

fn len_table(plain: usize, on_engine: usize, subset: usize, first: usize) -> Vec<(String, usize)> {
    vec![
        ("plain".to_owned(), plain),
        ("on_engine".to_owned(), on_engine),
        ("on_engine_subset".to_owned(), subset),
        ("first".to_owned(), first),
    ]
}

fn law_numbering(values: Vec<Idx>) -> ElementTypeMapArray<Idx> {
    let mut numbering = ElementTypeMapArray::new("numbering");
    numbering
        .as_map_mut()
        .insert(TRI, NG, Array::from_vec(values, 1).unwrap())
        .unwrap();
    numbering
}

#[test]
fn cl001_internals_on_a_second_engine_and_own_filter() {
    let model = TwoEngines::new();
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &model,
    };
    // the law holds element 0 only; the subset internal holds elements 0
    // and 1: its own filter has another size than the law's (an
    // init path sizing it on the law's filter gives 3, not 6)
    let mut m = law(&[0], "high");
    m.init_material(&ctx).unwrap();
    assert_eq!(lengths(&m), len_table(1, 3, 6, 1));
    // the field records its support
    let f = m.state.internals.real("on_engine_subset").unwrap();
    assert_eq!(f.fe_engine_id(), Some("high"));
    assert_eq!(
        f.element_filter().unwrap().get(TRI, NG).unwrap().as_slice(),
        [0, 1]
    );
    assert!(!m.state.internals.real("plain").unwrap().has_own_support());
    // the support-aware resize keeps each internal on its support (a resize
    // sizing the subset internal on the law's filter gives 3)
    m.resize_internals_with(&|engine, filter| internal_blocks(&ctx, engine, filter))
        .unwrap();
    assert_eq!(lengths(&m), len_table(1, 3, 6, 1));

    // a resize that only knows the law's support refuses them, before any
    // field is resized (`first` sorts before the refused fields and
    // keeps its length although the law now holds two elements) …
    m.add_element(Element::new(TRI, 1, NG)).unwrap();
    let blocks = m.base().blocks(&ctx);
    let e = m.resize_internals(&blocks).unwrap_err().to_string();
    assert!(
        e.contains("on_engine") && e.contains("resize_internals_with"),
        "{e}"
    );
    assert_eq!(lengths(&m), len_table(1, 3, 6, 1));
    let e = m
        .resize_internals_from_filter(&|_, _| 1)
        .unwrap_err()
        .to_string();
    assert!(e.contains("resize_internals_with"), "{e}");
    assert_eq!(lengths(&m), len_table(1, 3, 6, 1));
    // … the support-aware resize follows the law's filter for the plain
    // and the engine internals, and keeps the subset internal on its filter
    m.resize_internals_with(&|engine, filter| internal_blocks(&ctx, engine, filter))
        .unwrap();
    assert_eq!(lengths(&m), len_table(2, 6, 6, 2));

    // an identity law-local renumbering (the law's elements renumbered in
    // the mesh, none removed) leaves every internal as it is …
    m.remove_integration_points(&law_numbering(vec![0, 1]), &|_, _| 1)
        .unwrap();
    assert_eq!(lengths(&m), len_table(2, 6, 6, 2));
    // … a removal of a law element would have to compact the internals on
    // their own support: refused before any field changes
    let e = m
        .remove_integration_points(&law_numbering(vec![-1, 0]), &|_, _| 1)
        .unwrap_err();
    assert!(matches!(e, Error::Unsupported(_)), "{e}");
    assert!(e.to_string().contains("on_engine"), "{e}");
    assert_eq!(lengths(&m), len_table(2, 6, 6, 2));

    // an unknown engine is an error at initialisation
    let mut bad = law(&[0, 1], "nope");
    let e = bad.init_material(&ctx).unwrap_err();
    assert!(matches!(e, Error::NotFound { .. }), "{e}");
}
