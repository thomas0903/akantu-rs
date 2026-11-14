// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.cc (createNodalFieldReal, createNodalFieldBool,
// createElementalField, setTimeStep (default dumper only), constructor
// dumper registration),
// src/model/model.cc (dumper convenience API, addDumpGroupFieldToDumper
// field look-up order).

//! Dumping (STRUCT-013): the model's dumpers, the structural field names
//! and the [`FieldResolver`] reading live model state at dump time.
//!
//! Nodal fields are strided views of the DOF-sized arrays (upstream
//! `createStridedNodalField`): with `n = 2` (2-D) or `3` (3-D) translational
//! components per node,
//!
//! | name | array | components |
//! |---|---|---|
//! | `displacement`, `velocity`, `acceleration` | DOF family (orders 0, 1, 2) | `0..n` |
//! | `rotation` | displacement | `n..ndof` |
//! | `force`, `external_force` | external force | `0..n` |
//! | `momentum` | external force | `n..ndof` |
//! | `internal_force` | internal force | `0..n` |
//! | `internal_momentum` | internal force | `n..ndof` |
//! | `mass` | lumped mass | `0..n` |
//! | `rotary_inertia` | nodal rotary blocks of the lumped mass (not upstream; D-059) | all (`nr × nr`, row-major: 9 in 3-D, 1 in 2-D) |
//!
//! padded to 3 components by the `…Vector` variants (a 3-D `rotary_inertia`
//! keeps its 9 components). `blocked_dofs` is the full flag array (no
//! padding). Elemental: `stress` (every quadrature
//! point's generalised stresses per element, not homogenised: upstream
//! `InternalMaterialField` with `nb_stress · nb_quad` values per element)
//! and `element_index_by_material` (mesh data of that name, as upstream).
//! Other names fall back to mesh data (`createFieldFromAttachedData`).

use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshFilter, MeshSelection, Padding,
};
use akantu_mesh::Mesh;
use akantu_solver::DofManager;

use super::{DOF_ID, StructuralMechanicsModel, StructuralMechanicsPhysics};

/// Dumpers of a structural model (upstream: registered on the mesh by the
/// constructor: ParaView dumper `structural_mechanics_model`, file base name
/// = model id, default, with the structural elements of the model
/// dimension, not ghost).
#[derive(Debug)]
pub struct StructuralDumpers {
    /// The dumper registry.
    pub dumpable: Dumpable,
    spatial_dimension: usize,
}

impl StructuralDumpers {
    pub(super) fn new(id: &str, spatial_dimension: usize) -> Result<Self> {
        let mut dumpable = Dumpable::new();
        dumpable.register_paraview("structural_mechanics_model", Some(id), true);
        dumpable.add_dump_mesh(MeshSelection::new(
            Some(spatial_dimension),
            GhostType::NotGhost,
            ElementKind::Structural,
        ))?;
        Ok(Self {
            dumpable,
            spatial_dimension,
        })
    }

    /// Upstream `StructuralMechanicsModel::setTimeStep`:
    /// `mesh.getDumper().setTimeStep(dt)`, the **default** dumper only
    /// (`structural_mechanics_model.cc:130-134`): any other dumper keeps its
    /// own time step and writes no time description (`.pvd`) unless it is
    /// given one
    /// (`fixtures/model/structural/dump_set_time_step`).
    pub(super) fn set_time_step(&mut self, dt: Real) -> Result<()> {
        self.dumpable.set_time_step(dt)
    }

    /// Spatial dimension of the model.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }
}

