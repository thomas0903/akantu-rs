// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/material_igfem/
// material_igfem{.hh,.cc,_inline_impl.hh}, igfem_internal_field{.hh,_tmpl.hh},
// material_igfem_elastic{.hh,.cc,_inline_impl.hh}, material_igfem_includes.hh,
// and the IGFEM paths of src/model/solid_mechanics/material.cc
// (assembleInternalForces, assembleStiffnessMatrix for `_ek_igfem` elements).

//! IGFEM materials (IGFEM-005, IGFEM-006): upstream `MaterialIGFEM` and
//! `MaterialIGFEMElastic` (`igfem_elastic`).
//!
//! An IGFEM material handles the `_ek_igfem` elements of the model. It
//! combines two sub-materials (parameters `name_sub_mat_1`,
//! `name_sub_mat_2`, both `elastic` laws of the same model): every
//! integration point gets the index of the sub-material of its side of the
//! interface (internal `sub_material`, upstream `setSubMaterial`), and the
//! elastic law uses per-point Lamé fields (`lambda`, `mu`, `kappa`) copied
//! from that sub-material. The stress is
//! `σ = λ tr(∇u) I + μ (∇u + ∇uᵀ)` (dim ≥ 2; `σ = 2μ ∇u` in 1-D), the
//! tangent the isotropic Voigt matrix of `(λ, μ)`.
//!
//! The material works on the model's IGFEM FE engine (shared through
//! [`IgfemContext`]): gradients, `Bᵀσ` and `BᵀDB` are computed with the
//! IGFEM shapes; the model's generic small-strain pipeline is replaced
//! through the law hooks `compute_all_stresses`,
//! `assemble_internal_forces_custom` and `elemental_stiffness_custom`
//! (upstream `MaterialIGFEM` overrides of the `Material` virtuals with
//! `_ek_igfem` type loops).
//!
//! Sub-material assignment (upstream `setSubMaterial<type>`):
//! * `_igfem_triangle_4`: sub-element `s` gets `1 − isInside(barycenter_s,
//!   name_sub_mat_1)`;
//! * `_igfem_triangle_5`: from the orientation of the inside flags of the
//!   parent nodes (`0`: node 0 inside → sub-element 0 in sub-material 0,
//!   sub-element 1 in 1; `1`: the reverse).
//!
//! Upstream defects (D-003, `docs/UPSTREAM_DEFECTS.md`):
//! * UD-610 (B, no compat): Lamé fields are only set in `onElementsAdded`;
//!   IGFEM elements that exist when the material is initialised keep the
//!   internals' default `0` (zero stiffness, a singular system). The port
//!   assigns them in `initMaterial` (needs the geometry); a compat switch
//!   would only reproduce the singular system.
//! * UD-611 (B): `setSubMaterial<_igfem_triangle_5>` advances its
//!   connectivity iterator with the filter position instead of reading the
//!   connectivity of `filter(e)` (wrong nodes as soon as the filter is not
//!   the identity), and its unreachable `case 3` assigns
//!   `sub_material_index(0)` twice. The port reads the element's own
//!   connectivity (regression `review_v89_regressions::
//!   ud611_two_igfem_laws_non_identity_filter`); the unreachable cases are
//!   not ported.
//! * UD-612 (C): the 1-D law uses `2μ` (stress and tangent) where
//!   `elastic` uses `E` (equal only for `ν = 0`); kept.
//! * UD-613 (A): `computePotentialEnergy`, `computePotentialEnergyByElement`
//!   and the finite-deformation stress are `AKANTU_DEBUG_TO_IMPLEMENT`
//!   upstream: `Error::NotImplemented` here; the wave speeds fall back to
//!   `Material`'s `AKANTU_TO_IMPLEMENT` likewise.

use std::collections::HashSet;
use std::sync::Arc;

use akantu_core::array::ArrayViewMut;
use akantu_core::enums::MatrixType;
use akantu_core::tensor::MatrixX;
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result, TypeFilter, UInt,
};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_model::model::FeEngineRegistry;
use akantu_parser::ByteString;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu_solid::material::{Material, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solver::DofManager;

use crate::context::IgfemContext;
use crate::element::{self, is_igfem};

/// Registered name of the IGFEM elastic law.
pub const IGFEM_ELASTIC: &str = "igfem_elastic";

/// Elastic constants `(λ, μ, κ)` of a sub-material (upstream
/// `MaterialElastic::getLambda/getMu/getKappa`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SubMaterialConstants {
    /// First Lamé coefficient.
    pub lambda: Real,
    /// Shear modulus.
    pub mu: Real,
    /// Bulk modulus.
    pub kappa: Real,
}

/// Upstream `MaterialIGFEMElastic<dim>` (with the `MaterialIGFEM` base).
#[derive(Debug)]
pub struct MaterialIgfemElastic {
    base: MaterialBase,
    ctx: Arc<IgfemContext>,
    /// `name_sub_mat_1` (raw bytes).
    pub name_sub_mat_1: ByteString,
    /// `name_sub_mat_2` (raw bytes).
    pub name_sub_mat_2: ByteString,
    /// `Plane_Stress` (2-D; parsed for compatibility: the Lamé fields come
    /// from the sub-materials, which carry their own plane-stress flag, so
    /// the flag has no effect on the stress, as upstream's
    /// `PlaneStressToolbox<2, MaterialIGFEM>`).
    pub plane_stress: bool,
    /// Sub-material of each integration point (upstream internal
    /// `sub_material`).
    pub sub_material: InternalField<UInt>,
    /// First Lamé coefficient per point.
    pub lambda: InternalField<Real>,
    /// Shear modulus per point.
    pub mu: InternalField<Real>,
    /// Bulk modulus per point (upstream internal `kappa`).
    pub kpa: InternalField<Real>,
    /// Upstream's global `Math::getTolerance()` as seen by this law
    /// (`extrapolateInternal`, the damage-step comparisons of
    /// `igfem_iterative_stiffness_reduction`); default
    /// [`akantu_core::math::DEFAULT_TOLERANCE`] (upstream's default).
    pub tolerance: Real,
    sub_constants: Option<[SubMaterialConstants; 2]>,
}

