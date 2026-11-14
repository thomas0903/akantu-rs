// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_cohesive/solid_mechanics_model_cohesive.cc
// (constructor: the "cohesive elements" dumper; setTimeStep;
// addDumpGroupFieldToDumper; onDump), src/model/solid_mechanics/
// solid_mechanics_model_io.cc (dump variants, createNodalFieldReal/Bool,
// createElementalField), src/model/common/constitutive_laws/
// {constitutive_laws_handler_tmpl.hh, constitutive_law_tmpl.hh}
// (flattenInternal, isInternal, getInternalDataPerElem with an element
// kind), src/model/solid_mechanics/solid_mechanics_model_cohesive/
// materials/cohesive_internal_field.hh (element kinds of the cohesive and
// facet internals).

//! Cohesive dumping (COH-013).
//!
//! # Dumpers
//!
//! * [`COHESIVE_DUMPER`] (`"cohesive elements"`): registered by the model
//!   constructor as upstream — a ParaView dumper whose file base name is
//!   the model id (the same as the solid model's default dumper: set
//!   another base name before dumping both, **UD-753**), on the mesh with
//!   the **cohesive** element types of the model dimension (not ghost). Its
//!   time step follows [`SolidMechanicsModelCohesive::set_time_step`]
//!   (upstream sets it explicitly, besides the solid model's default
//!   dumper).
//! * [`FACETS_DUMPER`] (`"facets"`): upstream's `addDumpGroupFieldToDumper`
//!   knows the name (fields of dimension `dim − 1`) but no code registers
//!   it; [`SolidMechanicsModelCohesive::register_facets_dumper`] registers
//!   it the way an upstream user would (`mesh.registerDumper<
//!   DumperParaview>("facets", …)` + `addDumpMeshToDumper("facets",
//!   mesh_facets, dim − 1, _not_ghost, _ek_regular)`), and dumps write the
//!   facet mesh.
//!
//! Both dumpers live in the solid model's registry
//! (`smm.dumpers.dumpable`, upstream's mesh-level `Dumpable`), so the
//! generic calls (`set_base_name_to_dumper`, `set_directory_to_dumper`,
//! text mode, …) work on them. **Dumping** them, through
//! [`SolidMechanicsModelCohesive::dump_named`] or through the solid model
//! (`model.smm.dump_named(…)`), uses the cohesive resolver on the dumper's
//! own mesh — the cohesive elements of the main mesh, or the facet mesh —
//! with the cohesive and facet internals: the cohesive model installs a
//! dump route on its solid model ([`akantu_solid::model::DumpRoute`]).
//! Upstream's `Model::dump(name)` is `mesh.dump(name)`,
//! which writes the mesh the dumper was registered with whichever model
//! is called; without the route the solid model's resolver would write the
//! main mesh's `d − 1` boundary elements under the facets dumper's name.
//!
//! # Fields (upstream `createElementalField` with an element kind)
//!
//! A law internal is dumpable on a dumper when one law holds it **on that
//! dumper's elements** (upstream `isInternal(name, kind)`: the element kind
//! of the `InternalField`): on `"cohesive elements"` the cohesive internals
//! (`damage`, `opening`, `tractions`, `contact_opening`,
//! `contact_tractions`, `delta max`, `normal`, `reversible_energy`,
//! `total_energy`, and the law-specific `sigma_c_eff`, `delta_c_eff`,
//! `insertion_stress`, `delta_n_max`, …), on `"facets"` the facet internal
//! `sigma_c` (`FacetRandomInternalField`). Values are upstream's flattened
//! internal (`flattenInternal(name, kind)`): every element of the block
//! gets its law's values, averaged over its quadrature points (upstream
//! homogeniser), and elements whose law has no such internal get **zeros**
//! (upstream: zeros on the first dump, uninitialised memory afterwards,
//! **UD-752**). For a name that is an internal of another element kind
//! (`sigma_c` on cohesive elements, `damage` on facets, bulk internals) or
//! no field at all (`delta_c` is a parameter), upstream falls through to
//! `createFieldFromAttachedData`, which creates an empty mesh-data array of
//! that name, and its next dump raises; the port refuses these names at
//! registration and leaves the mesh untouched (**UD-754**).
//! On `"facets"` the regular-kind elemental fields (`material_index`,
//! `partitions`, bulk internals) are refused: upstream builds them over
//! the bulk mesh and writes data arrays inconsistent with the facet cells
//! (**UD-755**); so is a facet internal of an intrinsic model at dump time
//! (no facet filter exists: upstream writes an empty data array beside the
//! facet cells, UD-755). Nodal fields (`displacement`, `velocity`, `acceleration`,
//! `mass`, `external_force`, `internal_force`, `increment`,
//! `blocked_dofs`; padded to 3 components by the `*_vector` variants) are
//! read from the model on every dumper, as upstream.

