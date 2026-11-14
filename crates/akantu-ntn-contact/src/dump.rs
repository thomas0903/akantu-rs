// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/{ntn_base_contact.cc, ntn_contact.cc, ntrf_contact.cc}
// (constructor dumper registration, addDumpFieldToDumper) and the
// addDumpFieldToDumper chain of ntn_base_friction.cc,
// friction_regularisations/* and friction_laws/*.

//! Text dumper of a contact and its friction (TASN-001/004).
//!
//! Upstream every `NTNBaseContact` registers a default IOHelper text dumper
//! `text_all` (file name = contact id) on the boundary elements of its slave
//! nodes (`addDumpFilteredMesh(mesh, slave_elements, slaves, dim − 1,
//! _not_ghost, _ek_regular)`), and `addDumpFieldToDumper(dumper, field)`
//! adds, by name, the model nodal fields restricted to the slave nodes
//! (`displacement`, `mass`, `velocity`, `acceleration`, `external_force`,
//! `internal_force`, `blocked_dofs`, `increment`) or the per-contact-node
//! arrays of the contact (`normal`, `contact_pressure`, `is_in_contact`,
//! `lumped_boundary_slave`, `impedance`, `lumped_boundary_master`) and of the
//! friction (see [`NtnFriction::friction_field`]); an unknown name is
//! ignored with a message.
//!
//! [`ContactDumper`] owns the [`Dumpable`] registry (akantu-io) and resolves
//! the fields from the contact, the friction and a [`ModelState`] at dump
//! time; the filtered mesh is re-registered when the contact's node list has
//! changed (upstream keeps live references to the slave arrays).
//!
//! Difference (C, documented): upstream's `global_friction_traction(_old)`
//! fields are per mesh node but dumped on the contact-node mesh (IOHelper
//! writes the first `nb_contact_nodes` tuples of the global array); here they
//! are written at the slave nodes.

use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_io::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshFilter, MeshSelection, Padding,
};
use akantu_mesh::Mesh;

use crate::contact::NtnBaseContact;
use crate::friction::NtnFriction;
use crate::state::ModelState;

/// Model nodal fields of `NTNBaseContact::addDumpFieldToDumper`.
const MODEL_FIELDS: [&str; 8] = [
    "displacement",
    "mass",
    "velocity",
    "acceleration",
    "external_force",
    "internal_force",
    "blocked_dofs",
    "increment",
];

/// Name of the default dumper.
pub const TEXT_DUMPER: &str = "text_all";

/// The `text_all` dumper of a contact.
#[derive(Debug)]
pub struct ContactDumper {
    dumpable: Dumpable,
    registered_nodes: Option<Vec<Idx>>,
}

struct Resolver<'a> {
    contact: &'a NtnBaseContact,
    friction: Option<&'a NtnFriction>,
    state: &'a ModelState<'a>,
}

fn source_name(field: &FieldDescriptor) -> &str {
    match &field.source {
        FieldSource::ModelNodal(n) => n,
        _ => &field.name,
    }
}

impl Resolver<'_> {
    /// Scatter per-contact-node values to the slave nodes of an all-node
    /// buffer.
    fn scatter<T: Copy + Default>(&self, values: &[T], nbc: usize) -> Result<Vec<T>> {
        let nb_nodes = self.state.nb_nodes();
        let mut out = vec![T::default(); nb_nodes * nbc];
        for (i, &s) in self.contact.slaves().as_slice().iter().enumerate() {
            let s = usize::try_from(s).map_err(|_| Error::InvalidIndex {
                what: "node",
                value: i64::from(s),
            })?;
            out[s * nbc..(s + 1) * nbc].copy_from_slice(&values[i * nbc..(i + 1) * nbc]);
        }
        Ok(out)
    }
}

impl FieldResolver for Resolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        if ty.is_some() {
            return Err(Error::invalid("contact dumper: nodal fields only"));
        }
        let name = source_name(field);
        let dim = self.state.spatial_dimension;
        let st = self.state;
        let real = |values: &[Real], nb_component: usize| DumpBuffer::Real {
            nb_component,
            values: values.to_vec(),
        };
        match name {
            "displacement" => return Ok(real(&st.displacement, dim)),
            "mass" => return Ok(real(&st.mass, dim)),
            "velocity" => return Ok(real(&st.velocity, dim)),
            "acceleration" => return Ok(real(&st.acceleration, dim)),
            "external_force" => return Ok(real(&st.external_force, dim)),
            "internal_force" => return Ok(real(&st.internal_force, dim)),
            "increment" => return Ok(real(&st.increment, dim)),
            "blocked_dofs" => {
                return Ok(DumpBuffer::Bool {
                    nb_component: dim,
                    values: st.blocked_dofs.to_vec(),
                });
            }
            "is_in_contact" => {
                return Ok(DumpBuffer::Bool {
                    nb_component: 1,
                    values: self.scatter(self.contact.is_in_contact().as_slice(), 1)?,
                });
            }
            "is_sticking" => {
                if let Some(f) = self.friction {
                    return Ok(DumpBuffer::Bool {
                        nb_component: 1,
                        values: self.scatter(f.sticking_field(), 1)?,
                    });
                }
            }
            _ => {}
        }
        if let Some((v, nbc)) = self.contact.contact_field(name) {
            return Ok(real(&self.scatter(v, nbc)?, nbc));
        }
        if let Some((v, nbc)) = self.friction.and_then(|f| f.friction_field(name)) {
            if name.starts_with("global_") {
                return Ok(real(v, nbc));
            }
            return Ok(real(&self.scatter(v, nbc)?, nbc));
        }
        Err(Error::not_found("contact dump field", name))
    }
}

