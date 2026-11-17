// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_elastic_orthotropic.hh, material_elastic_orthotropic.cc.

//! `elastic_orthotropic`: orthotropic linear elasticity from engineering
//! constants (upstream `MaterialElasticOrthotropic<dim>`, derived from the
//! anisotropic law: axes `n1..`, rotation, Voigt stress, celerity —
//! [`super::anisotropic`]).
//!
//! Parameters `E1, E2, nu12, G12` (+ `E3, nu13, nu23, G13, G23` in 3-D), all
//! default 0, `_pat_parsmod`; the inherited `Cij` are registered but
//! **overwritten** (`C'` is zeroed and rebuilt on every update). With
//! `ν21 = ν12E2/E1`, `ν31 = ν13E3/E1`, `ν32 = ν23E3/E2`:
//!
//! * 3-D: `Γ = 1/(1 − ν12ν21 − ν23ν32 − ν31ν13 − 2ν21ν32ν13)`,
//!   `C'11 = E1(1 − ν23ν32)Γ`, `C'22 = E2(1 − ν13ν31)Γ`,
//!   `C'33 = E3(1 − ν12ν21)Γ`, `C'12 = E1(ν21 + ν31ν23)Γ`,
//!   `C'13 = E1(ν31 + ν21ν32)Γ`, `C'23 = E2(ν32 + ν12ν31)Γ`,
//!   `C'44 = G23`, `C'55 = G13`, `C'66 = G12`;
//! * 2-D (plane stress constants, the 3-D ones are 0): `Γ = 1/(1 − ν12ν21)`,
//!   `C'11 = E1(1 − 0)Γ`, `C'22 = E2(1 − 0)Γ`, `C'12 = E1(ν21 + 0)Γ`,
//!   `C'33 = G12`;
//! * 1-D: an error at `updateInternalParameters` ("makes no sense"), i.e.
//!   at the end of parsing or at `initMaterial`.
//!
//! **UD-105** (category B): upstream's orthotropic
//! `updateInternalParameters` does not call the parent's, so the
//! `eigen_grad_u` parameter is never copied into the internal and the
//! stiffness is not flagged as changed after a parameter update. By default
//! ([`LawOptions::orthotropic_parent_update`]) the port runs the parent
//! updates (`Material::updateInternalParameters` and the stiffness flag of
//! `MaterialElasticLinearAnisotropic`); the switch off reproduces upstream
//! (set it before `initMaterial`). The material axes follow
//! [`LawOptions::material_axes_as_directions`] (UD-104), as for
//! `elastic_anisotropic`.

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Real, Result};
use akantu_model::constitutive_law::{ConstitutiveLaw, LawContext, LawSpec, ModelServices};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::anisotropic::{
    AnisotropicCore, anisotropic_compute_stress, anisotropic_energy, anisotropic_energy_by_element,
    anisotropic_tangent,
};
use crate::material::{LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams};

/// Orthotropic linear elastic law (upstream `MaterialElasticOrthotropic<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialElasticOrthotropic<const DIM: usize> {
    base: MaterialBase,
    /// Anisotropic state (axes, `C'`, `C`, eigenvalues, celerity mode).
    pub core: AnisotropicCore<DIM>,
    /// Young's modulus along `n1`.
    pub e1: Real,
    /// Young's modulus along `n2`.
    pub e2: Real,
    /// Young's modulus along `n3` (3-D).
    pub e3: Real,
    /// Poisson's ratio 12.
    pub nu12: Real,
    /// Poisson's ratio 13 (3-D).
    pub nu13: Real,
    /// Poisson's ratio 23 (3-D).
    pub nu23: Real,
    /// Shear modulus 12.
    pub g12: Real,
    /// Shear modulus 13 (3-D).
    pub g13: Real,
    /// Shear modulus 23 (3-D).
    pub g23: Real,
    /// Upstream-defect switches (UD-033: `celerity`; UD-104:
    /// `material_axes_as_directions`; UD-105: `orthotropic_parent_update`).
    pub options: LawOptions,
}

