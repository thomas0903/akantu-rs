// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model_couplers/
// coupler_solid_contact_tmpl.hh (createNodalField{Real,Int,Bool},
// createElementalField, dump), coupler_solid_contact.cc and
// coupler_solid_cohesive_contact.cc (constructors: dumper registration),
// coupler_solid_phasefield.cc (constructor, createNodalField*,
// createElementalField, dump), src/model/model.cc (Model::setBaseName,
// setDirectory, addDumpField*, addDumpGroupFieldToDumper, dump,
// dumpGroup). It replaces an earlier stand-in of the examples.

//! Model-level dumpers of the couplers (CPL-001, CPL-003, CPL-005;
//! D-078).
//!
//! # The mesh's default dumper (UD-855)
//!
//! Upstream every model registers its ParaView dumper **on the mesh** and
//! makes it the mesh's default; `Model::setBaseName`, `addDumpField*` and
//! `dump()` act on the mesh's default dumper, i.e. the one of the model
//! constructed **last**, whichever model they are called on. The port's
//! models own their dumpers, so each coupler's model-level calls act on the
//! dumper of the part upstream's mesh default belongs to:
//!
//! | coupler | upstream's mesh default | the port's coupler dumps through |
//! |---|---|---|
//! | [`CouplerSolidContact`] | the contact model's `contact_mechanics` (file `contact_mechanics_model`) | the contact model's dumper (the same) |
//! | [`CouplerSolidCohesiveContact`] | the solid model's `solid_mechanics_model` (file `solid_mechanics_model_cohesive`; the contact model lives on the facet mesh) | the cohesive solid model's default dumper (the same) |
//! | [`CouplerSolidPhaseField`] | the phase-field model's `phase_field` (file `phase_field_model`), filled with the **solid's** fields | the solid model's default dumper, whose base name the coupler sets to `phase_field_model` |
//!
//! So the coupler's calls and those of the part named in the last column
//! share one series, as upstream. The calls of the **other** part act on
//! that part's own dumper (upstream: the same mesh default): e.g. for
//! `CouplerSolidContact` the solid model's `add_dump_field` / `dump` write
//! the solid's own `solid_mechanics_model` series, for
//! `CouplerSolidPhaseField` the phase-field model's write its own
//! `phase_field` series (a documented difference, UD-855). A field a
//! coupler registered for one part on the other part's dumper is resolved
//! by the coupler's dump; dumping that dumper through the part alone
//! refuses it (an error, never a silent omission).
//!
//! # Field resolution order
//!
//! `Model::addDumpGroupFieldToDumper` asks the coupler's
//! `createNodalFieldReal`, then `createNodalFieldInt`, then
//! `createNodalFieldBool`, then `createElementalField`, then the mesh data
//! (`Int`, then `Real`), and each coupler method asks the contact model
//! first, then the solid model (`coupler_solid_contact_tmpl.hh:270-334`):
//! the order is **type-major**: real (contact, solid),
//! integer (contact, solid), boolean (contact, solid), elemental (contact:
//! none; solid), mesh data. `blocked_dofs`, which both models know as a
//! boolean nodal field, is therefore the contact model's (one component,
//! never set by the coupler's BCs). The phase-field coupler asks the solid
//! only (`coupler_solid_phasefield.cc:469-490`; it has no integer nodal
//! field). A name nothing resolves is refused (UD-683).
//!
//! # Dump
//!
//! `coupler.dump(…)` is `solid->onDump()` (the solid's `BeforeDumpEvent`)
//! then `Model::dump(…)` of the mesh's dumper (`coupler_solid_contact_tmpl.hh:
//! 337-368`, `coupler_solid_phasefield.cc:493-527`); each registered field
//! is read from the model that provides it. The couplers' `setTimeStep` is
//! `ModelSolver::setTimeStep` (no dumper time step, so no `.pvd`, unlike
//! the solid model's `setTimeStep`).
//!
//! The couplers' own dumpers (`coupler_solid_contact`,
//! `coupler_solid_cohesive_contact`, `coupler_solid_phasefield`, never the
//! mesh default after the parts are built) are registered as upstream and
//! reachable by name.