fn err_poison() -> Error {
    Error::invalid("material base internals are incomplete")
}

impl MaterialIgfemElastic {
    /// New law (upstream constructor chain `Material → MaterialIGFEM →
    /// PlaneStressToolbox → MaterialIGFEMElastic`): element filter blocks
    /// for the IGFEM types of `mesh`.
    ///
    /// # Errors
    /// Bad dimension.
    pub fn new(spec: &LawSpec, mesh: Option<&Mesh>, ctx: Arc<IgfemContext>) -> Result<Self> {
        Self::with_law_type(spec, mesh, ctx, IGFEM_ELASTIC)
    }

    /// [`MaterialIgfemElastic::new`] as the base of a derived IGFEM law of
    /// type `law_type` (the damage laws).
    ///
    /// # Errors
    /// Bad dimension.
    pub(crate) fn with_law_type(
        spec: &LawSpec,
        mesh: Option<&Mesh>,
        ctx: Arc<IgfemContext>,
        law_type: &str,
    ) -> Result<Self> {
        let mut base = MaterialBase::new(spec, law_type)?;
        // Upstream `MaterialIGFEM::initialize`: `gradu`, `stress` and
        // `eigengradu` are `setElementKind(_ek_igfem)`, and the law's own
        // internals are `IGFEMInternalField`s (`element_kind = _ek_igfem`):
        // since the type restriction of the internals (CL-002) the whole
        // law handles the IGFEM types only; the law's own internals adopt
        // the kind at their first resize.
        base.set_element_kind(ElementKind::Igfem);
        if let Some(m) = mesh {
            base.initialize_element_filter(m, ElementKind::Igfem)?;
        }
        Ok(Self {
            base,
            ctx,
            name_sub_mat_1: ByteString::default(),
            name_sub_mat_2: ByteString::default(),
            plane_stress: false,
            sub_material: InternalField::new("sub_material", 1, 0)?,
            lambda: InternalField::new("lambda", 1, 0.0)?,
            mu: InternalField::new("mu", 1, 0.0)?,
            kpa: InternalField::new("kappa", 1, 0.0)?,
            tolerance: akantu_core::math::DEFAULT_TOLERANCE,
            sub_constants: None,
        })
    }

    /// The shared IGFEM model state (engine, geometry).
    #[must_use]
    pub fn context(&self) -> &Arc<IgfemContext> {
        &self.ctx
    }

    /// Names of the two sub-materials (upstream `sub_material_names`).
    #[must_use]
    pub fn sub_material_names(&self) -> [String; 2] {
        [
            self.name_sub_mat_1.to_string_lossy().into_owned(),
            self.name_sub_mat_2.to_string_lossy().into_owned(),
        ]
    }

    /// Set the elastic constants of the two sub-materials (the model reads
    /// them from the `elastic` laws named `name_sub_mat_1/2`; upstream
    /// `updateElasticInternals` reads them through the model).
    pub fn set_sub_material_constants(&mut self, c: [SubMaterialConstants; 2]) {
        self.sub_constants = Some(c);
    }

    /// The sub-material constants set by the model.
    #[must_use]
    pub fn sub_material_constants(&self) -> Option<[SubMaterialConstants; 2]> {
        self.sub_constants
    }

    fn dim(&self) -> usize {
        self.base.spatial_dimension
    }

    /// IGFEM types of the filter for `ghost` (law dimension).
    pub(crate) fn igfem_types(&self, ghost: GhostType) -> Vec<ElementType> {
        self.base
            .element_filter
            .element_types(TypeFilter {
                spatial_dimension: Some(self.dim()),
                ghost_type: ghost,
                kind: Some(ElementKind::Igfem),
            })
            .collect()
    }

