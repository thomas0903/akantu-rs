// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.cc
// (createNodalFieldReal, createNodalFieldBool, createElementalField,
// setTimeStep), src/model/diffusion_model/heat_transfer_model.hh (dump
// aliases), src/model/model.cc (dumper convenience API).

//! Dumping (DIFF-011): the model's dumpers and the [`FieldResolver`]
//! reading live model state at dump time.
//!
//! Fields: nodal `<dof>` and `<dof>_rate` (DOF manager), `external_flow`,
//! `internal_flow` (model), `blocked_dofs`; elemental `partitions` and any
//! law internal (`∇u`, `D∇u`, `diffusivity`), homogenised per element; any
//! other name falls back to mesh data (upstream
//! `createFieldFromAttachedData`). A heat-transfer model adds the aliases
//! `internal_heat_rate → internal_flow`, `external_heat_rate →
//! external_flow`, `conductivity → diffusivity`. `capacity_lumped` is
//! refused (upstream exception). Upstream's `increment` field and the rate
//! of a static model point to unallocated arrays and crash at dump time
//! (UD-286): the port reports "not allocated".

use akantu_core::ElementKind;
use akantu_core::{ElementType, Error, GhostType, Idx, Real, Result};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshFilter, MeshSelection, Padding,
};
use akantu_mesh::Mesh;
use akantu_solver::DofManager;

use super::{DiffusionModel, DiffusionPhysics};
use crate::law::{DIFFUSIVITY, DiffusionModelKind};

/// Dumpers of a diffusion model (upstream: `mesh.registerDumper<
/// DumperParaview>(id, id, true)` and `addDumpMesh(mesh, dim, _not_ghost,
/// _ek_regular)`).
#[derive(Debug)]
pub struct DiffusionDumpers {
    /// The dumper registry.
    pub dumpable: Dumpable,
    /// Name of the model's own dumper (the model id).
    id: String,
    spatial_dimension: usize,
}

impl DiffusionDumpers {
    pub(super) fn new(id: &str, spatial_dimension: usize) -> Result<Self> {
        let mut dumpable = Dumpable::new();
        dumpable.register_paraview(id, Some(id), true);
        dumpable.add_dump_mesh(MeshSelection::new(
            Some(spatial_dimension),
            GhostType::NotGhost,
            ElementKind::Regular,
        ))?;
        Ok(Self {
            dumpable,
            id: id.to_owned(),
            spatial_dimension,
        })
    }

    /// Upstream `setTimeStep`: `mesh.getDumper(id).setTimeStep(dt)`
    /// (`diffusion_model.cc:270-274`) — the model's own dumper only; other
    /// dumpers (group dumpers, user-registered ones) keep their time step.
    pub(super) fn set_time_step(&mut self, dt: Real) -> Result<()> {
        self.dumpable.set_time_step_to_dumper(&self.id, dt)
    }

    /// Spatial dimension used for elemental fields.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }
}

/// The source of a dump field name for a model with DOF `dof_name` (after
/// the heat-transfer aliases).
pub fn diffusion_field(
    name: &str,
    group: Option<&str>,
    padding: bool,
    kind: DiffusionModelKind,
    dof_name: &str,
    is_internal: impl Fn(&str) -> bool,
) -> Result<FieldDescriptor> {
    let pad = if padding {
        Padding::To3D
    } else {
        Padding::None
    };
    let d = |source, support, value_type, homogenize| FieldDescriptor {
        name: name.to_owned(),
        source,
        support,
        value_type,
        group: group.filter(|g| *g != "all").map(str::to_owned),
        padding: pad,
        homogenize,
    };
    let target = match (kind, name) {
        (DiffusionModelKind::HeatTransfer, "internal_heat_rate") => "internal_flow",
        (DiffusionModelKind::HeatTransfer, "external_heat_rate") => "external_flow",
        (DiffusionModelKind::HeatTransfer, "conductivity") => DIFFUSIVITY,
        (_, n) => n,
    };
    if target == "capacity_lumped" {
        return Err(Error::invalid(
            "Capacity lumped is a nodal field now stored in the DOF manager.Therefore it cannot \
             be used by a dumper anymore",
        ));
    }
    if target == dof_name {
        return Ok(d(
            FieldSource::Dof {
                family: dof_name.to_owned(),
                order: 0,
            },
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        ));
    }
    if target.strip_suffix("_rate") == Some(dof_name) {
        return Ok(d(
            FieldSource::Dof {
                family: dof_name.to_owned(),
                order: 1,
            },
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        ));
    }
    Ok(match target {
        "external_flow" | "internal_flow" | "increment" => d(
            FieldSource::ModelNodal(target.to_owned()),
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        ),
        "blocked_dofs" => FieldDescriptor {
            padding: Padding::None,
            ..d(
                FieldSource::Blocked {
                    family: dof_name.to_owned(),
                },
                FieldSupport::Nodal,
                FieldValueType::Bool,
                false,
            )
        },
        "partitions" => d(
            FieldSource::Partition,
            FieldSupport::Elemental,
            FieldValueType::Int,
            false,
        ),
        n if is_internal(n) => d(
            FieldSource::Internal {
                law: None,
                name: n.to_owned(),
            },
            FieldSupport::QuadraturePoint,
            FieldValueType::Real,
            true,
        ),
        n => FieldDescriptor {
            padding: Padding::None,
            ..d(
                FieldSource::Mesh(n.to_owned()),
                FieldSupport::QuadraturePoint,
                FieldValueType::Int,
                false,
            )
        },
    })
}

