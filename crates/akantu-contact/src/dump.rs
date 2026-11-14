// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/contact_mechanics/
// contact_mechanics_model.cc (constructor: dumper registration;
// createNodalFieldReal/Int/Bool), src/model/model.cc (Model::setBaseName,
// setDirectory, addDumpField*, addDumpGroupFieldToDumper, dump),
// src/mesh/mesh.cc (createFieldFromAttachedData).

//! Dumps of the contact model (CONT-018 dumper, D-078).
//!
//! Upstream's constructor registers a ParaView dumper named
//! `contact_mechanics` **on the mesh** (the main mesh, or the facet mesh of
//! the solid–cohesive–contact coupler), file base name = the model id,
//! made the mesh's default, with the regular elements of the model's
//! dimension, not ghost (`contact_mechanics_model.cc:48-51`). The model's
//! `setBaseName`, `addDumpField*` and `dump` act on the mesh's default
//! dumper (`model.cc:156-313`); in the port the model owns that dumper
//! (UD-855: the port's models own separate dumpers, the couplers' dumps are
//! documented in `akantu-couplers`).
//!
//! A field name is resolved as `Model::addDumpGroupFieldToDumper` does:
//! the real nodal fields (`contact_force` = the contact model's internal
//! force, `normal_force`, `tangential_force`, `normals`, `tangents`,
//! `gaps`, `areas` = the nodal areas, `tangential_traction`), then the
//! integer one (`contact_state`, the enum's integer value), then the boolean
//! one (`blocked_dofs`, one component); the model has no elemental field
//! (`Model::createElementalField` returns none); then elemental mesh data
//! of type `Int`, then `Real` (`mesh.createFieldFromAttachedData`). A name
//! none of these knows is refused at registration (UD-683, as the solid
//! model). The fields are read from the live model at each dump (the
//! resolver contract of the dumpers).

use std::path::Path;

use akantu_core::enums::ContactState;
use akantu_core::{Array, ElementKind, ElementType, Error, GhostType, Int, Real, Result};
use akantu_io::dumper::group::{ALL_GROUP, create_field_from_attached_data};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshSelection, Padding,
};
use akantu_mesh::Mesh;

use crate::element::contact_state_code;
use crate::fields::ContactFields;
use crate::model::ContactMechanicsModel;

/// Name of the contact model's dumper (upstream `"contact_mechanics"`).
pub const DUMPER: &str = "contact_mechanics";

/// The real nodal dump fields, in upstream's map order (names only).
pub const NODAL_REAL_FIELDS: [&str; 8] = [
    "contact_force",
    "normal_force",
    "tangential_force",
    "normals",
    "tangents",
    "gaps",
    "areas",
    "tangential_traction",
];

/// The contact model's dumper as its constructor registers it (module
/// docs): `contact_mechanics`, file base name `id`, default, the regular
/// elements of dimension `spatial_dimension`, not ghost.
pub(crate) fn new_dumpable(id: &str, spatial_dimension: usize) -> Result<Dumpable> {
    let mut d = Dumpable::new();
    d.register_paraview(DUMPER, Some(id), true);
    d.add_dump_mesh(MeshSelection::new(
        Some(spatial_dimension),
        GhostType::NotGhost,
        ElementKind::Regular,
    ))?;
    Ok(d)
}

