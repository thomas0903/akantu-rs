// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phase_field_model.cc
// (constructor dumper registration, createNodalFieldReal/Bool,
// createElementalField, setTimeStep), src/model/model.cc (dumper
// convenience API).

//! Dumping (PF-012): the model's ParaView dumper `phase_field` (file base
//! name = model id, default dumper, regular elements of the model
//! dimension, not ghost) and the [`FieldResolver`] reading the live model.
//!
//! Upstream's phase-field model creates the nodal fields `damage`,
//! `external_force`, `internal_force` (real), `blocked_dofs` (bool) and the
//! elemental field `partitions`; any other name gives no field (upstream:
//! a null field from `createNodalFieldReal`'s map lookup), which the port
//! reports as an error.

use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshSelection, Padding,
};
use akantu_mesh::Mesh;
use akantu_solver::DofManager;

use super::{DOF_ID, PhaseFieldModel, PhaseFieldPhysics};

/// Dumpers of a phase-field model (upstream: registered on the mesh by the
/// constructor, `registerDumper<DumperParaview>("phase_field", id, true)`).
#[derive(Debug)]
pub struct PhaseFieldDumpers {
    /// The dumper registry.
    pub dumpable: Dumpable,
}

impl PhaseFieldDumpers {
    pub(super) fn new(id: &str, spatial_dimension: usize) -> Result<Self> {
        let mut dumpable = Dumpable::new();
        dumpable.register_paraview("phase_field", Some(id), true);
        dumpable.add_dump_mesh(MeshSelection::new(
            Some(spatial_dimension),
            GhostType::NotGhost,
            ElementKind::Regular,
        ))?;
        Ok(Self { dumpable })
    }
}

/// Field descriptor of a phase-field dump field name (upstream
/// `createNodalFieldReal` / `createNodalFieldBool` /
/// `createElementalField`). `group` restricts the field to an element
/// group (`"all"`: the whole mesh).
pub fn phase_field_field(
    name: &str,
    group: Option<&str>,
    padding: bool,
) -> Result<FieldDescriptor> {
    let group = group.filter(|g| *g != "all").map(str::to_owned);
    let pad = if padding {
        Padding::To3D
    } else {
        Padding::None
    };
    let nodal = |source| FieldDescriptor {
        name: name.to_owned(),
        source,
        support: FieldSupport::Nodal,
        value_type: FieldValueType::Real,
        group: group.clone(),
        padding: pad,
        homogenize: false,
    };
    match name {
        "damage" => Ok(nodal(FieldSource::Dof {
            family: DOF_ID.into(),
            order: 0,
        })),
        "external_force" | "internal_force" => Ok(nodal(FieldSource::ModelNodal(name.into()))),
        "blocked_dofs" => Ok(FieldDescriptor {
            value_type: FieldValueType::Bool,
            padding: Padding::None,
            ..nodal(FieldSource::Blocked {
                family: DOF_ID.into(),
            })
        }),
        "partitions" => Ok(FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::Partition,
            support: FieldSupport::Elemental,
            value_type: FieldValueType::Int,
            group,
            padding: Padding::None,
            homogenize: false,
        }),
        other => Err(Error::not_found(
            "phase-field dump field",
            format!(
                "{other} (PhaseFieldModel dumps damage, external_force, internal_force, \
                 blocked_dofs, partitions)"
            ),
        )),
    }
}

/// Live model state for one dump.
struct Resolver<'a> {
    physics: &'a PhaseFieldPhysics,
    dofs: &'a DofManager,
}

impl FieldResolver for Resolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::Dof { family, order: 0 }, None) => {
                let a = self.dofs.dofs(family)?;
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
            (FieldSource::ModelNodal(n), None) => {
                let missing = || Error::not_found("nodal dump field (not allocated)", n);
                let (nb_component, values) = match n.as_str() {
                    "external_force" => {
                        let a = self.physics.external_force.as_ref().ok_or_else(missing)?;
                        (a.nb_component(), a.as_slice().to_vec())
                    }
                    "internal_force" => {
                        let a = self
                            .physics
                            .internal_force
                            .as_ref()
                            .ok_or_else(missing)?
                            .read()?;
                        (a.nb_component(), a.as_slice().to_vec())
                    }
                    _ => return Err(missing()),
                };
                Ok(DumpBuffer::Real {
                    nb_component,
                    values,
                })
            }
            _ => Err(Error::Unsupported(format!(
                "phase-field dump field {} cannot be resolved here",
                field.name
            ))),
        }
    }
}

impl PhaseFieldModel {
    /// Upstream `addDumpField(name)` (default dumper, no padding).
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        let d = phase_field_field(name, None, false)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldVector(name)` (padding to 3 components).
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        let d = phase_field_field(name, None, true)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = phase_field_field(name, None, false)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `setBaseName(name)` (default dumper).
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.set_base_name(name)
    }

    /// Upstream `setDirectory(dir)` (default dumper).
    pub fn set_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        self.dumpers.dumpable.set_directory(dir)
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` (default
    /// dumper `phase_field`).
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let r = Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        };
        self.dumpers.dumpable.dump(&mesh, &r, time, step)
    }

    /// Upstream `dump(dumper_name)` / `dump(dumper_name, step)` /
    /// `dump(dumper_name, time, step)` (`Model::dump`, any registered
    /// dumper).
    pub fn dump_named(
        &mut self,
        dumper: &str,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
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

    /// Upstream `PhaseFieldModel::setTimeStep`:
    /// `mesh.getDumper("phase_field").setTimeStep(dt)`, the model's own
    /// dumper only (`phase_field_model.cc:309-313`): any other dumper keeps
    /// its own time step and writes no time description (`.pvd`) unless it
    /// is given one
    /// (`fixtures/model/phase_field/dump_set_time_step`).
    pub(super) fn set_dumpers_time_step(&mut self, dt: Real) -> Result<()> {
        self.dumpers
            .dumpable
            .set_time_step_to_dumper("phase_field", dt)
    }
}