use akantu_core::array_shared::ArrayRef;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::shape::IntegrationOrders;
use akantu_io::dumper::{
    DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, MeshSelection, solid_mechanics_field,
};
use akantu_mesh::{Mesh, MeshDataTypeCode};
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_solid::model::{DOF_ID, SolidMechanicsModelEvent, SolidMechanicsPhysics};
use akantu_solid::{Material, SolidMechanicsModel};
use akantu_solver::DofManager;

use crate::material::{COHESIVE_FE_ENGINE, Domain, MaterialCohesive};
use crate::model::SolidMechanicsModelCohesive;

/// Name of the cohesive-element dumper (upstream `"cohesive elements"`).
pub const COHESIVE_DUMPER: &str = "cohesive elements";

/// Name of the facet dumper (upstream `"facets"`).
pub const FACETS_DUMPER: &str = "facets";

/// Which element domain a dumper writes (upstream: the element kind and
/// spatial dimension `addDumpGroupFieldToDumper` derives from the dumper
/// name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DumperDomain {
    /// `"cohesive elements"`: cohesive types of the model dimension.
    Cohesive,
    /// `"facets"`: regular types of dimension `dim − 1` of the facet mesh.
    Facets,
    /// Any other dumper: the solid model's fields (regular kind).
    Regular,
}

fn domain_of_dumper(name: &str) -> DumperDomain {
    match name {
        COHESIVE_DUMPER => DumperDomain::Cohesive,
        FACETS_DUMPER => DumperDomain::Facets,
        _ => DumperDomain::Regular,
    }
}

fn cohesive_ref(law: &dyn Material) -> Option<&MaterialCohesive> {
    law.as_any()
        .and_then(|a| a.downcast_ref::<MaterialCohesive>())
}

/// Register the `"cohesive elements"` dumper (upstream constructor:
/// `registerDumper<DumperParaview>("cohesive elements", id)`,
/// `addDumpMeshToDumper(…, mesh, dim, _not_ghost, _ek_cohesive)`).
pub(crate) fn register_cohesive_dumper(
    smm: &mut SolidMechanicsModel,
    id: &str,
    dim: usize,
) -> Result<()> {
    let d = &mut smm.dumpers.dumpable;
    d.register_paraview(COHESIVE_DUMPER, Some(id), false);
    d.add_dump_mesh_to_dumper(
        COHESIVE_DUMPER,
        MeshSelection::new(Some(dim), GhostType::NotGhost, ElementKind::Cohesive),
    )
}

/// Live model state for one dump of the cohesive or facet dumper.
struct CohesiveResolver<'a> {
    physics: &'a SolidMechanicsPhysics,
    dofs: &'a DofManager,
}

/// Quadrature points per facet of type `ty`: the rule of the model's facet
/// FE engine (`IntegrationOrders::facets_cohesive`, the order of the
/// cohesive type built on that facet), i.e. what upstream's
/// `FacetsFEEngine::getNbIntegrationPoints` returns. Computed from the
/// type alone so that a dump through the solid model's route (no access to
/// the cohesive model's engine) resolves the facet internals identically.
fn nb_facet_integration_points(ty: ElementType) -> Result<usize> {
    akantu_fe::quadrature::nb_quadrature_points(ty, IntegrationOrders::facets_cohesive().order(ty))
}

