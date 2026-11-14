// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_FE2/material_FE2.hh, material_FE2.cc, material_FE2_inline_impl.hh.

//! `material_FE2` (XMAT-014): FE² macro material (upstream `MaterialFE2<dim>
//! : MaterialThermal<dim>`) holding one [`SolidMechanicsModelRve`] per
//! not-ghost quadrature point of its single element type `element_type`.
//!
//! * `initMaterial`: the thermal parent's, then per point `q` an RVE on its
//!   own copy of `mesh_file` (gel selector, `nb_gel_pockets`) with the
//!   **same input file** as the macro model, `initFull` (static) and the
//!   initial homogenised stiffness `C_q` (internal `material_stiffness`,
//!   `voigt²`);
//! * `computeStress`: the thermal pre-step (UD-584: no thermal stress where
//!   `α·ΔT = 0`, upstream `0/0` with the parent's defaults), then `σ_I = Σ_J C_IJ ε_J` with
//!   the Voigt strain `ε_I = f_I·(∇u_ij + ∇u_ji)/2` (`f = 1` normal, 2
//!   shear), `σ_ij = σ_ji = σ_I + δ_ij·σ_th` (upstream's `(i == j) *
//!   sigma_th`: a product, so a NaN `σ_th` makes every component NaN);
//! * tangent `C_q`; potential energy and wave speeds as the thermal parent
//!   (not implemented upstream);
//! * `advanceASR(ε*)`: per point, the RVE corners at the macro `∇u_q`, the
//!   point's `delta_T`, `advanceASR(ε*)`, then the macro `eigen_grad_u_q`
//!   ← the RVE's average eigenstrain and `C_q` ← its stiffness.
//!
//! The input file and the upstream-defect switches reach the RVEs through
//! [`Fe2Config`] (upstream: the global parser); register the law with
//! [`register_material_fe2`]. The macro law itself and every RVE (its own
//! switches, UD-585, and the laws its model creates, e.g. UD-573) carry
//! [`Fe2Config::extra_options`]. Mesh paths are resolved against
//! [`Fe2Config::base_dir`] when relative (upstream: the working
//! directory). Only one element type is supported, as upstream; the RVE is
//! 2-D only (other dimensions are refused at `initMaterial`). A macro mesh
//! distributed over several ranks is refused at `initMaterial`: a scope
//! boundary, not a missing port: the RVE models are
//! serial, upstream's FE² does not compile at the reference
//! (`material_FE2.cc:73` passes an `Int` where the `Mesh` constructor takes a
//! communicator) and its test is serial-only in ctest. Inside an RVE
//! a `material_FE2` section creates an inert law ([`MaterialFE2::nested`],
//! upstream skips its `initMaterial`).

use std::path::PathBuf;
use std::sync::Arc;

use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::tensor::as_matrix;
use akantu_core::voigt::voigt_table;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, UInt,
};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::Parser;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{LawOptions, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::material_tensor::tangent_stiffness_voigt_size;
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, small_strain_only};
use crate::options::ExtraLawOptions;
use crate::rve::SolidMechanicsModelRve;

const NG: GhostType = GhostType::NotGhost;