use std::path::Path;

use akantu_contact::ContactMechanicsModel;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshSelection,
};
use akantu_mesh::Mesh;
use akantu_solid::SolidMechanicsModel;

use crate::contact::{CouplerSolidCohesiveContact, CouplerSolidContact};
use crate::phase_field::{CouplerSolidPhaseField, PHASE_ID};

/// Name of [`CouplerSolidContact`]'s own dumper.
pub const SOLID_CONTACT_DUMPER: &str = "coupler_solid_contact";
/// Name of [`CouplerSolidCohesiveContact`]'s own dumper.
pub const SOLID_COHESIVE_CONTACT_DUMPER: &str = "coupler_solid_cohesive_contact";
/// Name of [`CouplerSolidPhaseField`]'s own dumper.
pub const SOLID_PHASE_FIELD_DUMPER: &str = "coupler_solid_phasefield";
/// Base name of the default dumper of a [`CouplerSolidPhaseField`]'s
/// phase-field model (its own series; the coupler's series is
/// `phase_field_model`, upstream's mesh default).
pub const PHASE_PART_BASE_NAME: &str = "phase_field_model_part";

/// A coupler's own dumper as its constructor registers it (not the
/// default: the parts register theirs afterwards).
pub(crate) fn coupler_dumpable(
    name: &str,
    id: &str,
    spatial_dimension: usize,
    kind: ElementKind,
) -> Result<Dumpable> {
    let mut d = Dumpable::new();
    d.register_paraview(name, Some(id), false);
    d.add_dump_mesh_to_dumper(
        name,
        MeshSelection::new(Some(spatial_dimension), GhostType::NotGhost, kind),
    )?;
    Ok(d)
}

/// `true` if the solid's descriptor `d` is a nodal field of type `ty`
/// (mesh data, resolved last, excluded).
fn solid_nodal_of(d: &FieldDescriptor, ty: FieldValueType) -> bool {
    d.support == FieldSupport::Nodal
        && d.value_type == ty
        && !matches!(d.source, FieldSource::Mesh(_))
}

/// The descriptor a solid–contact coupler chooses for `name` (module docs:
/// type-major, contact first).
///
/// # Errors
/// A name neither part resolves (UD-683: the solid model's error).
pub fn solid_contact_descriptor(
    contact: &ContactMechanicsModel,
    solid: &SolidMechanicsModel,
    name: &str,
    padding: bool,
) -> Result<FieldDescriptor> {
    let solid_d = solid.dump_field_descriptor(name, None, padding);
    for ty in [
        FieldValueType::Real,
        FieldValueType::Int,
        FieldValueType::Bool,
    ] {
        if let Some(d) = contact.nodal_dump_field_descriptor(name, ty, padding) {
            return Ok(d);
        }
        if let Ok(d) = &solid_d
            && solid_nodal_of(d, ty)
        {
            return Ok(d.clone());
        }
    }
    // elemental fields: the contact model has none (`Model::
    // createElementalField`), then the solid's; then the mesh data
    // (`createFieldFromAttachedData` of the coupler's mesh, the solid's)
    solid_d
}

/// Dump-time resolution of a solid–contact coupler's dumper: the contact
/// model's fields by the contact model, every other field and every law
/// parameter by the solid model.
struct SolidContactFields<'a, S: FieldResolver> {
    contact: &'a ContactMechanicsModel,
    solid: S,
}

impl<S: FieldResolver> FieldResolver for SolidContactFields<'_, S> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        if self.contact.owns_dump_field(field) {
            self.contact.resolve(mesh, field, ty, ghost)
        } else {
            self.solid.resolve(mesh, field, ty, ghost)
        }
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        self.solid.law_parameter(mesh, ty, ghost, name)
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        self.solid.variable(name)
    }
}

/// Which dumpers hold a dumper name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Coupler,
    Contact,
    Solid,
}

fn unknown_dumper(name: &str) -> Error {
    Error::not_found("dumper", name)
}

fn has_dumper(d: &Dumpable, name: &str) -> bool {
    d.dumper_names().any(|n| n == name)
}

