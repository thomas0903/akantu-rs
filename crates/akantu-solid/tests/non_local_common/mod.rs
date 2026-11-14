// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// test_non_local_toolbox/{test_material.{hh,cc}, test_material_damage.{hh,cc}},
// test/test_model/test_solid_mechanics_model/test_materials/
// test_material_non_local/custom_non_local_test_material.{hh,cc}.

//! User-defined non-local test laws of the upstream non-local tests, built
//! on `MaterialElastic` through the public `Material` /
//! `MaterialNonLocal` interfaces (the way a user extends the port):
//!
//! * `TestMaterial` (`MaterialDamageNonLocal<dim, MaterialDamage<dim,
//!   MaterialElastic>>`, neighbourhood `test_region`): averages `grad_u` →
//!   `grad_u non local`, no non-local stress;
//! * `TestMaterialDamage` (same, neighbourhood = law name);
//! * `CustomNonLocalTestMaterial` (`MaterialNonLocal<dim,
//!   MaterialElastic<dim>>`): averages `local_damage` → `damage`, then
//!   `σ ← σ·(1 − damage)`.

#![allow(dead_code)] // each test binary uses a subset

use std::path::PathBuf;
use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::tensor::{Matrix, MatrixExt, as_matrix_mut};
use akantu_core::{Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_model::non_local::NonLocalVariable;
use akantu_parser::param::{ParamInfo, ParamValue};
use akantu_parser::{ParameterRegistry, Parsable, ParserType};
use akantu_solid::material::{LawOptions, Material, MaterialBase, MaterialFactory};
use akantu_solid::materials::damage::DamageFields;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::non_local::MaterialNonLocal;

/// Upstream test data directory copied with the akantu-model tests.
pub fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../akantu-model/tests/data/upstream/non_local")
        .join(name)
}

/// What the non-local stress step does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonLocalStress {
    /// Nothing (upstream `computeNonLocalStress(…) {}`).
    None,
    /// `σ ← σ·(1 − damage)` (`CustomNonLocalTestMaterial`).
    ScaleByDamage,
}

/// The test law.
#[derive(Clone, Debug)]
pub struct TestNonLocalMaterial<const DIM: usize> {
    pub elastic: MaterialElastic<DIM>,
    /// `MaterialDamage` parent internals (`damage`, …), if any.
    pub damage: Option<DamageFields>,
    /// Extra internals in registration order.
    pub extra: Vec<InternalField<Real>>,
    pub variables: Vec<NonLocalVariable>,
    pub neighborhood: Option<String>,
    pub stress: NonLocalStress,
}

impl<const DIM: usize> TestNonLocalMaterial<DIM> {
    fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>, law_type: &str) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = law_type.into();
        elastic.base_mut().is_non_local = true;
        Ok(Self {
            elastic,
            damage: None,
            extra: Vec::new(),
            variables: Vec::new(),
            neighborhood: None,
            stress: NonLocalStress::None,
        })
    }

    /// Upstream `TestMaterial<dim>`.
    pub fn test_material(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut m = Self::new(spec, mesh, "test_material")?;
        m.damage = Some(DamageFields::new()?);
        m.extra
            .push(InternalField::new("grad_u non local", DIM * DIM, 0.)?);
        m.variables.push(NonLocalVariable {
            local: "grad_u".into(),
            non_local: "grad_u non local".into(),
            nb_component: DIM * DIM,
        });
        m.neighborhood = Some("test_region".into());
        Ok(m)
    }

    /// Upstream `TestMaterialDamage<dim>`.
    pub fn test_material_damage(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut m = Self::test_material(spec, mesh)?;
        m.neighborhood = None;
        Ok(m)
    }

    /// Upstream `CustomNonLocalTestMaterial<dim>`.
    pub fn custom(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut m = Self::new(spec, mesh, "custom_non_local_test_material")?;
        m.extra.push(InternalField::new("local_damage", 1, 0.)?);
        m.extra.push(InternalField::new("damage", 1, 0.)?);
        m.variables.push(NonLocalVariable {
            local: "local_damage".into(),
            non_local: "damage".into(),
            nb_component: 1,
        });
        m.stress = NonLocalStress::ScaleByDamage;
        Ok(m)
    }

    /// An internal by name.
    pub fn internal(&self, name: &str) -> Option<&InternalField<Real>> {
        if let Some(d) = &self.damage
            && name == "damage"
        {
            return Some(&d.damage);
        }
        self.extra.iter().find(|f| f.name() == name)
    }

    /// A mutable internal by name.
    pub fn internal_mut(&mut self, name: &str) -> Option<&mut InternalField<Real>> {
        if let Some(d) = &mut self.damage
            && name == "damage"
        {
            return Some(&mut d.damage);
        }
        self.extra.iter_mut().find(|f| f.name() == name)
    }
}