    pub(crate) fn filter(&self, ty: ElementType, ghost: GhostType) -> Vec<Idx> {
        self.base
            .element_filter
            .get(ty, ghost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default()
    }

    /// Blocks with the IGFEM point counts (the model's services count the
    /// points of the regular FE engine, which has no IGFEM types).
    pub(crate) fn igfem_blocks(
        &self,
        blocks: &[(ElementType, GhostType, usize)],
    ) -> Result<Vec<(ElementType, GhostType, usize)>> {
        blocks
            .iter()
            .map(|&(ty, g, n)| {
                if is_igfem(ty) {
                    let nq = self.ctx.engine()?.nb_integration_points(ty)?;
                    Ok((ty, g, self.filter(ty, g).len() * nq))
                } else {
                    Ok((ty, g, n))
                }
            })
            .collect()
    }

    /// Upstream `setSubMaterial<type>(element_list, ghost)` for every IGFEM
    /// type, then the Lamé fields of every point from its sub-material
    /// (upstream `updateElasticInternals`).
    ///
    /// # Errors
    /// No geometry, missing sub-material constants, malformed mesh,
    /// undeterminable orientation.
    pub fn update_elastic_internals(
        &mut self,
        mesh: &Mesh,
        elements: &HashSet<Element>,
    ) -> Result<()> {
        let consts = self.sub_constants.ok_or_else(|| {
            Error::invalid(format!(
                "{}: the sub-material constants were not set (the IGFEM model reads them from \
                 the elastic laws {:?})",
                self.base.name,
                self.sub_material_names()
            ))
        })?;
        let domain = self.sub_material_names()[0].clone();
        for ghost in GhostType::ALL {
            for ty in self.igfem_types(ghost) {
                let filter = self.filter(ty, ghost);
                if filter.is_empty() {
                    continue;
                }
                self.set_sub_material(mesh, ty, ghost, &filter, elements, &domain)?;
                let sub = self.sub_material.get(ty, ghost)?.as_slice().to_vec();
                let mut l = self.lambda.get_mut(ty, ghost)?;
                if l.len() != sub.len() {
                    return Err(Error::DimensionMismatch {
                        context: "igfem_elastic updateElasticInternals: lambda",
                        expected: sub.len(),
                        got: l.len(),
                    });
                }
                for (v, &s) in l.as_mut_slice().iter_mut().zip(&sub) {
                    *v = consts[s as usize].lambda;
                }
                for (v, &s) in self
                    .mu
                    .get_mut(ty, ghost)?
                    .as_mut_slice()
                    .iter_mut()
                    .zip(&sub)
                {
                    *v = consts[s as usize].mu;
                }
                for (v, &s) in self
                    .kpa
                    .get_mut(ty, ghost)?
                    .as_mut_slice()
                    .iter_mut()
                    .zip(&sub)
                {
                    *v = consts[s as usize].kappa;
                }
            }
        }
        Ok(())
    }

    fn set_sub_material(
        &mut self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        elements: &HashSet<Element>,
        domain: &str,
    ) -> Result<()> {
        let dim = mesh.spatial_dimension();
        let engine = self.ctx.engine()?;
        let nq = engine.nb_integration_points(ty)?;
        drop(engine);
        let n1 = element::sub_element_type(ty, 0)
            .and_then(akantu_fe::quadrature::default_rule)?
            .nb_points();
        let mut sub = self.sub_material.get_mut(ty, ghost)?;
        if sub.len() != filter.len() * nq {
            return Err(Error::DimensionMismatch {
                context: "setSubMaterial: sub_material internal",
                expected: filter.len() * nq,
                got: sub.len(),
            });
        }
        let np = element::nb_parent_nodes(ty)?;
        for (pos, &el_id) in filter.iter().enumerate() {
            let el = Element::new(ty, el_id, ghost);
            if !elements.contains(&el) {
                continue;
            }
            let x = mesh.extract_nodal_values_from_element(&mesh.nodes(), el)?;
            let idx: [UInt; 2] = match ty {
                ElementType::IgfemTriangle4 => {
                    let mut idx = [0; 2];
                    let mut b = vec![0.0; dim];
                    for (s, slot) in idx.iter_mut().enumerate() {
                        element::sub_element_barycenter(ty, &x, dim, s, &mut b)?;
                        *slot = UInt::from(!self.ctx.is_inside(&b, domain)?);
                    }
                    idx
                }
                ElementType::IgfemTriangle5 => {
                    let mut inside = vec![false; np];
                    for (i, f) in inside.iter_mut().enumerate() {
                        *f = self.ctx.is_inside(&x[i * dim..(i + 1) * dim], domain)?;
                    }
                    match element::orientation(ty, &inside)? {
                        0 => [0, 1],
                        _ => [1, 0],
                    }
                }
                _ => {
                    return Err(Error::NotImplemented(
                        "MaterialIGFEM::setSubMaterial for this IGFEM type \
                         (AKANTU_DEBUG_TO_IMPLEMENT upstream)",
                    ));
                }
            };
            let row = &mut sub.as_mut_slice()[pos * nq..(pos + 1) * nq];
            for (q, v) in row.iter_mut().enumerate() {
                *v = if q < n1 { idx[0] } else { idx[1] };
            }
        }
        Ok(())
    }

    /// Upstream `MaterialIGFEM::applyEigenGradU(prescribed, sub_mat_name)`:
    /// the eigen gradient of every point (of `ghost`) whose sub-material is
    /// the one named `sub_mat_name`.
    ///
    /// # Errors
    /// Wrong matrix size.
    pub fn apply_eigen_grad_u_to_sub_material(
        &mut self,
        prescribed: &MatrixX,
        sub_mat_name: &str,
        ghost: GhostType,
    ) -> Result<()> {
        let d = self.dim();
        if prescribed.rows() != d || prescribed.cols() != d {
            return Err(Error::DimensionMismatch {
                context: "MaterialIGFEM::applyEigenGradU",
                expected: d * d,
                got: prescribed.rows() * prescribed.cols(),
            });
        }
        let names = self.sub_material_names();
        for (index, name) in names.iter().enumerate() {
            if name != sub_mat_name {
                continue;
            }
            let types: Vec<ElementType> = self
                .base
                .element_filter
                .as_map()
                .iter(ghost)
                .map(|(t, _)| t)
                .collect();
            for ty in types {
                if self.filter(ty, ghost).is_empty() {
                    continue;
                }
                let sub = self.sub_material.get(ty, ghost)?.as_slice().to_vec();
                let mut eig = self.base.eigen_grad_u.get_mut(ty, ghost)?;
                for (t, &s) in eig.as_mut_slice().chunks_exact_mut(d * d).zip(&sub) {
                    if s as usize == index {
                        t.copy_from_slice(prescribed.as_slice());
                    }
                }
            }
        }
        Ok(())
    }

    /// `σ` of one point (upstream `computeStressOnQuad`).
    pub(crate) fn stress_on_quad(dim: usize, g: &[Real], s: &mut [Real], lambda: Real, mu: Real) {
        if dim == 1 {
            s[0] = 2. * mu * g[0];
            return;
        }
        // Matrix<Real>::trace on the dynamic grad_u: sequential sum
        let mut trace = 0.0;
        for i in 0..dim {
            trace += g[i + i * dim];
        }
        for j in 0..dim {
            for i in 0..dim {
                let delta = if i == j { 1.0 } else { 0.0 };
                s[i + j * dim] = delta * lambda * trace + mu * (g[i + j * dim] + g[j + i * dim]);
            }
        }
    }

    /// Tangent of one point (upstream `computeTangentModuliOnQuad`).
    pub(crate) fn tangent_on_quad(dim: usize, out: &mut [Real], lambda: Real, mu: Real) {
        out.fill(0.0);
        let n = akantu_solid::material_tensor::tangent_stiffness_voigt_size(dim);
        let at = |i: usize, j: usize| i + j * n;
        if dim == 1 {
            out[0] = 2. * mu;
            return;
        }
        let miiii = lambda + 2. * mu;
        let miijj = lambda;
        let mijij = mu;
        out[at(0, 0)] = miiii;
        out[at(1, 1)] = miiii;
        out[at(0, 1)] = miijj;
        out[at(1, 0)] = miijj;
        out[at(n - 1, n - 1)] = mijij;
        if dim == 3 {
            out[at(2, 2)] = miiii;
            out[at(0, 2)] = miijj;
            out[at(1, 2)] = miijj;
            out[at(2, 0)] = miijj;
            out[at(2, 1)] = miijj;
            out[at(3, 3)] = mijij;
            out[at(4, 4)] = mijij;
        }
    }

    /// `grad_u ← ∇u − eigen_grad_u` of the IGFEM blocks of `ghost` with the
    /// IGFEM engine (upstream `MaterialIGFEM::computeAllStresses` without
    /// the stress).
    pub(crate) fn update_grad_u(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let dim = self.dim();
        let mesh = ctx.services.mesh();
        let u = ctx.services.nodal_field("displacement")?;
        let mut gu = Array::new(dim * dim);
        self.ctx.engine()?.gradient_on_integration_points(
            mesh,
            u.view(),
            &mut gu,
            dim,
            ty,
            ghost,
            Some(filter),
        )?;
        let eig = self.base.eigen_grad_u.get(ty, ghost)?;
        if eig.len() != gu.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialIGFEM::computeAllStresses: grad_u -= eigen_grad_u",
                expected: eig.len(),
                got: gu.len(),
            });
        }
        for (g, e) in gu.as_mut_slice().iter_mut().zip(eig.as_slice()) {
            *g -= *e;
        }
        self.base.grad_u.get_mut(ty, ghost)?.copy_from(&gu)
    }
}

