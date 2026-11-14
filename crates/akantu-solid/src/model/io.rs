// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_io.cc (createNodalFieldReal/Bool,
// createElementalField, dump, onDump), src/model/model.cc (dumper
// convenience API: addDumpField*, setBaseName, setDirectory, dumpGroup…),
// src/model/common/constitutive_laws/constitutive_laws_handler_tmpl.hh
// (flattenInternal, isInternal).

//! Dumping (SMM-017, MODEL-009): the model's dumpers and the
//! [`FieldResolver`] reading live model state at dump time.

use akantu_core::ElementKind;
use akantu_core::{ElementType, Error, GhostType, Idx, Real, Result};
use akantu_io::dumper::group::{ALL_GROUP, SharedMeshDumpers};
use akantu_io::dumper::{
    DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, MeshFilter, MeshSelection,
};
use akantu_io::dumper::{Dumpable, Dumper, solid_mechanics_field};
use akantu_mesh::{Mesh, MeshDataTypeCode};
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_solver::DofManager;

use super::{DOF_ID, SolidMechanicsModel, SolidMechanicsModelEvent, SolidMechanicsPhysics};
use crate::material::Material;

/// A dump route installed by a model built around this one:
/// [`SolidMechanicsModel::dump_named`] calls it first; `Some` is the
/// result of a dump the route performed itself (a dumper registered by the
/// extension, on its own mesh, with its own resolver), `None` hands the
/// dumper back to the solid model's resolver.
///
/// Upstream `Model::dump(name)` is `mesh.dump(name)`: a dumper writes the
/// mesh it was registered with and fields created by whichever model
/// registered them, so dumping the cohesive model's `"cohesive elements"`
/// or `"facets"` dumper through a `SolidMechanicsModel&` writes the same
/// files as through the cohesive model. The route reproduces that for the
/// port's split between the solid model (which owns the registry) and the
/// cohesive model (which knows the facet mesh and the cohesive internals).
pub type DumpRoute =
    fn(&mut SolidMechanicsModel, &str, Option<Real>, Option<usize>) -> Option<Result<()>>;

/// Dumpers of a solid mechanics model (upstream: registered on the mesh by
/// the model constructor: ParaView dumper named `solid_mechanics_model`,
/// file base name = model id, default, with the regular elements of the
/// model dimension, not-ghost).
#[derive(Debug)]
pub struct SmmDumpers {
    /// The dumper registry.
    pub dumpable: Dumpable,
    spatial_dimension: usize,
    route: Option<DumpRoute>,
}

impl SmmDumpers {
    pub(super) fn new(id: &str, spatial_dimension: usize) -> Result<Self> {
        let mut dumpable = Dumpable::new();
        dumpable.register_paraview("solid_mechanics_model", Some(id), true);
        dumpable.add_dump_mesh(MeshSelection::new(
            Some(spatial_dimension),
            GhostType::NotGhost,
            ElementKind::Regular,
        ))?;
        Ok(Self {
            dumpable,
            spatial_dimension,
            route: None,
        })
    }

    /// Install the dump route of a model built around this one (the
    /// cohesive model's dumpers, [`DumpRoute`]); replaces a previous one.
    pub fn set_route(&mut self, route: DumpRoute) {
        self.route = Some(route);
    }

    /// Upstream `SolidMechanicsModel::setTimeStep`: `mesh.getDumper()
    /// .setTimeStep(dt)`, the **default** dumper only (the others keep
    /// their own time step and write no `.pvd` unless given one; found by
    /// the SMM dump differential, `fixtures/model/smm2/dump_explicit_2d`).
    pub(super) fn set_time_step(&mut self, dt: Real) -> Result<()> {
        self.dumpable.set_time_step(dt)
    }

    /// Spatial dimension used for elemental fields.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }
}

/// Live model state for one dump.
struct Resolver<'a> {
    physics: &'a SolidMechanicsPhysics,
    dofs: &'a DofManager,
}