/// The dumpers of a solid–contact coupler (`own`, the contact model's, the
/// solid model's), the one upstream's mesh default is, and the dump.
macro_rules! solid_contact_dump_api {
    ($t:ty, $solid:ident $(. $inner:ident)?, $default:expr) => {
        impl $t {
            fn dumper_owner(&self, dumper: Option<&str>) -> Result<Owner> {
                let Some(name) = dumper else {
                    return Ok($default);
                };
                if has_dumper(&self.dumpable, name) {
                    Ok(Owner::Coupler)
                } else if has_dumper(self.contact.dumpable(), name) {
                    Ok(Owner::Contact)
                } else if has_dumper(&self.$solid$(.$inner)?.dumpers.dumpable, name) {
                    Ok(Owner::Solid)
                } else {
                    Err(unknown_dumper(name))
                }
            }

            fn owner_dumpable(&mut self, owner: Owner) -> &mut Dumpable {
                match owner {
                    Owner::Coupler => &mut self.dumpable,
                    Owner::Contact => self.contact.dumpable_mut(),
                    Owner::Solid => &mut self.$solid$(.$inner)?.dumpers.dumpable,
                }
            }

            fn add_coupler_field(
                &mut self,
                dumper: Option<&str>,
                name: &str,
                padding: bool,
            ) -> Result<()> {
                let owner = self.dumper_owner(dumper)?;
                let d = solid_contact_descriptor(
                    &self.contact,
                    &self.$solid$(.$inner)?,
                    name,
                    padding,
                )?;
                let target = self.owner_dumpable(owner);
                match dumper {
                    None => target.add_dump_field(d),
                    Some(n) => target.add_dump_field_to_dumper(n, d),
                }
            }

            /// Upstream `coupler.addDumpField(name)` (the mesh's default
            /// dumper, module docs; no padding).
            ///
            /// # Errors
            /// A name neither part resolves (UD-683); a duplicate field.
            pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
                self.add_coupler_field(None, name, false)
            }

            /// Upstream `coupler.addDumpFieldVector(name)` (real nodal
            /// fields padded to 3 components).
            ///
            /// # Errors
            /// As `add_dump_field`.
            pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
                self.add_coupler_field(None, name, true)
            }

            /// Upstream `coupler.addDumpFieldTensor(name)` (padding, as the
            /// vector variant).
            ///
            /// # Errors
            /// As `add_dump_field`.
            pub fn add_dump_field_tensor(&mut self, name: &str) -> Result<()> {
                self.add_coupler_field(None, name, true)
            }

            /// Upstream `coupler.addDumpFieldToDumper(dumper, name)`: the
            /// coupler's own dumper, the contact model's or the solid
            /// model's of that name (upstream: the mesh's dumper `dumper`).
            ///
            /// # Errors
            /// Unknown dumper; as `add_dump_field`.
            pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
                self.add_coupler_field(Some(dumper), name, false)
            }

            /// Upstream `coupler.addDumpFieldVectorToDumper(dumper, name)`.
            ///
            /// # Errors
            /// As `add_dump_field_to_dumper`.
            pub fn add_dump_field_vector_to_dumper(
                &mut self,
                dumper: &str,
                name: &str,
            ) -> Result<()> {
                self.add_coupler_field(Some(dumper), name, true)
            }

            /// Upstream `coupler.addDumpFieldTensorToDumper(dumper, name)`.
            ///
            /// # Errors
            /// As `add_dump_field_to_dumper`.
            pub fn add_dump_field_tensor_to_dumper(
                &mut self,
                dumper: &str,
                name: &str,
            ) -> Result<()> {
                self.add_coupler_field(Some(dumper), name, true)
            }

            /// Upstream `coupler.removeDumpField(name)` (default dumper).
            ///
            /// # Errors
            /// No default dumper.
            pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
                let owner = self.dumper_owner(None)?;
                self.owner_dumpable(owner).remove_dump_field(name)
            }

            /// Upstream `coupler.setBaseName(name)` (the mesh's default
            /// dumper).
            ///
            /// # Errors
            /// No default dumper.
            pub fn set_base_name(&mut self, name: &str) -> Result<()> {
                let owner = self.dumper_owner(None)?;
                self.owner_dumpable(owner).set_base_name(name)
            }

            /// Upstream `coupler.setBaseNameToDumper(dumper, name)`.
            ///
            /// # Errors
            /// Unknown dumper.
            pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
                let owner = self.dumper_owner(Some(dumper))?;
                self.owner_dumpable(owner)
                    .set_base_name_to_dumper(dumper, name)
            }

            /// Upstream `coupler.setDirectory(dir)` (the mesh's default
            /// dumper; `./paraview` by default).
            ///
            /// # Errors
            /// No default dumper.
            pub fn set_directory(&mut self, dir: &Path) -> Result<()> {
                let owner = self.dumper_owner(None)?;
                self.owner_dumpable(owner).set_directory(dir)
            }

            /// Upstream `coupler.setDirectoryToDumper(dumper, dir)`.
            ///
            /// # Errors
            /// Unknown dumper.
            pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &Path) -> Result<()> {
                let owner = self.dumper_owner(Some(dumper))?;
                self.owner_dumpable(owner)
                    .set_directory_to_dumper(dumper, dir)
            }

            /// Upstream `coupler.dump()` / `dump(step)` / `dump(time,
            /// step)`: `solid->onDump()`, then the mesh's default dumper
            /// (module docs), each field read from the part that provides
            /// it.
            ///
            /// # Errors
            /// Errors of the solid's dump event handlers and of the dump.
            pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
                self.dump_dumper(None, time, step)
            }

            /// Upstream `coupler.dump(dumper_name, …)`.
            ///
            /// # Errors
            /// As `dump`; unknown dumper.
            pub fn dump_named(
                &mut self,
                dumper: &str,
                time: Option<Real>,
                step: Option<usize>,
            ) -> Result<()> {
                self.dump_dumper(Some(dumper), time, step)
            }

            fn dump_dumper(
                &mut self,
                dumper: Option<&str>,
                time: Option<Real>,
                step: Option<usize>,
            ) -> Result<()> {
                let owner = self.dumper_owner(dumper)?;
                if owner == Owner::Solid {
                    // the solid model sends its dump event itself
                    let contact = &self.contact;
                    let owns = |d: &FieldDescriptor| contact.owns_dump_field(d);
                    return self.$solid$(.$inner)?.dump_with_fields_of(
                        contact, &owns, dumper, time, step,
                    );
                }
                self.$solid$(.$inner)?.on_dump()?;
                let solid = &self.$solid$(.$inner)?;
                match owner {
                    Owner::Contact => self.contact.dump_with_fields_of(
                        &solid.field_resolver(),
                        dumper,
                        time,
                        step,
                    ),
                    _ => {
                        let mesh_h = solid.physics.base.mesh.clone();
                        let mesh = mesh_h.read()?;
                        let fields = SolidContactFields {
                            contact: &self.contact,
                            solid: solid.field_resolver(),
                        };
                        match dumper {
                            None => self.dumpable.dump(&mesh, &fields, time, step),
                            Some(n) => self.dumpable.dump_named(n, &mesh, &fields, time, step),
                        }
                    }
                }
            }

            /// Upstream `coupler.dumpGroup(group)` (`model.cc:100-104`):
            /// the group's default dumper (the mesh domain's group-dumper
            /// registry), no dump event.
            ///
            /// # Errors
            /// Unknown group; the dump's errors.
            pub fn dump_group(&mut self, group: &str) -> Result<()> {
                self.$solid$(.$inner)?.dump_group(group)
            }
        }
    };
}