impl CohesiveResolver<'_> {
    /// Upstream `createNodalFieldReal` arrays not owned by the DOF manager.
    fn nodal_real(&self, name: &str) -> Result<DumpBuffer> {
        let p = self.physics;
        let a = match name {
            "external_force" => p.external_force()?,
            "internal_force" => p.internal_force()?,
            "mass" => p.mass()?,
            "increment" => self
                .dofs
                .family(DOF_ID)?
                .increment
                .as_ref()
                .ok_or_else(|| Error::not_found("nodal dump field (not allocated)", name))?
                .read()?,
            _ => return Err(Error::not_found("nodal dump field", name)),
        };
        Ok(DumpBuffer::Real {
            nb_component: a.nb_component(),
            values: a.as_slice().to_vec(),
        })
    }

    /// Upstream `flattenInternal(name, kind)` for one block `(ty, ghost)`
    /// of the cohesive (`facets = false`, main mesh, `CohesiveFEEngine`) or
    /// facet (`facets = true`, facet mesh, `FacetsFEEngine`) internals:
    /// `nb_quad × nb_component` values per element of the block, copied
    /// from every law holding the internal on that domain (element filter
    /// or facet filter order), zeros elsewhere (UD-752).
    fn flattened_internal(
        &self,
        mesh: &Mesh,
        law_name: Option<&str>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let facets = mesh.is_mesh_facets();
        let (want, nq) = if facets {
            (Domain::Facet, nb_facet_integration_points(ty)?)
        } else {
            let fe = self.physics.base.fems.get(COHESIVE_FE_ENGINE)?;
            (Domain::Cohesive, fe.nb_integration_points(ty, ghost)?)
        };
        let ne = mesh.nb_element(ty, ghost);
        let mut nbc = None;
        let mut any_filter = false;
        // one guard per law block (P8/P12: guards, no copies); read below
        let mut blocks: Vec<(&[Idx], ArrayRef<'_, Real>)> = Vec::new();
        for law in self.physics.materials.laws() {
            if law_name.is_some_and(|n| n != law.base().name) {
                continue;
            }
            let Some(c) = cohesive_ref(law.as_ref()) else {
                continue;
            };
            if c.domain_of(name) != want {
                continue;
            }
            let Ok(field) = c.internal(name) else {
                continue;
            };
            nbc = Some(field.nb_component());
            let filter = if facets {
                c.core.facet_filter.get(ty, ghost)
            } else {
                c.base().element_filter.get(ty, ghost)
            };
            let Ok(filter) = filter else {
                continue;
            };
            any_filter = true;
            if filter.is_empty() {
                continue;
            }
            blocks.push((filter.as_slice(), field.get(ty, ghost)?));
        }
        let Some(c) = nbc else {
            return Err(Error::not_found("internal field", name));
        };
        if facets && !any_filter {
            // UD-755: no law has a facet filter (intrinsic model: upstream
            // initialises facet filters only for extrinsic insertion and
            // writes an empty data array beside the facet cells)
            return Err(Error::InvalidArgument(format!(
                "facet internal '{name}' has no values on the {ty:?} facets: facet internals \
                 exist only with extrinsic insertion (UD-755)"
            )));
        }
        let per = nq * c;
        let mut values = vec![0.0; ne * per];
        for (filter, v_g) in &blocks {
            let v = v_g.as_slice();
            for (local, &el) in filter.iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                let src = v
                    .get(local * per..(local + 1) * per)
                    .ok_or(Error::OutOfRange {
                        what: "internal block (law-local element)",
                        index: local,
                        len: v.len().checked_div(per).unwrap_or(0),
                    })?;
                let dst = values
                    .get_mut(e * per..(e + 1) * per)
                    .ok_or(Error::OutOfRange {
                        what: "element of a law filter",
                        index: e,
                        len: ne,
                    })?;
                dst.copy_from_slice(src);
            }
        }
        Ok(DumpBuffer::Real {
            nb_component: c,
            values,
        })
    }
}

impl FieldResolver for CohesiveResolver<'_> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::Dof { family, order }, None) => {
                let a = if *order == 0 {
                    self.dofs.dofs(family)?
                } else {
                    self.dofs.dofs_derivative(family, *order)?
                };
                Ok(DumpBuffer::Real {
                    nb_component: a.nb_component(),
                    values: a.as_slice().to_vec(),
                })
            }
            (FieldSource::Blocked { family }, None) => {
                let b = self.dofs.blocked_dofs(family)?;
                Ok(DumpBuffer::Bool {
                    nb_component: b.nb_component(),
                    values: b.as_slice().to_vec(),
                })
            }
            (FieldSource::ModelNodal(n), None) => self.nodal_real(n),
            (FieldSource::ModelElemental(n), Some(t))
                if n == "material_index" && !mesh.is_mesh_facets() =>
            {
                let a = self.physics.materials.law_index_map().get(t, ghost)?;
                Ok(DumpBuffer::Int {
                    nb_component: 1,
                    values: a.as_slice().to_vec(),
                })
            }
            (FieldSource::Internal { law, name }, Some(t)) => {
                self.flattened_internal(mesh, law.as_deref(), name, t, ghost)
            }
            _ => Err(Error::not_found("cohesive dump field", field.name.clone())),
        }
    }
}