/// Where a real nodal dump field lives ([`ContactFields::dump_real`]).
enum RealSource<'a> {
    Shared(&'a akantu_core::SharedArray<Real>),
    Plain(&'a Array<Real>),
}

impl ContactFields {
    /// The real nodal dump field `name` (upstream `createNodalFieldReal`'s
    /// map), if it is one, as a guard: the exportable arrays through their
    /// guards, `tangents` and `tangential_traction` (plain storage)
    /// directly.
    ///
    /// # Errors
    /// An exported array outside a native section of its runtime.
    pub fn dump_real(&self, name: &str) -> Result<Option<RealField<'_>>> {
        Ok(match self.real_source(name) {
            Some(RealSource::Shared(a)) => Some(RealField::Shared(a.read()?)),
            Some(RealSource::Plain(a)) => Some(RealField::Plain(a)),
            None => None,
        })
    }

    /// [`ContactFields::dump_real`] for code that runs inside a native
    /// section (the model's own accessors): the exportable arrays through
    /// `SharedArray::borrow`, which panics on a protocol violation exactly
    /// as the sibling accessors do (`#[track_caller]`).
    ///
    /// # Panics
    /// An exported array outside a native section of its runtime (a
    /// protocol violation of the binding, unreachable through it).
    #[must_use]
    #[track_caller]
    pub fn dump_real_borrowed(&self, name: &str) -> Option<RealField<'_>> {
        match self.real_source(name)? {
            RealSource::Shared(a) => Some(RealField::Shared(a.borrow())),
            RealSource::Plain(a) => Some(RealField::Plain(a)),
        }
    }

    fn real_source(&self, name: &str) -> Option<RealSource<'_>> {
        Some(match name {
            "contact_force" => RealSource::Shared(&self.internal_force),
            "normal_force" => RealSource::Shared(&self.normal_force),
            "tangential_force" => RealSource::Shared(&self.tangential_force),
            "normals" => RealSource::Shared(&self.normals),
            "gaps" => RealSource::Shared(&self.gaps),
            "areas" => RealSource::Shared(&self.nodal_area),
            "tangents" => RealSource::Plain(&self.tangents),
            "tangential_traction" => RealSource::Plain(&self.tangential_tractions),
            _ => return None,
        })
    }

    /// The integer nodal dump field `contact_state` (upstream
    /// `ComputeIntFromEnum<ContactState>`: 0, 1, 2), if `name` is it.
    #[must_use]
    pub fn dump_int(&self, name: &str) -> Option<Vec<Int>> {
        (name == "contact_state").then(|| {
            self.contact_state
                .as_slice()
                .iter()
                .map(|&s: &ContactState| contact_state_code(s))
                .collect()
        })
    }

    /// The boolean nodal dump field `blocked_dofs`, if `name` is it.
    #[must_use]
    pub fn dump_bool(&self, name: &str) -> Option<&Array<bool>> {
        (name == "blocked_dofs").then_some(&self.blocked_dofs)
    }

    /// Whether the contact model provides `name` as a nodal dump field of
    /// type `value_type` (upstream `createNodalField{Real,Int,Bool}`).
    #[must_use]
    pub fn has_dump_field(&self, name: &str, value_type: FieldValueType) -> bool {
        match value_type {
            FieldValueType::Real => NODAL_REAL_FIELDS.contains(&name),
            FieldValueType::Int => name == "contact_state",
            FieldValueType::Bool => name == "blocked_dofs",
            _ => false,
        }
    }

    /// The values of a nodal dump field registered by the contact model.
    fn dump_buffer(&self, name: &str) -> Result<DumpBuffer> {
        if let Some(a) = self.dump_real(name)? {
            return Ok(DumpBuffer::Real {
                nb_component: a.nb_component(),
                values: a.as_slice().to_vec(),
            });
        }
        if let Some(v) = self.dump_int(name) {
            return Ok(DumpBuffer::Int {
                nb_component: 1,
                values: v,
            });
        }
        if let Some(b) = self.dump_bool(name) {
            return Ok(DumpBuffer::Bool {
                nb_component: b.nb_component(),
                values: b.as_slice().to_vec(),
            });
        }
        Err(Error::not_found("contact mechanics dump field", name))
    }
}

/// A real nodal dump field of the contact model.
#[derive(Debug)]
pub enum RealField<'a> {
    /// An exportable array, through its guard.
    Shared(akantu_core::array_shared::ArrayRef<'a, Real>),
    /// A plain array.
    Plain(&'a Array<Real>),
}

impl std::ops::Deref for RealField<'_> {
    type Target = Array<Real>;

    fn deref(&self) -> &Array<Real> {
        match self {
            Self::Shared(a) => a,
            Self::Plain(a) => a,
        }
    }
}

/// The nodal-field name a contact descriptor reads (`None` for a
/// descriptor the contact model does not resolve).
fn contact_source(field: &FieldDescriptor) -> Option<&str> {
    match (&field.source, field.support) {
        (FieldSource::ModelNodal(n), FieldSupport::Nodal) => Some(n.as_str()),
        _ => None,
    }
}