solid_contact_dump_api!(CouplerSolidContact, solid, Owner::Contact);

impl CouplerSolidContact {
    /// Upstream `coupler.getContactMechanicsModel().dump(…)`: `Model::dump`
    /// of the mesh's default dumper — the contact model's, which the
    /// coupler's `addDumpField*` fill (TEST-215's driver registers through
    /// the coupler and dumps through the contact model) — without the
    /// solid's `onDump`; the fields the coupler registered for the solid
    /// are read from the solid.
    ///
    /// # Errors
    /// Dump errors; unknown dumper.
    pub fn dump_contact(
        &mut self,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        let solid = &self.solid;
        self.contact
            .dump_with_fields_of(&solid.field_resolver(), dumper, time, step)
    }

    /// Upstream `coupler.getSolidMechanicsModel().setTimeStep(dt,
    /// solver_id)` (`solid_mechanics_model.cc:91-95`): the time step of the
    /// shared solver and of the **mesh's default dumper** — the contact
    /// model's, i.e. the coupler's series, which then writes its `.pvd`.
    /// `self.solid.set_time_step` gives the time step
    /// to the solid model's own dumper instead (the port's models own their
    /// dumpers, UD-982).
    ///
    /// # Errors
    /// Unknown solver.
    pub fn solid_set_time_step(&mut self, time_step: Real, solver_id: Option<&str>) -> Result<()> {
        self.solid.set_solver_time_step(time_step, solver_id)?;
        self.contact.dumpable_mut().set_time_step(time_step)
    }