impl SolidMechanicsModelCohesive {
    /// The element domain (`Cohesive`, `Facet`, `Regular`) of the law
    /// internal `name` for every law that has it.
    fn internal_domains(&self, name: &str) -> Vec<Domain> {
        let mut out = Vec::new();
        for law in self.smm.physics.materials.laws() {
            if let Some(c) = cohesive_ref(law.as_ref()) {
                if c.internal(name).is_ok() {
                    out.push(c.domain_of(name));
                }
            } else {
                let mut found = false;
                law.for_each_internal(&mut |n, _| found |= n == name);
                if found {
                    out.push(Domain::Regular);
                }
            }
        }
        out
    }

    /// Upstream `SolidMechanicsModelCohesive::addDumpGroupFieldToDumper`
    /// for the cohesive and facet dumpers: the field descriptor of `name`
    /// on the dumper's element domain.
    fn cohesive_descriptor(
        &self,
        domain: DumperDomain,
        name: &str,
        group: Option<&str>,
        padding: bool,
    ) -> Result<FieldDescriptor> {
        let want = match domain {
            DumperDomain::Cohesive => Domain::Cohesive,
            DumperDomain::Facets => Domain::Facet,
            DumperDomain::Regular => Domain::Regular,
        };
        if let Some(g) = group.filter(|g| *g != akantu_io::dumper::group::ALL_GROUP) {
            // UD-1080: upstream registers a group field on
            // the element group's own dumper (`getGroupDumper`), which has
            // no cohesive or facets dumper: the registration throws (or
            // terminates the process for a cohesive internal)
            return Err(Error::InvalidArgument(format!(
                "'{name}' restricted to the group '{g}' on the '{}' dumper: the cohesive and \
                 facets dumpers take whole-mesh fields only (group 'all'); upstream refuses \
                 the registration (UD-1080)",
                if domain == DumperDomain::Facets {
                    FACETS_DUMPER
                } else {
                    COHESIVE_DUMPER
                }
            )));
        }
        let domains = self.internal_domains(name);
        if !domains.is_empty() && !domains.contains(&want) {
            // UD-754: upstream's isInternal(name, kind) is false; it falls
            // through to createFieldFromAttachedData (which creates an
            // empty mesh-data array) and the next dump raises
            let what = match domain {
                DumperDomain::Facets => "facet",
                _ => "cohesive-element",
            };
            return Err(Error::InvalidArgument(format!(
                "'{name}' is not a {what} internal of any law (it lives on {}), so it is \
                 not a field of the '{}' dumper (UD-754){}",
                match domains.first() {
                    Some(Domain::Facet) => "the facets: dump it on the 'facets' dumper",
                    Some(Domain::Cohesive) =>
                        "the cohesive elements: dump it on the 'cohesive elements' dumper",
                    _ => "the bulk elements: dump it on the model's default dumper",
                },
                if domain == DumperDomain::Facets {
                    FACETS_DUMPER
                } else {
                    COHESIVE_DUMPER
                },
                if name == "sigma_c" && domain == DumperDomain::Cohesive {
                    "; the strength of each cohesive element is 'sigma_c_eff'"
                } else {
                    ""
                }
            )));
        }
        if domains.is_empty() && name == "delta_c" {
            return Err(Error::InvalidArgument(
                "'delta_c' is a law parameter, not an internal, so it is not a field of this \
                 dumper (UD-754); the per-element critical opening is 'delta_c_eff'"
                    .into(),
            ));
        }
        let d = solid_mechanics_field(name, group, padding, |n| {
            self.internal_domains(n).contains(&want)
        })?;
        if let FieldSource::Mesh(n) = &d.source {
            self.check_mesh_data_field(domain, n)?;
        }
        if domain == DumperDomain::Facets {
            // UD-755: upstream builds these over the bulk mesh
            if matches!(
                d.source,
                FieldSource::ModelElemental(_)
                    | FieldSource::Partition
                    | FieldSource::Derived { .. }
            ) {
                return Err(Error::InvalidArgument(format!(
                    "'{name}' is not available on the '{FACETS_DUMPER}' dumper: upstream builds \
                     it over the bulk elements, inconsistent with the facet cells (UD-755)"
                )));
            }
        }
        Ok(d)
    }