impl<const DIM: usize> MaterialElasticOrthotropic<DIM> {
    /// New law; `mesh` initialises the element filter blocks.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        if spec.spatial_dimension != DIM {
            return Err(Error::DimensionMismatch {
                context: "MaterialElasticOrthotropic",
                expected: DIM,
                got: spec.spatial_dimension,
            });
        }
        let base = match mesh {
            Some(m) => MaterialBase::with_mesh(spec, "elastic_orthotropic", m)?,
            None => MaterialBase::new(spec, "elastic_orthotropic")?,
        };
        Ok(Self {
            base,
            core: AnisotropicCore::new(),
            e1: 0.,
            e2: 0.,
            e3: 0.,
            nu12: 0.,
            nu13: 0.,
            nu23: 0.,
            g12: 0.,
            g13: 0.,
            g23: 0.,
            options: LawOptions::default(),
        })
    }

    const PARAMS_1D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(
        1,
        Self,
        crate::real_param!(
            Self,
            "E1",
            ParameterAccess::PARSMOD,
            "Young's modulus (n1)",
            e1
        ),
        crate::real_param!(
            Self,
            "E2",
            ParameterAccess::PARSMOD,
            "Young's modulus (n2)",
            e2
        ),
        crate::real_param!(
            Self,
            "nu12",
            ParameterAccess::PARSMOD,
            "Poisson's ratio (12)",
            nu12
        ),
        crate::real_param!(
            Self,
            "G12",
            ParameterAccess::PARSMOD,
            "Shear modulus (12)",
            g12
        )
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(
        2,
        Self,
        crate::real_param!(
            Self,
            "E1",
            ParameterAccess::PARSMOD,
            "Young's modulus (n1)",
            e1
        ),
        crate::real_param!(
            Self,
            "E2",
            ParameterAccess::PARSMOD,
            "Young's modulus (n2)",
            e2
        ),
        crate::real_param!(
            Self,
            "nu12",
            ParameterAccess::PARSMOD,
            "Poisson's ratio (12)",
            nu12
        ),
        crate::real_param!(
            Self,
            "G12",
            ParameterAccess::PARSMOD,
            "Shear modulus (12)",
            g12
        )
    );
    const PARAMS_3D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(
        3,
        Self,
        crate::real_param!(
            Self,
            "E1",
            ParameterAccess::PARSMOD,
            "Young's modulus (n1)",
            e1
        ),
        crate::real_param!(
            Self,
            "E2",
            ParameterAccess::PARSMOD,
            "Young's modulus (n2)",
            e2
        ),
        crate::real_param!(
            Self,
            "nu12",
            ParameterAccess::PARSMOD,
            "Poisson's ratio (12)",
            nu12
        ),
        crate::real_param!(
            Self,
            "G12",
            ParameterAccess::PARSMOD,
            "Shear modulus (12)",
            g12
        ),
        crate::real_param!(
            Self,
            "E3",
            ParameterAccess::PARSMOD,
            "Young's modulus (n3)",
            e3
        ),
        crate::real_param!(
            Self,
            "nu13",
            ParameterAccess::PARSMOD,
            "Poisson's ratio (13)",
            nu13
        ),
        crate::real_param!(
            Self,
            "nu23",
            ParameterAccess::PARSMOD,
            "Poisson's ratio (23)",
            nu23
        ),
        crate::real_param!(
            Self,
            "G13",
            ParameterAccess::PARSMOD,
            "Shear modulus (13)",
            g13
        ),
        crate::real_param!(
            Self,
            "G23",
            ParameterAccess::PARSMOD,
            "Shear modulus (23)",
            g23
        )
    );

    /// Upstream `MaterialElasticOrthotropic::updateInternalParameters`;
    /// with [`LawOptions::orthotropic_parent_update`] (default, UD-105) also
    /// the parent's `Material::updateInternalParameters` (the `eigen_grad_u`
    /// parameter into the internal) and the stiffness flag.
    fn update(&mut self) -> Result<()> {
        if self.options.orthotropic_parent_update {
            self.base.update_internal_parameters()?;
            self.core.was_stiffness_assembled = false;
        }
        let v = AnisotropicCore::<DIM>::VOIGT;
        let d2 = DIM * DIM;
        self.core.c = akantu_core::tensor::MatrixX::zeros(v, v);
        self.core.c_prime = akantu_core::tensor::MatrixX::zeros(d2, d2);
        let (e1, e2, e3) = (self.e1, self.e2, self.e3);
        let (nu12, nu13, nu23) = (self.nu12, self.nu13, self.nu23);
        let nu21 = nu12 * e2 / e1;
        let nu31 = nu13 * e3 / e1;
        let nu32 = nu23 * e3 / e2;
        if DIM == 1 {
            return Err(Error::invalid(
                "Dimensions 1 not implemented: makes no sense to have orthotropy for 1D",
            ));
        }
        let gamma = if DIM == 3 {
            1. / (1. - nu12 * nu21 - nu23 * nu32 - nu31 * nu13 - 2. * nu21 * nu32 * nu13)
        } else {
            1. / (1. - nu12 * nu21)
        };
        let cp = &mut self.core.c_prime;
        *cp.at_mut(0, 0) = e1 * (1. - nu23 * nu32) * gamma;
        *cp.at_mut(1, 1) = e2 * (1. - nu13 * nu31) * gamma;
        if DIM == 3 {
            *cp.at_mut(2, 2) = e3 * (1. - nu12 * nu21) * gamma;
        }
        let c01 = e1 * (nu21 + nu31 * nu23) * gamma;
        *cp.at_mut(0, 1) = c01;
        *cp.at_mut(1, 0) = c01;
        if DIM == 3 {
            let c02 = e1 * (nu31 + nu21 * nu32) * gamma;
            *cp.at_mut(0, 2) = c02;
            *cp.at_mut(2, 0) = c02;
            let c12 = e2 * (nu32 + nu12 * nu31) * gamma;
            *cp.at_mut(1, 2) = c12;
            *cp.at_mut(2, 1) = c12;
            *cp.at_mut(3, 3) = self.g23;
            *cp.at_mut(4, 4) = self.g13;
            *cp.at_mut(5, 5) = self.g12;
        } else {
            *cp.at_mut(2, 2) = self.g12;
        }
        self.core
            .rotate_c_prime(self.options.material_axes_as_directions)?;
        self.core.compute_eigenvalues()
    }
}