/// The thermal parameters (`Plane_Stress` in 2-D, `E`, `nu`, `alpha`,
/// `delta_T`), then `element_type`, `mesh_file`, `nb_gel_pockets`.
macro_rules! fe2_params {
    ($($ps:ident)?) => {
        &[
            $(fe2_params!(@$ps),)?
            akantu_solid::real_param!(Self, "E", ParameterAccess::PARSMOD, "Young's modulus", elastic.thermal.e),
            akantu_solid::real_param!(Self, "nu", ParameterAccess::PARSMOD, "Poisson's ratio", elastic.thermal.nu),
            akantu_solid::real_param!(Self, "alpha", ParameterAccess::PARSMOD, "Thermal expansion coefficient", elastic.thermal.alpha),
            ParamDescriptor::new(
                "delta_T",
                ParamKind::Real,
                ParameterAccess::PARSMOD,
                "Uniform temperature field",
                |s| ParamValue::Real(s.elastic.delta_t.parameter_value()),
                Some(|s, v| {
                    s.elastic.delta_t.set_default_value(v.as_real()?);
                    Ok(())
                }),
            ),
            ParamDescriptor::new(
                "element_type",
                ParamKind::Enum,
                ParameterAccess::PARSMOD,
                "element type in RVE mesh",
                |s| ParamValue::Enum(s.el_type.to_string()),
                Some(|s, v| {
                    s.el_type = v.as_str()?.trim().parse()?;
                    Ok(())
                }),
            ),
            ParamDescriptor::new(
                "mesh_file",
                ParamKind::String,
                ParameterAccess::PARSMOD,
                "the mesh file for the RVE",
                |s| ParamValue::String(s.mesh_file.clone()),
                Some(|s, v| {
                    s.mesh_file = v.as_str()?.trim().to_string();
                    Ok(())
                }),
            ),
            ParamDescriptor::new(
                "nb_gel_pockets",
                ParamKind::UInt,
                ParameterAccess::PARSMOD,
                "the number of gel pockets in each RVE",
                |s| ParamValue::Int(i64::from(s.nb_gel_pockets)),
                Some(|s, v| {
                    let n = v.as_int()?;
                    s.nb_gel_pockets = UInt::try_from(n)
                        .map_err(|_| Error::invalid(format!("nb_gel_pockets: {n} is not a UInt")))?;
                    Ok(())
                }),
            ),
        ]
    };
    (@plane_stress) => {
        akantu_solid::bool_param!(Self, "Plane_Stress", ParameterAccess::PARSMOD, "Is plane stress", elastic.plane_stress)
    };
}

/// What the RVEs of `material_FE2` need from the macro model (upstream:
/// the global parser and the working directory).
#[derive(Clone, Debug)]
pub struct Fe2Config {
    /// The macro model's input file (instantiates the RVE laws).
    pub parser: Parser,
    /// Directory of relative `mesh_file` paths.
    pub base_dir: Option<PathBuf>,
    /// Upstream-defect switches of the macro `material_FE2` law, of every
    /// RVE and of the laws of the RVE models (normally the options the
    /// macro model's extra materials were registered with).
    pub extra_options: ExtraLawOptions,
}

impl Fe2Config {
    /// The configuration with the corrected defaults
    /// ([`ExtraLawOptions::default`]).
    #[must_use]
    pub fn new(parser: Parser, base_dir: Option<PathBuf>) -> Self {
        Self {
            parser,
            base_dir,
            extra_options: ExtraLawOptions::default(),
        }
    }
}

/// `material_FE2` (upstream `MaterialFE2<DIM>`).
#[derive(Debug)]
pub struct MaterialFE2<const DIM: usize> {
    /// The thermal parent (`E`, `nu`, `alpha`, `delta_T`, thermal state).
    pub elastic: MaterialElastic<DIM>,
    /// Homogenised stiffness per point (`material_stiffness`, `voigt²`,
    /// column-major).
    pub c: InternalField<Real>,
    /// One RVE per not-ghost point of `element_type`.
    pub rves: Vec<SolidMechanicsModelRve>,
    /// `element_type` (default `_triangle_3`).
    pub el_type: ElementType,
    /// `mesh_file`.
    pub mesh_file: String,
    /// `nb_gel_pockets`.
    pub nb_gel_pockets: UInt,
    /// Extra-materials defect switches (UD-584; passed to the RVEs at
    /// `initMaterial`).
    pub extra_options: ExtraLawOptions,
    config: Option<Arc<Fe2Config>>,
    nested: bool,
}