impl MaterialParams for MaterialIgfemElastic {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if self.base.spatial_dimension == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.base.update_internal_parameters()
    }
}

macro_rules! igfem_params {
    ($($extra:expr),*) => {
        &[
            $($extra,)*
            ParamDescriptor::new(
                "name_sub_mat_1",
                ParamKind::ByteString,
                ParameterAccess::PARSMOD,
                "name of the first sub-material",
                |s| ParamValue::ByteString(s.name_sub_mat_1.clone()),
                Some(|s, v| {
                    s.name_sub_mat_1 = v.into_byte_string()?;
                    Ok(())
                }),
            ),
            ParamDescriptor::new(
                "name_sub_mat_2",
                ParamKind::ByteString,
                ParameterAccess::PARSMOD,
                "name of the second sub-material",
                |s| ParamValue::ByteString(s.name_sub_mat_2.clone()),
                Some(|s, v| {
                    s.name_sub_mat_2 = v.into_byte_string()?;
                    Ok(())
                }),
            ),
        ]
    };
}

impl MaterialIgfemElastic {
    /// Parameters (1-D and 3-D).
    const PARAMS: &'static [ParamDescriptor<Self>] = igfem_params!();
    /// Parameters of the 2-D law (`Plane_Stress` first, the toolbox being a
    /// base class).
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = igfem_params!(ParamDescriptor::new(
        "Plane_Stress",
        ParamKind::Bool,
        ParameterAccess::PARSMOD,
        "Is plane stress",
        |s| ParamValue::Bool(s.plane_stress),
        Some(|s, v| {
            s.plane_stress = v.as_bool()?;
            Ok(())
        }),
    ));
}

akantu_solid::impl_material_registry!(MaterialIgfemElastic);

impl ConstitutiveLaw for MaterialIgfemElastic {
    fn name(&self) -> &str {
        &self.base.name
    }
    fn law_id(&self) -> &str {
        &self.base.id
    }
    fn law_type(&self) -> &str {
        &self.base.law_type
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.base.element_filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.base.element_filter
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.base.internals() {
            f(n, InternalFieldRef::Real(field));
        }
        f("sub_material", InternalFieldRef::UInt(&self.sub_material));
        f("lambda", InternalFieldRef::Real(&self.lambda));
        f("mu", InternalFieldRef::Real(&self.mu));
        f("kappa", InternalFieldRef::Real(&self.kpa));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        let mut fields = self.base.internals_mut().into_iter();
        for name in ["stress", "eigen_grad_u", "grad_u", "potential_energy"] {
            f(
                name,
                InternalFieldMut::Real(fields.next().ok_or_else(err_poison)?),
            )?;
        }
        for field in fields {
            let n = if field.name() == "piola_kirchhoff_2" {
                "piola_kirchhoff_2"
            } else {
                "green_strain"
            };
            f(n, InternalFieldMut::Real(field))?;
        }
        f(
            "sub_material",
            InternalFieldMut::UInt(&mut self.sub_material),
        )?;
        f("lambda", InternalFieldMut::Real(&mut self.lambda))?;
        f("mu", InternalFieldMut::Real(&mut self.mu))?;
        f("kappa", InternalFieldMut::Real(&mut self.kpa))?;
        Ok(())
    }

    /// Resize with the IGFEM point counts (see [`MaterialIgfemElastic`]).
    fn resize_internals(&mut self, blocks: &[(ElementType, GhostType, usize)]) -> Result<()> {
        let blocks = self.igfem_blocks(blocks)?;
        resize_law_internals(self, &blocks)
    }