/// Live model state for one dump.
struct Resolver<'a> {
    physics: &'a DiffusionPhysics,
    dofs: &'a DofManager,
}

impl Resolver<'_> {
    fn nodal_real(&self, name: &str) -> Result<DumpBuffer> {
        let p = self.physics;
        let a = match name {
            "external_flow" => p.external_flow.as_ref(),
            "internal_flow" => p.internal_flow.as_ref(),
            // upstream: never allocated (nullptr field, crash at dump, UD-286)
            _ => None,
        }
        .ok_or_else(|| Error::not_found("nodal dump field (not allocated)", name))?
        .read()?;
        Ok(DumpBuffer::Real {
            nb_component: a.nb_component(),
            values: a.as_slice().to_vec(),
        })
    }

    /// Upstream `flattenInternal(name)` for one block: every element of the
    /// block gets the values of its law (`nb_quad × nb_component`), zeros
    /// for elements whose law has no such block.
    fn internal(
        &self,
        mesh: &Mesh,
        law_name: Option<&str>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let fe = self.physics.base.fems.get("")?;
        let nq = fe.nb_integration_points(ty, ghost).unwrap_or(0);
        let ne = mesh.nb_element(ty, ghost);
        let mut nbc = None;
        let mut blocks = Vec::new();
        for law in self.physics.laws.laws() {
            let b = law.base();
            if law_name.is_some_and(|n| n != b.name) {
                continue;
            }
            let Ok(field) = b.internal(name) else {
                continue;
            };
            let Ok(values) = field.get(ty, ghost) else {
                continue;
            };
            nbc = Some(field.nb_component());
            blocks.push((b.filter(ty, ghost).to_vec(), values.as_slice().to_vec()));
        }
        let Some(c) = nbc else {
            return Err(Error::not_found("internal field", name));
        };
        let mut values = vec![0.0; ne * nq * c];
        for (filter, v) in blocks {
            for (local, &el) in filter.iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                let src = v
                    .get(local * nq * c..(local + 1) * nq * c)
                    .ok_or(Error::OutOfRange {
                        what: "internal block",
                        index: local,
                        len: v.len() / (nq * c).max(1),
                    })?;
                values
                    .get_mut(e * nq * c..(e + 1) * nq * c)
                    .ok_or(Error::OutOfRange {
                        what: "element",
                        index: e,
                        len: ne,
                    })?
                    .copy_from_slice(src);
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
            (FieldSource::Internal { law, name }, Some(t)) => {
                self.internal(mesh, law.as_deref(), name, t, ghost)
            }
            _ => Err(Error::not_found("diffusion dump field", field.name.clone())),
        }
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        let laws = &self.physics.laws;
        let idx = laws.law_index_map().get(ty, ghost)?;
        let ne = mesh.nb_element(ty, ghost);
        let mut out = Vec::with_capacity(ne);
        for e in 0..ne {
            let i: Idx = idx.as_slice().get(e).copied().unwrap_or(-1);
            let Some(i) = akantu_core::decode_idx(i)? else {
                out.push(0.0);
                continue;
            };
            let v = akantu_parser::ParameterRegistry::get_param(laws.law(i)?, name)?;
            out.push(match v {
                akantu_parser::ParamValue::Real(x) => x,
                akantu_parser::ParamValue::Int(x) => x as Real,
                akantu_parser::ParamValue::Bool(b) => Real::from(u8::from(b)),
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

impl DiffusionModel {
    fn is_internal(&self, name: &str) -> bool {
        self.physics
            .laws
            .laws()
            .iter()
            .any(|l| l.base().internal(name).is_ok())
    }

    fn descriptor(
        &self,
        name: &str,
        group: Option<&str>,
        padding: bool,
    ) -> Result<FieldDescriptor> {
        diffusion_field(
            name,
            group,
            padding,
            self.physics.kind,
            &self.physics.dof_name,
            |n| self.is_internal(n),
        )
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

    /// Upstream `addDumpFieldToDumper(dumper, name)`.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        let d = self.descriptor(name, None, false)?;
        self.dumpers.dumpable.add_dump_field_to_dumper(dumper, d)
    }

    /// Upstream `addDumpGroupFieldToDumper(dumper, name, group, …, padding)`.
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
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.remove_dump_field(name)
    }

    /// Upstream `setBaseName(name)` (default dumper).
    pub fn set_base_name(&mut self, name: &str) -> Result<()> {
        self.dumpers.dumpable.set_base_name(name)
    }

    /// Upstream `setDirectory(dir)` (default dumper).
    pub fn set_directory(&mut self, dir: &std::path::Path) -> Result<()> {
        self.dumpers.dumpable.set_directory(dir)
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

    /// Upstream `dump()` / `dump(step)` / `dump(time, step)` (default
    /// dumper).
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