/// One law's `Real` internal on one `(type, ghost)` block, with its support
/// (upstream `ConstitutiveLaw::flattenInternal`,
/// `constitutive_law_tmpl.hh:529-578`: `internal_field.getFEEngine()` and
/// `internal_field.getFilter(type, ghost)`; CL-001).
struct LawInternalBlock {
    /// The field's FE engine (`None`: the model's default engine).
    engine: Option<String>,
    /// `true` if `filter` is the field's own element filter.
    own_filter: bool,
    /// The field's element filter of the block (its own, else the law's).
    filter: Vec<Idx>,
    nb_component: usize,
    values: Vec<Real>,
}

impl LawInternalBlock {
    /// Integration points per element of the field's engine.
    fn nb_quad(
        &self,
        physics: &SolidMechanicsPhysics,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<usize> {
        physics
            .base
            .fems
            .get(self.engine.as_deref().unwrap_or(""))?
            .nb_integration_points(ty, ghost)
    }
}

/// The block of `law`'s `Real` internal `name` on `(ty, ghost)` with its
/// support, `None` when the law has no such block or the field's filter
/// (its own, else the law's) has no block of that type.
fn law_internal_block(
    law: &dyn Material,
    name: &str,
    ty: ElementType,
    ghost: GhostType,
) -> Option<LawInternalBlock> {
    let mut out = None;
    let law_filter = &law.base().element_filter;
    law.for_each_internal(&mut |n, f| {
        if n != name || out.is_some() {
            return;
        }
        // absent block → none; a protocol violation panics (`borrow`)
        // instead of being taken for absence
        if let InternalFieldRef::Real(f) = f
            && let Some(a) = f.values().as_map().get(ty, ghost)
        {
            let own = f.element_filter();
            let Ok(filter) = own.unwrap_or(law_filter).get(ty, ghost) else {
                return;
            };
            out = Some(LawInternalBlock {
                engine: f.fe_engine_id().map(str::to_owned),
                own_filter: own.is_some(),
                filter: filter.as_slice().to_vec(),
                nb_component: f.nb_component(),
                values: a.borrow().as_slice().to_vec(),
            });
        }
    });
    out
}

impl Resolver<'_> {
    fn nodal_real(&self, name: &str) -> Result<DumpBuffer> {
        let p = self.physics;
        let a = match name {
            "external_force" => p.external_force.as_ref(),
            "internal_force" => p.internal_force.as_ref(),
            "mass" => p.mass.as_ref(),
            "increment" => self
                .dofs
                .family(DOF_ID)
                .ok()
                .and_then(|f| f.increment.as_ref()),
            _ => None,
        }
        .ok_or_else(|| Error::not_found("nodal dump field (not allocated)", name))?
        .read()?;
        Ok(DumpBuffer::Real {
            nb_component: a.nb_component(),
            values: a.as_slice().to_vec(),
        })
    }