    /// UD-1081 (the solid model's UD-683 rule on the
    /// cohesive path): a name that falls through to the mesh data of the
    /// dumper's mesh must be numeric elemental mesh data there, otherwise
    /// it is refused at registration. Upstream registers it and every later
    /// dump of the dumper raises (a typo, or a law's `bool` internal such
    /// as the fatigue law's `normal_regime`, which upstream's
    /// `isInternal<Real>` does not see either).
    fn check_mesh_data_field(&self, domain: DumperDomain, n: &str) -> Result<()> {
        let mesh = self.smm.physics.base.mesh.read()?;
        let target: &Mesh = match domain {
            DumperDomain::Facets => mesh
                .mesh_facets()
                .ok_or_else(|| Error::invalid("the facets dumper needs the facet mesh"))?,
            _ => &mesh,
        };
        let data = target.data();
        let dumpable = data.has_elemental(n)
            && matches!(
                data.type_code(n)?,
                MeshDataTypeCode::Int
                    | MeshDataTypeCode::UInt
                    | MeshDataTypeCode::Real
                    | MeshDataTypeCode::Bool
            );
        if dumpable {
            return Ok(());
        }
        let mut bool_internal = false;
        for law in self.smm.physics.materials.laws() {
            law.for_each_internal(&mut |m, f| {
                bool_internal |= m == n && matches!(f, InternalFieldRef::Bool(_));
            });
        }
        Err(Error::not_found(
            "dump field",
            format!(
                "{n} ({}; upstream registers it and every later dump of the dumper raises, \
                 UD-1081)",
                if bool_internal {
                    "a bool internal of a law: only Real internals are dumpable, as upstream's \
                     isInternal<Real>"
                } else {
                    "neither a model field, nor an internal of a law on this dumper's \
                     elements, nor numeric elemental mesh data"
                }
            ),
        ))
    }

    /// Upstream `addDumpGroupFieldToDumper(dumper, name, group, kind,
    /// padding)` (COH-013): on [`COHESIVE_DUMPER`] the cohesive internals,
    /// on [`FACETS_DUMPER`] the facet internals (module documentation);
    /// any other dumper is the solid model's
    /// ([`SolidMechanicsModel::add_dump_group_field_to_dumper`]).
    ///
    /// # Errors
    /// Unknown dumper, duplicate field name, a name refused on that dumper
    /// (UD-754, UD-755, UD-1081), a group other than `"all"` on the
    /// cohesive or facets dumper (UD-1080), the solid model's errors.
    pub fn add_dump_group_field_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        match domain_of_dumper(dumper) {
            DumperDomain::Regular => self
                .smm
                .add_dump_group_field_to_dumper(dumper, name, group, padding),
            domain => {
                let d = self.cohesive_descriptor(domain, name, Some(group), padding)?;
                self.smm
                    .dumpers
                    .dumpable
                    .add_dump_field_to_dumper(dumper, d)
            }
        }
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)` (no padding).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelCohesive::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", false)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, name)` (nodal fields
    /// padded to 3 components; elemental cohesive fields are not padded,
    /// as upstream).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelCohesive::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", true)
    }

    /// Upstream `addDumpFieldTensorToDumper(dumper, name)` (as the vector
    /// variant).
    ///
    /// # Errors
    /// See [`SolidMechanicsModelCohesive::add_dump_group_field_to_dumper`].
    pub fn add_dump_field_tensor_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, "all", true)
    }

    /// Upstream `addDumpField(name)`: the solid model's default dumper.
    ///
    /// # Errors
    /// See [`SolidMechanicsModel::add_dump_field`].
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        self.smm.add_dump_field(name)
    }

    /// Upstream `addDumpFieldVector(name)` (default dumper).
    ///
    /// # Errors
    /// See [`SolidMechanicsModel::add_dump_field_vector`].
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        self.smm.add_dump_field_vector(name)
    }

    /// Upstream `setBaseNameToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.smm.set_base_name_to_dumper(dumper, name)
    }

    /// Upstream `setDirectoryToDumper(dumper, dir)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &std::path::Path) -> Result<()> {
        self.smm.set_directory_to_dumper(dumper, dir)
    }

    /// Upstream `setTextModeToDumper(dumper)` (ASCII ParaView output).
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_text_mode_to_dumper(&mut self, dumper: &str) -> Result<()> {
        self.smm.dumpers.dumpable.set_text_mode_to_dumper(dumper)
    }

    /// Upstream `setTimeStepToDumper(dumper, dt)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_time_step_to_dumper(&mut self, dumper: &str, dt: Real) -> Result<()> {
        self.smm
            .dumpers
            .dumpable
            .set_time_step_to_dumper(dumper, dt)
    }

    /// Register the [`FACETS_DUMPER`] as an upstream user does
    /// (`mesh.registerDumper<DumperParaview>("facets", file_name)`,
    /// `mesh.addDumpMeshToDumper("facets", mesh.getMeshFacets(), dim − 1,
    /// _not_ghost, _ek_regular)`; upstream registers no such dumper
    /// itself). The file base name defaults to `"facets"`. It gets no time
    /// step from [`SolidMechanicsModelCohesive::set_time_step`] (as
    /// upstream): use [`SolidMechanicsModelCohesive::set_time_step_to_dumper`]
    /// for a `.pvd` time series.
    ///
    /// # Errors
    /// A model of dimension 1 (no facets of dimension 0 to dump).
    pub fn register_facets_dumper(&mut self, file_name: Option<&str>) -> Result<()> {
        let dim = self.smm.spatial_dimension();
        if dim < 2 {
            return Err(Error::Unsupported(
                "the facets dumper of a 1-D model (point facets)".into(),
            ));
        }
        let d = &mut self.smm.dumpers.dumpable;
        d.register_paraview(FACETS_DUMPER, file_name.or(Some(FACETS_DUMPER)), false);
        d.add_dump_mesh_to_dumper(
            FACETS_DUMPER,
            MeshSelection::new(Some(dim - 1), GhostType::NotGhost, ElementKind::Regular),
        )
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)`: the solid
    /// model's default dumper.
    ///
    /// # Errors
    /// See [`SolidMechanicsModel::dump`].
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.smm.dump(time, step)
    }

    /// Upstream `dump(dumper)` / `dump(dumper, step)` / `dump(dumper, time,
    /// step)`: [`COHESIVE_DUMPER`] (main mesh) and [`FACETS_DUMPER`]
    /// (facet mesh) with the cohesive resolver, any other dumper through
    /// the solid model.
    ///
    /// # Errors
    /// Unknown dumper, a field that cannot be resolved, write errors (the
    /// dumper state is unchanged on error).
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        match domain_of_dumper(dumper) {
            DumperDomain::Regular => self.smm.dump_named(dumper, time, step),
            domain => dump_cohesive_domain(&mut self.smm, dumper, domain, time, step),
        }
    }
}