    /// Upstream `coupler.getSolidMechanicsModel().addDumpField(name)` /
    /// `addDumpFieldVector(name)` (`padding`): the field the **solid**
    /// model creates for `name` (`Model::addDumpGroupFieldToDumper` with
    /// the solid's `createNodalField*`), registered on the mesh's default
    /// dumper — the coupler's series, read from the solid at dump time.
    /// `self.solid.add_dump_field` registers it on the
    /// solid model's own dumper instead (UD-982).
    ///
    /// # Errors
    /// A name the solid model does not resolve (UD-683); a duplicate field.
    pub fn solid_add_dump_field(&mut self, name: &str, padding: bool) -> Result<()> {
        let d = self.solid.dump_field_descriptor(name, None, padding)?;
        self.contact.dumpable_mut().add_dump_field(d)
    }
}
solid_contact_dump_api!(CouplerSolidCohesiveContact, solid.smm, Owner::Solid);

impl CouplerSolidCohesiveContact {
    /// Upstream `coupler.getSolidMechanicsModelCohesive().dump(…)`: the
    /// solid's dump event and `Model::dump` of the mesh's default dumper —
    /// the cohesive solid model's, which the coupler's `addDumpField*`
    /// fill — with the fields the coupler registered for the contact model
    /// read from the contact model (the symmetric case
    /// of [`CouplerSolidContact::dump_contact`]). The dumpers of the
    /// cohesive domains (`"cohesive elements"`, `"facets"`) are dumped by
    /// the cohesive model as before.
    ///
    /// # Errors
    /// Dump errors; unknown dumper.
    pub fn dump_solid(
        &mut self,
        dumper: Option<&str>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        if let Some(n) = dumper
            && (n == akantu_cohesive::COHESIVE_DUMPER || n == akantu_cohesive::FACETS_DUMPER)
        {
            return self.solid.dump_named(n, time, step);
        }
        let contact = &self.contact;
        let owns = |d: &FieldDescriptor| contact.owns_dump_field(d);
        self.solid
            .smm
            .dump_with_fields_of(contact, &owns, dumper, time, step)
    }
}

impl CouplerSolidPhaseField {
    /// The solid's base name upstream's mesh default dumper has after the
    /// coupler is built (module docs: the phase-field model's id). The
    /// phase-field model's own default dumper, which upstream does not
    /// have apart from the mesh default, gets the distinct base name
    /// [`PHASE_PART_BASE_NAME`] so that its dumps never overwrite the
    /// coupler's series in the same directory.
    pub(crate) fn init_default_dump_series(&mut self) -> Result<()> {
        self.solid.set_base_name(PHASE_ID)?;
        self.phase.set_base_name(PHASE_PART_BASE_NAME)
    }

    fn solid_dumper_or_own(&self, dumper: &str) -> Result<bool> {
        if has_dumper(&self.dumpable, dumper) {
            Ok(true)
        } else if has_dumper(&self.solid.dumpers.dumpable, dumper) {
            Ok(false)
        } else {
            Err(unknown_dumper(dumper))
        }
    }