    /// Compaction with the IGFEM point counts.
    fn remove_integration_points(
        &mut self,
        new_numbering: &ElementTypeMapArray<Idx>,
        nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
    ) -> Result<()> {
        let ctx = self.ctx.clone();
        remove_law_integration_points(self, &ctx, new_numbering, nb_quad)
    }
}

/// Resize every internal of an IGFEM law to `blocks` (point counts of the
/// IGFEM engine), in upstream's internal order (byte-wise sorted names, the
/// order of the random fields' draws).
///
/// # Errors
/// Allocation failures.
pub(crate) fn resize_law_internals<L: ConstitutiveLaw + ?Sized>(
    law: &mut L,
    blocks: &[(ElementType, GhostType, usize)],
) -> Result<()> {
    for name in law.internal_names_sorted() {
        law.for_each_internal_mut(&mut |n, f| {
            if n != name {
                return Ok(());
            }
            match f {
                InternalFieldMut::Real(f) => f.resize_blocks(blocks),
                InternalFieldMut::Int(f) => f.resize_blocks(blocks),
                InternalFieldMut::UInt(f) => f.resize_blocks(blocks),
                InternalFieldMut::Bool(f) => f.resize_blocks(blocks),
            }
        })?;
    }
    Ok(())
}

/// Compact every internal of an IGFEM law after an element removal, with
/// the IGFEM engine's point counts for the IGFEM types.
///
/// # Errors
/// Engine errors, size mismatches.
pub(crate) fn remove_law_integration_points<L: ConstitutiveLaw + ?Sized>(
    law: &mut L,
    ctx: &IgfemContext,
    new_numbering: &ElementTypeMapArray<Idx>,
    nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
) -> Result<()> {
    let nq_igfem: Vec<(ElementType, usize)> = [
        ElementType::IgfemSegment3,
        ElementType::IgfemTriangle4,
        ElementType::IgfemTriangle5,
    ]
    .into_iter()
    .map(|t| Ok((t, ctx.engine()?.nb_integration_points(t)?)))
    .collect::<Result<_>>()?;
    let nq = |ty: ElementType, g: GhostType| {
        nq_igfem
            .iter()
            .find(|(t, _)| *t == ty)
            .map_or_else(|| nb_quad(ty, g), |(_, n)| *n)
    };
    law.for_each_internal_mut(&mut |_, f| match f {
        InternalFieldMut::Real(f) => f.remove_integration_points(new_numbering, &nq),
        InternalFieldMut::Int(f) => f.remove_integration_points(new_numbering, &nq),
        InternalFieldMut::UInt(f) => f.remove_integration_points(new_numbering, &nq),
        InternalFieldMut::Bool(f) => f.remove_integration_points(new_numbering, &nq),
    })
}