/// Upstream `BeforeDumpEvent` and `onDump()`: the laws' `onDump`
/// (Cauchy stresses of finite-deformation bulk laws) and the registered
/// handlers. Upstream's `onDump` then flattens the cohesive internals and
/// the regular ones; here the resolver reads them live at dump time.
fn before_dump(smm: &mut SolidMechanicsModel) -> Result<()> {
    for law in smm.physics.materials.laws_mut() {
        law.on_dump()?;
    }
    smm.send_event(SolidMechanicsModelEvent::BeforeDump)
}

/// Dump [`COHESIVE_DUMPER`] (main mesh) or [`FACETS_DUMPER`] (facet mesh)
/// with the cohesive resolver.
fn dump_cohesive_domain(
    smm: &mut SolidMechanicsModel,
    dumper: &str,
    domain: DumperDomain,
    time: Option<Real>,
    step: Option<usize>,
) -> Result<()> {
    before_dump(smm)?;
    let mesh_h = smm.physics.base.mesh.clone();
    let guard = mesh_h.read()?;
    let mesh: &Mesh = if domain == DumperDomain::Facets {
        guard
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("the facets dumper needs the facet mesh".into()))?
    } else {
        &guard
    };
    let r = CohesiveResolver {
        physics: &smm.physics,
        dofs: &smm.solver.dof_manager,
    };
    smm.dumpers
        .dumpable
        .dump_named(dumper, mesh, &r, time, step)
}

/// The cohesive model's [`akantu_solid::model::DumpRoute`], installed on
/// its solid model by the constructor: a dump of
/// [`COHESIVE_DUMPER`] or [`FACETS_DUMPER`] through
/// `model.smm.dump_named(…)` writes exactly what
/// [`SolidMechanicsModelCohesive::dump_named`] writes (the cohesive
/// elements, or the facet mesh, with the cohesive and facet internals), as
/// upstream's `Model::dump(name)` = `mesh.dump(name)` does for a
/// `SolidMechanicsModel&`; every other dumper goes back to the solid
/// model's resolver.
pub(crate) fn route(
    smm: &mut SolidMechanicsModel,
    dumper: &str,
    time: Option<Real>,
    step: Option<usize>,
) -> Option<Result<()>> {
    match domain_of_dumper(dumper) {
        DumperDomain::Regular => None,
        domain => Some(dump_cohesive_domain(smm, dumper, domain, time, step)),
    }
}