impl<const DIM: usize> MaterialFE2<DIM> {
    /// New macro law with the configuration of its RVEs.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(
        spec: &LawSpec,
        mesh: Option<&Mesh>,
        config: Option<Arc<Fe2Config>>,
    ) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "material_FE2".into();
        let v = tangent_stiffness_voigt_size(DIM);
        Ok(Self {
            elastic,
            c: InternalField::new("material_stiffness", v * v, 0.0)?,
            rves: Vec::new(),
            el_type: ElementType::Triangle3,
            mesh_file: String::new(),
            nb_gel_pockets: 0,
            extra_options: config
                .as_ref()
                .map_or_else(ExtraLawOptions::default, |c| c.extra_options),
            config,
            nested: false,
        })
    }

    /// The inert law a `material_FE2` section creates inside an RVE.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn nested(spec: &LawSpec, mesh: Option<&Mesh>) -> Result<Self> {
        let mut m = Self::new(spec, mesh, None)?;
        m.nested = true;
        Ok(m)
    }

    fn mesh_path(&self) -> PathBuf {
        let p = PathBuf::from(&self.mesh_file);
        match self.config.as_ref().and_then(|c| c.base_dir.as_ref()) {
            Some(dir) if p.is_relative() => dir.join(p),
            _ => p,
        }
    }

    /// Upstream `advanceASR(prestrain)`.
    ///
    /// # Errors
    /// RVE errors.
    pub fn advance_asr(&mut self, prestrain: &akantu_core::tensor::Matrix<2, 2>) -> Result<()> {
        let n = DIM * DIM;
        let v = tangent_stiffness_voigt_size(DIM);
        let ty = self.el_type;
        let grad_u = self.elastic.base().grad_u.get(ty, NG)?.clone();
        let delta_t = self.elastic.delta_t.get(ty, NG)?.clone();
        check_lengths(
            "MaterialFE2::advanceASR",
            self.rves.len(),
            &[grad_u.len(), delta_t.len()],
        )?;
        if DIM != 2 {
            return Err(Error::Unsupported(
                "material_FE2: the RVE is 2D only".into(),
            ));
        }
        let mut eigen = self.elastic.base().eigen_grad_u.get(ty, NG)?.clone();
        let mut c = self.c.get(ty, NG)?.clone();
        for (q, rve) in self.rves.iter_mut().enumerate() {
            let g: &akantu_core::tensor::Matrix<2, 2> =
                as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            rve.apply_boundary_conditions(g)?;
            rve.apply_homogeneous_temperature(delta_t.as_slice()[q])?;
            rve.advance_asr(prestrain)?;
            let e = rve.homogenize_eigen_grad_u()?;
            eigen.as_mut_slice()[q * n..(q + 1) * n].copy_from_slice(e.as_flattened());
            let cq = rve.homogenize_stiffness()?;
            c.as_mut_slice()[q * v * v..(q + 1) * v * v].copy_from_slice(cq.as_flattened());
        }
        self.elastic
            .base_mut()
            .eigen_grad_u
            .get_mut(ty, NG)?
            .copy_from(&eigen)?;
        self.c.get_mut(ty, NG)?.copy_from(&c)?;
        Ok(())
    }

    const PARAMS: &'static [ParamDescriptor<Self>] = fe2_params!();
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = fe2_params!(plane_stress);
}

impl<const DIM: usize> MaterialParams for MaterialFE2<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)
    }
}

akantu_solid::impl_material_registry!(MaterialFE2<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialFE2<DIM> {
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
        f("material_stiffness", InternalFieldRef::Real(&self.c));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f("material_stiffness", InternalFieldMut::Real(&mut self.c))
    }
}