/// The contact model's fields as a dump-time resolver (borrows the fields
/// only, so that the model's dumper can be borrowed mutably meanwhile).
struct FieldsResolver<'a>(&'a ContactFields);

impl FieldResolver for FieldsResolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (contact_source(field), ty) {
            (Some(n), None) => self.0.dump_buffer(n),
            _ => Err(Error::not_found(
                "contact mechanics dump field",
                format!(
                    "{} (registered by another model on the contact model's dumper: dump it \
                     through that model or the coupler, UD-855)",
                    field.name
                ),
            )),
        }
    }
}

/// The contact model's fields first, the other model's (e.g. a coupler's
/// solid) for every other descriptor and for law parameters.
struct Routed<'a> {
    contact: FieldsResolver<'a>,
    other: &'a dyn FieldResolver,
}

impl FieldResolver for Routed<'_> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        if owns(self.contact.0, field) {
            self.contact.resolve(mesh, field, ty, ghost)
        } else {
            self.other.resolve(mesh, field, ty, ghost)
        }
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        self.other.law_parameter(mesh, ty, ghost, name)
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        self.other.variable(name)
    }
}

/// Whether `field` is a nodal field of the contact model of the type it
/// was registered with (the only descriptors the contact model records).
fn owns(fields: &ContactFields, field: &FieldDescriptor) -> bool {
    contact_source(field).is_some_and(|n| fields.has_dump_field(n, field.value_type))
}