impl Material for MaterialIgfemElastic {
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream `MaterialIGFEMElastic::initMaterial` (`Material::initMaterial`:
    /// history, internals, parameters; `sub_material_names`) plus UD-610:
    /// the Lamé fields of the IGFEM elements already in the filter.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if self.base.finite_deformation {
            return Err(Error::NotImplemented(
                "igfem_elastic with finite_deformation (AKANTU_DEBUG_TO_IMPLEMENT upstream)",
            ));
        }
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.base.update_internal_parameters()?;
        self.base.set_init();
        let mut existing = HashSet::new();
        for ghost in GhostType::ALL {
            for ty in self.igfem_types(ghost) {
                for e in self.filter(ty, ghost) {
                    existing.insert(Element::new(ty, e, ghost));
                }
            }
        }
        if !existing.is_empty() {
            self.update_elastic_internals(ctx.services.mesh(), &existing)?;
        }
        Ok(())
    }

    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dim = self.dim();
        let n = dim * dim;
        let g = self.base.grad_u.get(ty, ghost)?.clone();
        let l = self.lambda.get(ty, ghost)?.as_slice().to_vec();
        let m = self.mu.get(ty, ghost)?.as_slice().to_vec();
        let mut s = self.base.stress.get_mut(ty, ghost)?;
        if g.len() != s.len() || l.len() != s.len() || m.len() != s.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialIGFEMElastic::computeStress",
                expected: s.len(),
                got: g.len().min(l.len()).min(m.len()),
            });
        }
        for (q, (gq, sq)) in g
            .as_slice()
            .chunks_exact(n)
            .zip(s.as_mut_slice().chunks_exact_mut(n))
            .enumerate()
        {
            Self::stress_on_quad(dim, gq, sq, l[q], m[q]);
        }
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let dim = self.dim();
        let v = akantu_solid::material_tensor::tangent_stiffness_voigt_size(dim);
        let l_g = self.lambda.get(ty, ghost)?;
        let l = l_g.as_slice();
        let m_g = self.mu.get(ty, ghost)?;
        let m = m_g.as_slice();
        if tangent.len() != l.len() || tangent.nb_component() != v * v {
            return Err(Error::DimensionMismatch {
                context: "MaterialIGFEMElastic::computeTangentModuli",
                expected: l.len() * v * v,
                got: tangent.len() * tangent.nb_component(),
            });
        }
        for (q, t) in tangent.as_mut_slice().chunks_exact_mut(v * v).enumerate() {
            Self::tangent_on_quad(dim, t, l[q], m[q]);
        }
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, _ty: ElementType) -> Result<()> {
        Err(Error::NotImplemented(
            "MaterialIGFEMElastic::computePotentialEnergy (AKANTU_DEBUG_TO_IMPLEMENT upstream)",
        ))
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "igfem_elastic: getPushWaveSpeed (Material::AKANTU_TO_IMPLEMENT upstream)",
        ))
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "igfem_elastic: getShearWaveSpeed (Material::AKANTU_TO_IMPLEMENT upstream)",
        ))
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    /// Upstream `MaterialIGFEM::computeAllStresses(ghost)`.
    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in self.igfem_types(ghost) {
            let filter = self.filter(ty, ghost);
            if filter.is_empty() {
                continue;
            }
            self.update_grad_u(ctx, ty, ghost, &filter)?;
            self.compute_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// Upstream `onElementsAdded` → `updateElasticInternals(element_list)`.
    fn on_elements_added(&mut self, ctx: &LawContext<'_>, elements: &[Element]) -> Result<()> {
        let set: HashSet<Element> = elements
            .iter()
            .copied()
            .filter(|e| is_igfem(e.ty()))
            .collect();
        if set.is_empty() {
            return Ok(());
        }
        self.update_elastic_internals(ctx.services.mesh(), &set)
    }

    /// Upstream `Material::assembleInternalForces` for the IGFEM elements:
    /// `−∫ Bᵀσ` per type of the filter (IGFEM engine).
    fn assemble_internal_forces_custom(
        &self,
        _fems: &FeEngineRegistry,
        mesh: &Mesh,
        ghost: GhostType,
        f_int: ArrayViewMut<'_, Real>,
    ) -> Option<Result<()>> {
        Some(self.assemble_internal_forces_igfem(mesh, ghost, f_int))
    }

    /// Upstream `Material::assembleStiffnessMatrix` for the IGFEM elements:
    /// `grad_u ← ∇u − ε⁰`, tangent, `∫ BᵀDB` per type (symmetric blocks).
    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        Some(self.elemental_stiffness_igfem(ctx, ghost))
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl MaterialIgfemElastic {
    pub(crate) fn assemble_internal_forces_igfem(
        &self,
        mesh: &Mesh,
        ghost: GhostType,
        mut f_int: ArrayViewMut<'_, Real>,
    ) -> Result<()> {
        let dim = self.dim();
        let engine = self.ctx.engine()?;
        for ty in self.igfem_types(ghost) {
            let filter = self.filter(ty, ghost);
            if filter.is_empty() {
                continue;
            }
            let nn = ty.nb_nodes_per_element();
            let stress = self.base.stress.get(ty, ghost)?;
            let mut sigma_dphi = Array::new(dim * nn);
            engine.compute_btd(
                mesh,
                stress.view(),
                &mut sigma_dphi,
                ty,
                ghost,
                Some(&filter),
            )?;
            let mut r_e = Array::new(dim * nn);
            engine.integrate(
                mesh,
                sigma_dphi.view(),
                &mut r_e,
                dim * nn,
                ty,
                ghost,
                Some(&filter),
            )?;
            DofManager::assemble_elemental_array_local_view(
                mesh,
                r_e.view(),
                f_int.reborrow(),
                ty,
                ghost,
                -1.0,
                Some(&filter),
            )?;
        }
        Ok(())
    }

    #[allow(clippy::type_complexity)] // the hook's block list
    fn elemental_stiffness_igfem(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>> {
        self.elemental_stiffness_igfem_with(ctx, ghost, &mut |law, ctx, ty, ghost, tangent| {
            law.compute_tangent_moduli(ctx, ty, ghost, tangent)
        })
    }

    /// Upstream `Material::assembleStiffnessMatrix` for the IGFEM elements
    /// with the tangent of `tangent_fn` (the elastic one, or a damage law's
    /// `(1 − d)` scaled one): `grad_u ← ∇u − ε⁰`, tangent, `∫ BᵀDB` per
    /// type (symmetric blocks).
    ///
    /// # Errors
    /// Engine errors, size mismatches, `tangent_fn`'s errors.
    #[allow(clippy::type_complexity)] // the hook's block list and closure
    pub(crate) fn elemental_stiffness_igfem_with(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
        tangent_fn: &mut dyn FnMut(
            &mut Self,
            &LawContext<'_>,
            ElementType,
            GhostType,
            &mut Array<Real>,
        ) -> Result<()>,
    ) -> Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>> {
        let dim = self.dim();
        let mut out = Vec::new();
        for ty in self.igfem_types(ghost) {
            let filter = self.filter(ty, ghost);
            if filter.is_empty() {
                continue;
            }
            self.update_grad_u(ctx, ty, ghost, &filter)?;
            let nq = self.ctx.engine()?.nb_integration_points(ty)?;
            let voigt = akantu_solid::material_tensor::tangent_stiffness_voigt_size(dim);
            let mut tangent = Array::try_zeros(filter.len() * nq, voigt * voigt)?;
            tangent_fn(self, ctx, ty, ghost, &mut tangent)?;
            let size = dim * ty.nb_nodes_per_element();
            let engine = self.ctx.engine()?;
            let mesh = ctx.services.mesh();
            let mut bt_d_b = Array::new(size * size);
            engine.compute_btdb(
                mesh,
                tangent.view(),
                &mut bt_d_b,
                4,
                ty,
                ghost,
                Some(&filter),
            )?;
            let mut k_e = Array::new(size * size);
            engine.integrate(
                mesh,
                bt_d_b.view(),
                &mut k_e,
                size * size,
                ty,
                ghost,
                Some(&filter),
            )?;
            out.push((ty, filter, k_e, MatrixType::Symmetric));
        }
        Ok(out)
    }
}

/// What the IGFEM model needs from every IGFEM law (upstream
/// `MaterialIGFEM`, the base of `igfem_elastic`, `igfem_saw_tooth_damage`
/// and `igfem_iterative_stiffness_reduction`; IGFEM-005): the shared base
/// (sub-materials, Lamé fields, IGFEM engine pipelines), the internals that
/// an interface move transfers from the replaced elements
/// (`internals_to_transfer`, upstream member of the damage laws), the
/// parameters the laws read from their second sub-material, and the hook
/// after a transfer.
pub trait IgfemMaterial {
    /// The shared IGFEM base.
    fn igfem_base(&self) -> &MaterialIgfemElastic;

    /// The shared IGFEM base, mutable.
    fn igfem_base_mut(&mut self) -> &mut MaterialIgfemElastic;

    /// Upstream `internals_to_transfer` (empty for `igfem_elastic`).
    fn internals_to_transfer(&self) -> &'static [&'static str] {
        &[]
    }

    /// Read the parameters this law takes from its second sub-material
    /// (upstream `initMaterial`: `model.getMaterial(sub_material_names[1])
    /// .getParam(…)`); `param` returns a parameter of that law.
    ///
    /// # Errors
    /// A missing or mistyped parameter (upstream `getParam` throws).
    fn read_sub_material_parameters(
        &mut self,
        param: &dyn Fn(&str) -> Result<ParamValue>,
    ) -> Result<()> {
        let _ = param;
        Ok(())
    }

    /// After the transfer of internals into new elements (upstream: the
    /// end of the law's `onElementsAdded`).
    ///
    /// # Errors
    /// Size mismatches.
    fn after_transfer(&mut self) -> Result<()> {
        Ok(())
    }

    /// Upstream `MaterialIGFEM::extrapolateInternal(id, element, point,
    /// extrapolated)`: not a real extrapolation — every column of `out`
    /// (`nb_component` values each) receives the value of the element's
    /// first integration point whose norm exceeds the law's tolerance
    /// (`Math::getTolerance()`; point 0 when none does); zeros when `id` is
    /// not a `Real` internal of the law.
    ///
    /// # Errors
    /// `element` not in the law, an `out` that is not a multiple of the
    /// internal's components, engine errors.
    fn extrapolate_internal(&self, id: &str, element: Element, out: &mut [Real]) -> Result<()>
    where
        Self: ConstitutiveLaw + Sized,
    {
        let tol = self.igfem_base().tolerance;
        match law_element_values(self, id, element)? {
            None => {
                out.fill(0.0);
                Ok(())
            }
            Some((nc, vals)) => extrapolate_first_point(&vals, nc, tol, out),
        }
    }
}

/// The values of the `Real` internal `id` of `law` at the integration
/// points of `element` (`(nb_component, nq · nb_component values)`), or
/// `None` when the law has no such internal or no block for the element's
/// type (upstream `isInternal<Real>(id, element.kind)` false). The number
/// of points per element is the block's rows over the filter length.
///
/// # Errors
/// `element` not in the law's filter.
pub fn law_element_values<L: ConstitutiveLaw + ?Sized>(
    law: &L,
    id: &str,
    element: Element,
) -> Result<Option<(usize, Vec<Real>)>> {
    let (ty, ghost) = (element.ty(), element.ghost_type());
    let Ok(filter) = law.element_filter().get(ty, ghost) else {
        return Ok(None);
    };
    let local = filter
        .as_slice()
        .iter()
        .position(|&e| e == element.index())
        .ok_or_else(|| {
            Error::not_found(
                "element of the law",
                format!("{element} in {}", law.law_id()),
            )
        })?;
    let n_el = filter.len();
    let mut out: Option<Result<(usize, Vec<Real>)>> = None;
    law.for_each_internal(&mut |n, f| {
        if n != id || out.is_some() {
            return;
        }
        let InternalFieldRef::Real(f) = f else {
            return;
        };
        let Ok(block) = f.get(ty, ghost) else {
            return;
        };
        let nc = f.nb_component();
        if block.len() % n_el != 0 {
            out = Some(Err(Error::DimensionMismatch {
                context: "IGFEM transfer: internal rows vs filter length",
                expected: n_el,
                got: block.len(),
            }));
            return;
        }
        let nq = block.len() / n_el;
        let start = local * nq * nc;
        out = Some(Ok((nc, block.as_slice()[start..start + nq * nc].to_vec())));
    });
    out.transpose()
}

/// The copy rule of upstream's `extrapolateInternal` on the values `vals`
/// of one element (point-major, `nc` components per point): the first
/// point whose Euclidean norm (sequential sum, `Vector::norm`) is greater
/// than `threshold` (`0` for `Material::extrapolateInternal`,
/// `Math::getTolerance()` for `MaterialIGFEM`'s), point 0 when none is,
/// copied into every `nc`-chunk of `out`. Upstream's copy loop runs over
/// `extrapolated.size()` columns (out of bounds for `nc > 1`, UD-213); each
/// column is written once here.
///
/// # Errors
/// `out` not a multiple of `nc`, empty `vals`.
pub fn extrapolate_first_point(
    vals: &[Real],
    nc: usize,
    threshold: Real,
    out: &mut [Real],
) -> Result<()> {
    if nc == 0 || vals.len() < nc || !out.len().is_multiple_of(nc) {
        return Err(Error::DimensionMismatch {
            context: "extrapolateInternal: components of the output",
            expected: nc,
            got: out.len(),
        });
    }
    let nq = vals.len() / nc;
    let mut index = 0;
    for j in 0..nq {
        let mut s = 0.0;
        for x in &vals[j * nc..(j + 1) * nc] {
            s += x * x;
        }
        if s.sqrt() > threshold {
            index = j;
            break;
        }
    }
    for col in out.chunks_exact_mut(nc) {
        col.copy_from_slice(&vals[index * nc..(index + 1) * nc]);
    }
    Ok(())
}

impl MaterialIgfemElastic {
    /// Upstream `MaterialIGFEM::getNbDataForElements(elements, tag)`: for
    /// `_smm_stress`, the bytes of the stress of `elements` at the IGFEM
    /// engine's integration points (`dim² · sizeof(Real)` per point; the
    /// finite-deformation variant is refused at `initMaterial`, UD-613);
    /// `0` for any other tag. (Upstream's methods carry pre-v5 names and
    /// override nothing, UD-1206; parallel IGFEM is not ported, D-085.)
    ///
    /// # Errors
    /// Engine errors.
    pub fn nb_data_for_elements(
        &self,
        elements: &[Element],
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<usize> {
        if tag != akantu_core::enums::SynchronizationTag::SmmStress {
            return Ok(0);
        }
        let d = self.dim();
        let engine = self.ctx.engine()?;
        let mut nq = 0usize;
        for e in elements {
            nq += engine.nb_integration_points(e.ty())?;
        }
        Ok(nq * d * d * std::mem::size_of::<Real>())
    }

    /// Upstream `MaterialIGFEM::packElementData(buffer, elements, tag)`:
    /// for `_smm_stress`, the stress of every element (all its IGFEM
    /// integration points, upstream `packInternalFieldHelper`).
    ///
    /// # Errors
    /// An element not in the law, engine errors.
    pub fn pack_element_data(
        &self,
        buffer: &mut akantu_comm::CommunicationBuffer,
        elements: &[Element],
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        if tag != akantu_core::enums::SynchronizationTag::SmmStress {
            return Ok(());
        }
        for &e in elements {
            let (_, vals) = law_element_values(self, "stress", e)?
                .ok_or_else(|| Error::not_found("stress block of", format!("{e}")))?;
            buffer.pack_slice(&vals);
        }
        Ok(())
    }

    /// Upstream `MaterialIGFEM::unpackElementData(buffer, elements, tag)`:
    /// the inverse of [`MaterialIgfemElastic::pack_element_data`].
    ///
    /// # Errors
    /// An element not in the law, a short buffer, engine errors.
    pub fn unpack_element_data(
        &mut self,
        buffer: &mut akantu_comm::CommunicationBuffer,
        elements: &[Element],
        tag: akantu_core::enums::SynchronizationTag,
    ) -> Result<()> {
        if tag != akantu_core::enums::SynchronizationTag::SmmStress {
            return Ok(());
        }
        let d = self.dim();
        for &e in elements {
            let (ty, ghost) = (e.ty(), e.ghost_type());
            let local = self
                .filter(ty, ghost)
                .iter()
                .position(|&x| x == e.index())
                .ok_or_else(|| Error::not_found("element of the law", format!("{e}")))?;
            let nq = self.ctx.engine()?.nb_integration_points(ty)?;
            let n = nq * d * d;
            let mut vals = vec![0.0; n];
            buffer.unpack_slice(&mut vals)?;
            let mut s = self.base.stress.get_mut(ty, ghost)?;
            let dst = s
                .as_mut_slice()
                .get_mut(local * n..(local + 1) * n)
                .ok_or_else(|| Error::invalid(format!("{e}: stress block too short")))?;
            dst.copy_from_slice(&vals);
        }
        Ok(())
    }
}

impl IgfemMaterial for MaterialIgfemElastic {
    fn igfem_base(&self) -> &MaterialIgfemElastic {
        self
    }
    fn igfem_base_mut(&mut self) -> &mut MaterialIgfemElastic {
        self
    }
}

/// Register the IGFEM laws in a material factory, bound to the IGFEM
/// context `ctx` (upstream `INSTANTIATE_MATERIAL` of
/// `MaterialIGFEMElastic`, `MaterialIGFEMSawToothDamage`,
/// `MaterialIGFEMIterativeStiffnessReduction`; the damage laws take their
/// damage parameters from a damage law of the extra materials as second
/// sub-material, IGFEM-007 / 008).
///
/// # Errors
/// Name already registered.
pub fn register_igfem_materials(
    factory: &mut MaterialFactory,
    ctx: &Arc<IgfemContext>,
) -> Result<()> {
    let c = ctx.clone();
    factory.register(
        IGFEM_ELASTIC,
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            Ok(Box::new(MaterialIgfemElastic::new(
                spec,
                Some(services.mesh()),
                c.clone(),
            )?) as Box<dyn Material>)
        }),
    )?;
    crate::material_damage::register(factory, ctx)
}