impl<const DIM: usize> Material for MaterialFE2<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `initMaterial` (module docs); a nested law does nothing.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if self.nested {
            return Ok(());
        }
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        MaterialParams::update_params(self)?;
        self.elastic.base_mut().set_init();
        if DIM != 2 {
            return Err(Error::Unsupported(
                "material_FE2: the RVE (SolidMechanicsModelRVE) is 2D only".into(),
            ));
        }
        if ctx.services.mesh().communicator().size() > 1 {
            return Err(Error::Unsupported(
                "material_FE2 on a distributed macro mesh: the RVE models are serial (scope \
                 boundary: upstream's FE2 does not compile at the reference and its test is \
                 serial-only)"
                    .into(),
            ));
        }
        let config = self.config.clone().ok_or_else(|| {
            Error::invalid(
                "material_FE2 needs the input file of the model for its RVEs: register it with \
                 register_material_fe2",
            )
        })?;
        let nb_points = self.c.get(self.el_type, NG)?.len();
        let path = self.mesh_path();
        let v = tangent_stiffness_voigt_size(DIM);
        let mut c = self.c.get(self.el_type, NG)?.clone();
        self.rves.clear();
        for q in 0..nb_points {
            let mut mesh = Mesh::new(DIM, format!("RVE_mesh_{q}"))?;
            akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Auto)?;
            let mut rve = SolidMechanicsModelRve::new_with_options(
                mesh,
                true,
                self.nb_gel_pockets,
                config.parser.clone(),
                &format!("SMM_RVE_{q}"),
                self.extra_options,
            )?;
            rve.init_full()?;
            let cq = rve.homogenize_stiffness()?;
            c.as_mut_slice()[q * v * v..(q + 1) * v * v].copy_from_slice(cq.as_flattened());
            self.rves.push(rve);
        }
        self.c.get_mut(self.el_type, NG)?.copy_from(&c)?;
        Ok(())
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        small_strain_only("material_FE2", self.elastic.base().finite_deformation)?;
        // MaterialThermal::computeStress (the elastic stress it also writes
        // is overwritten below)
        self.elastic.compute_stress(ctx, ty, ghost)?;
        let n = DIM * DIM;
        let v = tangent_stiffness_voigt_size(DIM);
        let table = voigt_table(DIM)?;
        if self.extra_options.fe2_no_thermal_stress_without_expansion {
            // UD-584: no thermal stress where α·ΔT = 0 (upstream: 0/0 with
            // the parent's defaults E = 0, ν = 0.5)
            let alpha = self.elastic.thermal.alpha;
            let dt = self.elastic.delta_t.get(ty, ghost)?.clone();
            let mut th = self.elastic.sigma_th.get_mut(ty, ghost)?;
            for (s, &d) in th.as_mut_slice().iter_mut().zip(dt.as_slice()) {
                if alpha * d == 0. {
                    *s = 0.;
                }
            }
        }
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let sigma_th = self.elastic.sigma_th.get(ty, ghost)?.clone();
        let c = self.c.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        check_lengths(
            "MaterialFE2::computeStress",
            grad_u.len(),
            &[sigma_th.len(), c.len(), sigma.len()],
        )?;
        for q in 0..grad_u.len() {
            let g = &grad_u.as_slice()[q * n..(q + 1) * n];
            let cm = &c.as_slice()[q * v * v..(q + 1) * v * v];
            let s = &mut sigma.as_mut_slice()[q * n..(q + 1) * n];
            let th = sigma_th.as_slice()[q];
            let mut strain = vec![0.; v];
            for (big_i, e) in strain.iter_mut().enumerate() {
                let (i, j) = table.vec(big_i);
                *e = table.factor(big_i) * (g[i + j * DIM] + g[j + i * DIM]) / 2.;
            }
            for big_i in 0..v {
                let mut acc = 0.;
                for (big_j, e) in strain.iter().enumerate() {
                    acc += cm[big_i + big_j * v] * e;
                }
                let (i, j) = table.vec(big_i);
                // upstream `voigt_stress(I) + (i == j) * sigma_th`: the
                // product propagates a NaN σ_th to the shear (UD-584 compat)
                let val = acc + Real::from(u8::from(i == j)) * th;
                s[i + j * DIM] = val;
                s[j + i * DIM] = val;
            }
        }
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let c = self.c.get(ty, ghost)?;
        check_lengths(
            "MaterialFE2::computeTangentModuli",
            c.len(),
            &[tangent.len()],
        )?;
        if !c.is_empty() {
            tangent.copy_from(&c)?;
        }
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, _ty: ElementType) -> Result<()> {
        Err(Error::NotImplemented("MaterialFE2::computePotentialEnergy"))
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented("MaterialFE2::getPushWaveSpeed"))
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented("MaterialFE2::getShearWaveSpeed"))
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

/// Register `material_FE2` (upstream `INSTANTIATE_MATERIAL(material_FE2,
/// MaterialFE2)`) with the configuration of its RVEs.
///
/// # Errors
/// A duplicate registration.
pub fn register_material_fe2(factory: &mut MaterialFactory, config: Fe2Config) -> Result<()> {
    let config = Arc::new(config);
    factory.register(
        "material_FE2",
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            let config = Some(config.clone());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialFE2::<D>::new(spec, mesh, config)?) as Box<dyn Material>
            })
        }),
    )
}