/// Field descriptor for a structural dump field name (upstream look-up
/// order of `Model::addDumpGroupFieldToDumper`: `createNodalFieldReal`,
/// `createNodalFieldBool`, `createElementalField`, then attached mesh
/// data). `dim` is the model dimension, `ndof` the DOFs per node.
///
/// # Errors
/// None at present (unknown names become mesh-data fields, resolved — or
/// refused — at dump time).
pub fn structural_mechanics_field(
    name: &str,
    group: Option<&str>,
    padding: bool,
    dim: usize,
    ndof: usize,
) -> Result<FieldDescriptor> {
    let n = if dim == 2 { 2 } else { 3 };
    let pad = if padding {
        Padding::To3D
    } else {
        Padding::None
    };
    let group = group.filter(|g| *g != "all").map(str::to_owned);
    let strided = |source: FieldSource, first: usize, count: usize| FieldDescriptor {
        name: name.to_owned(),
        source: FieldSource::Components {
            source: Box::new(source),
            first,
            count,
        },
        support: FieldSupport::Nodal,
        value_type: FieldValueType::Real,
        group: group.clone(),
        padding: pad,
        homogenize: false,
    };
    let dof = |order| FieldSource::Dof {
        family: DOF_ID.into(),
        order,
    };
    let nodal = |a: &str| FieldSource::ModelNodal(a.into());
    let rot = ndof.saturating_sub(n);
    Ok(match name {
        "displacement" => strided(dof(0), 0, n),
        "velocity" => strided(dof(1), 0, n),
        "acceleration" => strided(dof(2), 0, n),
        "rotation" => strided(dof(0), n, rot),
        "force" | "external_force" => strided(nodal("external_force"), 0, n),
        "momentum" => strided(nodal("external_force"), n, rot),
        "internal_force" => strided(nodal("internal_force"), 0, n),
        "internal_momentum" => strided(nodal("internal_force"), n, rot),
        "mass" => strided(nodal("mass"), 0, n),
        // not upstream (D-059): the nodal rotary blocks of the lumped mass
        // (`rotary_inertia()`); upstream would read mesh data of that name
        "rotary_inertia" => FieldDescriptor {
            name: name.to_owned(),
            source: nodal("rotary_inertia"),
            support: FieldSupport::Nodal,
            value_type: FieldValueType::Real,
            group,
            padding: pad,
            homogenize: false,
        },
        "blocked_dofs" => FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::Blocked {
                family: DOF_ID.into(),
            },
            support: FieldSupport::Nodal,
            value_type: FieldValueType::Bool,
            group,
            padding: Padding::None,
            homogenize: false,
        },
        "stress" => FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::ModelElemental("stress".into()),
            support: FieldSupport::QuadraturePoint,
            value_type: FieldValueType::Real,
            group,
            padding: Padding::None,
            homogenize: false,
        },
        "element_index_by_material" => FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::Mesh(name.to_owned()),
            support: FieldSupport::Elemental,
            value_type: FieldValueType::Int,
            group,
            padding: Padding::None,
            homogenize: false,
        },
        _ => FieldDescriptor {
            name: name.to_owned(),
            source: FieldSource::Mesh(name.to_owned()),
            support: FieldSupport::QuadraturePoint,
            value_type: FieldValueType::Int,
            group,
            padding: Padding::None,
            homogenize: false,
        },
    })
}

/// Live model state for one dump.
struct Resolver<'a> {
    physics: &'a StructuralMechanicsPhysics,
    dofs: &'a DofManager,
}

impl FieldResolver for Resolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
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
            (FieldSource::ModelNodal(n), None) => {
                let p = self.physics;
                let missing = || Error::not_found("nodal dump field (not allocated)", n.clone());
                let (nb_component, values) = match n.as_str() {
                    "external_force" | "internal_force" => {
                        let a = if n == "external_force" {
                            p.external_force.as_ref()
                        } else {
                            p.internal_force.as_ref()
                        }
                        .ok_or_else(missing)?
                        .read()?;
                        (a.nb_component(), a.as_slice().to_vec())
                    }
                    "mass" | "rotary_inertia" => {
                        let a = if n == "mass" {
                            p.mass.as_ref()
                        } else {
                            p.rotary_inertia.as_ref()
                        }
                        .ok_or_else(missing)?;
                        (a.nb_component(), a.as_slice().to_vec())
                    }
                    _ => return Err(missing()),
                };
                Ok(DumpBuffer::Real {
                    nb_component,
                    values,
                })
            }
            (FieldSource::ModelElemental(n), Some(t)) if n == "stress" => {
                let a = self.physics.stress.get(t, ghost)?;
                Ok(DumpBuffer::Real {
                    nb_component: a.nb_component(),
                    values: a.as_slice().to_vec(),
                })
            }
            _ => Err(Error::not_found(
                "structural mechanics dump field",
                field.name.clone(),
            )),
        }
    }
}