impl ContactDumper {
    /// Upstream `registerDumper<DumperText>("text_all", id, true)`.
    #[must_use]
    pub fn new(contact: &NtnBaseContact) -> Self {
        let mut dumpable = Dumpable::new();
        dumpable.register_text(TEXT_DUMPER, Some(contact.id()), true);
        Self {
            dumpable,
            registered_nodes: None,
        }
    }

    /// The dumper registry (directory, base name, other dumpers).
    pub fn dumpable_mut(&mut self) -> &mut Dumpable {
        &mut self.dumpable
    }

    /// Upstream `addDumpFieldToDumper(dumper_name, field_id)` of the contact
    /// (and, with `friction`, of the friction chain). Returns `false` for an
    /// unknown name, which is ignored with a warning as upstream ("Could not
    /// add field ... Just ignored it.").
    ///
    /// # Errors
    /// Unknown dumper or a field registered twice.
    pub fn add_dump_field_to_dumper(
        &mut self,
        dumper_name: &str,
        field_id: &str,
        contact: &NtnBaseContact,
        friction: Option<&NtnFriction>,
    ) -> Result<bool> {
        let value_type = match field_id {
            "blocked_dofs" | "is_in_contact" | "is_sticking" => FieldValueType::Bool,
            _ => FieldValueType::Real,
        };
        let known = MODEL_FIELDS.contains(&field_id)
            || field_id == "is_in_contact"
            || contact.contact_field(field_id).is_some()
            || friction
                .is_some_and(|f| field_id == "is_sticking" || f.friction_field(field_id).is_some());
        if !known {
            log::warn!("Could not add field '{field_id}' to the dumper. Just ignored it.");
            return Ok(false);
        }
        self.dumpable.add_dump_field_to_dumper(
            dumper_name,
            FieldDescriptor {
                name: field_id.to_owned(),
                source: FieldSource::ModelNodal(field_id.to_owned()),
                support: FieldSupport::Nodal,
                value_type,
                group: None,
                padding: Padding::None,
                homogenize: false,
            },
        )?;
        Ok(true)
    }

    /// [`Self::add_dump_field_to_dumper`] on `text_all`.
    ///
    /// # Errors
    /// See [`Self::add_dump_field_to_dumper`].
    pub fn add_dump_field(
        &mut self,
        field_id: &str,
        contact: &NtnBaseContact,
        friction: Option<&NtnFriction>,
    ) -> Result<bool> {
        self.add_dump_field_to_dumper(TEXT_DUMPER, field_id, contact, friction)
    }

    fn refresh_mesh(&mut self, contact: &NtnBaseContact) -> Result<()> {
        let nodes = contact.slaves().as_slice().to_vec();
        if self.registered_nodes.as_ref() == Some(&nodes) {
            return Ok(());
        }
        let dim = contact.spatial_dimension();
        let mut elements: ElementTypeMapArray<Idx> =
            ElementTypeMapArray::new(format!("{}:slave_elements", contact.id()));
        for el in contact.slave_elements().iter() {
            let (ty, ghost) = (el.ty(), el.ghost_type());
            if !elements.exists(ty, ghost) {
                elements.alloc(0, 1, ty, ghost, 0)?;
            }
            elements.get_mut(ty, ghost)?.push_value(el.index());
        }
        let selection =
            MeshSelection::new(Some(dim - 1), GhostType::NotGhost, ElementKind::Regular).filtered(
                MeshFilter::Explicit {
                    elements: Box::new(elements),
                    nodes: nodes.clone(),
                },
            );
        if self.registered_nodes.is_some() {
            self.dumpable
                .dumper_mut(TEXT_DUMPER)?
                .unregister_field("position")?;
        }
        self.dumpable
            .add_dump_mesh_to_dumper(TEXT_DUMPER, selection)?;
        self.registered_nodes = Some(nodes);
        Ok(())
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` of `text_all`.
    ///
    /// # Errors
    /// Field resolution or I/O failure.
    pub fn dump(
        &mut self,
        contact: &NtnBaseContact,
        friction: Option<&NtnFriction>,
        state: &ModelState<'_>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.refresh_mesh(contact)?;
        let resolver = Resolver {
            contact,
            friction,
            state,
        };
        self.dumpable
            .dump_named(TEXT_DUMPER, state.mesh, &resolver, time, step)
    }
}