/// `true` if `law` is an IGFEM material.
#[must_use]
pub fn is_igfem_material(law: &dyn Material) -> bool {
    as_igfem(law).is_some()
}

/// The IGFEM interface of `law`, if it is an IGFEM material.
#[must_use]
pub fn as_igfem(law: &dyn Material) -> Option<&dyn IgfemMaterial> {
    use crate::material_damage::{
        MaterialIgfemIterativeStiffnessReduction, MaterialIgfemSawToothDamage,
    };
    let a = law.as_any()?;
    if let Some(m) = a.downcast_ref::<MaterialIgfemElastic>() {
        return Some(m);
    }
    if let Some(m) = a.downcast_ref::<MaterialIgfemSawToothDamage>() {
        return Some(m);
    }
    a.downcast_ref::<MaterialIgfemIterativeStiffnessReduction>()
        .map(|m| m as &dyn IgfemMaterial)
}

/// The IGFEM interface of `law`, mutable, if it is an IGFEM material.
pub fn as_igfem_mut(law: &mut dyn Material) -> Option<&mut dyn IgfemMaterial> {
    use crate::material_damage::{
        MaterialIgfemIterativeStiffnessReduction, MaterialIgfemSawToothDamage,
    };
    let a = law.as_any_mut()?;
    if a.is::<MaterialIgfemElastic>() {
        return a
            .downcast_mut::<MaterialIgfemElastic>()
            .map(|m| m as &mut dyn IgfemMaterial);
    }
    if a.is::<MaterialIgfemSawToothDamage>() {
        return a
            .downcast_mut::<MaterialIgfemSawToothDamage>()
            .map(|m| m as &mut dyn IgfemMaterial);
    }
    a.downcast_mut::<MaterialIgfemIterativeStiffnessReduction>()
        .map(|m| m as &mut dyn IgfemMaterial)
}

/// The IGFEM base behind `law`, if it is an IGFEM material.
pub fn as_igfem_material(law: &mut dyn Material) -> Option<&mut MaterialIgfemElastic> {
    as_igfem_mut(law).map(IgfemMaterial::igfem_base_mut)
}