    /// Upstream `flattenInternal(name)` for one block
    /// (`ConstitutiveLawsHandler::flattenInternal` →
    /// `ConstitutiveLaw::flattenInternal`): every law that holds the
    /// internal writes, at each element of the field's filter (its own,
    /// else the law's), the values of its points on the field's FE engine
    /// (its own, else the default engine: `nb_quad × nb_component` values
    /// per element); zeros for the elements no law
    /// writes.
    ///
    /// Two laws that write elements of the block with a different number of
    /// values per element (internals of one name on engines with different
    /// point counts, or with different component counts) are refused:
    /// upstream resizes the shared block for the second and reinterprets
    /// the first law's values with the second's stride (UD-1083 (d)).
    fn internal(
        &self,
        mesh: &Mesh,
        law_name: Option<&str>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let mats = &self.physics.materials;
        let ne = mesh.nb_element(ty, ghost);
        // (nb_quad, nb_component): of the first law writing elements, else
        // of the first law holding the block (upstream: the first law
        // allocates, a writing law resizes)
        let mut layout: Option<(usize, usize)> = None;
        let mut writing: Option<(usize, usize)> = None;
        let mut blocks = Vec::new();
        for law in mats.laws() {
            if law_name.is_some_and(|n| n != law.base().name) {
                continue;
            }
            let Some(b) = law_internal_block(law.as_ref(), name, ty, ghost) else {
                continue;
            };
            let nq = if b.engine.is_some() {
                b.nb_quad(self.physics, ty, ghost)?
            } else {
                // the default engine as before CL-001: a type it does not
                // know has no points
                self.physics
                    .base
                    .fems
                    .get("")?
                    .nb_integration_points(ty, ghost)
                    .unwrap_or(0)
            };
            let this = (nq, b.nb_component);
            layout.get_or_insert(this);
            if b.filter.is_empty() {
                continue;
            }
            match writing {
                None => writing = Some(this),
                Some(w) if w != this => {
                    return Err(Error::Unsupported(format!(
                        "dump of internal '{name}' on {ty:?}: laws hold it with {} and {} values \
                         per element ({} × {} and {} × {} points × components; upstream \
                         reinterprets the first law's values, UD-1083)",
                        w.0 * w.1,
                        nq * b.nb_component,
                        w.0,
                        w.1,
                        nq,
                        b.nb_component
                    )));
                }
                Some(_) => {}
            }
            blocks.push(b);
        }
        let Some((nq, c)) = writing.or(layout) else {
            return Err(Error::not_found("internal field", name));
        };
        let chunk = nq * c;
        let mut values = vec![0.0; ne * chunk];
        for b in blocks {
            if chunk == 0 {
                continue;
            }
            if b.values.len() != b.filter.len() * chunk {
                return Err(Error::DimensionMismatch {
                    context: "dump of an internal (values vs filter × points × components)",
                    expected: b.filter.len() * chunk,
                    got: b.values.len(),
                });
            }
            for (local, &el) in b.filter.iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                let dst =
                    values
                        .get_mut(e * chunk..(e + 1) * chunk)
                        .ok_or(Error::InvalidIndex {
                            what: "element filter of a dumped internal",
                            value: i64::from(el),
                        })?;
                dst.copy_from_slice(&b.values[local * chunk..(local + 1) * chunk]);
            }
        }
        Ok(DumpBuffer::Real {
            nb_component: c,
            values,
        })
    }
}

impl FieldResolver for Resolver<'_> {
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
            (FieldSource::ModelElemental(n), Some(t)) if n == "material_index" => {
                let a = self.physics.materials.law_index_map().get(t, ghost)?;
                Ok(DumpBuffer::Int {
                    nb_component: 1,
                    values: a.as_slice().to_vec(),
                })
            }
            (FieldSource::Internal { law, name }, Some(t)) => {
                self.internal(mesh, law.as_deref(), name, t, ghost)
            }
            _ => Err(Error::not_found(
                "solid mechanics dump field",
                field.name.clone(),
            )),
        }
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        let mats = &self.physics.materials;
        let idx = mats.law_index_map().get(ty, ghost)?;
        let ne = mesh.nb_element(ty, ghost);
        let mut out = Vec::with_capacity(ne);
        for e in 0..ne {
            let i: Idx = idx.as_slice().get(e).copied().unwrap_or(-1);
            let Some(i) = akantu_core::decode_idx(i)? else {
                out.push(0.0);
                continue;
            };
            let v = mats.law(i)?.get_param(name)?;
            out.push(match v {
                akantu_parser::ParamValue::Real(x) => x,
                akantu_parser::ParamValue::Bool(b) => {
                    if b {
                        1.0
                    } else {
                        0.0
                    }
                }
                akantu_parser::ParamValue::Int(x) => x as Real,
                other => {
                    return Err(Error::invalid(format!(
                        "law parameter '{name}' is not numeric: {other:?}"
                    )));
                }
            });
        }
        Ok(out)
    }
}

/// This model's fields, and another model's for the descriptors that
/// model created ([`SolidMechanicsModel::dump_with_fields_of`]).
struct Routed<'a> {
    own: Resolver<'a>,
    other: &'a dyn FieldResolver,
    owns: &'a dyn Fn(&FieldDescriptor) -> bool,
}