    fn add_coupler_field(&mut self, dumper: Option<&str>, name: &str, padding: bool) -> Result<()> {
        // upstream: the coupler's createNodalFieldReal / Bool and
        // createElementalField ask the solid only (no integer nodal field)
        let d = self.solid.dump_field_descriptor(name, None, padding)?;
        if d.support == FieldSupport::Nodal && d.value_type == FieldValueType::Int {
            return Err(Error::not_found("dump field", name));
        }
        match dumper {
            None => self.solid.dumpers.dumpable.add_dump_field(d),
            Some(n) => {
                if self.solid_dumper_or_own(n)? {
                    self.dumpable.add_dump_field_to_dumper(n, d)
                } else {
                    self.solid.dumpers.dumpable.add_dump_field_to_dumper(n, d)
                }
            }
        }
    }

    /// Upstream `coupler.addDumpField(name)` (the mesh's default dumper:
    /// the solid's here, module docs; the solid resolves the name).
    ///
    /// # Errors
    /// A name the solid model does not resolve (UD-683); a duplicate field.
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        self.add_coupler_field(None, name, false)
    }

    /// Upstream `coupler.addDumpFieldVector(name)` (padding).
    ///
    /// # Errors
    /// As [`CouplerSolidPhaseField::add_dump_field`].
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        self.add_coupler_field(None, name, true)
    }

    /// Upstream `coupler.addDumpFieldTensor(name)` (padding).
    ///
    /// # Errors
    /// As [`CouplerSolidPhaseField::add_dump_field`].
    pub fn add_dump_field_tensor(&mut self, name: &str) -> Result<()> {
        self.add_coupler_field(None, name, true)
    }

    /// Upstream `coupler.addDumpFieldToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper; as [`CouplerSolidPhaseField::add_dump_field`].
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_coupler_field(Some(dumper), name, false)
    }

    /// Upstream `coupler.addDumpFieldVectorToDumper(dumper, name)`.
    ///
    /// # Errors
    /// As [`CouplerSolidPhaseField::add_dump_field_to_dumper`].
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.add_coupler_field(Some(dumper), name, true)
    }

    /// Upstream `coupler.removeDumpField(name)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.solid.remove_dump_field(name)
    }

    /// Upstream `coupler.setBaseName(name)` (the default series).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.solid.set_base_name(name)
    }

    /// Upstream `coupler.setBaseNameToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        if self.solid_dumper_or_own(dumper)? {
            self.dumpable.set_base_name_to_dumper(dumper, name)
        } else {
            self.solid.set_base_name_to_dumper(dumper, name)
        }
    }

    /// Upstream `coupler.setDirectory(dir)` (the default series).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_directory(&mut self, dir: &Path) -> Result<()> {
        self.solid.set_directory(dir)
    }

    /// Upstream `coupler.setDirectoryToDumper(dumper, dir)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, dir: &Path) -> Result<()> {
        if self.solid_dumper_or_own(dumper)? {
            self.dumpable.set_directory_to_dumper(dumper, dir)
        } else {
            self.solid.set_directory_to_dumper(dumper, dir)
        }
    }

    /// Upstream `coupler.dump()` / `dump(step)` / `dump(time, step)`:
    /// `solid->onDump()`, then the default series (`mesh.dump()`).
    ///
    /// # Errors
    /// Errors of the solid's dump event handlers and of the dump.
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        self.solid.dump(time, step)
    }

    /// Upstream `coupler.dump(dumper_name, …)`.
    ///
    /// # Errors
    /// As [`CouplerSolidPhaseField::dump`]; unknown dumper.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        if self.solid_dumper_or_own(dumper)? {
            self.solid.on_dump()?;
            let mesh_h = self.solid.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let r = self.solid.field_resolver();
            self.dumpable.dump_named(dumper, &mesh, &r, time, step)
        } else {
            self.solid.dump_named(dumper, time, step)
        }
    }

    /// Upstream `coupler.dumpGroup(group)` (`model.cc:100-104`): the
    /// group's default dumper, no dump event.
    ///
    /// # Errors
    /// Unknown group; the dump's errors.
    pub fn dump_group(&mut self, group: &str) -> Result<()> {
        self.solid.dump_group(group)
    }
}