impl StructuralMechanicsModel {
    fn descriptor(
        &self,
        name: &str,
        group: Option<&str>,
        padding: bool,
    ) -> Result<FieldDescriptor> {
        structural_mechanics_field(
            name,
            group,
            padding,
            self.physics.base.spatial_dimension,
            self.physics.nb_degree_of_freedom,
        )
    }

    /// Upstream `addDumpField(name)` (default dumper, no padding).
    ///
    /// # Errors
    /// No default dumper.
    pub fn add_dump_field(&mut self, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, false)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldVector(name)` (padding to 3 components).
    ///
    /// # Errors
    /// No default dumper.
    pub fn add_dump_field_vector(&mut self, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, true)?;
        self.dumpers.dumpable.add_dump_field(d)
    }

    /// Upstream `addDumpFieldTensor(name)` (padding).
    ///
    /// # Errors
    /// No default dumper.
    pub fn add_dump_field_tensor(&mut self, name: &str) -> Result<()> {
        self.add_dump_field_vector(name)
    }

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, false)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpFieldVectorToDumper(dumper, name)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, true)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpGroupFieldToDumper(dumper, name, group, …, padding)`.
    ///
    /// # Errors
    /// Unknown dumper.
    pub fn add_dump_group_field_to_dumper(
        &mut self,
        dumper: &str,
        name: &str,
        group: &str,
        padding: bool,
    ) -> Result<()> {
        let d = self.descriptor(name, Some(group), padding)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `removeDumpField(name)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.remove_dump_field(name)
    }

    /// Upstream `setBaseName(name)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.set_base_name(name)
    }

    /// Upstream `setDirectory(dir)` (default dumper).
    ///
    /// # Errors
    /// No default dumper.
    pub fn set_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        self.dumpers.dumpable.set_directory(dir)
    }

    /// Register a ParaView dumper restricted to an element group (upstream
    /// `ElementGroup::registerDumper` used by `dumpGroup`).
    ///
    /// # Errors
    /// Mesh-selection errors.
    pub fn register_group_dumper(&mut self, group: &str, file_name: Option<&str>) -> Result<()> {
        self.dumpers
            .dumpable
            .register_paraview(group, Some(file_name.unwrap_or(group)), false);
        self.dumpers.dumpable.add_dump_mesh_to_dumper(
            group,
            MeshSelection::new(None, GhostType::NotGhost, ElementKind::Structural)
                .filtered(MeshFilter::ElementGroup(group.to_owned())),
        )
    }

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` (default
    /// dumper).
    ///
    /// # Errors
    /// A registered field that is not available (e.g. `velocity` in a
    /// static analysis; upstream silently skips such fields at
    /// registration), I/O errors.
    pub fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let r = Resolver {
            physics: &self.physics,
            dofs: &self.solver.dof_manager,
        };
        self.dumpers.dumpable.dump(&mesh, &r, time, step)
    }

    /// Upstream `dump(dumper_name, …)`.
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::dump`].
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strided_fields_follow_upstream() {
        let d = structural_mechanics_field("rotation", None, true, 2, 3).unwrap();
        assert_eq!(
            d.source,
            FieldSource::Components {
                source: Box::new(FieldSource::Dof {
                    family: DOF_ID.into(),
                    order: 0
                }),
                first: 2,
                count: 1
            }
        );
        assert_eq!(d.padding, Padding::To3D);
        let m = structural_mechanics_field("momentum", None, false, 3, 6).unwrap();
        assert_eq!(
            m.source,
            FieldSource::Components {
                source: Box::new(FieldSource::ModelNodal("external_force".into())),
                first: 3,
                count: 3
            }
        );
        let b = structural_mechanics_field("blocked_dofs", Some("all"), true, 3, 6).unwrap();
        assert_eq!(b.padding, Padding::None);
        assert!(b.group.is_none());
        let s = structural_mechanics_field("stress", None, true, 2, 3).unwrap();
        assert_eq!(s.support, FieldSupport::QuadraturePoint);
        assert!(!s.homogenize);
        // D-059: the whole nodal rotary-inertia array (not strided)
        let j = structural_mechanics_field("rotary_inertia", None, true, 3, 6).unwrap();
        assert_eq!(j.source, FieldSource::ModelNodal("rotary_inertia".into()));
        assert_eq!(j.support, FieldSupport::Nodal);
        assert_eq!(j.value_type, FieldValueType::Real);
    }
}