impl FieldResolver for Routed<'_> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        if (self.owns)(field) {
            self.other.resolve(mesh, field, ty, ghost)
        } else {
            self.own.resolve(mesh, field, ty, ghost)
        }
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        self.own.law_parameter(mesh, ty, ghost, name)
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        self.own.variable(name)
    }
}

impl SolidMechanicsModel {
    fn is_internal(&self, name: &str) -> bool {
        self.physics.materials.laws().iter().any(|l| {
            let mut found = false;
            l.for_each_internal(&mut |n, _| found |= n == name);
            found
        })
    }

    /// Field descriptor of a dump field name, resolved from the live model
    /// at each dump (UD-683, category C: a known model array that does not
    /// exist yet, e.g. `mass` before the first lumped assembly, is accepted
    /// and read when it is dumped).
    ///
    /// A name that can never be resolved is refused here, at registration
    /// (UD-683, category A): neither a model field, nor an
    /// internal of a law, nor elemental mesh data of a dumpable type.
    /// Upstream's `addDumpGroupFieldToDumper` refuses an unknown name too
    /// (`mesh.createFieldFromAttachedData<Int>` throws, `model.cc:255-293`)
    /// and the dumper stays usable; recording it would make every later
    /// dump of that dumper fail. (Mesh data of a type without a VTK
    /// representation — strings, element references — is silently not
    /// added upstream; the port refuses it as well.)
    fn descriptor(
        &self,
        name: &str,
        group: Option<&str>,
        padding: bool,
    ) -> Result<FieldDescriptor> {
        let d = solid_mechanics_field(name, group, padding, |n| self.is_internal(n))?;
        if let FieldSource::Mesh(n) = &d.source {
            let mesh = self.physics.base.mesh.read()?;
            let data = mesh.data();
            let dumpable = data.has_elemental(n)
                && matches!(
                    data.type_code(n)?,
                    MeshDataTypeCode::Int
                        | MeshDataTypeCode::UInt
                        | MeshDataTypeCode::Real
                        | MeshDataTypeCode::Bool
                );
            if !dumpable {
                return Err(Error::not_found(
                    "dump field",
                    format!(
                        "{n} (neither a solid mechanics model field, nor an internal of a \
                         material, nor numeric elemental mesh data; upstream \
                         addDumpFieldToDumper throws, UD-683)"
                    ),
                ));
            }
        }
        Ok(d)
    }

    /// The field descriptor upstream's `createNodalField*` /
    /// `createElementalField` of this model would choose for `name`
    /// (see `descriptor`), for a dumper outside the model: upstream's
    /// coupler dumpers ask the solid model for the fields the contact model
    /// does not know (`coupler_solid_contact_tmpl.hh:270-334`; the
    /// couplers' dumpers, `akantu_couplers::dump`).
    ///
    /// # Errors
    /// See `descriptor` (UD-683).
    pub fn dump_field_descriptor(
        &self,
        name: &str,
        group: Option<&str>,
        padding: bool,
    ) -> Result<FieldDescriptor> {
        self.descriptor(name, group, padding)
    }