impl<const DIM: usize> ParameterRegistry for TestNonLocalMaterial<DIM> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.elastic.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.elastic.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.elastic.param_names()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.elastic.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.elastic.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.elastic.set_param_from_input(name, value)
    }
}

impl<const DIM: usize> Parsable for TestNonLocalMaterial<DIM> {
    fn section_type(&self) -> ParserType {
        ParserType::Material
    }
    fn parsable_id(&self) -> &str {
        &self.elastic.base().id
    }
}

impl<const DIM: usize> ConstitutiveLaw for TestNonLocalMaterial<DIM> {
    fn name(&self) -> &str {
        self.elastic.name()
    }
    fn law_id(&self) -> &str {
        self.elastic.law_id()
    }
    fn law_type(&self) -> &str {
        self.elastic.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.elastic.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.elastic.element_filter_mut()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        if let Some(d) = &self.damage {
            d.for_each(f);
        }
        for e in &self.extra {
            f(e.name(), InternalFieldRef::Real(e));
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        if let Some(d) = &mut self.damage {
            d.for_each_mut(f)?;
        }
        for e in &mut self.extra {
            let name = e.name().to_owned();
            f(&name, InternalFieldMut::Real(e))?;
        }
        Ok(())
    }
}

impl<const DIM: usize> Material for TestNonLocalMaterial<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.init_material(ctx)?;
        let blocks = self.elastic.base().blocks(ctx);
        if let Some(d) = &mut self.damage {
            d.damage.resize_blocks(&blocks)?;
            d.dissipated_energy.resize_blocks(&blocks)?;
            d.int_sigma.resize_blocks(&blocks)?;
        }
        for e in &mut self.extra {
            e.resize_blocks(&blocks)?;
        }
        Ok(())
    }
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.elastic.compute_stress(ctx, ty, ghost)
    }
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
    }
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.elastic.compute_potential_energy(ctx, ty)
    }
    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }
    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }
    fn tangent_type(&self) -> MatrixType {
        self.elastic.tangent_type()
    }
    fn options(&self) -> Option<&LawOptions> {
        self.elastic.options()
    }
    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        self.elastic.options_mut()
    }
    fn as_non_local(&self) -> Option<&dyn MaterialNonLocal> {
        Some(self)
    }
    fn as_non_local_mut(&mut self) -> Option<&mut dyn MaterialNonLocal> {
        Some(self)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl<const DIM: usize> MaterialNonLocal for TestNonLocalMaterial<DIM> {
    fn neighborhood_name(&self) -> String {
        self.neighborhood
            .clone()
            .unwrap_or_else(|| self.elastic.name().to_owned())
    }
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        self.variables.clone()
    }
    fn compute_non_local_stresses(
        &mut self,
        _ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<()> {
        if self.stress == NonLocalStress::None {
            return Ok(());
        }
        for ty in self.elastic.base().filter_types(ghost) {
            let dam = self.internal("damage").unwrap().get(ty, ghost)?.clone();
            let mut sigma = self
                .elastic
                .base_mut()
                .sigma_field_mut()?
                .get_mut(ty, ghost)?;
            for (s, d) in sigma
                .as_mut_slice()
                .chunks_exact_mut(DIM * DIM)
                .zip(dam.as_slice())
            {
                let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
                *s = s.scale(1. - d);
            }
        }
        Ok(())
    }
}

/// Register `name` as the test law built by `ctor` (upstream
/// `instantiateMaterial<…>("test_material")` of each test).
pub fn register(
    factory: &mut MaterialFactory,
    name: &str,
    ctor: fn(&LawSpec, Option<&akantu_mesh::Mesh>) -> Result<TestNonLocalMaterial<2>>,
) {
    factory.replace(
        name,
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            Ok(Box::new(ctor(spec, Some(services.mesh()))?) as Box<dyn Material>)
        }),
    );
}

/// The test law at index `m` of a model's handler.
pub fn test_law(model: &akantu_solid::SolidMechanicsModel, m: usize) -> &TestNonLocalMaterial<2> {
    model
        .material(m)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<TestNonLocalMaterial<2>>()
        .unwrap()
}

/// Mutable [`test_law`].
pub fn test_law_mut(
    model: &mut akantu_solid::SolidMechanicsModel,
    m: usize,
) -> &mut TestNonLocalMaterial<2> {
    model
        .material_mut(m)
        .unwrap()
        .as_any_mut()
        .unwrap()
        .downcast_mut::<TestNonLocalMaterial<2>>()
        .unwrap()
}