impl<const DIM: usize> MaterialParams for MaterialElasticOrthotropic<DIM> {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        match DIM {
            1 => Self::PARAMS_1D,
            2 => Self::PARAMS_2D,
            _ => Self::PARAMS_3D,
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.update()
    }
}

crate::impl_material_registry!(MaterialElasticOrthotropic<DIM>, const DIM);
crate::impl_anisotropic_law!(MaterialElasticOrthotropic);

impl<const DIM: usize> Material for MaterialElasticOrthotropic<DIM> {
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream: `MaterialElasticLinearAnisotropic::initMaterial` (=
    /// `Material::initMaterial`, whose `updateInternalParameters` is the
    /// orthotropic one), then `updateInternalParameters` again.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base.set_init();
        if self.base.finite_deformation {
            return Err(Error::NotImplemented(
                "finite deformation not possible in material orthotropic (TO BE IMPLEMENTED)",
            ));
        }
        self.update()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let exec = crate::material::points_exec(self, ctx);
        anisotropic_compute_stress(exec, &mut self.base, &self.core, ty, ghost)
    }

    /// As the anisotropic law (the same kernel): a point's stress reads only
    /// its own gradient and the constant `C`.
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        anisotropic_tangent(&self.base, &self.core, ty, ghost, tangent)?;
        self.core.was_stiffness_assembled = true;
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        anisotropic_energy::<DIM>(&mut self.base, ty)
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let nq = ctx
            .services
            .nb_integration_points(element.ty(), GhostType::NotGhost);
        anisotropic_energy_by_element::<DIM>(&self.base, nq, element)
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getPushWaveSpeed of elastic_orthotropic",
        ))
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getShearWaveSpeed of elastic_orthotropic",
        ))
    }

    fn celerity(&self, _element: Element) -> Result<Real> {
        self.core.celerity(self.base.rho, self.options.celerity)
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.options)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        !self.core.was_stiffness_assembled
    }

    fn mark_stiffness_changed(&mut self) {
        self.core.was_stiffness_assembled = false;
    }
}

/// Register `elastic_orthotropic`.
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "elastic_orthotropic",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialElasticOrthotropic::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