    /// The live field values of this model for a dumper outside the model
    /// (the resolver its own dumpers use; see
    /// [`SolidMechanicsModel::dump_field_descriptor`]).
    #[must_use]
    pub fn field_resolver(&self) -> impl FieldResolver + '_ {
        Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        }
    }

    /// Upstream `onDump()` with the `BeforeDumpEvent` of every `dump`
    /// variant (materials: Cauchy stresses in finite deformation; user
    /// handlers), for a dump through a dumper outside the model (upstream
    /// `CouplerSolidContact::dump` calls `solid->onDump()`).
    ///
    /// # Errors
    /// Errors of the event handlers.
    pub fn on_dump(&mut self) -> Result<()> {
        self.before_dump()
    }

    /// Upstream `addDumpField(name)` (default dumper, no padding).
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, false)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldVector(name)` (padding to 3 components).
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, true)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldTensor(name)` (padding).
    pub fn add_dump_field_tensor(&mut self, name: &str) -> Result<()> {
        self.add_dump_field_vector(name)
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, false)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, name)`.
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, true)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpFieldTensorToDumper(dumper, name)` (padding, as
    /// `addDumpFieldVectorToDumper`).
    pub fn add_dump_field_tensor_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_field_vector_to_dumper(dumper, name)
    }

    /// Upstream `addDumpGroupFieldToDumper(dumper, name, group, …, padding)`
    /// (`model.cc:255-293`): the field `name` restricted to element group
    /// `group` (all elements for `"all"`) on the dumper upstream's
    /// `Mesh::getGroupDumper(dumper, group)` returns:
    ///
    /// * `group = "all"`: the model's dumper `dumper` (upstream: the
    ///   mesh's; the model's dumpers are the mesh's);
    /// * otherwise the dumper `dumper` of the element group (the mesh
    ///   domain's group-dumper registry, `SharedMeshDumpers`, shared by every
    ///   model on the mesh; ADR "group-dumper registry"), e.g. its default
    ///   `paraview_<group>`;
    /// * port convenience (no upstream counterpart): a dumper registered on
    ///   the model by [`SolidMechanicsModel::register_group_dumper`] (the
    ///   model's own dumper named `dumper`) takes precedence over the
    ///   group's dumpers.
    ///
    /// # Errors
    /// Unknown group or dumper (upstream: an exception), a name that can
    /// never be resolved (UD-683), a duplicate field name.
    pub fn add_dump_group_field_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        let d = self.descriptor(name, Some(group), padding)?;
        if group == ALL_GROUP || self.dumpers.dumpable.dumper(dumper).is_ok() {
            return self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d);
        }
        self.with_group_dumpable(group, |g| g.add_dump_field_to_dumper(dumper, d))?
    }

    /// Upstream `addDumpGroupField(name, group)` (`model.cc:155-161`): the
    /// field on the **default** dumper of element group `group`
    /// (`paraview_<group>` unless changed), no padding.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_group_field(&mut self, name: &str, group: &str) -> Result<()> {
        let dumper = self.group_default_dumper_name(group)?;
        self.add_group_field(&dumper, name, group, false)
    }

    /// Upstream `addDumpGroupFieldVector(name, group)` (`model.cc:187-192`):
    /// as [`SolidMechanicsModel::add_dump_group_field`], padded to 3
    /// components.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_group_field_vector(&mut self, name: &str, group: &str) -> Result<()> {
        let dumper = self.group_default_dumper_name(group)?;
        self.add_group_field(&dumper, name, group, true)
    }

    /// Upstream `addDumpGroupFieldVectorToDumper(dumper, name, group)`
    /// (`model.cc:195-200`): [`SolidMechanicsModel::add_dump_group_field_to_dumper`]
    /// with padding.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::add_dump_group_field_to_dumper`].
    pub fn add_dump_group_field_vector_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
    ) -> Result<()> {
        self.add_dump_group_field_to_dumper(dumper, name, group, true)
    }

    /// Upstream `removeDumpGroupField(name, group)` (`model.cc:164-169`):
    /// from the default dumper of element group `group`.
    ///
    /// # Errors
    /// Unknown group, or no field `name` on that dumper.
    pub fn remove_dump_group_field(&mut self, name: &str, group: &str) -> Result<()> {
        let dumper = self.group_default_dumper_name(group)?;
        self.remove_dump_group_field_from_dumper(&dumper, name, group)
    }

    /// Upstream `removeDumpGroupFieldFromDumper(dumper, name, group)`
    /// (`model.cc:172-177`: `group.removeDumpFieldFromDumper(dumper,
    /// name)`), on the group's dumper `dumper` (or the model's own dumper
    /// of that name registered by
    /// [`SolidMechanicsModel::register_group_dumper`], as
    /// [`SolidMechanicsModel::add_dump_group_field_to_dumper`]).
    ///
    /// # Errors
    /// Unknown group or dumper, or no field `name` on that dumper.
    pub fn remove_dump_group_field_from_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
    ) -> Result<()> {
        if group != ALL_GROUP && self.dumpers.dumpable.dumper(dumper).is_ok() {
            return self
                .dumpers
                .dumpable
                .remove_dump_field_from_dumper(dumper, name);
        }
        self.with_group_dumpable(group, |g| g.remove_dump_field_from_dumper(dumper, name))?
    }

    /// The group's field on `dumper` of its own dumpers (the default-dumper
    /// variants: never the model's dumpers).
    fn add_group_field(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        let d = self.descriptor(name, Some(group), padding)?;
        self.with_group_dumpable(group, |g| g.add_dump_field_to_dumper(dumper, d))?
    }

    fn group_default_dumper_name(&self, group: &str) -> Result<String> {
        self.with_group_dumpable(group, |g| {
            g.default_dumper_name()
                .map(str::to_owned)
                .ok_or_else(|| Error::not_found("default dumper of element group", group))
        })?
    }

    /// The dumpers of element group `group` (upstream: the `ElementGroup` is
    /// `Dumpable`, its constructor registers the default ParaView dumper
    /// `paraview_<group>`): the mesh domain's group-dumper registry,
    /// created on first access and shared by every model on the mesh (ADR
    /// "group-dumper registry"). Register further dumpers of the group here
    /// (upstream `group.registerDumper<…>(…)`); fields of the model are
    /// added with [`SolidMechanicsModel::add_dump_group_field_to_dumper`].
    ///
    /// # Errors
    /// No element group `group`; the registry is in use (re-entrant call).
    pub fn with_group_dumpable<R>(
        &self,
        group: &str,
        f: impl FnOnce(&mut Dumpable) -> R,
    ) -> Result<R> {
        let mesh_h = self.physics.base.mesh.clone();
        let registry = SharedMeshDumpers::of(&mesh_h)?;
        let mesh = mesh_h.read()?;
        let mut reg = registry.lock()?;
        Ok(f(reg.element_group(&mesh, group)?))
    }

    /// Upstream `getGroupDumper(group)` (`model.cc:139-142`): the default
    /// dumper of element group `group`, lent to `f`.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::with_group_dumpable`]; a group without
    /// default dumper.
    pub fn with_group_dumper<R>(
        &self,
        group: &str,
        f: impl FnOnce(&mut dyn Dumper) -> R,
    ) -> Result<R> {
        self.with_group_dumpable(group, |g| g.default_dumper_mut().map(f))?
    }

    /// Upstream `setGroupDirectory(directory, group)` (`model.cc:126-130`):
    /// the directory of the default dumper of element group `group`.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::with_group_dumpable`].
    pub fn set_group_directory(&mut self, directory: &std::path::Path, group: &str) -> Result<()> {
        self.with_group_dumpable(group, |g| g.set_directory(directory))?
    }

    /// Upstream `setGroupDirectory(directory)` (`model.cc:119-123`): the
    /// directory of the default dumper of every element group.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::with_group_dumpable`].
    pub fn set_all_groups_directory(&mut self, directory: &std::path::Path) -> Result<()> {
        for group in self.element_group_names()? {
            self.set_group_directory(directory, &group)?;
        }
        Ok(())
    }

    /// Upstream `setGroupBaseName(basename, group)` (`model.cc:133-137`):
    /// the file base name of the default dumper of element group `group`.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::with_group_dumpable`].
    pub fn set_group_base_name(&mut self, basename: &str, group: &str) -> Result<()> {
        self.with_group_dumpable(group, |g| g.set_base_name(basename))?
    }

    /// Upstream `dumpGroup(group)` (`model.cc:100-103`): the default dumper
    /// of element group `group` with the model's live fields. As upstream
    /// (`Model::dumpGroup` is not `SolidMechanicsModel::dump`), no
    /// `BeforeDumpEvent` is sent.
    ///
    /// # Errors
    /// Unknown group; the dump's errors.
    pub fn dump_group(&mut self, group: &str) -> Result<()> {
        self.dump_group_impl(group, None)
    }

    /// Upstream `dumpGroup(group, dumper)` (`model.cc:106-110`): the dumper
    /// `dumper` of element group `group`.
    ///
    /// # Errors
    /// Unknown group or dumper; the dump's errors.
    pub fn dump_group_named(&mut self, group: &str, dumper: &str) -> Result<()> {
        self.dump_group_impl(group, Some(dumper))
    }

    /// Upstream `dumpGroup()` (`model.cc:113-117`): the default dumper of
    /// every element group, in name order (upstream iterates its
    /// `std::map` of groups).
    ///
    /// # Errors
    /// The first failing group's error.
    pub fn dump_all_groups(&mut self) -> Result<()> {
        for group in self.element_group_names()? {
            self.dump_group_impl(&group, None)?;
        }
        Ok(())
    }

    fn element_group_names(&self) -> Result<Vec<String>> {
        let mesh = self.physics.base.mesh.read()?;
        Ok(mesh
            .groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect())
    }

    fn dump_group_impl(&mut self, group: &str, dumper: Option<&str>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let registry = SharedMeshDumpers::of(&mesh_h)?;
        let mesh = mesh_h.read()?;
        let r = Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        };
        let mut reg = registry.lock()?;
        let g = reg.element_group(&mesh, group)?;
        match dumper {
            None => g.dump(&mesh, &r, None, None),
            Some(name) => g.dump_named(name, &mesh, &r, None, None),
        }
    }

    /// Upstream `removeDumpField(name)` (default dumper).
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.remove_dump_field(name)
    }

    /// Upstream `setBaseName(name)` (default dumper).
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.set_base_name(name)
    }

    /// Upstream `setBaseNameToDumper(dumper, name)`.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.dumpers.dumpable.set_base_name_to_dumper(dumper, name)
    }

    /// Upstream `setDirectory(dir)` (default dumper).
    pub fn set_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        self.dumpers.dumpable.set_directory(dir)
    }

    /// Upstream `setDirectoryToDumper(dumper, dir)`.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &std::path::Path) -> Result<()> {
        self.dumpers.dumpable.set_directory_to_dumper(dumper, dir)
    }

    /// Register a ParaView dumper restricted to an element group (upstream
    /// `ElementGroup::registerDumper` used by `dumpGroup`).
    pub fn register_group_dumper(&mut self, group: &str, file_name: Option<&str>) -> Result<()> {
        self.dumpers
            .dumpable
            .register_paraview(group, Some(file_name.unwrap_or(group)), false);
        self.dumpers.dumpable.add_dump_mesh_to_dumper(
            group,
            MeshSelection::new(None, GhostType::NotGhost, ElementKind::NotDefined)
                .filtered(MeshFilter::ElementGroup(group.to_owned())),
        )
    }

    /// Upstream `EventManager::sendEvent(BeforeDumpEvent())` of every
    /// `dump` variant: materials (`onDump`: Cauchy stresses in finite
    /// deformation) and user handlers, newest first
    /// ([`SolidMechanicsModel::send_event`]).
    fn before_dump(&mut self) -> Result<()> {
        self.send_event(SolidMechanicsModelEvent::BeforeDump)
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` (default
    /// dumper): `BeforeDumpEvent` (materials and user handlers, newest
    /// first), then the dump.
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.before_dump()?;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let r = Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        };
        self.dumpers.dumpable.dump(&mesh, &r, time, step)
    }

    /// Upstream `dump(dumper_name, …)`. A dumper of a model built around
    /// this one (the cohesive model's `"cohesive elements"` and `"facets"`)
    /// is dumped through that model's route ([`DumpRoute`]), on its own mesh
    /// with its own fields, as upstream.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        if let Some(route) = self.dumpers.route
            && let Some(r) = route(self, dumper, time, step)
        {
            return r;
        }
        self.before_dump()?;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let r = Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        };
        self.dumpers
            .dumpable
            .dump_named(dumper, &mesh, &r, time, step)
    }

    /// Upstream `onDump()` then `mesh.dump(…)` of the default dumper
    /// (`dumper = None`) or of `dumper`, where some registered fields were
    /// created by another model (upstream: each field object reads the
    /// array of the model that created it; e.g. the contact fields a
    /// `CouplerSolidCohesiveContact` registers on the mesh's default
    /// dumper, which is this model's, `coupler_solid_contact_tmpl.hh:
    /// 270-350`): the fields for which `owns` holds are read from `other`,
    /// every other field and every law parameter from this model. Dumpers
    /// of a model built around this one ([`DumpRoute`]) are not reached
    /// here.
    ///
    /// # Errors
    /// Errors of the event handlers, of the dump and of the resolvers;
    /// unknown dumper.
    pub fn dump_with_fields_of(
        &mut self,
        other: &dyn FieldResolver,
        owns: &dyn Fn(&FieldDescriptor) -> bool,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.before_dump()?;
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let r = Routed {
            own: Resolver {
                physics: &self.physics,
                dofs: &self.solver.dof_manager,
            },
            other,
            owns,
        };
        match dumper {
            None => self.dumpers.dumpable.dump(&mesh, &r, time, step),
            Some(name) => self
                .dumpers
                .dumpable
                .dump_named(name, &mesh, &r, time, step),
        }
    }

    /// Upstream `extrapolateInternal(id, element, point, extrapolated)`
    /// (SMM-029): not a real extrapolation — every column of `out` receives
    /// the value of the first quadrature point of the element whose norm
    /// is positive (point 0 when all are zero), as upstream (which prints a
    /// warning). `out` has `nb_component` rows and any number of columns,
    /// column-major. For an `id` that is not an internal of the law, `out`
    /// is zero-filled (upstream `extrapolated.fill(0.)`).
    ///
    /// Upstream's copy loop runs over `extrapolated.size()` (rows × cols)
    /// *columns*, out of bounds for `nb_component > 1` (UD-213, category
    /// A); the port writes each column once (the evident intent).
    ///
    /// The points per element are those of the internal's FE engine, as
    /// upstream (`getInternal(name).getFEEngine()`; CL-001).
    /// An internal on its own element filter is read at the
    /// element's position in that filter; upstream indexes it with the
    /// law-local index (`convertToLocalElement`), another element's values
    /// or out of range (UD-1083 (c)).
    ///
    /// # Errors
    /// An unknown law, element or FE engine, an element outside an
    /// internal's own element filter, or an `out` whose length is not a
    /// multiple of the internal's number of components.
    pub fn extrapolate_internal(
        &self,
        material: usize,
        id: &str,
        element: akantu_core::Element,
        out: &mut [Real],
    ) -> Result<()> {
        let law = self.physics.materials.law(material)?;
        let local = self
            .physics
            .materials
            .local_numbering_map()
            .element(element)?[0];
        let Some(b) = law_internal_block(law, id, element.ty(), element.ghost_type()) else {
            out.fill(0.0); // upstream: `extrapolated.fill(0.)` when not an internal
            return Ok(());
        };
        let nq = b.nb_quad(&self.physics, element.ty(), element.ghost_type())?;
        let c = b.nb_component;
        let v = &b.values;
        if !out.len().is_multiple_of(c) {
            return Err(Error::DimensionMismatch {
                context: "extrapolateInternal: output rows",
                expected: c,
                got: out.len(),
            });
        }
        let l = if b.own_filter {
            b.filter
                .iter()
                .position(|&e| e == element.index())
                .ok_or_else(|| {
                    Error::invalid(format!(
                        "extrapolateInternal: {element} is not on the own element filter of \
                         internal '{id}'"
                    ))
                })?
        } else {
            akantu_core::try_to_usize(local)?
        };
        let vals = v
            .get(l * nq * c..(l + 1) * nq * c)
            .ok_or(Error::OutOfRange {
                what: "law-local element",
                index: l,
                len: v.len() / (nq * c).max(1),
            })?;
        let mut index = 0;
        for j in 0..nq {
            let col = &vals[j * c..(j + 1) * c];
            // Vector::norm() of a dynamic vector: sqrt of the sequential sum
            let mut s = 0.0;
            for x in col {
                s += x * x;
            }
            if s.sqrt() > 0.0 {
                index = j;
                break;
            }
        }
        for col in out.chunks_exact_mut(c) {
            col.copy_from_slice(&vals[index * c..(index + 1) * c]);
        }
        Ok(())
    }
}