impl ContactMechanicsModel {
    /// The descriptor of the contact model's nodal dump field `name` of
    /// type `value_type`, `None` if the model has no such field (upstream
    /// `createNodalField{Real,Int,Bool}` returning a null field). Real
    /// fields are padded to 3 components with `padding` (upstream
    /// `mesh.createNodalField(array, group, 3)`), the others never.
    #[must_use]
    pub fn nodal_dump_field_descriptor(
        &self,
        name: &str,
        value_type: FieldValueType,
        padding: bool,
    ) -> Option<FieldDescriptor> {
        if !self.fields.has_dump_field(name, value_type) {
            return None;
        }
        let padding = if padding && value_type == FieldValueType::Real {
            Padding::To3D
        } else {
            Padding::None
        };
        Some(FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::ModelNodal(name.to_owned()),
            support: FieldSupport::Nodal,
            value_type,
            group: None,
            padding,
            homogenize: false,
        })
    }

    /// Whether the dump field `field` is one of the contact model's (a
    /// nodal field it provides with that type): what a coupler's dumper
    /// asks the contact model to resolve.
    #[must_use]
    pub fn owns_dump_field(&self, field: &FieldDescriptor) -> bool {
        owns(&self.fields, field)
    }

    /// The field descriptor upstream's `Model::addDumpGroupFieldToDumper`
    /// chooses for `name` on this model (module docs: real, integer,
    /// boolean nodal fields, then `Int`, `Real` elemental mesh data).
    ///
    /// # Errors
    /// A name that is none of these (UD-683); errors of the mesh data
    /// lookup (`createFieldFromAttachedData`).
    pub fn dump_field_descriptor(&self, name: &str, padding: bool) -> Result<FieldDescriptor> {
        for ty in [
            FieldValueType::Real,
            FieldValueType::Int,
            FieldValueType::Bool,
        ] {
            if let Some(d) = self.nodal_dump_field_descriptor(name, ty, padding) {
                return Ok(d);
            }
        }
        let mesh_h = self.base.mesh.clone();
        let guard = mesh_h.read()?;
        let mesh = self.part_mesh(&guard)?;
        for ty in [FieldValueType::Int, FieldValueType::Real] {
            if let Some(d) =
                create_field_from_attached_data(mesh, name, ty, ALL_GROUP, ElementKind::Regular)?
            {
                return Ok(d);
            }
        }
        Err(Error::not_found(
            "dump field",
            format!(
                "{name} (neither a contact mechanics model field nor Int / Real elemental mesh \
                 data; upstream addDumpFieldToDumper registers nothing, UD-683)"
            ),
        ))
    }

    /// The mesh the model's dumper writes (its part of the domain).
    fn part_mesh<'a>(&self, guard: &'a Mesh) -> Result<&'a Mesh> {
        match self.part() {
            akantu_mesh::MeshPart::Main => Ok(guard),
            akantu_mesh::MeshPart::Facets => guard
                .mesh_facets()
                .ok_or_else(|| Error::invalid("contact model on the facet mesh: no facet mesh")),
        }
    }

    /// The model's dumpers (upstream: the mesh's; module docs).
    #[must_use]
    pub fn dumpable(&self) -> &Dumpable {
        &self.dumpable
    }

    /// The model's dumpers, mutable: a coupler registers the fields it
    /// resolves itself here (upstream `Model::addDumpGroupFieldToDumper` of
    /// a coupler acts on the mesh's default dumper, the contact model's for
    /// `CouplerSolidContact`).
    pub fn dumpable_mut(&mut self) -> &mut Dumpable {
        &mut self.dumpable
    }

    /// Upstream `addDumpField(name)` (default dumper, no padding).
    ///
    /// # Errors
    /// See [`ContactMechanicsModel::dump_field_descriptor`]; no default
    /// dumper.
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        let d = self.dump_field_descriptor(name, false)?;
        self.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldVector(name)` (real nodal fields padded to 3
    /// components).
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::add_dump_field`].
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        let d = self.dump_field_descriptor(name, true)?;
        self.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldTensor(name)` (padding, as
    /// `addDumpFieldVector`).
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::add_dump_field`].
    pub fn add_dump_field_tensor(&mut self, name: &str) -> Result<()> {
        self.add_dump_field_vector(name)
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::add_dump_field`]; unknown dumper.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.dump_field_descriptor(name, false)?;
        self.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, name)`.
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::add_dump_field_to_dumper`].
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.dump_field_descriptor(name, true)?;
        self.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpFieldTensorToDumper(dumper, name)`.
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::add_dump_field_to_dumper`].
    pub fn add_dump_field_tensor_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_dump_field_vector_to_dumper(dumper, name)
    }

    /// Upstream `removeDumpField(name)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.dumpable.remove_dump_field(name)
    }

    /// Upstream `setBaseName(name)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.dumpable.set_base_name(name)
    }

    /// Upstream `setBaseNameToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.dumpable.set_base_name_to_dumper(dumper, name)
    }

    /// Upstream `setDirectory(dir)` (default dumper; `./paraview` by
    /// default).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_directory(&mut self, dir: &Path) -> Result<()> {
        self.dumpable.set_directory(dir)
    }

    /// Upstream `setDirectoryToDumper(dumper, dir)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &Path) -> Result<()> {
        self.dumpable.set_directory_to_dumper(dumper, dir)
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` (default
    /// dumper; the contact model sends no dump event).
    ///
    /// # Errors
    /// Dump errors; a field another model registered on this dumper (it
    /// must be dumped through that model or the coupler).
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.dump_dumper(None, time, step, None)
    }

    /// Upstream `dump(dumper_name, …)`.
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::dump`]; unknown dumper.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.dump_dumper(Some(dumper), time, step, None)
    }

    /// Dump the default dumper (`dumper = None`) or `dumper` with the
    /// contact model's fields and, for every other registered field and for
    /// law parameters, `other` (upstream: each field object reads the array
    /// of the model that created it; a coupler's dump, `Model::dump` on the
    /// mesh's default dumper, the contact model's for
    /// `CouplerSolidContact`).
    ///
    /// # Errors
    /// As [`ContactMechanicsModel::dump`].
    pub fn dump_with_fields_of(
        &mut self,
        other: &dyn FieldResolver,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.dump_dumper(dumper, time, step, Some(other))
    }

    fn dump_dumper(
        &mut self,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
        other: Option<&dyn FieldResolver>,
    ) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let guard = mesh_h.read()?;
        let mesh = self.part_mesh(&guard)?;
        let own = FieldsResolver(&self.fields);
        let routed;
        let resolver: &dyn FieldResolver = match other {
            None => &own,
            Some(other) => {
                routed = Routed {
                    contact: FieldsResolver(&self.fields),
                    other,
                };
                &routed
            }
        };
        match dumper {
            None => self.dumpable.dump(mesh, resolver, time, step),
            Some(name) => self.dumpable.dump_named(name, mesh, resolver, time, step),
        }
    }
}
